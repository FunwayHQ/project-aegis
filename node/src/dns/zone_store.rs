//! In-Memory Zone Storage
//!
//! Thread-safe storage for DNS zones and records with support for
//! querying by domain name and record type.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use rand::Rng;

use super::{DnsError, DnsRecord, DnsRecordType, DnsRecordValue};

/// A DNS zone containing records for a domain
#[derive(Debug, Clone)]
pub struct Zone {
    /// The domain name (e.g., "example.com")
    pub domain: String,
    /// DNS records in this zone
    pub records: Vec<DnsRecord>,
    /// Whether A/AAAA records should return AEGIS anycast IP
    pub proxied: bool,
    /// DNSSEC enabled for this zone
    pub dnssec_enabled: bool,
    /// Creation timestamp (Unix seconds)
    pub created_at: u64,
    /// Last update timestamp (Unix seconds)
    pub updated_at: u64,
}

impl Zone {
    /// Create a new zone
    pub fn new(domain: impl Into<String>, proxied: bool) -> Self {
        let now = current_timestamp();
        Self {
            domain: domain.into(),
            records: Vec::new(),
            proxied,
            dnssec_enabled: false,
            created_at: now,
            updated_at: now,
        }
    }

    /// Add a record to the zone
    pub fn add_record(&mut self, record: DnsRecord) {
        self.records.push(record);
        self.updated_at = current_timestamp();
    }

    /// Remove a record by ID
    pub fn remove_record(&mut self, record_id: &str) -> bool {
        let len_before = self.records.len();
        self.records.retain(|r| r.id != record_id);
        let removed = self.records.len() < len_before;
        if removed {
            self.updated_at = current_timestamp();
        }
        removed
    }

    /// Find records by name and type
    pub fn find_records(&self, name: &str, record_type: DnsRecordType) -> Vec<&DnsRecord> {
        self.records
            .iter()
            .filter(|r| {
                (r.name == name || (name.is_empty() && r.name == "@"))
                    && r.record_type == record_type
            })
            .collect()
    }

    /// Find all records by name
    pub fn find_records_by_name(&self, name: &str) -> Vec<&DnsRecord> {
        self.records
            .iter()
            .filter(|r| r.name == name || (name.is_empty() && r.name == "@"))
            .collect()
    }

    /// Get the SOA record for this zone
    pub fn soa_record(&self) -> Option<&DnsRecord> {
        self.records
            .iter()
            .find(|r| r.record_type == DnsRecordType::SOA)
    }

    /// Get all NS records for this zone
    pub fn ns_records(&self) -> Vec<&DnsRecord> {
        self.records
            .iter()
            .filter(|r| r.record_type == DnsRecordType::NS && r.name == "@")
            .collect()
    }

    /// Create default SOA and NS records for a new zone
    pub fn create_default_records(&mut self, nameservers: &[String]) {
        // Add SOA record
        let soa = DnsRecord {
            id: generate_id(),
            name: "@".to_string(),
            record_type: DnsRecordType::SOA,
            ttl: 3600,
            value: DnsRecordValue::SOA {
                mname: nameservers.first().cloned().unwrap_or_else(|| "ns1.aegis.network".to_string()),
                rname: format!("hostmaster.{}", self.domain),
                serial: generate_serial(),
                refresh: 3600,
                retry: 600,
                expire: 604800,
                minimum: 300,
            },
            priority: None,
            proxied: false,
        };
        self.records.push(soa);

        // Add NS records
        for ns in nameservers {
            let ns_record = DnsRecord {
                id: generate_id(),
                name: "@".to_string(),
                record_type: DnsRecordType::NS,
                ttl: 86400,
                value: DnsRecordValue::NS(ns.clone()),
                priority: None,
                proxied: false,
            };
            self.records.push(ns_record);
        }

        self.updated_at = current_timestamp();
    }

    /// Increment the SOA serial number
    pub fn increment_serial(&mut self) {
        for record in &mut self.records {
            if record.record_type == DnsRecordType::SOA {
                if let DnsRecordValue::SOA { ref mut serial, .. } = record.value {
                    *serial = generate_serial();
                }
            }
        }
        self.updated_at = current_timestamp();
    }
}

/// Thread-safe zone storage
pub struct ZoneStore {
    /// Map of domain -> Zone
    zones: Arc<RwLock<HashMap<String, Zone>>>,
    /// Index of subdomain -> parent domain for faster lookups
    subdomain_index: Arc<RwLock<HashMap<String, String>>>,
}

impl ZoneStore {
    /// Create a new zone store
    pub fn new() -> Self {
        Self {
            zones: Arc::new(RwLock::new(HashMap::new())),
            subdomain_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add or update a zone
    pub async fn upsert_zone(&self, zone: Zone) -> Result<(), DnsError> {
        let domain = normalize_domain(&zone.domain);

        let mut zones = self.zones.write().await;
        zones.insert(domain, zone);

        Ok(())
    }

    /// Get a zone by domain
    pub async fn get_zone(&self, domain: &str) -> Option<Zone> {
        let domain = normalize_domain(domain);
        let zones = self.zones.read().await;
        zones.get(&domain).cloned()
    }

    /// Delete a zone
    pub async fn delete_zone(&self, domain: &str) -> bool {
        let domain = normalize_domain(domain);
        let mut zones = self.zones.write().await;
        zones.remove(&domain).is_some()
    }

    /// List all zones
    pub async fn list_zones(&self) -> Vec<Zone> {
        let zones = self.zones.read().await;
        zones.values().cloned().collect()
    }

    /// Get zone count
    pub async fn zone_count(&self) -> usize {
        let zones = self.zones.read().await;
        zones.len()
    }

    /// Resolve a DNS query
    ///
    /// Handles subdomain matching (e.g., www.example.com -> example.com zone)
    pub async fn resolve(
        &self,
        qname: &str,
        qtype: DnsRecordType,
    ) -> Option<(Zone, Vec<DnsRecord>)> {
        let qname = normalize_domain(qname);

        // Try exact zone match first
        {
            let zones = self.zones.read().await;
            if let Some(zone) = zones.get(&qname) {
                let records: Vec<DnsRecord> = zone
                    .find_records("@", qtype)
                    .into_iter()
                    .cloned()
                    .collect();
                if !records.is_empty() {
                    return Some((zone.clone(), records));
                }
            }
        }

        // Try to find parent zone for subdomain queries
        let parts: Vec<&str> = qname.split('.').collect();
        for i in 0..parts.len() {
            let parent = parts[i..].join(".");
            if let Some(zone) = self.get_zone(&parent).await {
                // Extract subdomain part (e.g., "www" from "www.example.com")
                let subdomain = if i == 0 {
                    "@".to_string()
                } else {
                    parts[..i].join(".")
                };

                let records: Vec<DnsRecord> = zone
                    .find_records(&subdomain, qtype)
                    .into_iter()
                    .cloned()
                    .collect();

                // Also check for CNAME records
                if records.is_empty() {
                    let cname_records: Vec<DnsRecord> = zone
                        .find_records(&subdomain, DnsRecordType::CNAME)
                        .into_iter()
                        .cloned()
                        .collect();
                    if !cname_records.is_empty() {
                        return Some((zone, cname_records));
                    }
                }

                if !records.is_empty() {
                    return Some((zone, records));
                }

                // Return zone for NXDOMAIN response (zone exists but record doesn't)
                return Some((zone, vec![]));
            }
        }

        None
    }

    /// Add a record to a zone
    pub async fn add_record(&self, domain: &str, record: DnsRecord) -> Result<(), DnsError> {
        let domain = normalize_domain(domain);
        let mut zones = self.zones.write().await;

        let zone = zones
            .get_mut(&domain)
            .ok_or_else(|| DnsError::ZoneNotFound(domain.clone()))?;

        zone.add_record(record);
        zone.increment_serial();

        Ok(())
    }

    /// Remove a record from a zone
    pub async fn remove_record(&self, domain: &str, record_id: &str) -> Result<(), DnsError> {
        let domain = normalize_domain(domain);
        let mut zones = self.zones.write().await;

        let zone = zones
            .get_mut(&domain)
            .ok_or_else(|| DnsError::ZoneNotFound(domain.clone()))?;

        if zone.remove_record(record_id) {
            zone.increment_serial();
            Ok(())
        } else {
            Err(DnsError::RecordNotFound(record_id.to_string()))
        }
    }

    /// Get all records for a zone
    pub async fn get_records(&self, domain: &str) -> Result<Vec<DnsRecord>, DnsError> {
        let domain = normalize_domain(domain);
        let zones = self.zones.read().await;

        let zone = zones
            .get(&domain)
            .ok_or_else(|| DnsError::ZoneNotFound(domain))?;

        Ok(zone.records.clone())
    }

    /// Update zone settings (proxied, dnssec)
    pub async fn update_zone_settings(
        &self,
        domain: &str,
        proxied: Option<bool>,
        dnssec_enabled: Option<bool>,
    ) -> Result<(), DnsError> {
        let domain = normalize_domain(domain);
        let mut zones = self.zones.write().await;

        let zone = zones
            .get_mut(&domain)
            .ok_or_else(|| DnsError::ZoneNotFound(domain))?;

        if let Some(p) = proxied {
            zone.proxied = p;
        }
        if let Some(d) = dnssec_enabled {
            zone.dnssec_enabled = d;
        }
        zone.updated_at = current_timestamp();

        Ok(())
    }

    /// Check if a zone exists
    pub async fn zone_exists(&self, domain: &str) -> bool {
        let domain = normalize_domain(domain);
        let zones = self.zones.read().await;
        zones.contains_key(&domain)
    }
}

impl Default for ZoneStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Normalize a domain name (lowercase, remove trailing dot)
fn normalize_domain(domain: &str) -> String {
    domain.to_lowercase().trim_end_matches('.').to_string()
}

/// Get current Unix timestamp
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Generate a unique ID
fn generate_id() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 8] = rng.gen();
    format!("rec_{}", hex::encode(bytes))
}

/// Generate a SOA serial number (YYYYMMDDNN format)
fn generate_serial() -> u32 {
    use chrono::Utc;
    let now = Utc::now();
    let base = now.format("%Y%m%d").to_string();
    let base_num: u32 = base.parse().unwrap_or(20240101);
    // Add a random suffix to ensure uniqueness
    let suffix: u32 = rand::thread_rng().gen_range(0..99);
    base_num * 100 + suffix
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[tokio::test]
    async fn test_zone_store_crud() {
        let store = ZoneStore::new();

        // Create zone
        let zone = Zone::new("example.com", true);
        store.upsert_zone(zone.clone()).await.unwrap();

        // Get zone
        let retrieved = store.get_zone("example.com").await.unwrap();
        assert_eq!(retrieved.domain, "example.com");
        assert!(retrieved.proxied);

        // List zones
        let zones = store.list_zones().await;
        assert_eq!(zones.len(), 1);

        // Delete zone
        assert!(store.delete_zone("example.com").await);
        assert!(store.get_zone("example.com").await.is_none());
    }

    #[tokio::test]
    async fn test_zone_with_records() {
        let store = ZoneStore::new();

        let mut zone = Zone::new("example.com", false);
        zone.create_default_records(&["ns1.aegis.network".to_string(), "ns2.aegis.network".to_string()]);

        store.upsert_zone(zone).await.unwrap();

        // Add A record
        let record = DnsRecord::a("www", "192.168.1.1".parse().unwrap(), 300);
        store.add_record("example.com", record).await.unwrap();

        // Resolve
        let (zone, records) = store.resolve("www.example.com", DnsRecordType::A).await.unwrap();
        assert_eq!(zone.domain, "example.com");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].name, "www");
    }

    #[tokio::test]
    async fn test_resolve_root_record() {
        let store = ZoneStore::new();

        let mut zone = Zone::new("example.com", false);
        let record = DnsRecord::a("@", "192.168.1.1".parse().unwrap(), 300);
        zone.add_record(record);

        store.upsert_zone(zone).await.unwrap();

        let (_, records) = store.resolve("example.com", DnsRecordType::A).await.unwrap();
        assert_eq!(records.len(), 1);
    }

    #[tokio::test]
    async fn test_resolve_cname() {
        let store = ZoneStore::new();

        let mut zone = Zone::new("example.com", false);
        zone.add_record(DnsRecord::cname("www", "example.com", 300));

        store.upsert_zone(zone).await.unwrap();

        // Query for A record, should return CNAME
        let (_, records) = store.resolve("www.example.com", DnsRecordType::A).await.unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].record_type, DnsRecordType::CNAME);
    }

    #[tokio::test]
    async fn test_remove_record() {
        let store = ZoneStore::new();

        let zone = Zone::new("example.com", false);
        store.upsert_zone(zone).await.unwrap();

        let record = DnsRecord::a("www", "192.168.1.1".parse().unwrap(), 300);
        let record_id = record.id.clone();
        store.add_record("example.com", record).await.unwrap();

        // Remove record
        store.remove_record("example.com", &record_id).await.unwrap();

        let records = store.get_records("example.com").await.unwrap();
        assert!(records.is_empty());
    }

    #[tokio::test]
    async fn test_normalize_domain() {
        let store = ZoneStore::new();

        let zone = Zone::new("Example.COM.", true);
        store.upsert_zone(zone).await.unwrap();

        // Should find with different cases
        assert!(store.get_zone("example.com").await.is_some());
        assert!(store.get_zone("EXAMPLE.COM").await.is_some());
        assert!(store.get_zone("example.com.").await.is_some());
    }

    #[tokio::test]
    async fn test_zone_settings_update() {
        let store = ZoneStore::new();

        let zone = Zone::new("example.com", false);
        store.upsert_zone(zone).await.unwrap();

        store.update_zone_settings("example.com", Some(true), Some(true))
            .await
            .unwrap();

        let zone = store.get_zone("example.com").await.unwrap();
        assert!(zone.proxied);
        assert!(zone.dnssec_enabled);
    }

    #[test]
    fn test_zone_find_records() {
        let mut zone = Zone::new("example.com", false);
        zone.add_record(DnsRecord::a("www", "192.168.1.1".parse().unwrap(), 300));
        zone.add_record(DnsRecord::a("www", "192.168.1.2".parse().unwrap(), 300));
        zone.add_record(DnsRecord::a("mail", "192.168.1.3".parse().unwrap(), 300));

        let records = zone.find_records("www", DnsRecordType::A);
        assert_eq!(records.len(), 2);

        let mail_records = zone.find_records("mail", DnsRecordType::A);
        assert_eq!(mail_records.len(), 1);
    }

    #[test]
    fn test_zone_default_records() {
        let mut zone = Zone::new("example.com", false);
        zone.create_default_records(&["ns1.aegis.network".to_string(), "ns2.aegis.network".to_string()]);

        // Should have SOA record
        assert!(zone.soa_record().is_some());

        // Should have NS records
        let ns = zone.ns_records();
        assert_eq!(ns.len(), 2);
    }

    #[test]
    fn test_serial_generation() {
        let serial = generate_serial();
        // Should be in YYYYMMDDNN format (10 digits)
        assert!(serial >= 2024010100);
        assert!(serial <= 2099123199);
    }

    #[tokio::test]
    async fn test_zone_exists() {
        let store = ZoneStore::new();

        assert!(!store.zone_exists("example.com").await);

        let zone = Zone::new("example.com", false);
        store.upsert_zone(zone).await.unwrap();

        assert!(store.zone_exists("example.com").await);
    }

    #[tokio::test]
    async fn test_error_zone_not_found() {
        let store = ZoneStore::new();

        let result = store.add_record("nonexistent.com", DnsRecord::a("www", "192.168.1.1".parse().unwrap(), 300)).await;
        assert!(matches!(result, Err(DnsError::ZoneNotFound(_))));
    }

    #[tokio::test]
    async fn test_error_record_not_found() {
        let store = ZoneStore::new();

        let zone = Zone::new("example.com", false);
        store.upsert_zone(zone).await.unwrap();

        let result = store.remove_record("example.com", "nonexistent_id").await;
        assert!(matches!(result, Err(DnsError::RecordNotFound(_))));
    }
}
