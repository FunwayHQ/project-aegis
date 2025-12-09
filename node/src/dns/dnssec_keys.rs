//! DNSSEC Key Management
//!
//! Sprint 30.4: DNSSEC Implementation
//!
//! Manages DNSSEC key pairs for zones, supporting key generation,
//! storage, and DS record generation for registrar submission.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// DNSSEC algorithm identifiers (RFC 8624)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DnssecAlgorithm {
    /// RSA/SHA-256 (Algorithm 8)
    RsaSha256 = 8,
    /// ECDSA P-256 with SHA-256 (Algorithm 13)
    EcdsaP256Sha256 = 13,
    /// Ed25519 (Algorithm 15) - Recommended
    Ed25519 = 15,
}

impl DnssecAlgorithm {
    /// Get the algorithm number
    pub fn number(&self) -> u8 {
        *self as u8
    }

    /// Get algorithm name
    pub fn name(&self) -> &'static str {
        match self {
            DnssecAlgorithm::RsaSha256 => "RSASHA256",
            DnssecAlgorithm::EcdsaP256Sha256 => "ECDSAP256SHA256",
            DnssecAlgorithm::Ed25519 => "ED25519",
        }
    }

    /// Parse from algorithm number
    pub fn from_number(num: u8) -> Option<Self> {
        match num {
            8 => Some(DnssecAlgorithm::RsaSha256),
            13 => Some(DnssecAlgorithm::EcdsaP256Sha256),
            15 => Some(DnssecAlgorithm::Ed25519),
            _ => None,
        }
    }
}

impl Default for DnssecAlgorithm {
    fn default() -> Self {
        DnssecAlgorithm::Ed25519
    }
}

/// DNSSEC digest type for DS records
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DigestType {
    /// SHA-1 (Digest Type 1) - Deprecated but still used
    Sha1 = 1,
    /// SHA-256 (Digest Type 2) - Recommended
    Sha256 = 2,
    /// SHA-384 (Digest Type 4)
    Sha384 = 4,
}

impl Default for DigestType {
    fn default() -> Self {
        DigestType::Sha256
    }
}

/// Key flags for DNSKEY records
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyFlags {
    /// Zone Signing Key (256)
    Zsk = 256,
    /// Key Signing Key (257)
    Ksk = 257,
}

impl KeyFlags {
    pub fn value(&self) -> u16 {
        *self as u16
    }
}

/// A DNSSEC key pair
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnssecKeyPair {
    /// Domain this key is for
    pub domain: String,
    /// Key tag (identifier)
    pub key_tag: u16,
    /// Algorithm identifier
    pub algorithm: u8,
    /// Key flags (256 for ZSK, 257 for KSK)
    pub flags: u16,
    /// Public key bytes
    pub public_key: Vec<u8>,
    /// Private key bytes (serialized Ed25519 PKCS8)
    #[serde(skip_serializing_if = "Option::is_none")]
    private_key: Option<Vec<u8>>,
    /// Creation timestamp
    pub created_at: u64,
    /// Key status (active, inactive, revoked)
    pub status: KeyStatus,
}

/// Key status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyStatus {
    Active,
    Inactive,
    Revoked,
}

impl Default for KeyStatus {
    fn default() -> Self {
        KeyStatus::Active
    }
}

impl DnssecKeyPair {
    /// Create a new key pair for a domain
    pub fn generate(domain: &str, flags: KeyFlags, algorithm: DnssecAlgorithm) -> Result<Self, DnssecKeyError> {
        match algorithm {
            DnssecAlgorithm::Ed25519 => Self::generate_ed25519(domain, flags),
            _ => Err(DnssecKeyError::UnsupportedAlgorithm(algorithm.number())),
        }
    }

    /// Generate Ed25519 key pair
    fn generate_ed25519(domain: &str, flags: KeyFlags) -> Result<Self, DnssecKeyError> {
        let rng = SystemRandom::new();

        // Generate Ed25519 key pair using ring
        let pkcs8_bytes = Ed25519KeyPair::generate_pkcs8(&rng)
            .map_err(|_| DnssecKeyError::KeyGenerationFailed)?;

        let key_pair = Ed25519KeyPair::from_pkcs8(pkcs8_bytes.as_ref())
            .map_err(|_| DnssecKeyError::KeyGenerationFailed)?;

        let public_key = key_pair.public_key().as_ref().to_vec();
        let key_tag = calculate_key_tag(flags.value(), DnssecAlgorithm::Ed25519.number(), &public_key);

        Ok(Self {
            domain: domain.to_string(),
            key_tag,
            algorithm: DnssecAlgorithm::Ed25519.number(),
            flags: flags.value(),
            public_key,
            private_key: Some(pkcs8_bytes.as_ref().to_vec()),
            created_at: current_timestamp(),
            status: KeyStatus::Active,
        })
    }

    /// Check if this is a Key Signing Key (KSK)
    pub fn is_ksk(&self) -> bool {
        self.flags == KeyFlags::Ksk.value()
    }

    /// Check if this is a Zone Signing Key (ZSK)
    pub fn is_zsk(&self) -> bool {
        self.flags == KeyFlags::Zsk.value()
    }

    /// Get the Ed25519 key pair for signing (if available)
    pub fn get_signing_key(&self) -> Result<Ed25519KeyPair, DnssecKeyError> {
        let private_key = self.private_key.as_ref()
            .ok_or(DnssecKeyError::PrivateKeyNotAvailable)?;

        Ed25519KeyPair::from_pkcs8(private_key)
            .map_err(|_| DnssecKeyError::InvalidKeyData)
    }

    /// Sign data with this key
    pub fn sign(&self, data: &[u8]) -> Result<Vec<u8>, DnssecKeyError> {
        let key_pair = self.get_signing_key()?;
        Ok(key_pair.sign(data).as_ref().to_vec())
    }

    /// Generate DS record for this key
    pub fn generate_ds_record(&self, digest_type: DigestType) -> DsRecord {
        let mut hasher = match digest_type {
            DigestType::Sha256 => {
                let mut h = Sha256::new();

                // DS digest = SHA-256(FQDN || DNSKEY RDATA)
                // FQDN in wire format
                let wire_name = domain_to_wire_format(&self.domain);
                h.update(&wire_name);

                // DNSKEY RDATA: flags (2) + protocol (1) + algorithm (1) + public key
                h.update(&self.flags.to_be_bytes());
                h.update(&[3u8]); // Protocol is always 3
                h.update(&[self.algorithm]);
                h.update(&self.public_key);

                h
            }
            _ => {
                // For other digest types, use SHA-256 as fallback
                let mut h = Sha256::new();
                let wire_name = domain_to_wire_format(&self.domain);
                h.update(&wire_name);
                h.update(&self.flags.to_be_bytes());
                h.update(&[3u8]);
                h.update(&[self.algorithm]);
                h.update(&self.public_key);
                h
            }
        };

        let digest = hasher.finalize().to_vec();

        DsRecord {
            key_tag: self.key_tag,
            algorithm: self.algorithm,
            digest_type: digest_type as u8,
            digest,
        }
    }

    /// Export public key only (for sharing)
    pub fn public_only(&self) -> Self {
        Self {
            domain: self.domain.clone(),
            key_tag: self.key_tag,
            algorithm: self.algorithm,
            flags: self.flags,
            public_key: self.public_key.clone(),
            private_key: None,
            created_at: self.created_at,
            status: self.status,
        }
    }

    /// Get DNSKEY RDATA
    pub fn to_dnskey_rdata(&self) -> Vec<u8> {
        let mut rdata = Vec::with_capacity(4 + self.public_key.len());
        rdata.extend_from_slice(&self.flags.to_be_bytes());
        rdata.push(3); // Protocol is always 3
        rdata.push(self.algorithm);
        rdata.extend_from_slice(&self.public_key);
        rdata
    }
}

/// DS (Delegation Signer) record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DsRecord {
    pub key_tag: u16,
    pub algorithm: u8,
    pub digest_type: u8,
    pub digest: Vec<u8>,
}

impl DsRecord {
    /// Format as zone file record
    pub fn to_zone_format(&self, domain: &str) -> String {
        format!(
            "{} IN DS {} {} {} {}",
            domain,
            self.key_tag,
            self.algorithm,
            self.digest_type,
            hex::encode(&self.digest).to_uppercase()
        )
    }

    /// Format for registrar submission
    pub fn to_registrar_format(&self) -> String {
        format!(
            "Key Tag: {}\nAlgorithm: {}\nDigest Type: {}\nDigest: {}",
            self.key_tag,
            self.algorithm,
            self.digest_type,
            hex::encode(&self.digest).to_uppercase()
        )
    }
}

/// DNSSEC Key Manager
pub struct DnssecKeyManager {
    /// Keys indexed by domain
    keys: Arc<RwLock<HashMap<String, Vec<DnssecKeyPair>>>>,
    /// Storage path for key persistence
    storage_path: Option<String>,
}

impl DnssecKeyManager {
    /// Create a new key manager
    pub fn new() -> Self {
        Self {
            keys: Arc::new(RwLock::new(HashMap::new())),
            storage_path: None,
        }
    }

    /// Create with storage path for persistence
    pub fn with_storage(path: &str) -> Self {
        Self {
            keys: Arc::new(RwLock::new(HashMap::new())),
            storage_path: Some(path.to_string()),
        }
    }

    /// Generate a new key pair for a domain
    pub async fn generate_key(
        &self,
        domain: &str,
        flags: KeyFlags,
        algorithm: DnssecAlgorithm,
    ) -> Result<DnssecKeyPair, DnssecKeyError> {
        let key = DnssecKeyPair::generate(domain, flags, algorithm)?;

        let mut keys = self.keys.write().await;
        keys.entry(domain.to_string())
            .or_insert_with(Vec::new)
            .push(key.clone());

        info!("Generated {} key for domain {} (tag: {})",
              if key.is_ksk() { "KSK" } else { "ZSK" },
              domain,
              key.key_tag);

        // Persist if storage path is configured
        if let Some(ref path) = self.storage_path {
            self.save_keys_to_file(domain, path).await?;
        }

        Ok(key)
    }

    /// Get all keys for a domain
    pub async fn get_keys(&self, domain: &str) -> Vec<DnssecKeyPair> {
        let keys = self.keys.read().await;
        keys.get(domain).cloned().unwrap_or_default()
    }

    /// Get active ZSK for a domain
    pub async fn get_active_zsk(&self, domain: &str) -> Option<DnssecKeyPair> {
        let keys = self.keys.read().await;
        keys.get(domain)?
            .iter()
            .find(|k| k.is_zsk() && k.status == KeyStatus::Active)
            .cloned()
    }

    /// Get active KSK for a domain
    pub async fn get_active_ksk(&self, domain: &str) -> Option<DnssecKeyPair> {
        let keys = self.keys.read().await;
        keys.get(domain)?
            .iter()
            .find(|k| k.is_ksk() && k.status == KeyStatus::Active)
            .cloned()
    }

    /// Get key by tag
    pub async fn get_key_by_tag(&self, domain: &str, key_tag: u16) -> Option<DnssecKeyPair> {
        let keys = self.keys.read().await;
        keys.get(domain)?
            .iter()
            .find(|k| k.key_tag == key_tag)
            .cloned()
    }

    /// Update key status
    pub async fn update_key_status(
        &self,
        domain: &str,
        key_tag: u16,
        status: KeyStatus,
    ) -> Result<(), DnssecKeyError> {
        let mut keys = self.keys.write().await;
        let domain_keys = keys.get_mut(domain)
            .ok_or(DnssecKeyError::KeyNotFound)?;

        let key = domain_keys.iter_mut()
            .find(|k| k.key_tag == key_tag)
            .ok_or(DnssecKeyError::KeyNotFound)?;

        key.status = status;

        info!("Updated key {} status to {:?} for domain {}", key_tag, status, domain);

        // Persist if storage path is configured
        drop(keys);
        if let Some(ref path) = self.storage_path {
            self.save_keys_to_file(domain, path).await?;
        }

        Ok(())
    }

    /// Remove a key
    pub async fn remove_key(&self, domain: &str, key_tag: u16) -> Result<(), DnssecKeyError> {
        let mut keys = self.keys.write().await;
        let domain_keys = keys.get_mut(domain)
            .ok_or(DnssecKeyError::KeyNotFound)?;

        let initial_len = domain_keys.len();
        domain_keys.retain(|k| k.key_tag != key_tag);

        if domain_keys.len() == initial_len {
            return Err(DnssecKeyError::KeyNotFound);
        }

        info!("Removed key {} from domain {}", key_tag, domain);

        // Persist if storage path is configured
        drop(keys);
        if let Some(ref path) = self.storage_path {
            self.save_keys_to_file(domain, path).await?;
        }

        Ok(())
    }

    /// Generate DS record for domain's KSK
    pub async fn get_ds_record(&self, domain: &str) -> Option<DsRecord> {
        let ksk = self.get_active_ksk(domain).await?;
        Some(ksk.generate_ds_record(DigestType::Sha256))
    }

    /// Save keys for a domain to file
    async fn save_keys_to_file(&self, domain: &str, base_path: &str) -> Result<(), DnssecKeyError> {
        let keys = self.keys.read().await;
        if let Some(domain_keys) = keys.get(domain) {
            let path = format!("{}/{}.keys.json", base_path, domain.replace('.', "_"));
            let json = serde_json::to_string_pretty(domain_keys)
                .map_err(|e| DnssecKeyError::StorageError(e.to_string()))?;
            fs::write(&path, json)
                .map_err(|e| DnssecKeyError::StorageError(e.to_string()))?;
            debug!("Saved keys for {} to {}", domain, path);
        }
        Ok(())
    }

    /// Load keys for a domain from file
    pub async fn load_keys_from_file(&self, domain: &str, base_path: &str) -> Result<(), DnssecKeyError> {
        let path = format!("{}/{}.keys.json", base_path, domain.replace('.', "_"));

        if !Path::new(&path).exists() {
            return Ok(());
        }

        let json = fs::read_to_string(&path)
            .map_err(|e| DnssecKeyError::StorageError(e.to_string()))?;
        let loaded_keys: Vec<DnssecKeyPair> = serde_json::from_str(&json)
            .map_err(|e| DnssecKeyError::StorageError(e.to_string()))?;

        let mut keys = self.keys.write().await;
        keys.insert(domain.to_string(), loaded_keys);

        info!("Loaded keys for {} from {}", domain, path);
        Ok(())
    }

    /// Check if domain has DNSSEC enabled (has active keys)
    pub async fn is_enabled(&self, domain: &str) -> bool {
        let keys = self.keys.read().await;
        keys.get(domain)
            .map(|k| k.iter().any(|key| key.status == KeyStatus::Active))
            .unwrap_or(false)
    }

    /// List all domains with keys
    pub async fn list_domains(&self) -> Vec<String> {
        let keys = self.keys.read().await;
        keys.keys().cloned().collect()
    }
}

impl Default for DnssecKeyManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Calculate DNSKEY key tag per RFC 4034
pub fn calculate_key_tag(flags: u16, algorithm: u8, public_key: &[u8]) -> u16 {
    let mut ac: u32 = 0;

    // DNSKEY RDATA format: flags (2) + protocol (1) + algorithm (1) + public key
    let mut rdata = Vec::with_capacity(4 + public_key.len());
    rdata.extend_from_slice(&flags.to_be_bytes());
    rdata.push(3); // Protocol is always 3
    rdata.push(algorithm);
    rdata.extend_from_slice(public_key);

    for (i, byte) in rdata.iter().enumerate() {
        if i % 2 == 0 {
            ac += (*byte as u32) << 8;
        } else {
            ac += *byte as u32;
        }
    }

    ac += (ac >> 16) & 0xFFFF;
    (ac & 0xFFFF) as u16
}

/// Convert domain name to wire format
fn domain_to_wire_format(domain: &str) -> Vec<u8> {
    let mut wire = Vec::new();

    for label in domain.split('.') {
        if label.is_empty() {
            continue;
        }
        wire.push(label.len() as u8);
        wire.extend_from_slice(label.as_bytes());
    }

    wire.push(0); // Root label
    wire
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// DNSSEC Key errors
#[derive(Debug, Clone)]
pub enum DnssecKeyError {
    KeyGenerationFailed,
    InvalidKeyData,
    PrivateKeyNotAvailable,
    KeyNotFound,
    UnsupportedAlgorithm(u8),
    StorageError(String),
}

impl std::fmt::Display for DnssecKeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DnssecKeyError::KeyGenerationFailed => write!(f, "Key generation failed"),
            DnssecKeyError::InvalidKeyData => write!(f, "Invalid key data"),
            DnssecKeyError::PrivateKeyNotAvailable => write!(f, "Private key not available"),
            DnssecKeyError::KeyNotFound => write!(f, "Key not found"),
            DnssecKeyError::UnsupportedAlgorithm(alg) => write!(f, "Unsupported algorithm: {}", alg),
            DnssecKeyError::StorageError(msg) => write!(f, "Storage error: {}", msg),
        }
    }
}

impl std::error::Error for DnssecKeyError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dnssec_algorithm_number() {
        assert_eq!(DnssecAlgorithm::RsaSha256.number(), 8);
        assert_eq!(DnssecAlgorithm::EcdsaP256Sha256.number(), 13);
        assert_eq!(DnssecAlgorithm::Ed25519.number(), 15);
    }

    #[test]
    fn test_dnssec_algorithm_from_number() {
        assert_eq!(DnssecAlgorithm::from_number(8), Some(DnssecAlgorithm::RsaSha256));
        assert_eq!(DnssecAlgorithm::from_number(15), Some(DnssecAlgorithm::Ed25519));
        assert_eq!(DnssecAlgorithm::from_number(99), None);
    }

    #[test]
    fn test_key_flags() {
        assert_eq!(KeyFlags::Zsk.value(), 256);
        assert_eq!(KeyFlags::Ksk.value(), 257);
    }

    #[test]
    fn test_generate_ed25519_key() {
        let key = DnssecKeyPair::generate("example.com", KeyFlags::Zsk, DnssecAlgorithm::Ed25519);
        assert!(key.is_ok());

        let key = key.unwrap();
        assert_eq!(key.domain, "example.com");
        assert_eq!(key.algorithm, 15);
        assert_eq!(key.flags, 256);
        assert!(key.is_zsk());
        assert!(!key.is_ksk());
        assert!(!key.public_key.is_empty());
        assert!(key.private_key.is_some());
    }

    #[test]
    fn test_generate_ksk() {
        let key = DnssecKeyPair::generate("example.com", KeyFlags::Ksk, DnssecAlgorithm::Ed25519).unwrap();

        assert_eq!(key.flags, 257);
        assert!(key.is_ksk());
        assert!(!key.is_zsk());
    }

    #[test]
    fn test_key_tag_calculation() {
        // Test that key tag is calculated correctly
        let key = DnssecKeyPair::generate("example.com", KeyFlags::Zsk, DnssecAlgorithm::Ed25519).unwrap();

        // Key tag should be non-zero and within u16 range
        assert!(key.key_tag > 0);

        // Recalculate and verify
        let recalculated = calculate_key_tag(key.flags, key.algorithm, &key.public_key);
        assert_eq!(key.key_tag, recalculated);
    }

    #[test]
    fn test_sign_data() {
        let key = DnssecKeyPair::generate("example.com", KeyFlags::Zsk, DnssecAlgorithm::Ed25519).unwrap();

        let data = b"test data to sign";
        let signature = key.sign(data);

        assert!(signature.is_ok());
        let sig = signature.unwrap();
        assert!(!sig.is_empty());
        assert_eq!(sig.len(), 64); // Ed25519 signature is 64 bytes
    }

    #[test]
    fn test_generate_ds_record() {
        let key = DnssecKeyPair::generate("example.com", KeyFlags::Ksk, DnssecAlgorithm::Ed25519).unwrap();

        let ds = key.generate_ds_record(DigestType::Sha256);

        assert_eq!(ds.key_tag, key.key_tag);
        assert_eq!(ds.algorithm, key.algorithm);
        assert_eq!(ds.digest_type, 2); // SHA-256
        assert_eq!(ds.digest.len(), 32); // SHA-256 is 32 bytes
    }

    #[test]
    fn test_ds_record_format() {
        let key = DnssecKeyPair::generate("example.com", KeyFlags::Ksk, DnssecAlgorithm::Ed25519).unwrap();
        let ds = key.generate_ds_record(DigestType::Sha256);

        let zone_format = ds.to_zone_format("example.com");
        assert!(zone_format.contains("example.com"));
        assert!(zone_format.contains("IN DS"));

        let registrar_format = ds.to_registrar_format();
        assert!(registrar_format.contains("Key Tag:"));
        assert!(registrar_format.contains("Algorithm:"));
    }

    #[test]
    fn test_public_only() {
        let key = DnssecKeyPair::generate("example.com", KeyFlags::Zsk, DnssecAlgorithm::Ed25519).unwrap();
        assert!(key.private_key.is_some());

        let public_only = key.public_only();
        assert!(public_only.private_key.is_none());
        assert_eq!(public_only.public_key, key.public_key);
        assert_eq!(public_only.key_tag, key.key_tag);
    }

    #[test]
    fn test_dnskey_rdata() {
        let key = DnssecKeyPair::generate("example.com", KeyFlags::Zsk, DnssecAlgorithm::Ed25519).unwrap();

        let rdata = key.to_dnskey_rdata();

        // Should be: flags (2) + protocol (1) + algorithm (1) + public key
        assert_eq!(rdata.len(), 4 + key.public_key.len());

        // Check flags
        let flags = u16::from_be_bytes([rdata[0], rdata[1]]);
        assert_eq!(flags, 256);

        // Check protocol (always 3)
        assert_eq!(rdata[2], 3);

        // Check algorithm
        assert_eq!(rdata[3], 15);
    }

    #[test]
    fn test_domain_to_wire_format() {
        let wire = domain_to_wire_format("example.com");

        // Should be: 7 "example" 3 "com" 0
        assert_eq!(wire[0], 7); // Length of "example"
        assert_eq!(&wire[1..8], b"example");
        assert_eq!(wire[8], 3); // Length of "com"
        assert_eq!(&wire[9..12], b"com");
        assert_eq!(wire[12], 0); // Root label
    }

    #[tokio::test]
    async fn test_key_manager_generate() {
        let manager = DnssecKeyManager::new();

        let key = manager.generate_key("example.com", KeyFlags::Zsk, DnssecAlgorithm::Ed25519).await;
        assert!(key.is_ok());

        let keys = manager.get_keys("example.com").await;
        assert_eq!(keys.len(), 1);
    }

    #[tokio::test]
    async fn test_key_manager_get_active_keys() {
        let manager = DnssecKeyManager::new();

        // Generate ZSK and KSK
        manager.generate_key("example.com", KeyFlags::Zsk, DnssecAlgorithm::Ed25519).await.unwrap();
        manager.generate_key("example.com", KeyFlags::Ksk, DnssecAlgorithm::Ed25519).await.unwrap();

        let zsk = manager.get_active_zsk("example.com").await;
        assert!(zsk.is_some());
        assert!(zsk.unwrap().is_zsk());

        let ksk = manager.get_active_ksk("example.com").await;
        assert!(ksk.is_some());
        assert!(ksk.unwrap().is_ksk());
    }

    #[tokio::test]
    async fn test_key_manager_update_status() {
        let manager = DnssecKeyManager::new();

        let key = manager.generate_key("example.com", KeyFlags::Zsk, DnssecAlgorithm::Ed25519).await.unwrap();

        // Update to inactive
        manager.update_key_status("example.com", key.key_tag, KeyStatus::Inactive).await.unwrap();

        // Should no longer be returned as active
        let active = manager.get_active_zsk("example.com").await;
        assert!(active.is_none());
    }

    #[tokio::test]
    async fn test_key_manager_remove_key() {
        let manager = DnssecKeyManager::new();

        let key = manager.generate_key("example.com", KeyFlags::Zsk, DnssecAlgorithm::Ed25519).await.unwrap();

        // Remove the key
        manager.remove_key("example.com", key.key_tag).await.unwrap();

        let keys = manager.get_keys("example.com").await;
        assert!(keys.is_empty());
    }

    #[tokio::test]
    async fn test_key_manager_get_ds_record() {
        let manager = DnssecKeyManager::new();

        // Generate KSK
        manager.generate_key("example.com", KeyFlags::Ksk, DnssecAlgorithm::Ed25519).await.unwrap();

        let ds = manager.get_ds_record("example.com").await;
        assert!(ds.is_some());
    }

    #[tokio::test]
    async fn test_key_manager_is_enabled() {
        let manager = DnssecKeyManager::new();

        assert!(!manager.is_enabled("example.com").await);

        manager.generate_key("example.com", KeyFlags::Zsk, DnssecAlgorithm::Ed25519).await.unwrap();

        assert!(manager.is_enabled("example.com").await);
    }

    #[tokio::test]
    async fn test_key_manager_list_domains() {
        let manager = DnssecKeyManager::new();

        manager.generate_key("example.com", KeyFlags::Zsk, DnssecAlgorithm::Ed25519).await.unwrap();
        manager.generate_key("test.org", KeyFlags::Zsk, DnssecAlgorithm::Ed25519).await.unwrap();

        let domains = manager.list_domains().await;
        assert_eq!(domains.len(), 2);
        assert!(domains.contains(&"example.com".to_string()));
        assert!(domains.contains(&"test.org".to_string()));
    }
}
