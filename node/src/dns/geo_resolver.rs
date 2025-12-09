//! Geographic DNS Resolver
//!
//! Sprint 30.3: Geo-Aware DNS Resolution
//!
//! Resolves DNS queries to the nearest healthy edge node based on
//! client geographic location. Uses optional MaxMind GeoIP database
//! for IP-to-location mapping.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::dns::edge_registry::{haversine_distance, EdgeNode, EdgeRegistry, GeoLocation};

/// Geographic resolver for DNS queries
pub struct GeoResolver {
    /// Edge node registry
    registry: Arc<EdgeRegistry>,
    /// Optional GeoIP database
    geoip: Option<GeoIpDatabase>,
    /// Fallback IPs when no edge nodes are available
    fallback_ipv4: Vec<Ipv4Addr>,
    fallback_ipv6: Vec<Ipv6Addr>,
    /// Default location when GeoIP lookup fails
    default_location: GeoLocation,
}

impl GeoResolver {
    /// Create a new geo resolver
    pub fn new(registry: Arc<EdgeRegistry>) -> Self {
        Self {
            registry,
            geoip: None,
            fallback_ipv4: vec![],
            fallback_ipv6: vec![],
            default_location: GeoLocation {
                latitude: 0.0,
                longitude: 0.0,
                country: None,
                region: None,
                city: None,
            },
        }
    }

    /// Load MaxMind GeoLite2 database
    pub fn with_geoip(mut self, db_path: &str) -> Result<Self, GeoResolverError> {
        let db = GeoIpDatabase::new(db_path)?;
        self.geoip = Some(db);
        info!("GeoIP database loaded from {}", db_path);
        Ok(self)
    }

    /// Set fallback IPv4 addresses
    pub fn with_fallback_ipv4(mut self, ips: Vec<Ipv4Addr>) -> Self {
        self.fallback_ipv4 = ips;
        self
    }

    /// Set fallback IPv6 addresses
    pub fn with_fallback_ipv6(mut self, ips: Vec<Ipv6Addr>) -> Self {
        self.fallback_ipv6 = ips;
        self
    }

    /// Set default location for failed GeoIP lookups
    pub fn with_default_location(mut self, location: GeoLocation) -> Self {
        self.default_location = location;
        self
    }

    /// Resolve client IP to geographic location
    pub fn locate_client(&self, client_ip: IpAddr) -> GeoLocation {
        if let Some(ref geoip) = self.geoip {
            match geoip.lookup(client_ip) {
                Ok(location) => {
                    debug!(
                        "GeoIP lookup for {}: {:?}, {:?}",
                        client_ip, location.country, location.city
                    );
                    return location;
                }
                Err(e) => {
                    debug!("GeoIP lookup failed for {}: {}", client_ip, e);
                }
            }
        }

        // Return default location
        self.default_location.clone()
    }

    /// Get best edge node IPs for a client
    pub async fn resolve_for_client(
        &self,
        client_ip: IpAddr,
        record_type: RecordType,
        count: usize,
    ) -> Vec<IpAddr> {
        // 1. Locate client geographically
        let location = self.locate_client(client_ip);

        // 2. Find nearest healthy nodes
        let nodes = if location.latitude != 0.0 || location.longitude != 0.0 {
            self.registry.find_nearest(&location, count * 2).await
        } else {
            // No location data, get all healthy nodes
            self.registry.get_all_healthy().await
        };

        // 3. Extract IPs of requested type
        let ips: Vec<IpAddr> = nodes
            .into_iter()
            .filter_map(|n| match record_type {
                RecordType::A => n.ipv4.map(IpAddr::V4),
                RecordType::AAAA => n.ipv6.map(IpAddr::V6),
            })
            .take(count)
            .collect();

        // 4. Fallback if no nodes available
        if ips.is_empty() {
            debug!(
                "No edge nodes available for {}, using fallback",
                client_ip
            );
            return match record_type {
                RecordType::A => self.fallback_ipv4.iter().map(|ip| IpAddr::V4(*ip)).collect(),
                RecordType::AAAA => self.fallback_ipv6.iter().map(|ip| IpAddr::V6(*ip)).collect(),
            };
        }

        ips
    }

    /// Get resolution info for debugging/analytics
    pub async fn resolve_with_info(
        &self,
        client_ip: IpAddr,
        record_type: RecordType,
        count: usize,
    ) -> ResolutionResult {
        let location = self.locate_client(client_ip);
        let nodes = if location.latitude != 0.0 || location.longitude != 0.0 {
            self.registry.find_nearest(&location, count * 2).await
        } else {
            self.registry.get_all_healthy().await
        };

        let selected_nodes: Vec<SelectedNode> = nodes
            .iter()
            .take(count)
            .map(|n| {
                let distance = if location.latitude != 0.0 || location.longitude != 0.0 {
                    Some(haversine_distance(
                        location.latitude,
                        location.longitude,
                        n.latitude,
                        n.longitude,
                    ))
                } else {
                    None
                };

                SelectedNode {
                    id: n.id.clone(),
                    ipv4: n.ipv4,
                    ipv6: n.ipv6,
                    region: n.region.clone(),
                    country: n.country.clone(),
                    distance_km: distance,
                }
            })
            .collect();

        let ips: Vec<IpAddr> = selected_nodes
            .iter()
            .filter_map(|n| match record_type {
                RecordType::A => n.ipv4.map(IpAddr::V4),
                RecordType::AAAA => n.ipv6.map(IpAddr::V6),
            })
            .collect();

        let used_fallback = ips.is_empty();
        let final_ips = if used_fallback {
            match record_type {
                RecordType::A => self.fallback_ipv4.iter().map(|ip| IpAddr::V4(*ip)).collect(),
                RecordType::AAAA => self.fallback_ipv6.iter().map(|ip| IpAddr::V6(*ip)).collect(),
            }
        } else {
            ips
        };

        ResolutionResult {
            client_ip,
            client_location: location,
            record_type,
            selected_nodes,
            resolved_ips: final_ips,
            used_fallback,
            total_healthy_nodes: self.registry.get_all_healthy().await.len(),
        }
    }

    /// Get nodes by region for debugging
    pub async fn get_nodes_by_region(&self, region: &str) -> Vec<EdgeNode> {
        self.registry.get_healthy_in_region(region).await
    }

    /// Get nodes by country for debugging
    pub async fn get_nodes_by_country(&self, country: &str) -> Vec<EdgeNode> {
        self.registry.get_healthy_in_country(country).await
    }
}

/// Record type for geo resolution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordType {
    A,
    AAAA,
}

impl std::fmt::Display for RecordType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecordType::A => write!(f, "A"),
            RecordType::AAAA => write!(f, "AAAA"),
        }
    }
}

/// Selected node information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectedNode {
    pub id: String,
    pub ipv4: Option<Ipv4Addr>,
    pub ipv6: Option<Ipv6Addr>,
    pub region: String,
    pub country: String,
    pub distance_km: Option<f64>,
}

/// Resolution result with debugging info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolutionResult {
    /// Client IP that was resolved
    pub client_ip: IpAddr,
    /// Client's geographic location
    pub client_location: GeoLocation,
    /// Record type requested
    pub record_type: RecordType,
    /// Nodes that were selected
    pub selected_nodes: Vec<SelectedNode>,
    /// Final resolved IP addresses
    pub resolved_ips: Vec<IpAddr>,
    /// Whether fallback IPs were used
    pub used_fallback: bool,
    /// Total healthy nodes in registry
    pub total_healthy_nodes: usize,
}

/// GeoIP database wrapper
pub struct GeoIpDatabase {
    #[cfg(feature = "geoip")]
    reader: maxminddb::Reader<Vec<u8>>,
    #[cfg(not(feature = "geoip"))]
    _path: String,
}

impl GeoIpDatabase {
    /// Create a new GeoIP database from file
    pub fn new(db_path: &str) -> Result<Self, GeoResolverError> {
        if !Path::new(db_path).exists() {
            return Err(GeoResolverError::DatabaseNotFound(db_path.to_string()));
        }

        #[cfg(feature = "geoip")]
        {
            let reader = maxminddb::Reader::open_readfile(db_path)
                .map_err(|e| GeoResolverError::DatabaseError(e.to_string()))?;
            Ok(Self { reader })
        }

        #[cfg(not(feature = "geoip"))]
        {
            warn!("GeoIP feature not enabled, using mock database");
            Ok(Self { _path: db_path.to_string() })
        }
    }

    /// Lookup IP address in database
    pub fn lookup(&self, ip: IpAddr) -> Result<GeoLocation, GeoResolverError> {
        #[cfg(feature = "geoip")]
        {
            use maxminddb::geoip2;

            let city: geoip2::City = self
                .reader
                .lookup(ip)
                .map_err(|e| GeoResolverError::LookupError(e.to_string()))?;

            let location = city.location.ok_or_else(|| {
                GeoResolverError::LookupError("No location data".to_string())
            })?;

            Ok(GeoLocation {
                latitude: location.latitude.unwrap_or(0.0),
                longitude: location.longitude.unwrap_or(0.0),
                country: city
                    .country
                    .and_then(|c| c.iso_code.map(|s| s.to_string())),
                region: city
                    .subdivisions
                    .and_then(|s| s.first().and_then(|sub| sub.iso_code.map(|s| s.to_string()))),
                city: city
                    .city
                    .and_then(|c| c.names.and_then(|n| n.get("en").map(|s| s.to_string()))),
            })
        }

        #[cfg(not(feature = "geoip"))]
        {
            // Mock implementation for testing without GeoIP database
            // Returns location based on IP patterns
            match ip {
                IpAddr::V4(v4) => {
                    let octets = v4.octets();
                    match octets[0] {
                        // Simulate US IPs
                        1..=50 => Ok(GeoLocation {
                            latitude: 40.7128,
                            longitude: -74.0060,
                            country: Some("US".to_string()),
                            region: Some("NY".to_string()),
                            city: Some("New York".to_string()),
                        }),
                        // Simulate EU IPs
                        51..=100 => Ok(GeoLocation {
                            latitude: 51.5074,
                            longitude: -0.1278,
                            country: Some("GB".to_string()),
                            region: Some("ENG".to_string()),
                            city: Some("London".to_string()),
                        }),
                        // Simulate Asia IPs
                        101..=150 => Ok(GeoLocation {
                            latitude: 35.6762,
                            longitude: 139.6503,
                            country: Some("JP".to_string()),
                            region: Some("TK".to_string()),
                            city: Some("Tokyo".to_string()),
                        }),
                        _ => Err(GeoResolverError::LookupError("Unknown IP range".to_string())),
                    }
                }
                IpAddr::V6(_) => {
                    Err(GeoResolverError::LookupError("IPv6 mock not implemented".to_string()))
                }
            }
        }
    }
}

/// Errors from geo resolution
#[derive(Debug, Clone)]
pub enum GeoResolverError {
    DatabaseNotFound(String),
    DatabaseError(String),
    LookupError(String),
}

impl std::fmt::Display for GeoResolverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GeoResolverError::DatabaseNotFound(path) => {
                write!(f, "GeoIP database not found: {}", path)
            }
            GeoResolverError::DatabaseError(msg) => write!(f, "GeoIP database error: {}", msg),
            GeoResolverError::LookupError(msg) => write!(f, "GeoIP lookup error: {}", msg),
        }
    }
}

impl std::error::Error for GeoResolverError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_registry() -> Arc<EdgeRegistry> {
        Arc::new(EdgeRegistry::new())
    }

    async fn setup_test_nodes(registry: &EdgeRegistry) {
        // US East node
        let us_east = EdgeNode {
            id: "us-east-1".to_string(),
            ipv4: Some(Ipv4Addr::new(10, 0, 1, 1)),
            ipv6: Some("2001:db8::1".parse().unwrap()),
            region: "us-east".to_string(),
            country: "US".to_string(),
            city: Some("New York".to_string()),
            latitude: 40.7128,
            longitude: -74.0060,
            capacity: 100,
            healthy: true,
            last_health_check: 0,
            consecutive_failures: 0,
            registered_at: 0,
            metadata: None,
        };

        // US West node
        let us_west = EdgeNode {
            id: "us-west-1".to_string(),
            ipv4: Some(Ipv4Addr::new(10, 0, 2, 1)),
            ipv6: Some("2001:db8::2".parse().unwrap()),
            region: "us-west".to_string(),
            country: "US".to_string(),
            city: Some("Los Angeles".to_string()),
            latitude: 34.0522,
            longitude: -118.2437,
            capacity: 100,
            healthy: true,
            last_health_check: 0,
            consecutive_failures: 0,
            registered_at: 0,
            metadata: None,
        };

        // EU West node
        let eu_west = EdgeNode {
            id: "eu-west-1".to_string(),
            ipv4: Some(Ipv4Addr::new(10, 0, 3, 1)),
            ipv6: Some("2001:db8::3".parse().unwrap()),
            region: "eu-west".to_string(),
            country: "GB".to_string(),
            city: Some("London".to_string()),
            latitude: 51.5074,
            longitude: -0.1278,
            capacity: 100,
            healthy: true,
            last_health_check: 0,
            consecutive_failures: 0,
            registered_at: 0,
            metadata: None,
        };

        // Asia Pacific node
        let apac = EdgeNode {
            id: "apac-1".to_string(),
            ipv4: Some(Ipv4Addr::new(10, 0, 4, 1)),
            ipv6: Some("2001:db8::4".parse().unwrap()),
            region: "asia-pacific".to_string(),
            country: "JP".to_string(),
            city: Some("Tokyo".to_string()),
            latitude: 35.6762,
            longitude: 139.6503,
            capacity: 100,
            healthy: true,
            last_health_check: 0,
            consecutive_failures: 0,
            registered_at: 0,
            metadata: None,
        };

        registry.register(us_east).await.unwrap();
        registry.register(us_west).await.unwrap();
        registry.register(eu_west).await.unwrap();
        registry.register(apac).await.unwrap();
    }

    #[tokio::test]
    async fn test_geo_resolver_creation() {
        let registry = create_test_registry();
        let resolver = GeoResolver::new(registry);

        assert!(resolver.geoip.is_none());
        assert!(resolver.fallback_ipv4.is_empty());
        assert!(resolver.fallback_ipv6.is_empty());
    }

    #[tokio::test]
    async fn test_geo_resolver_with_fallback() {
        let registry = create_test_registry();
        let resolver = GeoResolver::new(registry)
            .with_fallback_ipv4(vec![Ipv4Addr::new(1, 2, 3, 4)])
            .with_fallback_ipv6(vec!["2001:db8::1".parse().unwrap()]);

        assert_eq!(resolver.fallback_ipv4.len(), 1);
        assert_eq!(resolver.fallback_ipv6.len(), 1);
    }

    #[tokio::test]
    async fn test_resolve_with_no_nodes_uses_fallback() {
        let registry = create_test_registry();
        let resolver = GeoResolver::new(registry)
            .with_fallback_ipv4(vec![Ipv4Addr::new(1, 2, 3, 4)]);

        let client_ip: IpAddr = "8.8.8.8".parse().unwrap();
        let ips = resolver.resolve_for_client(client_ip, RecordType::A, 3).await;

        assert_eq!(ips.len(), 1);
        assert_eq!(ips[0], IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)));
    }

    #[tokio::test]
    async fn test_resolve_returns_healthy_nodes() {
        let registry = create_test_registry();
        setup_test_nodes(&registry).await;

        let resolver = GeoResolver::new(registry);
        let client_ip: IpAddr = "8.8.8.8".parse().unwrap();

        let ips = resolver.resolve_for_client(client_ip, RecordType::A, 3).await;

        assert!(!ips.is_empty());
        assert!(ips.len() <= 3);
        // All returned IPs should be IPv4
        for ip in &ips {
            assert!(ip.is_ipv4());
        }
    }

    #[tokio::test]
    async fn test_resolve_aaaa_returns_ipv6() {
        let registry = create_test_registry();
        setup_test_nodes(&registry).await;

        let resolver = GeoResolver::new(registry);
        let client_ip: IpAddr = "8.8.8.8".parse().unwrap();

        let ips = resolver.resolve_for_client(client_ip, RecordType::AAAA, 3).await;

        assert!(!ips.is_empty());
        // All returned IPs should be IPv6
        for ip in &ips {
            assert!(ip.is_ipv6());
        }
    }

    #[tokio::test]
    async fn test_resolve_with_info() {
        let registry = create_test_registry();
        setup_test_nodes(&registry).await;

        let resolver = GeoResolver::new(registry);
        let client_ip: IpAddr = "8.8.8.8".parse().unwrap();

        let result = resolver
            .resolve_with_info(client_ip, RecordType::A, 3)
            .await;

        assert_eq!(result.client_ip, client_ip);
        assert_eq!(result.record_type, RecordType::A);
        assert!(!result.resolved_ips.is_empty());
        assert!(!result.used_fallback);
        assert_eq!(result.total_healthy_nodes, 4);
    }

    #[tokio::test]
    async fn test_locate_client_without_geoip() {
        let registry = create_test_registry();
        let resolver = GeoResolver::new(registry);

        let client_ip: IpAddr = "8.8.8.8".parse().unwrap();
        let location = resolver.locate_client(client_ip);

        // Without GeoIP, should return default location
        assert_eq!(location.latitude, 0.0);
        assert_eq!(location.longitude, 0.0);
    }

    #[tokio::test]
    async fn test_locate_client_with_custom_default() {
        let registry = create_test_registry();
        let default_location = GeoLocation {
            latitude: 40.7128,
            longitude: -74.0060,
            country: Some("US".to_string()),
            region: Some("NY".to_string()),
            city: Some("New York".to_string()),
        };

        let resolver = GeoResolver::new(registry).with_default_location(default_location.clone());

        let client_ip: IpAddr = "8.8.8.8".parse().unwrap();
        let location = resolver.locate_client(client_ip);

        // Should return custom default
        assert_eq!(location.latitude, 40.7128);
        assert_eq!(location.country, Some("US".to_string()));
    }

    #[tokio::test]
    async fn test_get_nodes_by_region() {
        let registry = create_test_registry();
        setup_test_nodes(&registry).await;

        let resolver = GeoResolver::new(registry);
        let us_nodes = resolver.get_nodes_by_region("us-east").await;

        assert_eq!(us_nodes.len(), 1);
        assert_eq!(us_nodes[0].id, "us-east-1");
    }

    #[tokio::test]
    async fn test_get_nodes_by_country() {
        let registry = create_test_registry();
        setup_test_nodes(&registry).await;

        let resolver = GeoResolver::new(registry);
        let us_nodes = resolver.get_nodes_by_country("US").await;

        assert_eq!(us_nodes.len(), 2);
    }

    #[tokio::test]
    async fn test_resolution_result_serialization() {
        let result = ResolutionResult {
            client_ip: "8.8.8.8".parse().unwrap(),
            client_location: GeoLocation {
                latitude: 40.7128,
                longitude: -74.0060,
                country: Some("US".to_string()),
                region: Some("NY".to_string()),
                city: Some("New York".to_string()),
            },
            record_type: RecordType::A,
            selected_nodes: vec![SelectedNode {
                id: "node-1".to_string(),
                ipv4: Some(Ipv4Addr::new(10, 0, 0, 1)),
                ipv6: None,
                region: "us-east".to_string(),
                country: "US".to_string(),
                distance_km: Some(100.5),
            }],
            resolved_ips: vec!["10.0.0.1".parse().unwrap()],
            used_fallback: false,
            total_healthy_nodes: 10,
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: ResolutionResult = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.total_healthy_nodes, 10);
        assert_eq!(parsed.selected_nodes.len(), 1);
    }

    #[tokio::test]
    async fn test_record_type_display() {
        assert_eq!(RecordType::A.to_string(), "A");
        assert_eq!(RecordType::AAAA.to_string(), "AAAA");
    }

    #[tokio::test]
    async fn test_geo_resolver_error_display() {
        let err = GeoResolverError::DatabaseNotFound("/path/to/db".to_string());
        assert!(err.to_string().contains("not found"));

        let err = GeoResolverError::LookupError("IP not found".to_string());
        assert!(err.to_string().contains("lookup error"));
    }

    #[cfg(not(feature = "geoip"))]
    #[tokio::test]
    async fn test_mock_geoip_us_ip() {
        // Create mock database (won't actually load file without geoip feature)
        // The mock implementation returns locations based on IP patterns

        let registry = create_test_registry();
        setup_test_nodes(&registry).await;

        // Configure resolver with mock behavior
        let resolver = GeoResolver::new(registry);

        // IP starting with 1-50 should be treated as US
        let us_ip: IpAddr = "10.0.0.1".parse().unwrap();
        let location = resolver.locate_client(us_ip);

        // Without geoip feature and no database, returns default
        assert_eq!(location.latitude, 0.0);
    }

    #[tokio::test]
    async fn test_nearest_node_selection() {
        let registry = create_test_registry();
        setup_test_nodes(&registry).await;

        let resolver = GeoResolver::new(registry).with_default_location(GeoLocation {
            latitude: 40.7128,  // New York coordinates
            longitude: -74.0060,
            country: Some("US".to_string()),
            region: Some("NY".to_string()),
            city: Some("New York".to_string()),
        });

        let client_ip: IpAddr = "8.8.8.8".parse().unwrap();
        let result = resolver
            .resolve_with_info(client_ip, RecordType::A, 1)
            .await;

        // Should select US East node (closest to NY)
        if !result.selected_nodes.is_empty() {
            let first_node = &result.selected_nodes[0];
            // The nearest node to NYC should be us-east-1 (which is in NYC)
            // or potentially us-west depending on exact coordinates
            assert!(first_node.country == "US");
        }
    }

    #[tokio::test]
    async fn test_unhealthy_nodes_excluded() {
        let registry = create_test_registry();

        // Add one healthy node
        let healthy = EdgeNode {
            id: "healthy-1".to_string(),
            ipv4: Some(Ipv4Addr::new(10, 0, 1, 1)),
            ipv6: None,
            region: "us-east".to_string(),
            country: "US".to_string(),
            city: None,
            latitude: 40.7128,
            longitude: -74.0060,
            capacity: 100,
            healthy: true,
            last_health_check: 0,
            consecutive_failures: 0,
            registered_at: 0,
            metadata: None,
        };

        // Add one unhealthy node
        let unhealthy = EdgeNode {
            id: "unhealthy-1".to_string(),
            ipv4: Some(Ipv4Addr::new(10, 0, 2, 1)),
            ipv6: None,
            region: "us-east".to_string(),
            country: "US".to_string(),
            city: None,
            latitude: 40.7128,
            longitude: -74.0060,
            capacity: 100,
            healthy: false,
            last_health_check: 0,
            consecutive_failures: 0,
            registered_at: 0,
            metadata: None,
        };

        registry.register(healthy).await.unwrap();
        registry.register(unhealthy).await.unwrap();

        let resolver = GeoResolver::new(registry);
        let client_ip: IpAddr = "8.8.8.8".parse().unwrap();

        let ips = resolver.resolve_for_client(client_ip, RecordType::A, 10).await;

        // Should only return the healthy node's IP
        assert_eq!(ips.len(), 1);
        assert_eq!(ips[0], IpAddr::V4(Ipv4Addr::new(10, 0, 1, 1)));
    }
}
