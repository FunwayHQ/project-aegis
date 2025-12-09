//! DNS Management HTTP API
//!
//! Provides REST API endpoints for DNS zone and record management:
//! - Zone CRUD operations
//! - Record CRUD operations
//! - Usage analytics and metering
//! - Account management and tier info
//!
//! Base path: /aegis/dns/api

use hyper::{Body, Method, Request, Response, Server, StatusCode};
use hyper::service::{make_service_fn, service_fn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use super::{
    DnsConfig, DnsError, DnsRecord, DnsRecordType, DnsRecordValue, Zone, ZoneStore,
};
use super::edge_registry::{EdgeNode, EdgeRegistry, GeoLocation};
use super::health_checker::{HealthChecker, HealthCheckResult, HealthSummary};
use super::geo_resolver::{GeoResolver, RecordType as GeoRecordType, ResolutionResult};
use super::dnssec_keys::{DnssecKeyManager, DnssecAlgorithm, KeyFlags, DigestType, DsRecord as DnssecDsRecord};
use super::dnssec::{DnssecSigner, DnssecConfig, DnssecResigner, SignedZone};

// =============================================================================
// API RESPONSE
// =============================================================================

/// Standard API response format
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            message: "Success".to_string(),
            data: Some(data),
        }
    }

    pub fn success_message(message: &str) -> Self {
        Self {
            success: true,
            message: message.to_string(),
            data: None,
        }
    }
}

impl ApiResponse<()> {
    pub fn error(message: &str) -> Self {
        Self {
            success: false,
            message: message.to_string(),
            data: None,
        }
    }
}

// =============================================================================
// REQUEST/RESPONSE TYPES
// =============================================================================

/// Request to create a new zone
#[derive(Debug, Deserialize)]
pub struct CreateZoneRequest {
    pub domain: String,
    #[serde(default)]
    pub proxied: bool,
}

/// Zone response with additional info
#[derive(Debug, Serialize)]
pub struct ZoneResponse {
    pub domain: String,
    pub proxied: bool,
    pub dnssec_enabled: bool,
    pub nameservers: Vec<String>,
    pub record_count: usize,
    pub created_at: u64,
    pub updated_at: u64,
}

impl ZoneResponse {
    pub fn from_zone(zone: &Zone, nameservers: &[String]) -> Self {
        Self {
            domain: zone.domain.clone(),
            proxied: zone.proxied,
            dnssec_enabled: zone.dnssec_enabled,
            nameservers: nameservers.to_vec(),
            record_count: zone.records.len(),
            created_at: zone.created_at,
            updated_at: zone.updated_at,
        }
    }
}

/// Request to update zone settings
#[derive(Debug, Deserialize)]
pub struct UpdateZoneRequest {
    pub proxied: Option<bool>,
    pub dnssec_enabled: Option<bool>,
}

/// Request to create a new record
#[derive(Debug, Deserialize)]
pub struct CreateRecordRequest {
    pub name: String,
    #[serde(rename = "type")]
    pub record_type: String,
    pub value: String,
    #[serde(default = "default_ttl")]
    pub ttl: u32,
    pub priority: Option<u16>,
    #[serde(default)]
    pub proxied: bool,
}

fn default_ttl() -> u32 {
    300
}

/// Record response
#[derive(Debug, Serialize)]
pub struct RecordResponse {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub record_type: String,
    pub value: String,
    pub ttl: u32,
    pub priority: Option<u16>,
    pub proxied: bool,
}

impl RecordResponse {
    pub fn from_record(record: &DnsRecord) -> Self {
        Self {
            id: record.id.clone(),
            name: record.name.clone(),
            record_type: record.record_type.to_string(),
            value: record.value.to_display_string(),
            ttl: record.ttl,
            priority: record.priority,
            proxied: record.proxied,
        }
    }
}

/// Account information response
#[derive(Debug, Serialize)]
pub struct AccountResponse {
    pub zone_count: usize,
    pub max_zones: usize,
    pub tier: String,
    pub features: AccountFeatures,
}

#[derive(Debug, Serialize)]
pub struct AccountFeatures {
    pub dnssec: bool,
    pub advanced_analytics: bool,
    pub custom_nameservers: bool,
    pub priority_support: bool,
}

/// Usage statistics response
#[derive(Debug, Serialize)]
pub struct UsageResponse {
    pub total_zones: usize,
    pub total_records: usize,
    pub queries_today: u64,
    pub queries_month: u64,
}

/// Zone statistics response
#[derive(Debug, Serialize)]
pub struct ZoneStatsResponse {
    pub domain: String,
    pub total_queries: u64,
    pub queries_by_type: HashMap<String, u64>,
    pub cache_hit_ratio: f64,
}

// =============================================================================
// EDGE NODE REQUEST/RESPONSE TYPES
// =============================================================================

/// Request to register an edge node
#[derive(Debug, Deserialize)]
pub struct RegisterEdgeRequest {
    pub id: String,
    pub ipv4: Option<String>,
    pub ipv6: Option<String>,
    pub region: String,
    pub country: String,
    pub city: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
    #[serde(default = "default_capacity")]
    pub capacity: u32,
}

fn default_capacity() -> u32 {
    100
}

/// Edge node response
#[derive(Debug, Serialize)]
pub struct EdgeNodeResponse {
    pub id: String,
    pub ipv4: Option<String>,
    pub ipv6: Option<String>,
    pub region: String,
    pub country: String,
    pub city: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
    pub capacity: u32,
    pub healthy: bool,
    pub last_health_check: u64,
}

impl EdgeNodeResponse {
    pub fn from_node(node: &EdgeNode) -> Self {
        Self {
            id: node.id.clone(),
            ipv4: node.ipv4.map(|ip| ip.to_string()),
            ipv6: node.ipv6.map(|ip| ip.to_string()),
            region: node.region.clone(),
            country: node.country.clone(),
            city: node.city.clone(),
            latitude: node.latitude,
            longitude: node.longitude,
            capacity: node.capacity,
            healthy: node.healthy,
            last_health_check: node.last_health_check,
        }
    }
}

/// Request to test geo resolution
#[derive(Debug, Deserialize)]
pub struct GeoResolveRequest {
    pub client_ip: String,
    #[serde(default = "default_record_type")]
    pub record_type: String,
    #[serde(default = "default_count")]
    pub count: usize,
}

fn default_record_type() -> String {
    "A".to_string()
}

fn default_count() -> usize {
    3
}

// =============================================================================
// DNSSEC REQUEST/RESPONSE TYPES
// =============================================================================

/// DNSSEC status response
#[derive(Debug, Serialize)]
pub struct DnssecStatusResponse {
    pub domain: String,
    pub enabled: bool,
    pub algorithm: Option<String>,
    pub key_tag_zsk: Option<u16>,
    pub key_tag_ksk: Option<u16>,
    pub signed_at: Option<u64>,
    pub expires_at: Option<u64>,
}

/// DS record response for registrar
#[derive(Debug, Serialize)]
pub struct DsRecordResponse {
    pub domain: String,
    pub key_tag: u16,
    pub algorithm: u8,
    pub algorithm_name: String,
    pub digest_type: u8,
    pub digest: String,
    pub zone_format: String,
    pub registrar_format: String,
}

impl DsRecordResponse {
    pub fn from_ds_record(domain: &str, ds: &DnssecDsRecord) -> Self {
        Self {
            domain: domain.to_string(),
            key_tag: ds.key_tag,
            algorithm: ds.algorithm,
            algorithm_name: DnssecAlgorithm::from_number(ds.algorithm)
                .map(|a| a.name().to_string())
                .unwrap_or_else(|| format!("UNKNOWN({})", ds.algorithm)),
            digest_type: ds.digest_type,
            digest: hex::encode(&ds.digest).to_uppercase(),
            zone_format: ds.to_zone_format(domain),
            registrar_format: ds.to_registrar_format(),
        }
    }
}

/// Request to enable DNSSEC
#[derive(Debug, Deserialize)]
pub struct EnableDnssecRequest {
    #[serde(default = "default_dnssec_algorithm")]
    pub algorithm: String,
}

fn default_dnssec_algorithm() -> String {
    "ED25519".to_string()
}

/// Signed zone info response
#[derive(Debug, Serialize)]
pub struct SignedZoneResponse {
    pub domain: String,
    pub record_count: usize,
    pub rrsig_count: usize,
    pub nsec_count: usize,
    pub dnskey_count: usize,
    pub signed_at: u64,
    pub expires_at: u64,
    pub key_tags: Vec<u16>,
}

impl SignedZoneResponse {
    pub fn from_signed_zone(zone: &SignedZone) -> Self {
        Self {
            domain: zone.domain.clone(),
            record_count: zone.records.len(),
            rrsig_count: zone.get_rrsigs().len(),
            nsec_count: zone.get_nsec_records().len(),
            dnskey_count: zone.get_dnskey_records().len(),
            signed_at: zone.signed_at,
            expires_at: zone.expires_at,
            key_tags: zone.key_tags.clone(),
        }
    }
}

// =============================================================================
// DNS API SERVER
// =============================================================================

/// DNS API server
pub struct DnsApi {
    zone_store: Arc<ZoneStore>,
    config: DnsConfig,
    edge_registry: Option<Arc<EdgeRegistry>>,
    health_checker: Option<Arc<HealthChecker>>,
    geo_resolver: Option<Arc<GeoResolver>>,
    // DNSSEC components (Sprint 30.4)
    dnssec_key_manager: Option<Arc<DnssecKeyManager>>,
    dnssec_resigner: Option<Arc<DnssecResigner>>,
}

impl DnsApi {
    /// Create a new DNS API instance
    pub fn new(zone_store: Arc<ZoneStore>, config: DnsConfig) -> Self {
        Self {
            zone_store,
            config,
            edge_registry: None,
            health_checker: None,
            geo_resolver: None,
            dnssec_key_manager: None,
            dnssec_resigner: None,
        }
    }

    /// Create DNS API with edge components
    pub fn with_edge(
        zone_store: Arc<ZoneStore>,
        config: DnsConfig,
        edge_registry: Arc<EdgeRegistry>,
        health_checker: Arc<HealthChecker>,
        geo_resolver: Arc<GeoResolver>,
    ) -> Self {
        Self {
            zone_store,
            config,
            edge_registry: Some(edge_registry),
            health_checker: Some(health_checker),
            geo_resolver: Some(geo_resolver),
            dnssec_key_manager: None,
            dnssec_resigner: None,
        }
    }

    /// Set edge registry
    pub fn set_edge_registry(&mut self, registry: Arc<EdgeRegistry>) {
        self.edge_registry = Some(registry);
    }

    /// Set health checker
    pub fn set_health_checker(&mut self, checker: Arc<HealthChecker>) {
        self.health_checker = Some(checker);
    }

    /// Set geo resolver
    pub fn set_geo_resolver(&mut self, resolver: Arc<GeoResolver>) {
        self.geo_resolver = Some(resolver);
    }

    /// Set DNSSEC key manager
    pub fn set_dnssec_key_manager(&mut self, manager: Arc<DnssecKeyManager>) {
        self.dnssec_key_manager = Some(manager);
    }

    /// Set DNSSEC resigner
    pub fn set_dnssec_resigner(&mut self, resigner: Arc<DnssecResigner>) {
        self.dnssec_resigner = Some(resigner);
    }

    /// Handle an incoming HTTP request
    pub async fn handle_request(
        &self,
        req: Request<Body>,
    ) -> Result<Response<Body>, Infallible> {
        let path = req.uri().path().to_string();
        let method = req.method().clone();

        debug!("DNS API request: {} {}", method, path);

        let response = match (method, path.as_str()) {
            // Health check
            (Method::GET, "/aegis/dns/api/health") => self.handle_health().await,

            // Zone management
            (Method::GET, "/aegis/dns/api/zones") => self.handle_list_zones().await,
            (Method::POST, "/aegis/dns/api/zones") => self.handle_create_zone(req).await,
            (Method::GET, p) if p.starts_with("/aegis/dns/api/zones/") && !p.contains("/records") && !p.contains("/analytics") => {
                let domain = extract_domain(p, "/aegis/dns/api/zones/");
                self.handle_get_zone(&domain).await
            }
            (Method::PUT, p) if p.starts_with("/aegis/dns/api/zones/") && !p.contains("/records") => {
                let domain = extract_domain(p, "/aegis/dns/api/zones/");
                self.handle_update_zone(&domain, req).await
            }
            (Method::DELETE, p) if p.starts_with("/aegis/dns/api/zones/") && !p.contains("/records") => {
                let domain = extract_domain(p, "/aegis/dns/api/zones/");
                self.handle_delete_zone(&domain).await
            }

            // Record management
            (Method::GET, p) if p.contains("/records") && !p.contains("/records/") => {
                if let Some(domain) = extract_zone_domain(p) {
                    self.handle_list_records(&domain).await
                } else {
                    self.not_found()
                }
            }
            (Method::POST, p) if p.contains("/records") => {
                if let Some(domain) = extract_zone_domain(p) {
                    self.handle_create_record(&domain, req).await
                } else {
                    self.not_found()
                }
            }
            (Method::DELETE, p) if p.contains("/records/") => {
                if let Some((domain, record_id)) = extract_zone_and_record(p) {
                    self.handle_delete_record(&domain, &record_id).await
                } else {
                    self.not_found()
                }
            }

            // Nameservers
            (Method::GET, "/aegis/dns/api/nameservers") => self.handle_get_nameservers().await,

            // Account & Usage
            (Method::GET, "/aegis/dns/api/account") => self.handle_get_account().await,
            (Method::GET, "/aegis/dns/api/account/usage") => self.handle_get_usage().await,
            (Method::GET, "/aegis/dns/api/account/tier") => self.handle_get_tier().await,

            // Statistics
            (Method::GET, "/aegis/dns/api/stats") => self.handle_get_stats().await,
            (Method::GET, p) if p.starts_with("/aegis/dns/api/stats/") => {
                let domain = extract_domain(p, "/aegis/dns/api/stats/");
                self.handle_get_zone_stats(&domain).await
            }

            // Analytics
            (Method::GET, p) if p.contains("/analytics") && !p.contains("/timeseries") => {
                if let Some(domain) = extract_analytics_domain(p) {
                    self.handle_get_analytics(&domain).await
                } else {
                    self.not_found()
                }
            }
            (Method::GET, p) if p.contains("/analytics/timeseries") => {
                if let Some(domain) = extract_analytics_domain(p) {
                    self.handle_get_timeseries(&domain, req).await
                } else {
                    self.not_found()
                }
            }

            // Edge node management
            (Method::GET, "/aegis/dns/api/edges") => self.handle_list_edges().await,
            (Method::POST, "/aegis/dns/api/edges") => self.handle_register_edge(req).await,
            (Method::GET, p) if p.starts_with("/aegis/dns/api/edges/") && !p.contains("/health") => {
                let node_id = extract_domain(p, "/aegis/dns/api/edges/");
                self.handle_get_edge(&node_id).await
            }
            (Method::DELETE, p) if p.starts_with("/aegis/dns/api/edges/") => {
                let node_id = extract_domain(p, "/aegis/dns/api/edges/");
                self.handle_unregister_edge(&node_id).await
            }
            (Method::GET, p) if p.contains("/edges/") && p.ends_with("/health") => {
                let node_id = extract_edge_id_from_health_path(p);
                self.handle_get_edge_health(&node_id).await
            }

            // Edge health summary
            (Method::GET, "/aegis/dns/api/health/edges") => self.handle_get_edges_health_summary().await,

            // Geo resolution testing
            (Method::POST, "/aegis/dns/api/geo/resolve") => self.handle_geo_resolve(req).await,
            (Method::GET, "/aegis/dns/api/geo/regions") => self.handle_list_regions().await,
            (Method::GET, p) if p.starts_with("/aegis/dns/api/geo/regions/") => {
                let region = extract_domain(p, "/aegis/dns/api/geo/regions/");
                self.handle_get_region_nodes(&region).await
            }

            // DNSSEC management (Sprint 30.4)
            (Method::GET, p) if p.contains("/dnssec") && !p.contains("/dnssec/") => {
                if let Some(domain) = extract_dnssec_domain(p) {
                    self.handle_get_dnssec_status(&domain).await
                } else {
                    self.not_found()
                }
            }
            (Method::POST, p) if p.ends_with("/dnssec/enable") => {
                if let Some(domain) = extract_dnssec_domain(p) {
                    self.handle_enable_dnssec(&domain, req).await
                } else {
                    self.not_found()
                }
            }
            (Method::POST, p) if p.ends_with("/dnssec/disable") => {
                if let Some(domain) = extract_dnssec_domain(p) {
                    self.handle_disable_dnssec(&domain).await
                } else {
                    self.not_found()
                }
            }
            (Method::GET, p) if p.ends_with("/dnssec/ds") => {
                if let Some(domain) = extract_dnssec_domain(p) {
                    self.handle_get_ds_record(&domain).await
                } else {
                    self.not_found()
                }
            }
            (Method::POST, p) if p.ends_with("/dnssec/resign") => {
                if let Some(domain) = extract_dnssec_domain(p) {
                    self.handle_resign_zone(&domain).await
                } else {
                    self.not_found()
                }
            }
            (Method::GET, p) if p.ends_with("/dnssec/signed") => {
                if let Some(domain) = extract_dnssec_domain(p) {
                    self.handle_get_signed_zone(&domain).await
                } else {
                    self.not_found()
                }
            }

            _ => self.not_found(),
        };

        Ok(response)
    }

    // =========================================================================
    // HEALTH
    // =========================================================================

    async fn handle_health(&self) -> Response<Body> {
        let zone_count = self.zone_store.zone_count().await;
        let response = serde_json::json!({
            "status": "healthy",
            "service": "aegis-dns",
            "zone_count": zone_count,
        });
        json_response(StatusCode::OK, &response)
    }

    // =========================================================================
    // ZONE HANDLERS
    // =========================================================================

    async fn handle_list_zones(&self) -> Response<Body> {
        let zones = self.zone_store.list_zones().await;
        let nameservers = &self.config.edge.nameservers;

        let responses: Vec<ZoneResponse> = zones
            .iter()
            .map(|z| ZoneResponse::from_zone(z, nameservers))
            .collect();

        json_response(StatusCode::OK, &ApiResponse::success(responses))
    }

    async fn handle_create_zone(&self, req: Request<Body>) -> Response<Body> {
        // Parse request body
        let body = match parse_body::<CreateZoneRequest>(req).await {
            Ok(b) => b,
            Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
        };

        // Validate domain
        if body.domain.is_empty() || !is_valid_domain(&body.domain) {
            return json_error(StatusCode::BAD_REQUEST, "Invalid domain name");
        }

        // Check if zone already exists
        if self.zone_store.zone_exists(&body.domain).await {
            return json_error(StatusCode::CONFLICT, "Zone already exists");
        }

        // Check zone limit (free tier = 5 zones)
        let zone_count = self.zone_store.zone_count().await;
        if zone_count >= 5 {
            return json_error(
                StatusCode::PAYMENT_REQUIRED,
                "Zone limit reached. Stake $AEGIS tokens to create more zones.",
            );
        }

        // Create zone with default records
        let mut zone = Zone::new(&body.domain, body.proxied);
        zone.create_default_records(&self.config.edge.nameservers);

        if let Err(e) = self.zone_store.upsert_zone(zone.clone()).await {
            error!("Failed to create zone: {}", e);
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to create zone");
        }

        info!("Created zone: {}", body.domain);
        let response = ZoneResponse::from_zone(&zone, &self.config.edge.nameservers);
        json_response(StatusCode::CREATED, &ApiResponse::success(response))
    }

    async fn handle_get_zone(&self, domain: &str) -> Response<Body> {
        match self.zone_store.get_zone(domain).await {
            Some(zone) => {
                let response = ZoneResponse::from_zone(&zone, &self.config.edge.nameservers);
                json_response(StatusCode::OK, &ApiResponse::success(response))
            }
            None => json_error(StatusCode::NOT_FOUND, "Zone not found"),
        }
    }

    async fn handle_update_zone(&self, domain: &str, req: Request<Body>) -> Response<Body> {
        let body = match parse_body::<UpdateZoneRequest>(req).await {
            Ok(b) => b,
            Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
        };

        // Check DNSSEC access (paid feature)
        if body.dnssec_enabled == Some(true) {
            return json_error(
                StatusCode::PAYMENT_REQUIRED,
                "DNSSEC requires staking 2,500 $AEGIS tokens",
            );
        }

        match self
            .zone_store
            .update_zone_settings(domain, body.proxied, body.dnssec_enabled)
            .await
        {
            Ok(_) => {
                if let Some(zone) = self.zone_store.get_zone(domain).await {
                    let response = ZoneResponse::from_zone(&zone, &self.config.edge.nameservers);
                    json_response(StatusCode::OK, &ApiResponse::success(response))
                } else {
                    json_error(StatusCode::NOT_FOUND, "Zone not found")
                }
            }
            Err(DnsError::ZoneNotFound(_)) => json_error(StatusCode::NOT_FOUND, "Zone not found"),
            Err(e) => {
                error!("Failed to update zone: {}", e);
                json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to update zone")
            }
        }
    }

    async fn handle_delete_zone(&self, domain: &str) -> Response<Body> {
        if self.zone_store.delete_zone(domain).await {
            info!("Deleted zone: {}", domain);
            json_response(
                StatusCode::OK,
                &ApiResponse::<()>::success_message("Zone deleted"),
            )
        } else {
            json_error(StatusCode::NOT_FOUND, "Zone not found")
        }
    }

    // =========================================================================
    // RECORD HANDLERS
    // =========================================================================

    async fn handle_list_records(&self, domain: &str) -> Response<Body> {
        match self.zone_store.get_records(domain).await {
            Ok(records) => {
                let responses: Vec<RecordResponse> =
                    records.iter().map(RecordResponse::from_record).collect();
                json_response(StatusCode::OK, &ApiResponse::success(responses))
            }
            Err(DnsError::ZoneNotFound(_)) => json_error(StatusCode::NOT_FOUND, "Zone not found"),
            Err(e) => {
                error!("Failed to list records: {}", e);
                json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to list records")
            }
        }
    }

    async fn handle_create_record(&self, domain: &str, req: Request<Body>) -> Response<Body> {
        let body = match parse_body::<CreateRecordRequest>(req).await {
            Ok(b) => b,
            Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
        };

        // Parse record type
        let record_type = match body.record_type.parse::<DnsRecordType>() {
            Ok(rt) => rt,
            Err(_) => return json_error(StatusCode::BAD_REQUEST, "Invalid record type"),
        };

        // Parse record value
        let value = match parse_record_value(&record_type, &body.value) {
            Ok(v) => v,
            Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
        };

        // Create record
        let mut record = DnsRecord::new(&body.name, record_type, body.ttl, value);
        record.priority = body.priority;
        record.proxied = body.proxied;

        match self.zone_store.add_record(domain, record.clone()).await {
            Ok(_) => {
                info!("Created record {} in zone {}", body.name, domain);
                let response = RecordResponse::from_record(&record);
                json_response(StatusCode::CREATED, &ApiResponse::success(response))
            }
            Err(DnsError::ZoneNotFound(_)) => json_error(StatusCode::NOT_FOUND, "Zone not found"),
            Err(e) => {
                error!("Failed to create record: {}", e);
                json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to create record")
            }
        }
    }

    async fn handle_delete_record(&self, domain: &str, record_id: &str) -> Response<Body> {
        match self.zone_store.remove_record(domain, record_id).await {
            Ok(_) => {
                info!("Deleted record {} from zone {}", record_id, domain);
                json_response(
                    StatusCode::OK,
                    &ApiResponse::<()>::success_message("Record deleted"),
                )
            }
            Err(DnsError::ZoneNotFound(_)) => json_error(StatusCode::NOT_FOUND, "Zone not found"),
            Err(DnsError::RecordNotFound(_)) => {
                json_error(StatusCode::NOT_FOUND, "Record not found")
            }
            Err(e) => {
                error!("Failed to delete record: {}", e);
                json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to delete record")
            }
        }
    }

    // =========================================================================
    // NAMESERVERS
    // =========================================================================

    async fn handle_get_nameservers(&self) -> Response<Body> {
        let nameservers = &self.config.edge.nameservers;
        json_response(StatusCode::OK, &ApiResponse::success(nameservers.clone()))
    }

    // =========================================================================
    // ACCOUNT & USAGE
    // =========================================================================

    async fn handle_get_account(&self) -> Response<Body> {
        let zone_count = self.zone_store.zone_count().await;

        // For now, everyone is on free tier
        // In production, this would check Solana staking
        let response = AccountResponse {
            zone_count,
            max_zones: 5,
            tier: "free".to_string(),
            features: AccountFeatures {
                dnssec: false,
                advanced_analytics: false,
                custom_nameservers: false,
                priority_support: false,
            },
        };

        json_response(StatusCode::OK, &ApiResponse::success(response))
    }

    async fn handle_get_usage(&self) -> Response<Body> {
        let zones = self.zone_store.list_zones().await;
        let total_records: usize = zones.iter().map(|z| z.records.len()).sum();

        // TODO: Get actual query counts from metering
        let response = UsageResponse {
            total_zones: zones.len(),
            total_records,
            queries_today: 0,
            queries_month: 0,
        };

        json_response(StatusCode::OK, &ApiResponse::success(response))
    }

    async fn handle_get_tier(&self) -> Response<Body> {
        // For now, return free tier info
        // In production, this would check Solana staking
        let response = serde_json::json!({
            "current_tier": "free",
            "staked_amount": 0,
            "upgrade_options": [
                {"feature": "unlimited_zones", "required_stake": 1000},
                {"feature": "dnssec", "required_stake": 2500},
                {"feature": "advanced_analytics", "required_stake": 5000},
                {"feature": "priority_support", "required_stake": 10000},
                {"feature": "custom_nameservers", "required_stake": 25000}
            ]
        });

        json_response(StatusCode::OK, &response)
    }

    // =========================================================================
    // STATISTICS
    // =========================================================================

    async fn handle_get_stats(&self) -> Response<Body> {
        let zones = self.zone_store.list_zones().await;
        let total_records: usize = zones.iter().map(|z| z.records.len()).sum();

        let response = serde_json::json!({
            "total_zones": zones.len(),
            "total_records": total_records,
            "queries_today": 0,
            "cache_hit_ratio": 0.0,
        });

        json_response(StatusCode::OK, &response)
    }

    async fn handle_get_zone_stats(&self, domain: &str) -> Response<Body> {
        if !self.zone_store.zone_exists(domain).await {
            return json_error(StatusCode::NOT_FOUND, "Zone not found");
        }

        // TODO: Get actual stats from metering
        let response = ZoneStatsResponse {
            domain: domain.to_string(),
            total_queries: 0,
            queries_by_type: HashMap::new(),
            cache_hit_ratio: 0.0,
        };

        json_response(StatusCode::OK, &ApiResponse::success(response))
    }

    // =========================================================================
    // ANALYTICS
    // =========================================================================

    async fn handle_get_analytics(&self, domain: &str) -> Response<Body> {
        if !self.zone_store.zone_exists(domain).await {
            return json_error(StatusCode::NOT_FOUND, "Zone not found");
        }

        // TODO: Get actual analytics from metering
        let response = serde_json::json!({
            "domain": domain,
            "period": "24h",
            "total_queries": 0,
            "queries_by_type": {},
            "queries_by_country": {},
            "cache_hit_ratio": 0.0,
            "latency_p50_ms": 0,
            "latency_p95_ms": 0,
            "latency_p99_ms": 0,
        });

        json_response(StatusCode::OK, &response)
    }

    async fn handle_get_timeseries(&self, domain: &str, _req: Request<Body>) -> Response<Body> {
        if !self.zone_store.zone_exists(domain).await {
            return json_error(StatusCode::NOT_FOUND, "Zone not found");
        }

        // TODO: Get actual timeseries from metering
        let response = serde_json::json!({
            "domain": domain,
            "interval": "1h",
            "data": []
        });

        json_response(StatusCode::OK, &response)
    }

    // =========================================================================
    // EDGE NODE HANDLERS
    // =========================================================================

    async fn handle_list_edges(&self) -> Response<Body> {
        let Some(registry) = &self.edge_registry else {
            return json_error(StatusCode::SERVICE_UNAVAILABLE, "Edge registry not configured");
        };

        let nodes = registry.get_all_nodes().await;
        let responses: Vec<EdgeNodeResponse> = nodes.iter().map(EdgeNodeResponse::from_node).collect();

        json_response(StatusCode::OK, &ApiResponse::success(responses))
    }

    async fn handle_register_edge(&self, req: Request<Body>) -> Response<Body> {
        let Some(registry) = &self.edge_registry else {
            return json_error(StatusCode::SERVICE_UNAVAILABLE, "Edge registry not configured");
        };

        let body = match parse_body::<RegisterEdgeRequest>(req).await {
            Ok(b) => b,
            Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
        };

        // Parse IP addresses
        let ipv4 = body.ipv4.as_ref().and_then(|s| s.parse().ok());
        let ipv6 = body.ipv6.as_ref().and_then(|s| s.parse().ok());

        if ipv4.is_none() && ipv6.is_none() {
            return json_error(StatusCode::BAD_REQUEST, "At least one IP address required");
        }

        let now = current_timestamp();
        let node = EdgeNode {
            id: body.id.clone(),
            ipv4,
            ipv6,
            region: body.region,
            country: body.country,
            city: body.city,
            latitude: body.latitude,
            longitude: body.longitude,
            capacity: body.capacity,
            healthy: true,
            last_health_check: now,
            consecutive_failures: 0,
            registered_at: now,
            metadata: None,
        };

        match registry.register(node.clone()).await {
            Ok(_) => {
                info!("Registered edge node: {}", body.id);
                let response = EdgeNodeResponse::from_node(&node);
                json_response(StatusCode::CREATED, &ApiResponse::success(response))
            }
            Err(e) => {
                error!("Failed to register edge node: {}", e);
                json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to register node")
            }
        }
    }

    async fn handle_get_edge(&self, node_id: &str) -> Response<Body> {
        let Some(registry) = &self.edge_registry else {
            return json_error(StatusCode::SERVICE_UNAVAILABLE, "Edge registry not configured");
        };

        match registry.get_node(node_id).await {
            Some(node) => {
                let response = EdgeNodeResponse::from_node(&node);
                json_response(StatusCode::OK, &ApiResponse::success(response))
            }
            None => json_error(StatusCode::NOT_FOUND, "Edge node not found"),
        }
    }

    async fn handle_unregister_edge(&self, node_id: &str) -> Response<Body> {
        let Some(registry) = &self.edge_registry else {
            return json_error(StatusCode::SERVICE_UNAVAILABLE, "Edge registry not configured");
        };

        if registry.unregister(node_id).await {
            info!("Unregistered edge node: {}", node_id);
            json_response(
                StatusCode::OK,
                &ApiResponse::<()>::success_message("Edge node unregistered"),
            )
        } else {
            json_error(StatusCode::NOT_FOUND, "Edge node not found")
        }
    }

    async fn handle_get_edge_health(&self, node_id: &str) -> Response<Body> {
        let Some(checker) = &self.health_checker else {
            return json_error(StatusCode::SERVICE_UNAVAILABLE, "Health checker not configured");
        };

        match checker.get_result(node_id).await {
            Some(result) => json_response(StatusCode::OK, &ApiResponse::success(result)),
            None => json_error(StatusCode::NOT_FOUND, "No health data for node"),
        }
    }

    async fn handle_get_edges_health_summary(&self) -> Response<Body> {
        let Some(checker) = &self.health_checker else {
            return json_error(StatusCode::SERVICE_UNAVAILABLE, "Health checker not configured");
        };

        let summary = checker.get_summary().await;
        json_response(StatusCode::OK, &ApiResponse::success(summary))
    }

    // =========================================================================
    // GEO RESOLUTION HANDLERS
    // =========================================================================

    async fn handle_geo_resolve(&self, req: Request<Body>) -> Response<Body> {
        let Some(resolver) = &self.geo_resolver else {
            return json_error(StatusCode::SERVICE_UNAVAILABLE, "Geo resolver not configured");
        };

        let body = match parse_body::<GeoResolveRequest>(req).await {
            Ok(b) => b,
            Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
        };

        let client_ip = match body.client_ip.parse() {
            Ok(ip) => ip,
            Err(_) => return json_error(StatusCode::BAD_REQUEST, "Invalid IP address"),
        };

        let record_type = match body.record_type.to_uppercase().as_str() {
            "A" => GeoRecordType::A,
            "AAAA" => GeoRecordType::AAAA,
            _ => return json_error(StatusCode::BAD_REQUEST, "Invalid record type (use A or AAAA)"),
        };

        let result = resolver
            .resolve_with_info(client_ip, record_type, body.count)
            .await;

        json_response(StatusCode::OK, &ApiResponse::success(result))
    }

    async fn handle_list_regions(&self) -> Response<Body> {
        let Some(registry) = &self.edge_registry else {
            return json_error(StatusCode::SERVICE_UNAVAILABLE, "Edge registry not configured");
        };

        let regions = registry.get_all_regions().await;
        json_response(StatusCode::OK, &ApiResponse::success(regions))
    }

    async fn handle_get_region_nodes(&self, region: &str) -> Response<Body> {
        let Some(registry) = &self.edge_registry else {
            return json_error(StatusCode::SERVICE_UNAVAILABLE, "Edge registry not configured");
        };

        let nodes = registry.get_healthy_in_region(region).await;
        let responses: Vec<EdgeNodeResponse> = nodes.iter().map(EdgeNodeResponse::from_node).collect();

        json_response(StatusCode::OK, &ApiResponse::success(responses))
    }

    // =========================================================================
    // DNSSEC HANDLERS (Sprint 30.4)
    // =========================================================================

    async fn handle_get_dnssec_status(&self, domain: &str) -> Response<Body> {
        // Check if zone exists
        if !self.zone_store.zone_exists(domain).await {
            return json_error(StatusCode::NOT_FOUND, "Zone not found");
        }

        let Some(key_manager) = &self.dnssec_key_manager else {
            // DNSSEC not configured, return disabled status
            let response = DnssecStatusResponse {
                domain: domain.to_string(),
                enabled: false,
                algorithm: None,
                key_tag_zsk: None,
                key_tag_ksk: None,
                signed_at: None,
                expires_at: None,
            };
            return json_response(StatusCode::OK, &ApiResponse::success(response));
        };

        let enabled = key_manager.is_enabled(domain).await;
        let zsk = key_manager.get_active_zsk(domain).await;
        let ksk = key_manager.get_active_ksk(domain).await;

        // Get signed zone info if available
        let (signed_at, expires_at) = if let Some(resigner) = &self.dnssec_resigner {
            if let Some(signed) = resigner.get_signed_zone(domain).await {
                (Some(signed.signed_at), Some(signed.expires_at))
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

        let response = DnssecStatusResponse {
            domain: domain.to_string(),
            enabled,
            algorithm: zsk.as_ref().map(|k| {
                DnssecAlgorithm::from_number(k.algorithm)
                    .map(|a| a.name().to_string())
                    .unwrap_or_else(|| format!("UNKNOWN({})", k.algorithm))
            }),
            key_tag_zsk: zsk.map(|k| k.key_tag),
            key_tag_ksk: ksk.map(|k| k.key_tag),
            signed_at,
            expires_at,
        };

        json_response(StatusCode::OK, &ApiResponse::success(response))
    }

    async fn handle_enable_dnssec(&self, domain: &str, req: Request<Body>) -> Response<Body> {
        // Check if zone exists
        if !self.zone_store.zone_exists(domain).await {
            return json_error(StatusCode::NOT_FOUND, "Zone not found");
        }

        let Some(key_manager) = &self.dnssec_key_manager else {
            return json_error(StatusCode::SERVICE_UNAVAILABLE, "DNSSEC not configured");
        };

        // Parse request
        let body = match parse_body::<EnableDnssecRequest>(req).await {
            Ok(b) => b,
            Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
        };

        // Parse algorithm
        let algorithm = match body.algorithm.to_uppercase().as_str() {
            "ED25519" => DnssecAlgorithm::Ed25519,
            "ECDSAP256SHA256" | "ECDSA" => DnssecAlgorithm::EcdsaP256Sha256,
            "RSASHA256" | "RSA" => DnssecAlgorithm::RsaSha256,
            _ => return json_error(StatusCode::BAD_REQUEST, "Unsupported algorithm. Use ED25519, ECDSAP256SHA256, or RSASHA256"),
        };

        // Check if already enabled
        if key_manager.is_enabled(domain).await {
            return json_error(StatusCode::CONFLICT, "DNSSEC already enabled for this zone");
        }

        // Generate ZSK and KSK
        if let Err(e) = key_manager.generate_key(domain, KeyFlags::Zsk, algorithm).await {
            error!("Failed to generate ZSK: {}", e);
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to generate ZSK");
        }

        if let Err(e) = key_manager.generate_key(domain, KeyFlags::Ksk, algorithm).await {
            error!("Failed to generate KSK: {}", e);
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to generate KSK");
        }

        // Sign the zone if resigner is available
        if let Some(resigner) = &self.dnssec_resigner {
            if let Err(e) = resigner.resign_zone(domain).await {
                warn!("Failed to sign zone after enabling DNSSEC: {}", e);
            }
        }

        // Update zone settings
        let _ = self.zone_store.update_zone_settings(domain, None, Some(true)).await;

        info!("Enabled DNSSEC for zone {} with algorithm {}", domain, body.algorithm);

        // Return DS record for registrar
        if let Some(ds) = key_manager.get_ds_record(domain).await {
            let response = DsRecordResponse::from_ds_record(domain, &ds);
            json_response(StatusCode::OK, &ApiResponse::success(response))
        } else {
            json_response(
                StatusCode::OK,
                &ApiResponse::<()>::success_message("DNSSEC enabled. DS record will be available shortly."),
            )
        }
    }

    async fn handle_disable_dnssec(&self, domain: &str) -> Response<Body> {
        // Check if zone exists
        if !self.zone_store.zone_exists(domain).await {
            return json_error(StatusCode::NOT_FOUND, "Zone not found");
        }

        let Some(key_manager) = &self.dnssec_key_manager else {
            return json_error(StatusCode::SERVICE_UNAVAILABLE, "DNSSEC not configured");
        };

        // Check if enabled
        if !key_manager.is_enabled(domain).await {
            return json_error(StatusCode::BAD_REQUEST, "DNSSEC not enabled for this zone");
        }

        // Get all keys and remove them
        let keys = key_manager.get_keys(domain).await;
        for key in keys {
            if let Err(e) = key_manager.remove_key(domain, key.key_tag).await {
                warn!("Failed to remove key {}: {}", key.key_tag, e);
            }
        }

        // Update zone settings
        let _ = self.zone_store.update_zone_settings(domain, None, Some(false)).await;

        info!("Disabled DNSSEC for zone {}", domain);

        json_response(
            StatusCode::OK,
            &ApiResponse::<()>::success_message("DNSSEC disabled. Remember to remove DS record from your registrar."),
        )
    }

    async fn handle_get_ds_record(&self, domain: &str) -> Response<Body> {
        // Check if zone exists
        if !self.zone_store.zone_exists(domain).await {
            return json_error(StatusCode::NOT_FOUND, "Zone not found");
        }

        let Some(key_manager) = &self.dnssec_key_manager else {
            return json_error(StatusCode::SERVICE_UNAVAILABLE, "DNSSEC not configured");
        };

        // Check if DNSSEC is enabled
        if !key_manager.is_enabled(domain).await {
            return json_error(StatusCode::BAD_REQUEST, "DNSSEC not enabled for this zone");
        }

        // Get DS record
        match key_manager.get_ds_record(domain).await {
            Some(ds) => {
                let response = DsRecordResponse::from_ds_record(domain, &ds);
                json_response(StatusCode::OK, &ApiResponse::success(response))
            }
            None => json_error(StatusCode::NOT_FOUND, "No KSK found for this zone"),
        }
    }

    async fn handle_resign_zone(&self, domain: &str) -> Response<Body> {
        // Check if zone exists
        if !self.zone_store.zone_exists(domain).await {
            return json_error(StatusCode::NOT_FOUND, "Zone not found");
        }

        let Some(resigner) = &self.dnssec_resigner else {
            return json_error(StatusCode::SERVICE_UNAVAILABLE, "DNSSEC signing not configured");
        };

        // Force re-sign
        match resigner.resign_zone(domain).await {
            Ok(signed) => {
                info!("Re-signed zone {}", domain);
                let response = SignedZoneResponse::from_signed_zone(&signed);
                json_response(StatusCode::OK, &ApiResponse::success(response))
            }
            Err(e) => {
                error!("Failed to re-sign zone {}: {}", domain, e);
                json_error(StatusCode::INTERNAL_SERVER_ERROR, &format!("Failed to sign zone: {}", e))
            }
        }
    }

    async fn handle_get_signed_zone(&self, domain: &str) -> Response<Body> {
        // Check if zone exists
        if !self.zone_store.zone_exists(domain).await {
            return json_error(StatusCode::NOT_FOUND, "Zone not found");
        }

        let Some(resigner) = &self.dnssec_resigner else {
            return json_error(StatusCode::SERVICE_UNAVAILABLE, "DNSSEC signing not configured");
        };

        match resigner.get_signed_zone(domain).await {
            Some(signed) => {
                let response = SignedZoneResponse::from_signed_zone(&signed);
                json_response(StatusCode::OK, &ApiResponse::success(response))
            }
            None => json_error(StatusCode::NOT_FOUND, "Zone not signed. Enable DNSSEC first."),
        }
    }

    // =========================================================================
    // HELPERS
    // =========================================================================

    fn not_found(&self) -> Response<Body> {
        json_error(StatusCode::NOT_FOUND, "Endpoint not found")
    }

    /// Run the API server (convenience method)
    pub async fn run(self: Arc<Self>, addr: SocketAddr) -> anyhow::Result<()> {
        run_dns_api(addr, self).await
    }
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Parse JSON body from request
async fn parse_body<T: for<'de> Deserialize<'de>>(req: Request<Body>) -> Result<T, String> {
    let bytes = hyper::body::to_bytes(req.into_body())
        .await
        .map_err(|e| format!("Failed to read body: {}", e))?;

    serde_json::from_slice(&bytes).map_err(|e| format!("Invalid JSON: {}", e))
}

/// Create JSON response
fn json_response<T: Serialize>(status: StatusCode, body: &T) -> Response<Body> {
    let json = serde_json::to_string(body).unwrap_or_else(|_| "{}".to_string());
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(Body::from(json))
        .unwrap()
}

/// Create JSON error response
fn json_error(status: StatusCode, message: &str) -> Response<Body> {
    json_response(status, &ApiResponse::<()>::error(message))
}

/// Extract domain from path like /aegis/dns/api/zones/example.com
fn extract_domain(path: &str, prefix: &str) -> String {
    path.trim_start_matches(prefix)
        .split('/')
        .next()
        .unwrap_or("")
        .to_string()
}

/// Extract zone domain from path like /aegis/dns/api/zones/example.com/records
fn extract_zone_domain(path: &str) -> Option<String> {
    let parts: Vec<&str> = path.split('/').collect();
    // /aegis/dns/api/zones/{domain}/records
    // 0("")  1    2   3     4       5        6
    if parts.len() >= 7 && !parts[5].is_empty() {
        Some(parts[5].to_string())
    } else {
        None
    }
}

/// Extract zone and record ID from path like /aegis/dns/api/zones/example.com/records/rec_123
fn extract_zone_and_record(path: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = path.split('/').collect();
    // /aegis/dns/api/zones/{domain}/records/{record_id}
    // 0("")  1    2   3     4       5        6        7
    if parts.len() >= 8 && !parts[5].is_empty() && !parts[7].is_empty() {
        Some((parts[5].to_string(), parts[7].to_string()))
    } else {
        None
    }
}

/// Extract domain from analytics path like /aegis/dns/api/zones/example.com/analytics
fn extract_analytics_domain(path: &str) -> Option<String> {
    let parts: Vec<&str> = path.split('/').collect();
    // /aegis/dns/api/zones/{domain}/analytics
    // 0("")  1    2   3     4       5         6
    if parts.len() >= 7 && !parts[5].is_empty() {
        Some(parts[5].to_string())
    } else {
        None
    }
}

/// Extract edge node ID from health path like /aegis/dns/api/edges/node-1/health
fn extract_edge_id_from_health_path(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    // /aegis/dns/api/edges/{node_id}/health
    // 0("")  1    2   3      4         5
    if parts.len() >= 6 {
        parts[5].to_string()
    } else {
        String::new()
    }
}

/// Extract domain from DNSSEC paths like /aegis/dns/api/zones/example.com/dnssec
fn extract_dnssec_domain(path: &str) -> Option<String> {
    let parts: Vec<&str> = path.split('/').collect();
    // /aegis/dns/api/zones/{domain}/dnssec
    // 0("")  1    2   3     4       5       6
    if parts.len() >= 7 && !parts[5].is_empty() {
        Some(parts[5].to_string())
    } else {
        None
    }
}

/// Get current Unix timestamp
fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Validate domain name (basic validation)
fn is_valid_domain(domain: &str) -> bool {
    if domain.is_empty() || domain.len() > 253 {
        return false;
    }

    // Check each label
    for label in domain.split('.') {
        if label.is_empty() || label.len() > 63 {
            return false;
        }
        // Labels must start with alphanumeric
        if !label.chars().next().map(|c| c.is_alphanumeric()).unwrap_or(false) {
            return false;
        }
        // Labels can contain alphanumeric and hyphens
        if !label.chars().all(|c| c.is_alphanumeric() || c == '-') {
            return false;
        }
    }

    true
}

/// Parse record value based on type
fn parse_record_value(record_type: &DnsRecordType, value: &str) -> Result<DnsRecordValue, String> {
    match record_type {
        DnsRecordType::A => {
            let ip = value
                .parse()
                .map_err(|_| "Invalid IPv4 address".to_string())?;
            Ok(DnsRecordValue::A(ip))
        }
        DnsRecordType::AAAA => {
            let ip = value
                .parse()
                .map_err(|_| "Invalid IPv6 address".to_string())?;
            Ok(DnsRecordValue::AAAA(ip))
        }
        DnsRecordType::CNAME => Ok(DnsRecordValue::CNAME(value.to_string())),
        DnsRecordType::MX => Ok(DnsRecordValue::MX {
            exchange: value.to_string(),
        }),
        DnsRecordType::TXT => Ok(DnsRecordValue::TXT(value.to_string())),
        DnsRecordType::NS => Ok(DnsRecordValue::NS(value.to_string())),
        DnsRecordType::PTR => Ok(DnsRecordValue::PTR(value.to_string())),
        _ => Err(format!("Unsupported record type: {:?}", record_type)),
    }
}

// =============================================================================
// SERVER
// =============================================================================

/// Run the DNS API server
pub async fn run_dns_api(addr: SocketAddr, api: Arc<DnsApi>) -> anyhow::Result<()> {
    let make_svc = make_service_fn(move |_conn| {
        let api = Arc::clone(&api);
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                let api = Arc::clone(&api);
                async move { api.handle_request(req).await }
            }))
        }
    });

    let server = Server::bind(&addr).serve(make_svc);
    info!("DNS API listening on http://{}", addr);
    server.await?;
    Ok(())
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_domain() {
        assert!(is_valid_domain("example.com"));
        assert!(is_valid_domain("sub.example.com"));
        assert!(is_valid_domain("my-domain.co.uk"));
        assert!(is_valid_domain("a.b.c.d.e.f"));

        assert!(!is_valid_domain(""));
        assert!(!is_valid_domain("."));
        assert!(!is_valid_domain("example..com"));
        assert!(!is_valid_domain("-example.com"));
        assert!(!is_valid_domain("exam ple.com"));
    }

    #[test]
    fn test_extract_domain() {
        assert_eq!(
            extract_domain("/aegis/dns/api/zones/example.com", "/aegis/dns/api/zones/"),
            "example.com"
        );
        assert_eq!(
            extract_domain("/aegis/dns/api/zones/test.org/records", "/aegis/dns/api/zones/"),
            "test.org"
        );
    }

    #[test]
    fn test_extract_zone_domain() {
        assert_eq!(
            extract_zone_domain("/aegis/dns/api/zones/example.com/records"),
            Some("example.com".to_string())
        );
        assert_eq!(
            extract_zone_domain("/aegis/dns/api/zones/test.org/records"),
            Some("test.org".to_string())
        );
        assert_eq!(extract_zone_domain("/aegis/dns/api/zones"), None);
    }

    #[test]
    fn test_extract_zone_and_record() {
        assert_eq!(
            extract_zone_and_record("/aegis/dns/api/zones/example.com/records/rec_123"),
            Some(("example.com".to_string(), "rec_123".to_string()))
        );
        assert_eq!(
            extract_zone_and_record("/aegis/dns/api/zones/example.com/records"),
            None
        );
    }

    #[test]
    fn test_parse_record_value_a() {
        let value = parse_record_value(&DnsRecordType::A, "192.168.1.1").unwrap();
        assert!(matches!(value, DnsRecordValue::A(_)));
    }

    #[test]
    fn test_parse_record_value_aaaa() {
        let value = parse_record_value(&DnsRecordType::AAAA, "2001:db8::1").unwrap();
        assert!(matches!(value, DnsRecordValue::AAAA(_)));
    }

    #[test]
    fn test_parse_record_value_cname() {
        let value = parse_record_value(&DnsRecordType::CNAME, "www.example.com").unwrap();
        assert!(matches!(value, DnsRecordValue::CNAME(_)));
    }

    #[test]
    fn test_parse_record_value_invalid_ip() {
        let result = parse_record_value(&DnsRecordType::A, "invalid");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_api_health() {
        let zone_store = Arc::new(ZoneStore::new());
        let config = DnsConfig::default();
        let api = DnsApi::new(zone_store, config);

        let req = Request::builder()
            .method(Method::GET)
            .uri("/aegis/dns/api/health")
            .body(Body::empty())
            .unwrap();

        let response = api.handle_request(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_api_list_zones_empty() {
        let zone_store = Arc::new(ZoneStore::new());
        let config = DnsConfig::default();
        let api = DnsApi::new(zone_store, config);

        let req = Request::builder()
            .method(Method::GET)
            .uri("/aegis/dns/api/zones")
            .body(Body::empty())
            .unwrap();

        let response = api.handle_request(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_api_create_zone() {
        let zone_store = Arc::new(ZoneStore::new());
        let config = DnsConfig::default();
        let api = DnsApi::new(zone_store.clone(), config);

        let body = serde_json::json!({
            "domain": "example.com",
            "proxied": true
        });

        let req = Request::builder()
            .method(Method::POST)
            .uri("/aegis/dns/api/zones")
            .header("Content-Type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();

        let response = api.handle_request(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        // Verify zone was created
        assert!(zone_store.zone_exists("example.com").await);
    }

    #[tokio::test]
    async fn test_api_create_zone_invalid_domain() {
        let zone_store = Arc::new(ZoneStore::new());
        let config = DnsConfig::default();
        let api = DnsApi::new(zone_store, config);

        let body = serde_json::json!({
            "domain": "",
            "proxied": true
        });

        let req = Request::builder()
            .method(Method::POST)
            .uri("/aegis/dns/api/zones")
            .header("Content-Type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();

        let response = api.handle_request(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_api_create_zone_duplicate() {
        let zone_store = Arc::new(ZoneStore::new());
        let config = DnsConfig::default();
        let api = DnsApi::new(zone_store.clone(), config);

        // Create zone first
        zone_store.upsert_zone(Zone::new("example.com", false)).await.unwrap();

        let body = serde_json::json!({
            "domain": "example.com",
            "proxied": true
        });

        let req = Request::builder()
            .method(Method::POST)
            .uri("/aegis/dns/api/zones")
            .header("Content-Type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();

        let response = api.handle_request(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_api_get_zone() {
        let zone_store = Arc::new(ZoneStore::new());
        let config = DnsConfig::default();
        let api = DnsApi::new(zone_store.clone(), config);

        zone_store.upsert_zone(Zone::new("example.com", true)).await.unwrap();

        let req = Request::builder()
            .method(Method::GET)
            .uri("/aegis/dns/api/zones/example.com")
            .body(Body::empty())
            .unwrap();

        let response = api.handle_request(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_api_get_zone_not_found() {
        let zone_store = Arc::new(ZoneStore::new());
        let config = DnsConfig::default();
        let api = DnsApi::new(zone_store, config);

        let req = Request::builder()
            .method(Method::GET)
            .uri("/aegis/dns/api/zones/nonexistent.com")
            .body(Body::empty())
            .unwrap();

        let response = api.handle_request(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_api_delete_zone() {
        let zone_store = Arc::new(ZoneStore::new());
        let config = DnsConfig::default();
        let api = DnsApi::new(zone_store.clone(), config);

        zone_store.upsert_zone(Zone::new("example.com", false)).await.unwrap();

        let req = Request::builder()
            .method(Method::DELETE)
            .uri("/aegis/dns/api/zones/example.com")
            .body(Body::empty())
            .unwrap();

        let response = api.handle_request(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Verify zone was deleted
        assert!(!zone_store.zone_exists("example.com").await);
    }

    #[tokio::test]
    async fn test_api_nameservers() {
        let zone_store = Arc::new(ZoneStore::new());
        let config = DnsConfig::default();
        let api = DnsApi::new(zone_store, config);

        let req = Request::builder()
            .method(Method::GET)
            .uri("/aegis/dns/api/nameservers")
            .body(Body::empty())
            .unwrap();

        let response = api.handle_request(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_api_account() {
        let zone_store = Arc::new(ZoneStore::new());
        let config = DnsConfig::default();
        let api = DnsApi::new(zone_store, config);

        let req = Request::builder()
            .method(Method::GET)
            .uri("/aegis/dns/api/account")
            .body(Body::empty())
            .unwrap();

        let response = api.handle_request(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
