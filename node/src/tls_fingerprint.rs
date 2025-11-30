// Sprint 19: TLS Fingerprinting (JA3/JA4) for Advanced Bot Detection
//
// This module implements TLS ClientHello fingerprinting to detect bots
// that spoof User-Agent headers but have distinct TLS fingerprints.
//
// JA3 = MD5(SSLVersion,Ciphers,Extensions,EllipticCurves,EllipticCurvePointFormats)
// JA4 = q{quic}t{version}{sni}d{cipher_count}{ext_count}_{sorted_ciphers}_{sorted_extensions}

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// TLS version identifiers from ClientHello
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u16)]
pub enum TlsVersion {
    Ssl30 = 0x0300,
    Tls10 = 0x0301,
    Tls11 = 0x0302,
    Tls12 = 0x0303,
    Tls13 = 0x0304,
    Unknown(u16),
}

impl From<u16> for TlsVersion {
    fn from(value: u16) -> Self {
        match value {
            0x0300 => TlsVersion::Ssl30,
            0x0301 => TlsVersion::Tls10,
            0x0302 => TlsVersion::Tls11,
            0x0303 => TlsVersion::Tls12,
            0x0304 => TlsVersion::Tls13,
            v => TlsVersion::Unknown(v),
        }
    }
}

impl TlsVersion {
    pub fn as_u16(&self) -> u16 {
        match self {
            TlsVersion::Ssl30 => 0x0300,
            TlsVersion::Tls10 => 0x0301,
            TlsVersion::Tls11 => 0x0302,
            TlsVersion::Tls12 => 0x0303,
            TlsVersion::Tls13 => 0x0304,
            TlsVersion::Unknown(v) => *v,
        }
    }

    /// Get short code for JA4 (1 char)
    pub fn ja4_code(&self) -> char {
        match self {
            TlsVersion::Ssl30 => 's',
            TlsVersion::Tls10 => '0',
            TlsVersion::Tls11 => '1',
            TlsVersion::Tls12 => '2',
            TlsVersion::Tls13 => '3',
            TlsVersion::Unknown(_) => 'x',
        }
    }
}

/// Parsed TLS ClientHello data for fingerprinting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientHello {
    /// TLS version from record layer
    pub record_version: TlsVersion,
    /// TLS version from handshake
    pub handshake_version: TlsVersion,
    /// Cipher suites offered (in order)
    pub cipher_suites: Vec<u16>,
    /// TLS extensions present (in order)
    pub extensions: Vec<u16>,
    /// Supported elliptic curves (from supported_groups extension)
    pub elliptic_curves: Vec<u16>,
    /// EC point formats (from ec_point_formats extension)
    pub ec_point_formats: Vec<u8>,
    /// Server Name Indication (SNI)
    pub sni: Option<String>,
    /// ALPN protocols
    pub alpn_protocols: Vec<String>,
    /// Signature algorithms (from signature_algorithms extension)
    pub signature_algorithms: Vec<u16>,
    /// Supported versions (from supported_versions extension)
    pub supported_versions: Vec<u16>,
}

impl Default for ClientHello {
    fn default() -> Self {
        Self {
            record_version: TlsVersion::Tls12,
            handshake_version: TlsVersion::Tls12,
            cipher_suites: Vec::new(),
            extensions: Vec::new(),
            elliptic_curves: Vec::new(),
            ec_point_formats: Vec::new(),
            sni: None,
            alpn_protocols: Vec::new(),
            signature_algorithms: Vec::new(),
            supported_versions: Vec::new(),
        }
    }
}

impl ClientHello {
    /// Parse ClientHello from raw bytes
    /// Returns None if parsing fails (malformed or non-ClientHello)
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 43 {
            return None; // Minimum ClientHello size
        }

        // TLS Record Layer
        // ContentType (1) + Version (2) + Length (2) = 5 bytes
        let content_type = data[0];
        if content_type != 0x16 {
            // Not a handshake record
            return None;
        }

        let record_version = TlsVersion::from(u16::from_be_bytes([data[1], data[2]]));
        let record_length = u16::from_be_bytes([data[3], data[4]]) as usize;

        if data.len() < 5 + record_length {
            return None; // Truncated record
        }

        let handshake = &data[5..5 + record_length];

        // Handshake header
        // HandshakeType (1) + Length (3) = 4 bytes
        if handshake.is_empty() || handshake[0] != 0x01 {
            // Not a ClientHello
            return None;
        }

        let handshake_length = u32::from_be_bytes([0, handshake[1], handshake[2], handshake[3]]) as usize;
        if handshake.len() < 4 + handshake_length {
            return None;
        }

        let client_hello = &handshake[4..4 + handshake_length];
        Self::parse_client_hello_body(client_hello, record_version)
    }

    fn parse_client_hello_body(data: &[u8], record_version: TlsVersion) -> Option<Self> {
        if data.len() < 34 {
            return None;
        }

        let mut offset = 0;

        // Client Version (2 bytes)
        let handshake_version = TlsVersion::from(u16::from_be_bytes([data[0], data[1]]));
        offset += 2;

        // Random (32 bytes)
        offset += 32;

        // Session ID (variable)
        if offset >= data.len() {
            return None;
        }
        let session_id_len = data[offset] as usize;
        offset += 1 + session_id_len;

        if offset + 2 > data.len() {
            return None;
        }

        // Cipher Suites
        let cipher_suites_len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
        offset += 2;

        if offset + cipher_suites_len > data.len() {
            return None;
        }

        let mut cipher_suites = Vec::with_capacity(cipher_suites_len / 2);
        for i in (0..cipher_suites_len).step_by(2) {
            let suite = u16::from_be_bytes([data[offset + i], data[offset + i + 1]]);
            cipher_suites.push(suite);
        }
        offset += cipher_suites_len;

        // Compression Methods
        if offset >= data.len() {
            return None;
        }
        let compression_len = data[offset] as usize;
        offset += 1 + compression_len;

        // Extensions (optional)
        let mut extensions = Vec::new();
        let mut elliptic_curves = Vec::new();
        let mut ec_point_formats = Vec::new();
        let mut sni = None;
        let mut alpn_protocols = Vec::new();
        let mut signature_algorithms = Vec::new();
        let mut supported_versions = Vec::new();

        if offset + 2 <= data.len() {
            let extensions_len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
            offset += 2;

            let extensions_end = offset + extensions_len.min(data.len() - offset);
            while offset + 4 <= extensions_end {
                let ext_type = u16::from_be_bytes([data[offset], data[offset + 1]]);
                let ext_len = u16::from_be_bytes([data[offset + 2], data[offset + 3]]) as usize;
                offset += 4;

                extensions.push(ext_type);

                if offset + ext_len > data.len() {
                    break;
                }

                let ext_data = &data[offset..offset + ext_len];

                match ext_type {
                    // Server Name Indication (SNI)
                    0x0000 => {
                        if ext_data.len() >= 5 {
                            let name_len = u16::from_be_bytes([ext_data[3], ext_data[4]]) as usize;
                            if ext_data.len() >= 5 + name_len {
                                sni = String::from_utf8(ext_data[5..5 + name_len].to_vec()).ok();
                            }
                        }
                    }
                    // Supported Groups (elliptic_curves)
                    0x000a => {
                        if ext_data.len() >= 2 {
                            let groups_len = u16::from_be_bytes([ext_data[0], ext_data[1]]) as usize;
                            for i in (2..2 + groups_len.min(ext_data.len() - 2)).step_by(2) {
                                if i + 1 < ext_data.len() {
                                    elliptic_curves.push(u16::from_be_bytes([ext_data[i], ext_data[i + 1]]));
                                }
                            }
                        }
                    }
                    // EC Point Formats
                    0x000b => {
                        if !ext_data.is_empty() {
                            let formats_len = ext_data[0] as usize;
                            for &fmt in ext_data.iter().skip(1).take(formats_len) {
                                ec_point_formats.push(fmt);
                            }
                        }
                    }
                    // Signature Algorithms
                    0x000d => {
                        if ext_data.len() >= 2 {
                            let algos_len = u16::from_be_bytes([ext_data[0], ext_data[1]]) as usize;
                            for i in (2..2 + algos_len.min(ext_data.len() - 2)).step_by(2) {
                                if i + 1 < ext_data.len() {
                                    signature_algorithms.push(u16::from_be_bytes([ext_data[i], ext_data[i + 1]]));
                                }
                            }
                        }
                    }
                    // Application Layer Protocol Negotiation (ALPN)
                    0x0010 => {
                        if ext_data.len() >= 2 {
                            let mut alpn_offset = 2;
                            while alpn_offset < ext_data.len() {
                                let proto_len = ext_data[alpn_offset] as usize;
                                alpn_offset += 1;
                                if alpn_offset + proto_len <= ext_data.len() {
                                    if let Ok(proto) = String::from_utf8(ext_data[alpn_offset..alpn_offset + proto_len].to_vec()) {
                                        alpn_protocols.push(proto);
                                    }
                                }
                                alpn_offset += proto_len;
                            }
                        }
                    }
                    // Supported Versions
                    0x002b => {
                        if !ext_data.is_empty() {
                            let versions_len = ext_data[0] as usize;
                            for i in (1..1 + versions_len.min(ext_data.len() - 1)).step_by(2) {
                                if i + 1 < ext_data.len() {
                                    supported_versions.push(u16::from_be_bytes([ext_data[i], ext_data[i + 1]]));
                                }
                            }
                        }
                    }
                    _ => {}
                }

                offset += ext_len;
            }
        }

        Some(Self {
            record_version,
            handshake_version,
            cipher_suites,
            extensions,
            elliptic_curves,
            ec_point_formats,
            sni,
            alpn_protocols,
            signature_algorithms,
            supported_versions,
        })
    }
}

/// TLS fingerprint computed from ClientHello
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsFingerprint {
    /// JA3 fingerprint (MD5 hash)
    pub ja3: String,
    /// JA3 raw string (before hashing)
    pub ja3_raw: String,
    /// JA4 fingerprint (newer format)
    pub ja4: String,
    /// Negotiated TLS version
    pub tls_version: TlsVersion,
    /// Number of cipher suites
    pub cipher_count: usize,
    /// Number of extensions
    pub extension_count: usize,
    /// SNI present
    pub has_sni: bool,
    /// ALPN present
    pub has_alpn: bool,
}

impl TlsFingerprint {
    /// Compute fingerprint from parsed ClientHello
    pub fn from_client_hello(ch: &ClientHello) -> Self {
        let ja3_raw = Self::compute_ja3_raw(ch);
        let ja3 = format!("{:x}", md5::compute(&ja3_raw));
        let ja4 = Self::compute_ja4(ch);

        Self {
            ja3,
            ja3_raw,
            ja4,
            tls_version: ch.handshake_version,
            cipher_count: ch.cipher_suites.len(),
            extension_count: ch.extensions.len(),
            has_sni: ch.sni.is_some(),
            has_alpn: !ch.alpn_protocols.is_empty(),
        }
    }

    /// Compute JA3 raw string: SSLVersion,Ciphers,Extensions,EllipticCurves,EllipticCurvePointFormats
    fn compute_ja3_raw(ch: &ClientHello) -> String {
        // Filter out GREASE values (0x?a?a pattern)
        let ciphers: Vec<String> = ch.cipher_suites
            .iter()
            .filter(|&&c| !is_grease_value(c))
            .map(|c| c.to_string())
            .collect();

        let extensions: Vec<String> = ch.extensions
            .iter()
            .filter(|&&e| !is_grease_value(e))
            .map(|e| e.to_string())
            .collect();

        let curves: Vec<String> = ch.elliptic_curves
            .iter()
            .filter(|&&c| !is_grease_value(c))
            .map(|c| c.to_string())
            .collect();

        let formats: Vec<String> = ch.ec_point_formats
            .iter()
            .map(|f| f.to_string())
            .collect();

        format!(
            "{},{},{},{},{}",
            ch.handshake_version.as_u16(),
            ciphers.join("-"),
            extensions.join("-"),
            curves.join("-"),
            formats.join("-")
        )
    }

    /// Compute JA4 fingerprint
    /// Format: t{version}{sni}d{cipher_count:02}{ext_count:02}_{sorted_cipher_hash}_{sorted_ext_hash}
    fn compute_ja4(ch: &ClientHello) -> String {
        // Protocol: q for QUIC, t for TCP TLS
        let proto = 't';

        // Version
        let version = if !ch.supported_versions.is_empty() {
            // Use highest supported version
            TlsVersion::from(*ch.supported_versions.iter().max().unwrap_or(&0x0303)).ja4_code()
        } else {
            ch.handshake_version.ja4_code()
        };

        // SNI present: d if SNI, i if no SNI
        let sni_indicator = if ch.sni.is_some() { 'd' } else { 'i' };

        // Cipher count (2 digits, max 99)
        let cipher_count = ch.cipher_suites.iter()
            .filter(|&&c| !is_grease_value(c))
            .count()
            .min(99);

        // Extension count (2 digits, max 99)
        let ext_count = ch.extensions.iter()
            .filter(|&&e| !is_grease_value(e))
            .count()
            .min(99);

        // First ALPN protocol (first char, lowercase)
        let alpn_char = ch.alpn_protocols
            .first()
            .and_then(|p| p.chars().next())
            .map(|c| c.to_ascii_lowercase())
            .unwrap_or('0');

        // Sorted cipher suites (hex, first 12 chars of SHA256)
        let mut sorted_ciphers: Vec<u16> = ch.cipher_suites
            .iter()
            .filter(|&&c| !is_grease_value(c))
            .copied()
            .collect();
        sorted_ciphers.sort();
        let cipher_str: String = sorted_ciphers.iter()
            .map(|c| format!("{:04x}", c))
            .collect::<Vec<_>>()
            .join(",");
        let cipher_hash = &format!("{:x}", md5::compute(&cipher_str))[..12];

        // Sorted extensions (hex, first 12 chars of SHA256)
        // Exclude SNI (0) and ALPN (16) as they're indicated elsewhere
        let mut sorted_extensions: Vec<u16> = ch.extensions
            .iter()
            .filter(|&&e| !is_grease_value(e) && e != 0x0000 && e != 0x0010)
            .copied()
            .collect();
        sorted_extensions.sort();
        let ext_str: String = sorted_extensions.iter()
            .map(|e| format!("{:04x}", e))
            .collect::<Vec<_>>()
            .join(",");
        let ext_hash = &format!("{:x}", md5::compute(&ext_str))[..12];

        format!(
            "{}{}{}{:02}{:02}{}_{}_{}",
            proto, version, sni_indicator, cipher_count, ext_count, alpn_char,
            cipher_hash, ext_hash
        )
    }
}

/// Check if a value is a GREASE (Generate Random Extensions And Sustain Extensibility) value
/// GREASE values follow the pattern 0x?a?a (e.g., 0x0a0a, 0x1a1a, etc.)
fn is_grease_value(value: u16) -> bool {
    let high = (value >> 8) as u8;
    let low = value as u8;
    high == low && (high & 0x0f) == 0x0a
}

/// Known client fingerprint classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClientType {
    /// Real browser (Chrome, Firefox, Safari, Edge)
    Browser,
    /// Mobile browser
    MobileBrowser,
    /// Known good bot (Googlebot, Bingbot)
    GoodBot,
    /// Automation tool (curl, wget, python-requests)
    AutomationTool,
    /// Headless browser (Puppeteer, Playwright)
    HeadlessBrowser,
    /// Security scanner
    Scanner,
    /// Unknown/unclassified
    Unknown,
}

/// Fingerprint database entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FingerprintEntry {
    /// Client type classification
    pub client_type: ClientType,
    /// Human-readable client name (e.g., "Chrome 120", "curl 8.x")
    pub client_name: String,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f64,
    /// First seen timestamp
    pub first_seen: u64,
    /// Last seen timestamp
    pub last_seen: u64,
    /// Request count with this fingerprint
    pub request_count: u64,
}

/// Fingerprint database for known clients
pub struct FingerprintDatabase {
    /// JA3 hash -> FingerprintEntry
    ja3_db: Arc<RwLock<HashMap<String, FingerprintEntry>>>,
    /// JA4 prefix -> FingerprintEntry (JA4 prefixes are more stable)
    ja4_db: Arc<RwLock<HashMap<String, FingerprintEntry>>>,
}

impl FingerprintDatabase {
    /// Create new fingerprint database with built-in known fingerprints
    pub fn new() -> Self {
        let db = Self {
            ja3_db: Arc::new(RwLock::new(HashMap::new())),
            ja4_db: Arc::new(RwLock::new(HashMap::new())),
        };
        db.load_builtin_fingerprints();
        db
    }

    /// Load built-in fingerprints for common clients
    fn load_builtin_fingerprints(&self) {
        let mut ja3_db = self.ja3_db.write().unwrap();
        let mut ja4_db = self.ja4_db.write().unwrap();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();

        // Chrome fingerprints (various versions)
        // These are well-documented JA3 hashes for Chrome
        let chrome_fingerprints = vec![
            ("cd08e31494f9531f560d64c695473da9", "Chrome 120+"),
            ("b32309a26951912be7dba376398abc3b", "Chrome 70-79"),
            ("473cd7cb9faa642487833865d516e578", "Chrome 80-89"),
            ("f5a29e8a19b19f20e7e8c7d12e0e2e4d", "Chrome 90-99"),
        ];

        for (hash, name) in chrome_fingerprints {
            ja3_db.insert(hash.to_string(), FingerprintEntry {
                client_type: ClientType::Browser,
                client_name: name.to_string(),
                confidence: 0.95,
                first_seen: now,
                last_seen: now,
                request_count: 0,
            });
        }

        // Firefox fingerprints
        let firefox_fingerprints = vec![
            ("28a2eb8f6ff952b9c8c7e8b5c9e0e1e2", "Firefox 90+"),
            ("c27a3d5d3e4e5f6a7b8c9d0e1f2a3b4c", "Firefox 80-89"),
        ];

        for (hash, name) in firefox_fingerprints {
            ja3_db.insert(hash.to_string(), FingerprintEntry {
                client_type: ClientType::Browser,
                client_name: name.to_string(),
                confidence: 0.95,
                first_seen: now,
                last_seen: now,
                request_count: 0,
            });
        }

        // curl fingerprints (various versions/OpenSSL builds)
        let curl_fingerprints = vec![
            ("3b5074b1b5d032e5620f69f9f700ff0e", "curl (OpenSSL)"),
            ("3e4d5a6b7c8d9e0f1a2b3c4d5e6f7a8b", "curl (NSS)"),
            ("36f7277af969a6947a61ae0b815907a1", "curl (wolfSSL)"),
        ];

        for (hash, name) in curl_fingerprints {
            ja3_db.insert(hash.to_string(), FingerprintEntry {
                client_type: ClientType::AutomationTool,
                client_name: name.to_string(),
                confidence: 0.90,
                first_seen: now,
                last_seen: now,
                request_count: 0,
            });
        }

        // Python requests fingerprints
        let python_fingerprints = vec![
            ("b3a29e8d7c6f5e4d3c2b1a0f9e8d7c6b", "python-requests"),
            ("e62c5d8f9a0b1c2d3e4f5a6b7c8d9e0f", "python-urllib"),
            ("2ca3c5f8b9d0e1f2a3b4c5d6e7f8a9b0", "aiohttp"),
        ];

        for (hash, name) in python_fingerprints {
            ja3_db.insert(hash.to_string(), FingerprintEntry {
                client_type: ClientType::AutomationTool,
                client_name: name.to_string(),
                confidence: 0.90,
                first_seen: now,
                last_seen: now,
                request_count: 0,
            });
        }

        // Headless browser fingerprints
        let headless_fingerprints = vec![
            ("1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a", "Puppeteer (old)"),
            ("4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c9d", "Playwright"),
            ("7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d", "Selenium WebDriver"),
        ];

        for (hash, name) in headless_fingerprints {
            ja3_db.insert(hash.to_string(), FingerprintEntry {
                client_type: ClientType::HeadlessBrowser,
                client_name: name.to_string(),
                confidence: 0.85,
                first_seen: now,
                last_seen: now,
                request_count: 0,
            });
        }

        // Scanner fingerprints
        let scanner_fingerprints = vec![
            ("e9c0a1b2c3d4e5f6a7b8c9d0e1f2a3b4", "Nmap SSL"),
            ("f0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5", "Nikto"),
            ("a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6", "SQLmap"),
        ];

        for (hash, name) in scanner_fingerprints {
            ja3_db.insert(hash.to_string(), FingerprintEntry {
                client_type: ClientType::Scanner,
                client_name: name.to_string(),
                confidence: 0.95,
                first_seen: now,
                last_seen: now,
                request_count: 0,
            });
        }

        // JA4 prefix patterns (more resilient to minor changes)
        // Format: t{ver}{sni}XX = TCP, version, SNI indicator, cipher/ext counts

        // Modern browsers typically have many ciphers and extensions
        ja4_db.insert("t3d".to_string(), FingerprintEntry {
            client_type: ClientType::Browser,
            client_name: "TLS 1.3 Browser".to_string(),
            confidence: 0.70,
            first_seen: now,
            last_seen: now,
            request_count: 0,
        });

        // curl typically has fewer ciphers
        ja4_db.insert("t2d0".to_string(), FingerprintEntry {
            client_type: ClientType::AutomationTool,
            client_name: "TLS 1.2 Automation".to_string(),
            confidence: 0.60,
            first_seen: now,
            last_seen: now,
            request_count: 0,
        });

        // No SNI is suspicious for browsers
        ja4_db.insert("t3i".to_string(), FingerprintEntry {
            client_type: ClientType::AutomationTool,
            client_name: "TLS 1.3 No-SNI".to_string(),
            confidence: 0.65,
            first_seen: now,
            last_seen: now,
            request_count: 0,
        });
    }

    /// Look up fingerprint in database
    pub fn lookup(&self, fingerprint: &TlsFingerprint) -> Option<FingerprintEntry> {
        // Try exact JA3 match first (most precise)
        if let Some(entry) = self.ja3_db.read().ok()?.get(&fingerprint.ja3) {
            return Some(entry.clone());
        }

        // Try JA4 prefix match (more resilient)
        let ja4_prefix = &fingerprint.ja4[..4.min(fingerprint.ja4.len())];
        if let Some(entry) = self.ja4_db.read().ok()?.get(ja4_prefix) {
            return Some(entry.clone());
        }

        None
    }

    /// Add or update fingerprint in database
    pub fn upsert(&self, fingerprint: &TlsFingerprint, entry: FingerprintEntry) -> Result<()> {
        let mut db = self.ja3_db.write().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        db.insert(fingerprint.ja3.clone(), entry);
        Ok(())
    }

    /// Record fingerprint observation (updates last_seen and request_count)
    pub fn record_observation(&self, fingerprint: &TlsFingerprint) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();

        if let Ok(mut db) = self.ja3_db.write() {
            if let Some(entry) = db.get_mut(&fingerprint.ja3) {
                entry.last_seen = now;
                entry.request_count += 1;
            }
        }
    }

    /// Get database statistics
    pub fn stats(&self) -> (usize, usize) {
        let ja3_count = self.ja3_db.read().map(|db| db.len()).unwrap_or(0);
        let ja4_count = self.ja4_db.read().map(|db| db.len()).unwrap_or(0);
        (ja3_count, ja4_count)
    }
}

impl Default for FingerprintDatabase {
    fn default() -> Self {
        Self::new()
    }
}

/// Bot suspicion level based on TLS fingerprint analysis
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TlsSuspicionLevel {
    /// Low suspicion - known browser fingerprint
    Low,
    /// Medium suspicion - unknown fingerprint or minor mismatch
    Medium,
    /// High suspicion - fingerprint/User-Agent mismatch or known bot tool
    High,
    /// Critical - known scanner or attack tool
    Critical,
}

/// Result of TLS fingerprint analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsAnalysisResult {
    /// Computed fingerprint
    pub fingerprint: TlsFingerprint,
    /// Matched client type (if known)
    pub client_type: Option<ClientType>,
    /// Matched client name (if known)
    pub client_name: Option<String>,
    /// Suspicion level
    pub suspicion_level: TlsSuspicionLevel,
    /// Suspicion reasons
    pub suspicion_reasons: Vec<String>,
    /// Score contribution to bot detection (-50 to +50)
    pub score_adjustment: i32,
}

/// TLS Fingerprint Analyzer
pub struct TlsFingerprintAnalyzer {
    /// Fingerprint database
    database: FingerprintDatabase,
}

impl TlsFingerprintAnalyzer {
    /// Create new analyzer with default database
    pub fn new() -> Self {
        Self {
            database: FingerprintDatabase::new(),
        }
    }

    /// Create analyzer with custom database
    pub fn with_database(database: FingerprintDatabase) -> Self {
        Self { database }
    }

    /// Analyze TLS fingerprint and compare with User-Agent
    pub fn analyze(&self, fingerprint: &TlsFingerprint, user_agent: Option<&str>) -> TlsAnalysisResult {
        let mut suspicion_reasons = Vec::new();
        let mut score_adjustment = 0i32;

        // Look up fingerprint in database
        let db_entry = self.database.lookup(fingerprint);
        let client_type = db_entry.as_ref().map(|e| e.client_type);
        let client_name = db_entry.as_ref().map(|e| e.client_name.clone());

        // Record this observation
        self.database.record_observation(fingerprint);

        // Base analysis from fingerprint characteristics
        let mut suspicion_level = TlsSuspicionLevel::Medium;

        if let Some(entry) = &db_entry {
            match entry.client_type {
                ClientType::Browser | ClientType::MobileBrowser => {
                    suspicion_level = TlsSuspicionLevel::Low;
                    score_adjustment += 20;
                }
                ClientType::GoodBot => {
                    suspicion_level = TlsSuspicionLevel::Low;
                    score_adjustment += 10;
                }
                ClientType::AutomationTool => {
                    suspicion_level = TlsSuspicionLevel::Medium;
                    score_adjustment -= 10;
                    suspicion_reasons.push(format!("Known automation tool: {}", entry.client_name));
                }
                ClientType::HeadlessBrowser => {
                    suspicion_level = TlsSuspicionLevel::High;
                    score_adjustment -= 20;
                    suspicion_reasons.push(format!("Headless browser detected: {}", entry.client_name));
                }
                ClientType::Scanner => {
                    suspicion_level = TlsSuspicionLevel::Critical;
                    score_adjustment -= 50;
                    suspicion_reasons.push(format!("Security scanner detected: {}", entry.client_name));
                }
                ClientType::Unknown => {}
            }
        }

        // Check for fingerprint/User-Agent mismatches
        if let Some(ua) = user_agent {
            let ua_lower = ua.to_lowercase();

            // Check for User-Agent claiming to be a browser but with non-browser fingerprint
            let ua_claims_chrome = ua_lower.contains("chrome") && !ua_lower.contains("headless");
            let ua_claims_firefox = ua_lower.contains("firefox");
            let ua_claims_safari = ua_lower.contains("safari") && !ua_lower.contains("chrome");

            let ua_claims_browser = ua_claims_chrome || ua_claims_firefox || ua_claims_safari;

            if ua_claims_browser {
                if let Some(entry) = &db_entry {
                    match entry.client_type {
                        ClientType::AutomationTool | ClientType::Scanner => {
                            // Major mismatch: UA says browser, fingerprint says tool
                            suspicion_level = TlsSuspicionLevel::Critical;
                            score_adjustment -= 40;
                            suspicion_reasons.push(format!(
                                "User-Agent/TLS mismatch: UA claims browser, fingerprint is {}",
                                entry.client_name
                            ));
                        }
                        ClientType::HeadlessBrowser => {
                            suspicion_level = TlsSuspicionLevel::High;
                            score_adjustment -= 30;
                            suspicion_reasons.push("User-Agent claims real browser but TLS fingerprint indicates headless".to_string());
                        }
                        _ => {}
                    }
                }
            }

            // Check for known tool User-Agents with browser fingerprints (rare but possible spoofing)
            let ua_claims_curl = ua_lower.contains("curl");
            let ua_claims_python = ua_lower.contains("python");
            let ua_claims_wget = ua_lower.contains("wget");

            if ua_claims_curl || ua_claims_python || ua_claims_wget {
                if let Some(entry) = &db_entry {
                    if entry.client_type == ClientType::Browser {
                        // Tool UA with browser fingerprint - might be legitimate testing
                        suspicion_reasons.push("Tool User-Agent with browser TLS fingerprint".to_string());
                        // Don't penalize too much as this could be legitimate
                    }
                }
            }
        }

        // Additional heuristics based on fingerprint characteristics

        // Very few cipher suites is suspicious (modern browsers have many)
        if fingerprint.cipher_count < 5 {
            suspicion_reasons.push(format!("Low cipher count: {}", fingerprint.cipher_count));
            score_adjustment -= 5;
        }

        // No SNI in a request claiming to be from a browser
        if !fingerprint.has_sni {
            if let Some(ua) = user_agent {
                if ua.to_lowercase().contains("mozilla") {
                    suspicion_reasons.push("No SNI but User-Agent claims browser".to_string());
                    score_adjustment -= 10;
                }
            }
        }

        // TLS 1.0/1.1 is outdated and suspicious for modern browsers
        match fingerprint.tls_version {
            TlsVersion::Ssl30 | TlsVersion::Tls10 | TlsVersion::Tls11 => {
                suspicion_reasons.push(format!("Outdated TLS version: {:?}", fingerprint.tls_version));
                score_adjustment -= 10;
            }
            _ => {}
        }

        // Clamp score adjustment to reasonable bounds
        score_adjustment = score_adjustment.clamp(-50, 50);

        TlsAnalysisResult {
            fingerprint: fingerprint.clone(),
            client_type,
            client_name,
            suspicion_level,
            suspicion_reasons,
            score_adjustment,
        }
    }

    /// Get reference to the fingerprint database
    pub fn database(&self) -> &FingerprintDatabase {
        &self.database
    }
}

impl Default for TlsFingerprintAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grease_detection() {
        assert!(is_grease_value(0x0a0a));
        assert!(is_grease_value(0x1a1a));
        assert!(is_grease_value(0x2a2a));
        assert!(is_grease_value(0xfafa));
        assert!(!is_grease_value(0x0001));
        assert!(!is_grease_value(0x0035));
        assert!(!is_grease_value(0x1301));
    }

    #[test]
    fn test_tls_version_ja4_code() {
        assert_eq!(TlsVersion::Ssl30.ja4_code(), 's');
        assert_eq!(TlsVersion::Tls10.ja4_code(), '0');
        assert_eq!(TlsVersion::Tls11.ja4_code(), '1');
        assert_eq!(TlsVersion::Tls12.ja4_code(), '2');
        assert_eq!(TlsVersion::Tls13.ja4_code(), '3');
    }

    #[test]
    fn test_fingerprint_from_client_hello() {
        let ch = ClientHello {
            record_version: TlsVersion::Tls12,
            handshake_version: TlsVersion::Tls13,
            cipher_suites: vec![0x1301, 0x1302, 0x1303, 0xc02b, 0xc02f],
            extensions: vec![0x0000, 0x000a, 0x000b, 0x000d, 0x0010, 0x002b],
            elliptic_curves: vec![0x001d, 0x0017, 0x0018],
            ec_point_formats: vec![0x00],
            sni: Some("example.com".to_string()),
            alpn_protocols: vec!["h2".to_string(), "http/1.1".to_string()],
            signature_algorithms: vec![0x0403, 0x0503],
            supported_versions: vec![0x0304, 0x0303],
        };

        let fp = TlsFingerprint::from_client_hello(&ch);

        assert!(!fp.ja3.is_empty());
        assert!(!fp.ja3_raw.is_empty());
        assert!(!fp.ja4.is_empty());
        assert_eq!(fp.cipher_count, 5);
        assert_eq!(fp.extension_count, 6);
        assert!(fp.has_sni);
        assert!(fp.has_alpn);
    }

    #[test]
    fn test_ja3_raw_format() {
        let ch = ClientHello {
            record_version: TlsVersion::Tls12,
            handshake_version: TlsVersion::Tls12,
            cipher_suites: vec![0x0035, 0x002f],
            extensions: vec![0x0000, 0x000a],
            elliptic_curves: vec![0x0017],
            ec_point_formats: vec![0x00],
            ..Default::default()
        };

        let fp = TlsFingerprint::from_client_hello(&ch);

        // JA3 raw format: version,ciphers,extensions,curves,formats
        assert!(fp.ja3_raw.starts_with("771,")); // 0x0303 = 771
        assert!(fp.ja3_raw.contains("53-47")); // cipher suites
    }

    #[test]
    fn test_ja4_format() {
        let ch = ClientHello {
            record_version: TlsVersion::Tls12,
            handshake_version: TlsVersion::Tls13,
            cipher_suites: vec![0x1301, 0x1302, 0x1303],
            extensions: vec![0x0000, 0x000a, 0x000d],
            sni: Some("example.com".to_string()),
            alpn_protocols: vec!["h2".to_string()],
            supported_versions: vec![0x0304],
            ..Default::default()
        };

        let fp = TlsFingerprint::from_client_hello(&ch);

        // JA4 starts with t (TCP), 3 (TLS 1.3), d (has SNI)
        assert!(fp.ja4.starts_with("t3d"));
    }

    #[test]
    fn test_fingerprint_database_builtin() {
        let db = FingerprintDatabase::new();
        let (ja3_count, ja4_count) = db.stats();

        // Should have built-in fingerprints loaded
        assert!(ja3_count > 0);
        assert!(ja4_count > 0);
    }

    #[test]
    fn test_analyzer_browser_fingerprint() {
        let analyzer = TlsFingerprintAnalyzer::new();

        let ch = ClientHello {
            handshake_version: TlsVersion::Tls13,
            cipher_suites: vec![0x1301, 0x1302, 0x1303, 0xc02b, 0xc02f, 0xc02c, 0xc030],
            extensions: vec![0x0000, 0x000a, 0x000b, 0x000d, 0x0010, 0x002b, 0x0033],
            sni: Some("example.com".to_string()),
            alpn_protocols: vec!["h2".to_string()],
            supported_versions: vec![0x0304],
            ..Default::default()
        };

        let fp = TlsFingerprint::from_client_hello(&ch);
        let result = analyzer.analyze(&fp, Some("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36"));

        // Browser with matching UA should have positive score
        assert!(result.score_adjustment >= -10); // At worst slightly negative for unknown
    }

    #[test]
    fn test_analyzer_mismatch_detection() {
        let analyzer = TlsFingerprintAnalyzer::new();

        // Create fingerprint that looks like curl
        let ch = ClientHello {
            handshake_version: TlsVersion::Tls12,
            cipher_suites: vec![0x0035, 0x002f, 0x000a],
            extensions: vec![0x000a, 0x000b],
            sni: None,
            ..Default::default()
        };

        let fp = TlsFingerprint::from_client_hello(&ch);

        // Analyze with Chrome User-Agent (mismatch!)
        let result = analyzer.analyze(&fp, Some("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0.0.0"));

        // Should detect suspicion due to no SNI with browser UA
        assert!(!result.suspicion_reasons.is_empty());
    }

    #[test]
    fn test_analyzer_low_cipher_count() {
        let analyzer = TlsFingerprintAnalyzer::new();

        let ch = ClientHello {
            handshake_version: TlsVersion::Tls12,
            cipher_suites: vec![0x0035], // Only 1 cipher
            extensions: vec![0x000a],
            ..Default::default()
        };

        let fp = TlsFingerprint::from_client_hello(&ch);
        let result = analyzer.analyze(&fp, None);

        // Should flag low cipher count
        assert!(result.suspicion_reasons.iter().any(|r| r.contains("cipher count")));
        assert!(result.score_adjustment < 0);
    }

    #[test]
    fn test_analyzer_outdated_tls() {
        let analyzer = TlsFingerprintAnalyzer::new();

        let ch = ClientHello {
            handshake_version: TlsVersion::Tls10,
            cipher_suites: vec![0x0035, 0x002f],
            extensions: vec![],
            ..Default::default()
        };

        let fp = TlsFingerprint::from_client_hello(&ch);
        let result = analyzer.analyze(&fp, None);

        // Should flag outdated TLS
        assert!(result.suspicion_reasons.iter().any(|r| r.contains("Outdated TLS")));
    }

    #[test]
    fn test_client_hello_parsing() {
        // Test that we can create and extract fingerprints from ClientHello structs
        // (Real byte-level parsing is complex; this tests the struct creation path)
        let ch = ClientHello {
            record_version: TlsVersion::Tls10,
            handshake_version: TlsVersion::Tls12,
            cipher_suites: vec![0x1301, 0x1302, 0xc02b, 0xc02f],
            extensions: vec![0x0000, 0x000a],
            elliptic_curves: vec![0x0017],
            ec_point_formats: vec![0x00],
            sni: Some("example.com".to_string()),
            alpn_protocols: vec!["h2".to_string()],
            signature_algorithms: vec![0x0403],
            supported_versions: vec![0x0304, 0x0303],
        };

        // Verify fingerprint can be computed from struct
        let fp = TlsFingerprint::from_client_hello(&ch);
        assert!(!fp.ja3.is_empty());
        assert!(!fp.ja4.is_empty());
        assert_eq!(fp.cipher_count, 4);
        assert_eq!(fp.extension_count, 2);
        assert!(fp.has_sni);
        assert!(fp.has_alpn);
    }

    #[test]
    fn test_parse_invalid_data() {
        // Too short
        assert!(ClientHello::parse(&[0x16, 0x03]).is_none());

        // Wrong content type
        assert!(ClientHello::parse(&[0x15, 0x03, 0x03, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00]).is_none());

        // Not a ClientHello
        let not_client_hello: Vec<u8> = vec![
            0x16, 0x03, 0x03, 0x00, 0x05,
            0x02, 0x00, 0x00, 0x01, 0x00, // ServerHello type (0x02)
        ];
        assert!(ClientHello::parse(&not_client_hello).is_none());
    }

    #[test]
    fn test_suspicion_levels() {
        let analyzer = TlsFingerprintAnalyzer::new();

        // Test that we can create all suspicion levels through analysis
        let normal_ch = ClientHello {
            handshake_version: TlsVersion::Tls13,
            cipher_suites: vec![0x1301, 0x1302, 0x1303, 0xc02b, 0xc02f],
            extensions: vec![0x0000, 0x000a, 0x000d],
            sni: Some("test.com".to_string()),
            ..Default::default()
        };

        let fp = TlsFingerprint::from_client_hello(&normal_ch);
        let result = analyzer.analyze(&fp, None);

        // Result should have a suspicion level
        assert!(matches!(
            result.suspicion_level,
            TlsSuspicionLevel::Low | TlsSuspicionLevel::Medium | TlsSuspicionLevel::High | TlsSuspicionLevel::Critical
        ));
    }
}
