//! Edge Node Registry
//!
//! Manages the registry of AEGIS edge nodes with their geographic locations,
//! health status, and capacity information. Enables geo-aware DNS resolution
//! by finding the nearest healthy edge nodes to serve client requests.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

/// An edge node in the AEGIS network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeNode {
    /// Unique identifier for this node
    pub id: String,
    /// Node's public IPv4 address (for A records)
    pub ipv4: Option<Ipv4Addr>,
    /// Node's public IPv6 address (for AAAA records)
    pub ipv6: Option<Ipv6Addr>,
    /// Geographic region (e.g., "us-east", "eu-west", "asia-pacific")
    pub region: String,
    /// ISO 3166-1 alpha-2 country code
    pub country: String,
    /// City name (optional)
    pub city: Option<String>,
    /// Latitude in decimal degrees
    pub latitude: f64,
    /// Longitude in decimal degrees
    pub longitude: f64,
    /// Relative capacity weight (higher = more traffic)
    pub capacity: u32,
    /// Current health status
    pub healthy: bool,
    /// Timestamp of last health check (Unix seconds)
    pub last_health_check: u64,
    /// Number of consecutive health check failures
    pub consecutive_failures: u32,
    /// Node registration timestamp
    pub registered_at: u64,
    /// Optional metadata
    pub metadata: Option<HashMap<String, String>>,
}

impl EdgeNode {
    /// Create a new edge node
    pub fn new(
        id: impl Into<String>,
        region: impl Into<String>,
        country: impl Into<String>,
        latitude: f64,
        longitude: f64,
    ) -> Self {
        let now = current_timestamp();
        Self {
            id: id.into(),
            ipv4: None,
            ipv6: None,
            region: region.into(),
            country: country.into(),
            city: None,
            latitude,
            longitude,
            capacity: 100,
            healthy: true,
            last_health_check: now,
            consecutive_failures: 0,
            registered_at: now,
            metadata: None,
        }
    }

    /// Set IPv4 address
    pub fn with_ipv4(mut self, ip: Ipv4Addr) -> Self {
        self.ipv4 = Some(ip);
        self
    }

    /// Set IPv6 address
    pub fn with_ipv6(mut self, ip: Ipv6Addr) -> Self {
        self.ipv6 = Some(ip);
        self
    }

    /// Set city
    pub fn with_city(mut self, city: impl Into<String>) -> Self {
        self.city = Some(city.into());
        self
    }

    /// Set capacity weight
    pub fn with_capacity(mut self, capacity: u32) -> Self {
        self.capacity = capacity;
        self
    }

    /// Get the node's primary IP address
    pub fn primary_ip(&self) -> Option<IpAddr> {
        self.ipv4
            .map(IpAddr::V4)
            .or_else(|| self.ipv6.map(IpAddr::V6))
    }
}

/// Geographic location
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeoLocation {
    /// Latitude in decimal degrees
    pub latitude: f64,
    /// Longitude in decimal degrees
    pub longitude: f64,
    /// ISO country code
    pub country: Option<String>,
    /// Region/state/province
    pub region: Option<String>,
    /// City name
    pub city: Option<String>,
}

impl GeoLocation {
    /// Create a new location
    pub fn new(latitude: f64, longitude: f64) -> Self {
        Self {
            latitude,
            longitude,
            country: None,
            region: None,
            city: None,
        }
    }

    /// Create with country
    pub fn with_country(mut self, country: impl Into<String>) -> Self {
        self.country = Some(country.into());
        self
    }
}

/// Registry of edge nodes
pub struct EdgeRegistry {
    /// Map of node ID -> EdgeNode
    nodes: Arc<RwLock<HashMap<String, EdgeNode>>>,
    /// Index by region for fast lookup
    region_index: Arc<RwLock<HashMap<String, Vec<String>>>>,
    /// Index by country for fast lookup
    country_index: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

impl EdgeRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            nodes: Arc::new(RwLock::new(HashMap::new())),
            region_index: Arc::new(RwLock::new(HashMap::new())),
            country_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new edge node
    pub async fn register(&self, node: EdgeNode) -> Result<(), String> {
        let id = node.id.clone();
        let region = node.region.clone();
        let country = node.country.clone();

        // Add to main registry
        let mut nodes = self.nodes.write().await;
        if nodes.contains_key(&id) {
            return Err(format!("Node {} already registered", id));
        }
        nodes.insert(id.clone(), node);

        // Update region index
        let mut region_index = self.region_index.write().await;
        region_index.entry(region).or_default().push(id.clone());

        // Update country index
        let mut country_index = self.country_index.write().await;
        country_index.entry(country).or_default().push(id);

        Ok(())
    }

    /// Update an existing node
    pub async fn update(&self, node: EdgeNode) -> Result<(), String> {
        let mut nodes = self.nodes.write().await;
        if !nodes.contains_key(&node.id) {
            return Err(format!("Node {} not found", node.id));
        }
        nodes.insert(node.id.clone(), node);
        Ok(())
    }

    /// Unregister a node
    pub async fn unregister(&self, node_id: &str) -> bool {
        let mut nodes = self.nodes.write().await;
        let node = match nodes.remove(node_id) {
            Some(n) => n,
            None => return false,
        };

        // Remove from region index
        let mut region_index = self.region_index.write().await;
        if let Some(ids) = region_index.get_mut(&node.region) {
            ids.retain(|id| id != node_id);
        }

        // Remove from country index
        let mut country_index = self.country_index.write().await;
        if let Some(ids) = country_index.get_mut(&node.country) {
            ids.retain(|id| id != node_id);
        }

        true
    }

    /// Get a node by ID
    pub async fn get(&self, node_id: &str) -> Option<EdgeNode> {
        let nodes = self.nodes.read().await;
        nodes.get(node_id).cloned()
    }

    /// Update node health status
    pub async fn update_health(&self, node_id: &str, healthy: bool) -> Result<(), String> {
        let mut nodes = self.nodes.write().await;
        let node = nodes
            .get_mut(node_id)
            .ok_or_else(|| format!("Node {} not found", node_id))?;

        node.healthy = healthy;
        node.last_health_check = current_timestamp();

        if healthy {
            node.consecutive_failures = 0;
        } else {
            node.consecutive_failures += 1;
        }

        Ok(())
    }

    /// Get all healthy nodes
    pub async fn get_all_healthy(&self) -> Vec<EdgeNode> {
        let nodes = self.nodes.read().await;
        nodes.values().filter(|n| n.healthy).cloned().collect()
    }

    /// Get all nodes (including unhealthy)
    pub async fn get_all(&self) -> Vec<EdgeNode> {
        let nodes = self.nodes.read().await;
        nodes.values().cloned().collect()
    }

    /// Get healthy nodes in a specific region
    pub async fn get_healthy_in_region(&self, region: &str) -> Vec<EdgeNode> {
        let region_index = self.region_index.read().await;
        let node_ids = match region_index.get(region) {
            Some(ids) => ids.clone(),
            None => return vec![],
        };
        drop(region_index);

        let nodes = self.nodes.read().await;
        node_ids
            .iter()
            .filter_map(|id| nodes.get(id))
            .filter(|n| n.healthy)
            .cloned()
            .collect()
    }

    /// Get healthy nodes in a specific country
    pub async fn get_healthy_in_country(&self, country: &str) -> Vec<EdgeNode> {
        let country_index = self.country_index.read().await;
        let node_ids = match country_index.get(country) {
            Some(ids) => ids.clone(),
            None => return vec![],
        };
        drop(country_index);

        let nodes = self.nodes.read().await;
        node_ids
            .iter()
            .filter_map(|id| nodes.get(id))
            .filter(|n| n.healthy)
            .cloned()
            .collect()
    }

    /// Find the nearest healthy nodes to a location
    pub async fn find_nearest(&self, location: &GeoLocation, count: usize) -> Vec<EdgeNode> {
        let nodes = self.nodes.read().await;

        let mut healthy_nodes: Vec<(EdgeNode, f64)> = nodes
            .values()
            .filter(|n| n.healthy)
            .map(|n| {
                let distance = haversine_distance(
                    location.latitude,
                    location.longitude,
                    n.latitude,
                    n.longitude,
                );
                (n.clone(), distance)
            })
            .collect();

        // Sort by distance
        healthy_nodes.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take the nearest `count` nodes
        healthy_nodes.into_iter().take(count).map(|(n, _)| n).collect()
    }

    /// Find nearest nodes with weighted selection based on capacity
    pub async fn find_nearest_weighted(
        &self,
        location: &GeoLocation,
        count: usize,
    ) -> Vec<EdgeNode> {
        let nodes = self.nodes.read().await;

        let mut healthy_nodes: Vec<(EdgeNode, f64)> = nodes
            .values()
            .filter(|n| n.healthy)
            .map(|n| {
                let distance = haversine_distance(
                    location.latitude,
                    location.longitude,
                    n.latitude,
                    n.longitude,
                );
                // Apply capacity weight (lower distance score for higher capacity)
                let weighted_distance = distance / (n.capacity as f64 / 100.0).max(0.1);
                (n.clone(), weighted_distance)
            })
            .collect();

        healthy_nodes.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        healthy_nodes.into_iter().take(count).map(|(n, _)| n).collect()
    }

    /// Get node count
    pub async fn node_count(&self) -> usize {
        let nodes = self.nodes.read().await;
        nodes.len()
    }

    /// Get healthy node count
    pub async fn healthy_count(&self) -> usize {
        let nodes = self.nodes.read().await;
        nodes.values().filter(|n| n.healthy).count()
    }

    /// Get list of all regions with nodes
    pub async fn list_regions(&self) -> Vec<String> {
        let region_index = self.region_index.read().await;
        region_index.keys().cloned().collect()
    }

    /// Get list of all countries with nodes
    pub async fn list_countries(&self) -> Vec<String> {
        let country_index = self.country_index.read().await;
        country_index.keys().cloned().collect()
    }

    /// Alias for get() - get a node by ID
    pub async fn get_node(&self, node_id: &str) -> Option<EdgeNode> {
        self.get(node_id).await
    }

    /// Alias for get_all() - get all nodes
    pub async fn get_all_nodes(&self) -> Vec<EdgeNode> {
        self.get_all().await
    }

    /// Alias for list_regions() - get all regions
    pub async fn get_all_regions(&self) -> Vec<String> {
        self.list_regions().await
    }

    /// Mark all nodes in a region as unhealthy (for maintenance)
    pub async fn mark_region_unhealthy(&self, region: &str) {
        let region_index = self.region_index.read().await;
        let node_ids = match region_index.get(region) {
            Some(ids) => ids.clone(),
            None => return,
        };
        drop(region_index);

        let mut nodes = self.nodes.write().await;
        for id in node_ids {
            if let Some(node) = nodes.get_mut(&id) {
                node.healthy = false;
            }
        }
    }
}

impl Default for EdgeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Calculate distance between two points using Haversine formula
/// Returns distance in kilometers
pub fn haversine_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const EARTH_RADIUS_KM: f64 = 6371.0;

    let lat1_rad = lat1.to_radians();
    let lat2_rad = lat2.to_radians();
    let delta_lat = (lat2 - lat1).to_radians();
    let delta_lon = (lon2 - lon1).to_radians();

    let a = (delta_lat / 2.0).sin().powi(2)
        + lat1_rad.cos() * lat2_rad.cos() * (delta_lon / 2.0).sin().powi(2);

    let c = 2.0 * a.sqrt().asin();

    EARTH_RADIUS_KM * c
}

/// Get current Unix timestamp
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_node(id: &str, region: &str, country: &str, lat: f64, lon: f64) -> EdgeNode {
        EdgeNode::new(id, region, country, lat, lon)
            .with_ipv4("192.168.1.1".parse().unwrap())
    }

    #[test]
    fn test_haversine_distance() {
        // New York to London (approx 5570 km)
        let ny_lat = 40.7128;
        let ny_lon = -74.0060;
        let london_lat = 51.5074;
        let london_lon = -0.1278;

        let distance = haversine_distance(ny_lat, ny_lon, london_lat, london_lon);
        assert!(distance > 5500.0 && distance < 5700.0);

        // Same point should be 0
        let same = haversine_distance(ny_lat, ny_lon, ny_lat, ny_lon);
        assert!(same < 0.001);
    }

    #[test]
    fn test_edge_node_creation() {
        let node = EdgeNode::new("node-1", "us-east", "US", 40.7128, -74.0060)
            .with_ipv4("192.168.1.1".parse().unwrap())
            .with_ipv6("2001:db8::1".parse().unwrap())
            .with_city("New York")
            .with_capacity(150);

        assert_eq!(node.id, "node-1");
        assert_eq!(node.region, "us-east");
        assert_eq!(node.country, "US");
        assert!(node.ipv4.is_some());
        assert!(node.ipv6.is_some());
        assert_eq!(node.city, Some("New York".to_string()));
        assert_eq!(node.capacity, 150);
        assert!(node.healthy);
    }

    #[test]
    fn test_edge_node_primary_ip() {
        let node_v4 = EdgeNode::new("n1", "us-east", "US", 0.0, 0.0)
            .with_ipv4("192.168.1.1".parse().unwrap());
        assert!(matches!(node_v4.primary_ip(), Some(IpAddr::V4(_))));

        let node_v6 = EdgeNode::new("n2", "us-east", "US", 0.0, 0.0)
            .with_ipv6("2001:db8::1".parse().unwrap());
        assert!(matches!(node_v6.primary_ip(), Some(IpAddr::V6(_))));

        let node_none = EdgeNode::new("n3", "us-east", "US", 0.0, 0.0);
        assert!(node_none.primary_ip().is_none());
    }

    #[tokio::test]
    async fn test_registry_register() {
        let registry = EdgeRegistry::new();

        let node = create_test_node("node-1", "us-east", "US", 40.7128, -74.0060);
        registry.register(node).await.unwrap();

        assert_eq!(registry.node_count().await, 1);
        assert!(registry.get("node-1").await.is_some());
    }

    #[tokio::test]
    async fn test_registry_register_duplicate() {
        let registry = EdgeRegistry::new();

        let node1 = create_test_node("node-1", "us-east", "US", 40.7128, -74.0060);
        let node2 = create_test_node("node-1", "eu-west", "UK", 51.5074, -0.1278);

        registry.register(node1).await.unwrap();
        let result = registry.register(node2).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_registry_unregister() {
        let registry = EdgeRegistry::new();

        let node = create_test_node("node-1", "us-east", "US", 40.7128, -74.0060);
        registry.register(node).await.unwrap();

        assert!(registry.unregister("node-1").await);
        assert_eq!(registry.node_count().await, 0);
        assert!(registry.get("node-1").await.is_none());

        // Unregister non-existent
        assert!(!registry.unregister("node-1").await);
    }

    #[tokio::test]
    async fn test_registry_health_update() {
        let registry = EdgeRegistry::new();

        let node = create_test_node("node-1", "us-east", "US", 40.7128, -74.0060);
        registry.register(node).await.unwrap();

        // Initially healthy
        assert_eq!(registry.healthy_count().await, 1);

        // Mark unhealthy
        registry.update_health("node-1", false).await.unwrap();
        assert_eq!(registry.healthy_count().await, 0);

        let node = registry.get("node-1").await.unwrap();
        assert!(!node.healthy);
        assert_eq!(node.consecutive_failures, 1);

        // Mark healthy again
        registry.update_health("node-1", true).await.unwrap();
        let node = registry.get("node-1").await.unwrap();
        assert!(node.healthy);
        assert_eq!(node.consecutive_failures, 0);
    }

    #[tokio::test]
    async fn test_registry_get_healthy() {
        let registry = EdgeRegistry::new();

        let node1 = create_test_node("node-1", "us-east", "US", 40.7128, -74.0060);
        let mut node2 = create_test_node("node-2", "us-east", "US", 34.0522, -118.2437);
        node2.healthy = false;

        registry.register(node1).await.unwrap();
        registry.register(node2).await.unwrap();

        let healthy = registry.get_all_healthy().await;
        assert_eq!(healthy.len(), 1);
        assert_eq!(healthy[0].id, "node-1");

        let all = registry.get_all().await;
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_registry_get_by_region() {
        let registry = EdgeRegistry::new();

        let node1 = create_test_node("node-1", "us-east", "US", 40.7128, -74.0060);
        let node2 = create_test_node("node-2", "us-west", "US", 34.0522, -118.2437);
        let node3 = create_test_node("node-3", "us-east", "US", 42.3601, -71.0589);

        registry.register(node1).await.unwrap();
        registry.register(node2).await.unwrap();
        registry.register(node3).await.unwrap();

        let us_east = registry.get_healthy_in_region("us-east").await;
        assert_eq!(us_east.len(), 2);

        let us_west = registry.get_healthy_in_region("us-west").await;
        assert_eq!(us_west.len(), 1);

        let eu = registry.get_healthy_in_region("eu-west").await;
        assert!(eu.is_empty());
    }

    #[tokio::test]
    async fn test_registry_get_by_country() {
        let registry = EdgeRegistry::new();

        let node1 = create_test_node("node-1", "us-east", "US", 40.7128, -74.0060);
        let node2 = create_test_node("node-2", "eu-west", "UK", 51.5074, -0.1278);
        let node3 = create_test_node("node-3", "eu-west", "DE", 52.5200, 13.4050);

        registry.register(node1).await.unwrap();
        registry.register(node2).await.unwrap();
        registry.register(node3).await.unwrap();

        let us = registry.get_healthy_in_country("US").await;
        assert_eq!(us.len(), 1);

        let uk = registry.get_healthy_in_country("UK").await;
        assert_eq!(uk.len(), 1);

        let fr = registry.get_healthy_in_country("FR").await;
        assert!(fr.is_empty());
    }

    #[tokio::test]
    async fn test_registry_find_nearest() {
        let registry = EdgeRegistry::new();

        // New York
        let node1 = create_test_node("nyc", "us-east", "US", 40.7128, -74.0060);
        // Los Angeles
        let node2 = create_test_node("la", "us-west", "US", 34.0522, -118.2437);
        // London
        let node3 = create_test_node("london", "eu-west", "UK", 51.5074, -0.1278);

        registry.register(node1).await.unwrap();
        registry.register(node2).await.unwrap();
        registry.register(node3).await.unwrap();

        // Query from Boston - NYC should be closest
        let boston = GeoLocation::new(42.3601, -71.0589);
        let nearest = registry.find_nearest(&boston, 2).await;
        assert_eq!(nearest.len(), 2);
        assert_eq!(nearest[0].id, "nyc");

        // Query from Paris - London should be closest
        let paris = GeoLocation::new(48.8566, 2.3522);
        let nearest = registry.find_nearest(&paris, 1).await;
        assert_eq!(nearest.len(), 1);
        assert_eq!(nearest[0].id, "london");
    }

    #[tokio::test]
    async fn test_registry_find_nearest_excludes_unhealthy() {
        let registry = EdgeRegistry::new();

        let node1 = create_test_node("nyc", "us-east", "US", 40.7128, -74.0060);
        let node2 = create_test_node("la", "us-west", "US", 34.0522, -118.2437);

        registry.register(node1).await.unwrap();
        registry.register(node2).await.unwrap();

        // Mark NYC unhealthy
        registry.update_health("nyc", false).await.unwrap();

        // Query from Boston - should get LA even though NYC is closer
        let boston = GeoLocation::new(42.3601, -71.0589);
        let nearest = registry.find_nearest(&boston, 2).await;
        assert_eq!(nearest.len(), 1);
        assert_eq!(nearest[0].id, "la");
    }

    #[tokio::test]
    async fn test_registry_list_regions() {
        let registry = EdgeRegistry::new();

        registry.register(create_test_node("n1", "us-east", "US", 0.0, 0.0)).await.unwrap();
        registry.register(create_test_node("n2", "us-west", "US", 0.0, 0.0)).await.unwrap();
        registry.register(create_test_node("n3", "eu-west", "UK", 0.0, 0.0)).await.unwrap();

        let regions = registry.list_regions().await;
        assert_eq!(regions.len(), 3);
        assert!(regions.contains(&"us-east".to_string()));
        assert!(regions.contains(&"us-west".to_string()));
        assert!(regions.contains(&"eu-west".to_string()));
    }

    #[tokio::test]
    async fn test_registry_mark_region_unhealthy() {
        let registry = EdgeRegistry::new();

        registry.register(create_test_node("n1", "us-east", "US", 0.0, 0.0)).await.unwrap();
        registry.register(create_test_node("n2", "us-east", "US", 0.0, 0.0)).await.unwrap();
        registry.register(create_test_node("n3", "us-west", "US", 0.0, 0.0)).await.unwrap();

        assert_eq!(registry.healthy_count().await, 3);

        registry.mark_region_unhealthy("us-east").await;

        assert_eq!(registry.healthy_count().await, 1);
        let us_east = registry.get_healthy_in_region("us-east").await;
        assert!(us_east.is_empty());
    }

    #[test]
    fn test_geo_location() {
        let loc = GeoLocation::new(40.7128, -74.0060)
            .with_country("US");

        assert_eq!(loc.latitude, 40.7128);
        assert_eq!(loc.longitude, -74.0060);
        assert_eq!(loc.country, Some("US".to_string()));
    }
}
