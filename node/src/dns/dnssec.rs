//! DNSSEC Signing Implementation
//!
//! Sprint 30.4: DNSSEC Implementation
//!
//! Provides DNSSEC signing for DNS zones, including:
//! - Zone signing with RRSIG records
//! - NSEC chain generation for authenticated denial
//! - Background re-signing to refresh signatures before expiration

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use crate::dns::dnssec_keys::{DnssecKeyManager, DnssecKeyPair, KeyFlags, KeyStatus};
use crate::dns::dns_types::{DnsRecord, DnsRecordType, DnsRecordValue};
use crate::dns::zone_store::{Zone, ZoneStore};

/// Configuration for DNSSEC signing
#[derive(Debug, Clone)]
pub struct DnssecConfig {
    /// Signature validity duration (default: 30 days)
    pub signature_validity: Duration,
    /// Inception offset (start validity slightly in the past, default: 1 hour)
    pub inception_offset: Duration,
    /// Re-signing interval (how often to check for expiring signatures)
    pub resign_interval: Duration,
    /// Days before expiration to trigger re-signing
    pub resign_before_days: u32,
}

impl Default for DnssecConfig {
    fn default() -> Self {
        Self {
            signature_validity: Duration::from_secs(86400 * 30), // 30 days
            inception_offset: Duration::from_secs(3600),         // 1 hour
            resign_interval: Duration::from_secs(86400),         // Check daily
            resign_before_days: 7,                               // Re-sign 7 days before expiration
        }
    }
}

/// A signed DNS zone with RRSIG and NSEC records
#[derive(Debug, Clone)]
pub struct SignedZone {
    /// The original zone domain
    pub domain: String,
    /// All records including signatures
    pub records: Vec<DnsRecord>,
    /// Timestamp when the zone was signed
    pub signed_at: u64,
    /// Signature expiration timestamp
    pub expires_at: u64,
    /// Key tags used for signing
    pub key_tags: Vec<u16>,
}

impl SignedZone {
    /// Check if the zone signature is about to expire
    pub fn is_expiring_soon(&self, days_threshold: u32) -> bool {
        let now = current_timestamp();
        let threshold_secs = days_threshold as u64 * 86400;
        now + threshold_secs >= self.expires_at
    }

    /// Get all RRSIG records
    pub fn get_rrsigs(&self) -> Vec<&DnsRecord> {
        self.records
            .iter()
            .filter(|r| r.record_type == DnsRecordType::RRSIG)
            .collect()
    }

    /// Get all NSEC records
    pub fn get_nsec_records(&self) -> Vec<&DnsRecord> {
        self.records
            .iter()
            .filter(|r| r.record_type == DnsRecordType::NSEC)
            .collect()
    }

    /// Get DNSKEY records
    pub fn get_dnskey_records(&self) -> Vec<&DnsRecord> {
        self.records
            .iter()
            .filter(|r| r.record_type == DnsRecordType::DNSKEY)
            .collect()
    }
}

/// An RRset (Resource Record Set) - records with same name and type
#[derive(Debug, Clone)]
pub struct RRset {
    /// Owner name (FQDN)
    pub name: String,
    /// Record type
    pub record_type: DnsRecordType,
    /// TTL
    pub ttl: u32,
    /// All records in this set
    pub records: Vec<DnsRecord>,
}

impl RRset {
    /// Get the number of labels in the name
    pub fn label_count(&self) -> u8 {
        count_labels(&self.name)
    }
}

/// DNSSEC Zone Signer
pub struct DnssecSigner {
    /// Key manager for accessing signing keys
    key_manager: Arc<DnssecKeyManager>,
    /// Configuration
    config: DnssecConfig,
}

impl DnssecSigner {
    /// Create a new DNSSEC signer
    pub fn new(key_manager: Arc<DnssecKeyManager>, config: DnssecConfig) -> Self {
        Self {
            key_manager,
            config,
        }
    }

    /// Sign all records in a zone
    pub async fn sign_zone(&self, zone: &Zone) -> Result<SignedZone, DnssecError> {
        let domain = &zone.domain;

        // Get active ZSK for signing RRsets
        let zsk = self.key_manager.get_active_zsk(domain).await
            .ok_or(DnssecError::NoActiveKey("No active ZSK found".to_string()))?;

        // Get active KSK for signing DNSKEY RRset
        let ksk = self.key_manager.get_active_ksk(domain).await
            .ok_or(DnssecError::NoActiveKey("No active KSK found".to_string()))?;

        info!("Signing zone {} with ZSK {} and KSK {}", domain, zsk.key_tag, ksk.key_tag);

        // Calculate signature times
        let now = current_timestamp();
        let inception = (now - self.config.inception_offset.as_secs()) as u32;
        let expiration = (now + self.config.signature_validity.as_secs()) as u32;

        // Collect all records including DNSKEY records for keys
        let mut all_records = zone.records.clone();

        // Add DNSKEY records
        let zsk_dnskey = self.create_dnskey_record(domain, &zsk);
        let ksk_dnskey = self.create_dnskey_record(domain, &ksk);
        all_records.push(zsk_dnskey);
        all_records.push(ksk_dnskey);

        // Group records into RRsets
        let rrsets = self.group_into_rrsets(domain, &all_records);

        let mut signed_records = all_records.clone();

        // Sign each RRset
        for rrset in &rrsets {
            // Use KSK to sign DNSKEY RRset, ZSK for everything else
            let signing_key = if rrset.record_type == DnsRecordType::DNSKEY {
                &ksk
            } else {
                &zsk
            };

            let rrsig = self.sign_rrset(domain, rrset, signing_key, inception, expiration)?;
            signed_records.push(rrsig);
        }

        // Generate NSEC chain for authenticated denial of existence
        let nsec_records = self.generate_nsec_chain(domain, &rrsets);

        // Sign NSEC RRsets
        let nsec_rrsets = self.group_into_rrsets(domain, &nsec_records);
        for rrset in &nsec_rrsets {
            let rrsig = self.sign_rrset(domain, rrset, &zsk, inception, expiration)?;
            signed_records.push(rrsig);
        }
        signed_records.extend(nsec_records);

        info!("Zone {} signed: {} total records", domain, signed_records.len());

        Ok(SignedZone {
            domain: domain.clone(),
            records: signed_records,
            signed_at: now,
            expires_at: expiration as u64,
            key_tags: vec![zsk.key_tag, ksk.key_tag],
        })
    }

    /// Group records into RRsets (same name and type)
    fn group_into_rrsets(&self, domain: &str, records: &[DnsRecord]) -> Vec<RRset> {
        let mut rrset_map: HashMap<(String, DnsRecordType), Vec<DnsRecord>> = HashMap::new();

        for record in records {
            // Skip RRSIG and NSEC records when grouping
            if matches!(record.record_type, DnsRecordType::RRSIG | DnsRecordType::NSEC | DnsRecordType::NSEC3) {
                continue;
            }

            let fqdn = record.fqdn(domain);
            let key = (fqdn, record.record_type);
            rrset_map.entry(key).or_default().push(record.clone());
        }

        rrset_map
            .into_iter()
            .map(|((name, record_type), records)| {
                let ttl = records.first().map(|r| r.ttl).unwrap_or(300);
                RRset {
                    name,
                    record_type,
                    ttl,
                    records,
                }
            })
            .collect()
    }

    /// Sign a single RRset
    fn sign_rrset(
        &self,
        domain: &str,
        rrset: &RRset,
        key: &DnssecKeyPair,
        inception: u32,
        expiration: u32,
    ) -> Result<DnsRecord, DnssecError> {
        // Build the data to sign per RFC 4034
        let mut data_to_sign = Vec::new();

        // 1. RRSIG RDATA (without signature)
        data_to_sign.extend_from_slice(&(rrset.record_type as u16).to_be_bytes()); // Type covered
        data_to_sign.push(key.algorithm); // Algorithm
        data_to_sign.push(rrset.label_count()); // Labels
        data_to_sign.extend_from_slice(&rrset.ttl.to_be_bytes()); // Original TTL
        data_to_sign.extend_from_slice(&expiration.to_be_bytes()); // Signature expiration
        data_to_sign.extend_from_slice(&inception.to_be_bytes()); // Signature inception
        data_to_sign.extend_from_slice(&key.key_tag.to_be_bytes()); // Key tag
        data_to_sign.extend_from_slice(&domain_to_wire_format(domain)); // Signer's name

        // 2. RRset records in canonical order
        let canonical_records = self.canonicalize_rrset(rrset, domain);
        data_to_sign.extend_from_slice(&canonical_records);

        // Sign the data
        let signature = key.sign(&data_to_sign)
            .map_err(|e| DnssecError::SigningFailed(e.to_string()))?;

        debug!(
            "Signed RRset {}/{} with key {} (sig: {} bytes)",
            rrset.name, rrset.record_type, key.key_tag, signature.len()
        );

        // Create RRSIG record
        Ok(DnsRecord {
            id: generate_record_id(),
            name: rrset.name.clone(),
            record_type: DnsRecordType::RRSIG,
            ttl: rrset.ttl,
            value: DnsRecordValue::RRSIG {
                type_covered: rrset.record_type,
                algorithm: key.algorithm,
                labels: rrset.label_count(),
                original_ttl: rrset.ttl,
                expiration,
                inception,
                key_tag: key.key_tag,
                signer_name: domain.to_string(),
                signature,
            },
            priority: None,
            proxied: false,
        })
    }

    /// Canonicalize RRset for signing (RFC 4034 Section 6.3)
    fn canonicalize_rrset(&self, rrset: &RRset, domain: &str) -> Vec<u8> {
        let mut result = Vec::new();

        // Get wire format for each record, sorted canonically
        let mut wire_records: Vec<Vec<u8>> = rrset.records.iter()
            .map(|r| self.record_to_wire(r, domain, rrset.ttl))
            .collect();

        // Sort records in canonical order (by RDATA)
        wire_records.sort();

        for wire in wire_records {
            result.extend(wire);
        }

        result
    }

    /// Convert a record to wire format for signing
    fn record_to_wire(&self, record: &DnsRecord, domain: &str, ttl: u32) -> Vec<u8> {
        let mut wire = Vec::new();

        // Owner name in wire format
        let fqdn = record.fqdn(domain);
        wire.extend_from_slice(&domain_to_wire_format(&fqdn));

        // Type
        wire.extend_from_slice(&(record.record_type as u16).to_be_bytes());

        // Class (IN = 1)
        wire.extend_from_slice(&1u16.to_be_bytes());

        // TTL
        wire.extend_from_slice(&ttl.to_be_bytes());

        // RDATA
        let rdata = self.value_to_rdata(&record.value, record.priority);
        wire.extend_from_slice(&(rdata.len() as u16).to_be_bytes());
        wire.extend_from_slice(&rdata);

        wire
    }

    /// Convert record value to RDATA wire format
    fn value_to_rdata(&self, value: &DnsRecordValue, priority: Option<u16>) -> Vec<u8> {
        match value {
            DnsRecordValue::A(ip) => ip.octets().to_vec(),
            DnsRecordValue::AAAA(ip) => ip.octets().to_vec(),
            DnsRecordValue::CNAME(name) => domain_to_wire_format(name),
            DnsRecordValue::MX { exchange } => {
                let mut rdata = Vec::new();
                rdata.extend_from_slice(&priority.unwrap_or(10).to_be_bytes());
                rdata.extend_from_slice(&domain_to_wire_format(exchange));
                rdata
            }
            DnsRecordValue::TXT(text) => {
                let bytes = text.as_bytes();
                let mut rdata = Vec::new();
                // TXT records are split into 255-byte chunks
                for chunk in bytes.chunks(255) {
                    rdata.push(chunk.len() as u8);
                    rdata.extend_from_slice(chunk);
                }
                rdata
            }
            DnsRecordValue::NS(name) => domain_to_wire_format(name),
            DnsRecordValue::SOA { mname, rname, serial, refresh, retry, expire, minimum } => {
                let mut rdata = Vec::new();
                rdata.extend_from_slice(&domain_to_wire_format(mname));
                rdata.extend_from_slice(&domain_to_wire_format(rname));
                rdata.extend_from_slice(&serial.to_be_bytes());
                rdata.extend_from_slice(&refresh.to_be_bytes());
                rdata.extend_from_slice(&retry.to_be_bytes());
                rdata.extend_from_slice(&expire.to_be_bytes());
                rdata.extend_from_slice(&minimum.to_be_bytes());
                rdata
            }
            DnsRecordValue::CAA { flags, tag, value: val } => {
                let mut rdata = Vec::new();
                rdata.push(*flags);
                rdata.push(tag.len() as u8);
                rdata.extend_from_slice(tag.as_bytes());
                rdata.extend_from_slice(val.as_bytes());
                rdata
            }
            DnsRecordValue::SRV { weight, port, target } => {
                let mut rdata = Vec::new();
                rdata.extend_from_slice(&priority.unwrap_or(0).to_be_bytes());
                rdata.extend_from_slice(&weight.to_be_bytes());
                rdata.extend_from_slice(&port.to_be_bytes());
                rdata.extend_from_slice(&domain_to_wire_format(target));
                rdata
            }
            DnsRecordValue::PTR(name) => domain_to_wire_format(name),
            DnsRecordValue::DNSKEY { flags, protocol, algorithm, public_key } => {
                let mut rdata = Vec::new();
                rdata.extend_from_slice(&flags.to_be_bytes());
                rdata.push(*protocol);
                rdata.push(*algorithm);
                rdata.extend_from_slice(public_key);
                rdata
            }
            DnsRecordValue::RRSIG { .. } => Vec::new(), // Not used for signing
            DnsRecordValue::NSEC { next_domain, types } => {
                let mut rdata = Vec::new();
                rdata.extend_from_slice(&domain_to_wire_format(next_domain));
                rdata.extend_from_slice(&types_to_bitmap(types));
                rdata
            }
            DnsRecordValue::DS { key_tag, algorithm, digest_type, digest } => {
                let mut rdata = Vec::new();
                rdata.extend_from_slice(&key_tag.to_be_bytes());
                rdata.push(*algorithm);
                rdata.push(*digest_type);
                rdata.extend_from_slice(digest);
                rdata
            }
        }
    }

    /// Generate NSEC chain for authenticated denial of existence
    fn generate_nsec_chain(&self, domain: &str, rrsets: &[RRset]) -> Vec<DnsRecord> {
        // Collect unique owner names and their record types
        let mut owner_types: HashMap<String, Vec<DnsRecordType>> = HashMap::new();

        for rrset in rrsets {
            owner_types
                .entry(rrset.name.clone())
                .or_default()
                .push(rrset.record_type);
        }

        // Sort owner names in canonical order
        let mut names: Vec<String> = owner_types.keys().cloned().collect();
        names.sort_by(|a, b| canonical_compare(a, b));

        if names.is_empty() {
            return Vec::new();
        }

        let mut nsec_records = Vec::new();

        // Create NSEC chain
        for (i, name) in names.iter().enumerate() {
            let next_name = if i + 1 < names.len() {
                names[i + 1].clone()
            } else {
                // Last name points back to first (circular chain)
                names[0].clone()
            };

            let mut types = owner_types.get(name).cloned().unwrap_or_default();
            types.push(DnsRecordType::NSEC); // NSEC always includes itself
            types.push(DnsRecordType::RRSIG); // And RRSIG

            // Deduplicate and sort types
            types.sort_by_key(|t| *t as u16);
            types.dedup();

            // Create NSEC record
            let record_name = if name == domain {
                "@".to_string()
            } else {
                name.strip_suffix(&format!(".{}", domain))
                    .unwrap_or(name)
                    .to_string()
            };

            let nsec = DnsRecord {
                id: generate_record_id(),
                name: record_name,
                record_type: DnsRecordType::NSEC,
                ttl: 300, // NSEC typically has a short TTL
                value: DnsRecordValue::NSEC {
                    next_domain: next_name,
                    types,
                },
                priority: None,
                proxied: false,
            };

            nsec_records.push(nsec);
        }

        debug!("Generated {} NSEC records for zone {}", nsec_records.len(), domain);

        nsec_records
    }

    /// Create a DNSKEY record from a key pair
    fn create_dnskey_record(&self, domain: &str, key: &DnssecKeyPair) -> DnsRecord {
        DnsRecord {
            id: generate_record_id(),
            name: "@".to_string(),
            record_type: DnsRecordType::DNSKEY,
            ttl: 3600, // 1 hour
            value: DnsRecordValue::DNSKEY {
                flags: key.flags,
                protocol: 3, // Always 3 for DNSSEC
                algorithm: key.algorithm,
                public_key: key.public_key.clone(),
            },
            priority: None,
            proxied: false,
        }
    }

    /// Verify that a zone has valid keys for signing
    pub async fn can_sign_zone(&self, domain: &str) -> bool {
        let has_zsk = self.key_manager.get_active_zsk(domain).await.is_some();
        let has_ksk = self.key_manager.get_active_ksk(domain).await.is_some();
        has_zsk && has_ksk
    }
}

/// DNSSEC Re-signing Task Manager
pub struct DnssecResigner {
    /// DNSSEC signer
    signer: Arc<DnssecSigner>,
    /// Zone store
    zone_store: Arc<ZoneStore>,
    /// Signed zones cache
    signed_zones: Arc<RwLock<HashMap<String, SignedZone>>>,
    /// Configuration
    config: DnssecConfig,
}

impl DnssecResigner {
    /// Create a new re-signer
    pub fn new(
        signer: Arc<DnssecSigner>,
        zone_store: Arc<ZoneStore>,
        config: DnssecConfig,
    ) -> Self {
        Self {
            signer,
            zone_store,
            signed_zones: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Get the signed zones cache
    pub fn signed_zones(&self) -> Arc<RwLock<HashMap<String, SignedZone>>> {
        Arc::clone(&self.signed_zones)
    }

    /// Sign all zones initially
    pub async fn sign_all_zones(&self) -> Result<usize, DnssecError> {
        let zones = self.zone_store.list_zones().await;
        let mut signed_count = 0;

        for zone in zones {
            if self.signer.can_sign_zone(&zone.domain).await {
                match self.signer.sign_zone(&zone).await {
                    Ok(signed) => {
                        let mut cache = self.signed_zones.write().await;
                        cache.insert(zone.domain.clone(), signed);
                        signed_count += 1;
                        info!("Signed zone: {}", zone.domain);
                    }
                    Err(e) => {
                        warn!("Failed to sign zone {}: {}", zone.domain, e);
                    }
                }
            } else {
                debug!("Zone {} does not have DNSSEC keys, skipping", zone.domain);
            }
        }

        Ok(signed_count)
    }

    /// Run the background re-signing task
    pub async fn run(&self) {
        let mut interval_timer = interval(self.config.resign_interval);

        loop {
            interval_timer.tick().await;
            self.check_and_resign().await;
        }
    }

    /// Check for expiring signatures and re-sign
    async fn check_and_resign(&self) {
        let zones = {
            let cache = self.signed_zones.read().await;
            cache.clone()
        };

        for (domain, signed_zone) in zones {
            if signed_zone.is_expiring_soon(self.config.resign_before_days) {
                info!("Zone {} signatures expiring soon, re-signing", domain);

                if let Some(zone) = self.zone_store.get_zone(&domain).await {
                    match self.signer.sign_zone(&zone).await {
                        Ok(new_signed) => {
                            let mut cache = self.signed_zones.write().await;
                            cache.insert(domain.clone(), new_signed);
                            info!("Re-signed zone: {}", domain);
                        }
                        Err(e) => {
                            error!("Failed to re-sign zone {}: {}", domain, e);
                        }
                    }
                }
            }
        }
    }

    /// Get a signed zone by domain
    pub async fn get_signed_zone(&self, domain: &str) -> Option<SignedZone> {
        let cache = self.signed_zones.read().await;
        cache.get(domain).cloned()
    }

    /// Force re-sign a specific zone
    pub async fn resign_zone(&self, domain: &str) -> Result<SignedZone, DnssecError> {
        let zone = self.zone_store.get_zone(domain).await
            .ok_or_else(|| DnssecError::ZoneNotFound(domain.to_string()))?;

        let signed = self.signer.sign_zone(&zone).await?;

        let mut cache = self.signed_zones.write().await;
        cache.insert(domain.to_string(), signed.clone());

        Ok(signed)
    }
}

/// Convert domain name to DNS wire format
fn domain_to_wire_format(domain: &str) -> Vec<u8> {
    let mut wire = Vec::new();

    for label in domain.split('.') {
        if label.is_empty() {
            continue;
        }
        // Convert to lowercase for canonical form
        let lower = label.to_lowercase();
        wire.push(lower.len() as u8);
        wire.extend_from_slice(lower.as_bytes());
    }

    wire.push(0); // Root label
    wire
}

/// Count labels in a domain name
fn count_labels(domain: &str) -> u8 {
    domain.split('.').filter(|s| !s.is_empty()).count() as u8
}

/// Canonical comparison of domain names (RFC 4034)
fn canonical_compare(a: &str, b: &str) -> std::cmp::Ordering {
    let a_labels: Vec<&str> = a.split('.').filter(|s| !s.is_empty()).collect();
    let b_labels: Vec<&str> = b.split('.').filter(|s| !s.is_empty()).collect();

    // Compare from the right (most significant) label
    let a_rev: Vec<_> = a_labels.iter().rev().collect();
    let b_rev: Vec<_> = b_labels.iter().rev().collect();

    for (a_label, b_label) in a_rev.iter().zip(b_rev.iter()) {
        let cmp = a_label.to_lowercase().cmp(&b_label.to_lowercase());
        if cmp != std::cmp::Ordering::Equal {
            return cmp;
        }
    }

    a_labels.len().cmp(&b_labels.len())
}

/// Convert record types to NSEC type bitmap
fn types_to_bitmap(types: &[DnsRecordType]) -> Vec<u8> {
    // Simplified bitmap generation
    // Real implementation would follow RFC 4034 Section 4.1.2
    let mut bitmap = Vec::new();

    if types.is_empty() {
        return bitmap;
    }

    // Group types by window (256 types per window)
    let mut windows: HashMap<u8, Vec<u8>> = HashMap::new();

    for rtype in types {
        let type_num = *rtype as u16;
        let window = (type_num / 256) as u8;
        let offset = (type_num % 256) as u8;

        windows.entry(window).or_default().push(offset);
    }

    // Build bitmap for each window
    let mut window_nums: Vec<u8> = windows.keys().cloned().collect();
    window_nums.sort();

    for window_num in window_nums {
        let offsets = windows.get(&window_num).unwrap();

        // Find the highest bit we need
        let max_offset = offsets.iter().max().copied().unwrap_or(0);
        let bitmap_len = (max_offset / 8) + 1;

        let mut window_bitmap = vec![0u8; bitmap_len as usize];

        for offset in offsets {
            let byte_idx = (offset / 8) as usize;
            let bit_idx = 7 - (offset % 8);
            window_bitmap[byte_idx] |= 1 << bit_idx;
        }

        bitmap.push(window_num);
        bitmap.push(bitmap_len);
        bitmap.extend(window_bitmap);
    }

    bitmap
}

/// Generate a unique record ID
fn generate_record_id() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 8] = rng.gen();
    format!("rec_{}", hex::encode(bytes))
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// DNSSEC errors
#[derive(Debug, Clone)]
pub enum DnssecError {
    NoActiveKey(String),
    SigningFailed(String),
    ZoneNotFound(String),
    InvalidConfiguration(String),
}

impl std::fmt::Display for DnssecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DnssecError::NoActiveKey(msg) => write!(f, "No active key: {}", msg),
            DnssecError::SigningFailed(msg) => write!(f, "Signing failed: {}", msg),
            DnssecError::ZoneNotFound(domain) => write!(f, "Zone not found: {}", domain),
            DnssecError::InvalidConfiguration(msg) => write!(f, "Invalid configuration: {}", msg),
        }
    }
}

impl std::error::Error for DnssecError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dns::dnssec_keys::{DnssecAlgorithm, KeyFlags};
    use std::net::Ipv4Addr;

    fn create_test_zone() -> Zone {
        Zone {
            domain: "example.com".to_string(),
            records: vec![
                DnsRecord::a("@", Ipv4Addr::new(192, 168, 1, 1), 300),
                DnsRecord::a("www", Ipv4Addr::new(192, 168, 1, 2), 300),
                DnsRecord::ns("@", "ns1.example.com", 3600),
                DnsRecord::ns("@", "ns2.example.com", 3600),
                DnsRecord::mx("@", "mail.example.com", 10, 300),
            ],
            proxied: false,
            dnssec_enabled: false,
            created_at: current_timestamp(),
            updated_at: current_timestamp(),
        }
    }

    #[test]
    fn test_domain_to_wire_format() {
        let wire = domain_to_wire_format("example.com");

        assert_eq!(wire[0], 7); // Length of "example"
        assert_eq!(&wire[1..8], b"example");
        assert_eq!(wire[8], 3); // Length of "com"
        assert_eq!(&wire[9..12], b"com");
        assert_eq!(wire[12], 0); // Root label
    }

    #[test]
    fn test_domain_to_wire_format_lowercase() {
        let wire = domain_to_wire_format("EXAMPLE.COM");

        // Should be lowercase
        assert_eq!(&wire[1..8], b"example");
        assert_eq!(&wire[9..12], b"com");
    }

    #[test]
    fn test_count_labels() {
        assert_eq!(count_labels("example.com"), 2);
        assert_eq!(count_labels("www.example.com"), 3);
        assert_eq!(count_labels("a.b.c.d.example.com"), 6);
        assert_eq!(count_labels("com"), 1);
    }

    #[test]
    fn test_canonical_compare() {
        assert_eq!(canonical_compare("a.example.com", "b.example.com"), std::cmp::Ordering::Less);
        assert_eq!(canonical_compare("z.example.com", "a.example.com"), std::cmp::Ordering::Greater);
        assert_eq!(canonical_compare("example.com", "example.com"), std::cmp::Ordering::Equal);

        // Different TLDs
        assert_eq!(canonical_compare("a.example.com", "a.example.org"), std::cmp::Ordering::Less);

        // Subdomain vs root
        assert_eq!(canonical_compare("www.example.com", "example.com"), std::cmp::Ordering::Greater);
    }

    #[test]
    fn test_types_to_bitmap() {
        let types = vec![DnsRecordType::A, DnsRecordType::NS, DnsRecordType::MX];
        let bitmap = types_to_bitmap(&types);

        // Should have window 0
        assert!(!bitmap.is_empty());
        assert_eq!(bitmap[0], 0); // Window 0
    }

    #[test]
    fn test_dnssec_config_default() {
        let config = DnssecConfig::default();

        assert_eq!(config.signature_validity.as_secs(), 86400 * 30);
        assert_eq!(config.inception_offset.as_secs(), 3600);
        assert_eq!(config.resign_before_days, 7);
    }

    #[test]
    fn test_rrset_label_count() {
        let rrset = RRset {
            name: "www.example.com".to_string(),
            record_type: DnsRecordType::A,
            ttl: 300,
            records: vec![],
        };

        assert_eq!(rrset.label_count(), 3);
    }

    #[tokio::test]
    async fn test_signer_can_sign_zone() {
        let key_manager = Arc::new(DnssecKeyManager::new());
        let config = DnssecConfig::default();
        let signer = DnssecSigner::new(key_manager.clone(), config);

        // Without keys, should not be able to sign
        assert!(!signer.can_sign_zone("example.com").await);

        // Generate keys
        key_manager.generate_key("example.com", KeyFlags::Zsk, DnssecAlgorithm::Ed25519)
            .await.unwrap();
        key_manager.generate_key("example.com", KeyFlags::Ksk, DnssecAlgorithm::Ed25519)
            .await.unwrap();

        // Now should be able to sign
        assert!(signer.can_sign_zone("example.com").await);
    }

    #[tokio::test]
    async fn test_sign_zone() {
        let key_manager = Arc::new(DnssecKeyManager::new());
        let config = DnssecConfig::default();
        let signer = DnssecSigner::new(key_manager.clone(), config);

        // Generate keys
        key_manager.generate_key("example.com", KeyFlags::Zsk, DnssecAlgorithm::Ed25519)
            .await.unwrap();
        key_manager.generate_key("example.com", KeyFlags::Ksk, DnssecAlgorithm::Ed25519)
            .await.unwrap();

        let zone = create_test_zone();
        let signed = signer.sign_zone(&zone).await;

        assert!(signed.is_ok());
        let signed = signed.unwrap();

        assert_eq!(signed.domain, "example.com");
        assert!(!signed.records.is_empty());

        // Should have RRSIG records
        let rrsigs = signed.get_rrsigs();
        assert!(!rrsigs.is_empty());

        // Should have DNSKEY records
        let dnskeys = signed.get_dnskey_records();
        assert_eq!(dnskeys.len(), 2); // ZSK + KSK

        // Should have NSEC records
        let nsec_records = signed.get_nsec_records();
        assert!(!nsec_records.is_empty());
    }

    #[tokio::test]
    async fn test_sign_zone_no_keys() {
        let key_manager = Arc::new(DnssecKeyManager::new());
        let config = DnssecConfig::default();
        let signer = DnssecSigner::new(key_manager, config);

        let zone = create_test_zone();
        let result = signer.sign_zone(&zone).await;

        assert!(result.is_err());
    }

    #[test]
    fn test_signed_zone_is_expiring_soon() {
        let signed = SignedZone {
            domain: "example.com".to_string(),
            records: vec![],
            signed_at: current_timestamp(),
            expires_at: current_timestamp() + 86400 * 5, // Expires in 5 days
            key_tags: vec![12345],
        };

        // Should be expiring soon with 7-day threshold
        assert!(signed.is_expiring_soon(7));

        // Should not be expiring with 3-day threshold
        assert!(!signed.is_expiring_soon(3));
    }

    #[tokio::test]
    async fn test_group_into_rrsets() {
        let key_manager = Arc::new(DnssecKeyManager::new());
        let config = DnssecConfig::default();
        let signer = DnssecSigner::new(key_manager, config);

        let records = vec![
            DnsRecord::a("www", Ipv4Addr::new(192, 168, 1, 1), 300),
            DnsRecord::a("www", Ipv4Addr::new(192, 168, 1, 2), 300),
            DnsRecord::ns("@", "ns1.example.com", 3600),
            DnsRecord::ns("@", "ns2.example.com", 3600),
        ];

        let rrsets = signer.group_into_rrsets("example.com", &records);

        // Should have 2 RRsets: A and NS
        assert_eq!(rrsets.len(), 2);
    }

    #[tokio::test]
    async fn test_generate_nsec_chain() {
        let key_manager = Arc::new(DnssecKeyManager::new());
        let config = DnssecConfig::default();
        let signer = DnssecSigner::new(key_manager, config);

        let rrsets = vec![
            RRset {
                name: "example.com".to_string(),
                record_type: DnsRecordType::A,
                ttl: 300,
                records: vec![],
            },
            RRset {
                name: "www.example.com".to_string(),
                record_type: DnsRecordType::A,
                ttl: 300,
                records: vec![],
            },
        ];

        let nsec_chain = signer.generate_nsec_chain("example.com", &rrsets);

        // Should have 2 NSEC records (one for each name)
        assert_eq!(nsec_chain.len(), 2);

        for nsec in &nsec_chain {
            assert_eq!(nsec.record_type, DnsRecordType::NSEC);
        }
    }

    #[tokio::test]
    async fn test_create_dnskey_record() {
        let key_manager = Arc::new(DnssecKeyManager::new());
        let key = key_manager.generate_key("example.com", KeyFlags::Zsk, DnssecAlgorithm::Ed25519)
            .await.unwrap();

        let config = DnssecConfig::default();
        let signer = DnssecSigner::new(key_manager, config);

        let dnskey = signer.create_dnskey_record("example.com", &key);

        assert_eq!(dnskey.record_type, DnsRecordType::DNSKEY);
        assert_eq!(dnskey.name, "@");

        match dnskey.value {
            DnsRecordValue::DNSKEY { flags, protocol, algorithm, public_key } => {
                assert_eq!(flags, 256); // ZSK
                assert_eq!(protocol, 3);
                assert_eq!(algorithm, 15); // Ed25519
                assert!(!public_key.is_empty());
            }
            _ => panic!("Expected DNSKEY value"),
        }
    }

    #[test]
    fn test_dnssec_error_display() {
        let err = DnssecError::NoActiveKey("test".to_string());
        assert!(err.to_string().contains("No active key"));

        let err = DnssecError::SigningFailed("test".to_string());
        assert!(err.to_string().contains("Signing failed"));

        let err = DnssecError::ZoneNotFound("example.com".to_string());
        assert!(err.to_string().contains("Zone not found"));
    }
}
