//! DNS Record Type Definitions
//!
//! Defines all DNS record types and values supported by AEGIS DNS.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

/// DNS record types supported by AEGIS
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "UPPERCASE")]
pub enum DnsRecordType {
    /// IPv4 address record
    A,
    /// IPv6 address record
    AAAA,
    /// Canonical name (alias)
    CNAME,
    /// Mail exchange
    MX,
    /// Text record
    TXT,
    /// Name server
    NS,
    /// Start of authority
    SOA,
    /// Certification Authority Authorization
    CAA,
    /// Service location
    SRV,
    /// Pointer record (reverse DNS)
    PTR,
    /// DNSSEC signature
    RRSIG,
    /// DNSSEC key
    DNSKEY,
    /// Next secure record
    NSEC,
    /// Next secure record version 3
    NSEC3,
    /// NSEC3 parameters
    NSEC3PARAM,
    /// Delegation signer
    DS,
}

impl fmt::Display for DnsRecordType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DnsRecordType::A => write!(f, "A"),
            DnsRecordType::AAAA => write!(f, "AAAA"),
            DnsRecordType::CNAME => write!(f, "CNAME"),
            DnsRecordType::MX => write!(f, "MX"),
            DnsRecordType::TXT => write!(f, "TXT"),
            DnsRecordType::NS => write!(f, "NS"),
            DnsRecordType::SOA => write!(f, "SOA"),
            DnsRecordType::CAA => write!(f, "CAA"),
            DnsRecordType::SRV => write!(f, "SRV"),
            DnsRecordType::PTR => write!(f, "PTR"),
            DnsRecordType::RRSIG => write!(f, "RRSIG"),
            DnsRecordType::DNSKEY => write!(f, "DNSKEY"),
            DnsRecordType::NSEC => write!(f, "NSEC"),
            DnsRecordType::NSEC3 => write!(f, "NSEC3"),
            DnsRecordType::NSEC3PARAM => write!(f, "NSEC3PARAM"),
            DnsRecordType::DS => write!(f, "DS"),
        }
    }
}

impl FromStr for DnsRecordType {
    type Err = DnsError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "A" => Ok(DnsRecordType::A),
            "AAAA" => Ok(DnsRecordType::AAAA),
            "CNAME" => Ok(DnsRecordType::CNAME),
            "MX" => Ok(DnsRecordType::MX),
            "TXT" => Ok(DnsRecordType::TXT),
            "NS" => Ok(DnsRecordType::NS),
            "SOA" => Ok(DnsRecordType::SOA),
            "CAA" => Ok(DnsRecordType::CAA),
            "SRV" => Ok(DnsRecordType::SRV),
            "PTR" => Ok(DnsRecordType::PTR),
            "RRSIG" => Ok(DnsRecordType::RRSIG),
            "DNSKEY" => Ok(DnsRecordType::DNSKEY),
            "NSEC" => Ok(DnsRecordType::NSEC),
            "NSEC3" => Ok(DnsRecordType::NSEC3),
            "NSEC3PARAM" => Ok(DnsRecordType::NSEC3PARAM),
            "DS" => Ok(DnsRecordType::DS),
            _ => Err(DnsError::InvalidRecordType(s.to_string())),
        }
    }
}

/// Values for different DNS record types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "data")]
pub enum DnsRecordValue {
    /// IPv4 address
    A(Ipv4Addr),
    /// IPv6 address
    AAAA(Ipv6Addr),
    /// Canonical name
    CNAME(String),
    /// Mail exchange with priority handled separately
    MX { exchange: String },
    /// Text record
    TXT(String),
    /// Name server
    NS(String),
    /// Start of authority
    SOA {
        /// Primary nameserver
        mname: String,
        /// Admin email (with . instead of @)
        rname: String,
        /// Serial number
        serial: u32,
        /// Refresh interval (seconds)
        refresh: u32,
        /// Retry interval (seconds)
        retry: u32,
        /// Expire time (seconds)
        expire: u32,
        /// Minimum TTL (seconds)
        minimum: u32,
    },
    /// Certification Authority Authorization
    CAA {
        flags: u8,
        tag: String,
        value: String,
    },
    /// Service location
    SRV {
        weight: u16,
        port: u16,
        target: String,
    },
    /// Pointer record
    PTR(String),
    /// DNSSEC signature
    RRSIG {
        type_covered: DnsRecordType,
        algorithm: u8,
        labels: u8,
        original_ttl: u32,
        expiration: u32,
        inception: u32,
        key_tag: u16,
        signer_name: String,
        signature: Vec<u8>,
    },
    /// DNSSEC public key
    DNSKEY {
        flags: u16,
        protocol: u8,
        algorithm: u8,
        public_key: Vec<u8>,
    },
    /// Next secure record
    NSEC {
        next_domain: String,
        types: Vec<DnsRecordType>,
    },
    /// Delegation signer
    DS {
        key_tag: u16,
        algorithm: u8,
        digest_type: u8,
        digest: Vec<u8>,
    },
}

impl DnsRecordValue {
    /// Get the record type for this value
    pub fn record_type(&self) -> DnsRecordType {
        match self {
            DnsRecordValue::A(_) => DnsRecordType::A,
            DnsRecordValue::AAAA(_) => DnsRecordType::AAAA,
            DnsRecordValue::CNAME(_) => DnsRecordType::CNAME,
            DnsRecordValue::MX { .. } => DnsRecordType::MX,
            DnsRecordValue::TXT(_) => DnsRecordType::TXT,
            DnsRecordValue::NS(_) => DnsRecordType::NS,
            DnsRecordValue::SOA { .. } => DnsRecordType::SOA,
            DnsRecordValue::CAA { .. } => DnsRecordType::CAA,
            DnsRecordValue::SRV { .. } => DnsRecordType::SRV,
            DnsRecordValue::PTR(_) => DnsRecordType::PTR,
            DnsRecordValue::RRSIG { .. } => DnsRecordType::RRSIG,
            DnsRecordValue::DNSKEY { .. } => DnsRecordType::DNSKEY,
            DnsRecordValue::NSEC { .. } => DnsRecordType::NSEC,
            DnsRecordValue::DS { .. } => DnsRecordType::DS,
        }
    }

    /// Format the value as a string for display
    pub fn to_display_string(&self) -> String {
        match self {
            DnsRecordValue::A(ip) => ip.to_string(),
            DnsRecordValue::AAAA(ip) => ip.to_string(),
            DnsRecordValue::CNAME(name) => name.clone(),
            DnsRecordValue::MX { exchange } => exchange.clone(),
            DnsRecordValue::TXT(text) => format!("\"{}\"", text),
            DnsRecordValue::NS(name) => name.clone(),
            DnsRecordValue::SOA {
                mname,
                rname,
                serial,
                refresh,
                retry,
                expire,
                minimum,
            } => format!(
                "{} {} {} {} {} {} {}",
                mname, rname, serial, refresh, retry, expire, minimum
            ),
            DnsRecordValue::CAA { flags, tag, value } => {
                format!("{} {} \"{}\"", flags, tag, value)
            }
            DnsRecordValue::SRV {
                weight,
                port,
                target,
            } => format!("{} {} {}", weight, port, target),
            DnsRecordValue::PTR(name) => name.clone(),
            DnsRecordValue::RRSIG { signer_name, .. } => format!("RRSIG ({})", signer_name),
            DnsRecordValue::DNSKEY { algorithm, .. } => format!("DNSKEY (alg: {})", algorithm),
            DnsRecordValue::NSEC { next_domain, .. } => format!("NSEC {}", next_domain),
            DnsRecordValue::DS { key_tag, .. } => format!("DS (tag: {})", key_tag),
        }
    }
}

/// A DNS record
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DnsRecord {
    /// Unique identifier for this record
    #[serde(default)]
    pub id: String,
    /// Record name (e.g., "www" or "@" for root)
    pub name: String,
    /// Record type
    pub record_type: DnsRecordType,
    /// Time to live in seconds
    pub ttl: u32,
    /// Record value
    pub value: DnsRecordValue,
    /// Priority (for MX/SRV records)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<u16>,
    /// Whether this record is proxied through AEGIS (return anycast IP)
    #[serde(default)]
    pub proxied: bool,
}

impl DnsRecord {
    /// Create a new DNS record
    pub fn new(
        name: impl Into<String>,
        record_type: DnsRecordType,
        ttl: u32,
        value: DnsRecordValue,
    ) -> Self {
        Self {
            id: generate_record_id(),
            name: name.into(),
            record_type,
            ttl,
            value,
            priority: None,
            proxied: false,
        }
    }

    /// Create an A record
    pub fn a(name: impl Into<String>, ip: Ipv4Addr, ttl: u32) -> Self {
        Self::new(name, DnsRecordType::A, ttl, DnsRecordValue::A(ip))
    }

    /// Create an AAAA record
    pub fn aaaa(name: impl Into<String>, ip: Ipv6Addr, ttl: u32) -> Self {
        Self::new(name, DnsRecordType::AAAA, ttl, DnsRecordValue::AAAA(ip))
    }

    /// Create a CNAME record
    pub fn cname(name: impl Into<String>, target: impl Into<String>, ttl: u32) -> Self {
        Self::new(
            name,
            DnsRecordType::CNAME,
            ttl,
            DnsRecordValue::CNAME(target.into()),
        )
    }

    /// Create an MX record
    pub fn mx(name: impl Into<String>, exchange: impl Into<String>, priority: u16, ttl: u32) -> Self {
        let mut record = Self::new(
            name,
            DnsRecordType::MX,
            ttl,
            DnsRecordValue::MX {
                exchange: exchange.into(),
            },
        );
        record.priority = Some(priority);
        record
    }

    /// Create a TXT record
    pub fn txt(name: impl Into<String>, text: impl Into<String>, ttl: u32) -> Self {
        Self::new(
            name,
            DnsRecordType::TXT,
            ttl,
            DnsRecordValue::TXT(text.into()),
        )
    }

    /// Create an NS record
    pub fn ns(name: impl Into<String>, nameserver: impl Into<String>, ttl: u32) -> Self {
        Self::new(
            name,
            DnsRecordType::NS,
            ttl,
            DnsRecordValue::NS(nameserver.into()),
        )
    }

    /// Set the record as proxied
    pub fn with_proxied(mut self, proxied: bool) -> Self {
        self.proxied = proxied;
        self
    }

    /// Get the fully qualified domain name for this record
    pub fn fqdn(&self, zone_domain: &str) -> String {
        if self.name == "@" || self.name.is_empty() {
            zone_domain.to_string()
        } else {
            format!("{}.{}", self.name, zone_domain)
        }
    }
}

/// Generate a unique record ID
fn generate_record_id() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 8] = rng.gen();
    format!("rec_{}", hex::encode(bytes))
}

/// DNS-specific errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum DnsError {
    #[error("Invalid record type: {0}")]
    InvalidRecordType(String),

    #[error("Zone not found: {0}")]
    ZoneNotFound(String),

    #[error("Record not found: {0}")]
    RecordNotFound(String),

    #[error("Duplicate record: {0}")]
    DuplicateRecord(String),

    #[error("Invalid domain name: {0}")]
    InvalidDomain(String),

    #[error("Invalid IP address: {0}")]
    InvalidIpAddress(String),

    #[error("Rate limited: {0}")]
    RateLimited(String),

    #[error("Server error: {0}")]
    ServerError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_type_parsing() {
        assert_eq!(DnsRecordType::from_str("A").unwrap(), DnsRecordType::A);
        assert_eq!(DnsRecordType::from_str("a").unwrap(), DnsRecordType::A);
        assert_eq!(DnsRecordType::from_str("AAAA").unwrap(), DnsRecordType::AAAA);
        assert_eq!(DnsRecordType::from_str("cname").unwrap(), DnsRecordType::CNAME);
        assert!(DnsRecordType::from_str("invalid").is_err());
    }

    #[test]
    fn test_record_type_display() {
        assert_eq!(DnsRecordType::A.to_string(), "A");
        assert_eq!(DnsRecordType::AAAA.to_string(), "AAAA");
        assert_eq!(DnsRecordType::CNAME.to_string(), "CNAME");
    }

    #[test]
    fn test_a_record_creation() {
        let ip: Ipv4Addr = "192.168.1.1".parse().unwrap();
        let record = DnsRecord::a("www", ip, 300);

        assert_eq!(record.name, "www");
        assert_eq!(record.record_type, DnsRecordType::A);
        assert_eq!(record.ttl, 300);
        assert!(!record.proxied);
        assert!(record.id.starts_with("rec_"));
    }

    #[test]
    fn test_aaaa_record_creation() {
        let ip: Ipv6Addr = "2001:db8::1".parse().unwrap();
        let record = DnsRecord::aaaa("www", ip, 300);

        assert_eq!(record.name, "www");
        assert_eq!(record.record_type, DnsRecordType::AAAA);
    }

    #[test]
    fn test_mx_record_creation() {
        let record = DnsRecord::mx("@", "mail.example.com", 10, 300);

        assert_eq!(record.name, "@");
        assert_eq!(record.record_type, DnsRecordType::MX);
        assert_eq!(record.priority, Some(10));
    }

    #[test]
    fn test_fqdn() {
        let record = DnsRecord::a("www", "192.168.1.1".parse().unwrap(), 300);
        assert_eq!(record.fqdn("example.com"), "www.example.com");

        let root_record = DnsRecord::a("@", "192.168.1.1".parse().unwrap(), 300);
        assert_eq!(root_record.fqdn("example.com"), "example.com");
    }

    #[test]
    fn test_proxied_record() {
        let record = DnsRecord::a("www", "192.168.1.1".parse().unwrap(), 300).with_proxied(true);
        assert!(record.proxied);
    }

    #[test]
    fn test_record_value_display() {
        let value = DnsRecordValue::A("192.168.1.1".parse().unwrap());
        assert_eq!(value.to_display_string(), "192.168.1.1");

        let txt = DnsRecordValue::TXT("hello world".to_string());
        assert_eq!(txt.to_display_string(), "\"hello world\"");
    }

    #[test]
    fn test_record_value_type() {
        let value = DnsRecordValue::A("192.168.1.1".parse().unwrap());
        assert_eq!(value.record_type(), DnsRecordType::A);

        let cname = DnsRecordValue::CNAME("example.com".to_string());
        assert_eq!(cname.record_type(), DnsRecordType::CNAME);
    }

    #[test]
    fn test_soa_record_value() {
        let soa = DnsRecordValue::SOA {
            mname: "ns1.example.com".to_string(),
            rname: "admin.example.com".to_string(),
            serial: 2024010101,
            refresh: 3600,
            retry: 600,
            expire: 604800,
            minimum: 300,
        };

        assert_eq!(soa.record_type(), DnsRecordType::SOA);
        let display = soa.to_display_string();
        assert!(display.contains("ns1.example.com"));
        assert!(display.contains("2024010101"));
    }
}
