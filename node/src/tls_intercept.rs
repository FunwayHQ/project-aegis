// Sprint 19: TLS Intercept Proxy for ClientHello Extraction
//
// This module implements a transparent TLS proxy layer that:
// 1. Accepts incoming TLS connections
// 2. Reads the ClientHello to extract JA3/JA4 fingerprints
// 3. Forwards the complete connection (including ClientHello) to the backend
// 4. Stores fingerprints for lookup by the main proxy
//
// Architecture:
// ```
// Client → TLS Intercept (port 443) → Pingora (internal port) → Origin
//                ↓
//         Extract ClientHello
//                ↓
//         Compute JA3/JA4
//                ↓
//         Store in shared state
// ```

use crate::tls_fingerprint::{ClientHello, TlsFingerprint, TlsFingerprintAnalyzer};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
// AsyncReadExt and AsyncWriteExt needed for future full implementation
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;

/// Maximum size of ClientHello we'll buffer (16KB should be plenty)
const MAX_CLIENT_HELLO_SIZE: usize = 16384;

/// How long to keep fingerprints in the lookup cache
const FINGERPRINT_CACHE_TTL: Duration = Duration::from_secs(30);

/// TLS record content types
const TLS_HANDSHAKE: u8 = 0x16;

/// Cached fingerprint entry with expiration
#[derive(Clone)]
pub struct CachedTlsFingerprint {
    pub fingerprint: TlsFingerprint,
    pub client_addr: SocketAddr,
    pub expires_at: Instant,
}

/// Shared fingerprint store for communication between intercept and proxy
pub struct FingerprintStore {
    /// Map from client address to fingerprint
    fingerprints: RwLock<HashMap<SocketAddr, CachedTlsFingerprint>>,
    /// TLS analyzer for scoring
    analyzer: TlsFingerprintAnalyzer,
}

impl FingerprintStore {
    pub fn new() -> Self {
        Self {
            fingerprints: RwLock::new(HashMap::new()),
            analyzer: TlsFingerprintAnalyzer::new(),
        }
    }

    /// Store fingerprint for a client address
    pub async fn store(&self, addr: SocketAddr, fingerprint: TlsFingerprint) {
        let entry = CachedTlsFingerprint {
            fingerprint,
            client_addr: addr,
            expires_at: Instant::now() + FINGERPRINT_CACHE_TTL,
        };

        let mut store = self.fingerprints.write().await;
        store.insert(addr, entry);

        // Cleanup expired entries periodically (every 100 inserts)
        if store.len() % 100 == 0 {
            let now = Instant::now();
            store.retain(|_, v| v.expires_at > now);
        }
    }

    /// Get fingerprint for a client address
    pub async fn get(&self, addr: &SocketAddr) -> Option<TlsFingerprint> {
        let store = self.fingerprints.read().await;
        store.get(addr).and_then(|entry| {
            if entry.expires_at > Instant::now() {
                Some(entry.fingerprint.clone())
            } else {
                None
            }
        })
    }

    /// Get fingerprint and remove from store (single use)
    pub async fn take(&self, addr: &SocketAddr) -> Option<TlsFingerprint> {
        let mut store = self.fingerprints.write().await;
        store.remove(addr).and_then(|entry| {
            if entry.expires_at > Instant::now() {
                Some(entry.fingerprint)
            } else {
                None
            }
        })
    }

    /// Get the TLS analyzer for additional analysis
    pub fn analyzer(&self) -> &TlsFingerprintAnalyzer {
        &self.analyzer
    }

    /// Get current cache size
    pub async fn len(&self) -> usize {
        self.fingerprints.read().await.len()
    }

    /// Clear all cached fingerprints
    pub async fn clear(&self) {
        self.fingerprints.write().await.clear();
    }
}

impl Default for FingerprintStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration for the TLS intercept proxy
#[derive(Clone)]
pub struct TlsInterceptConfig {
    /// Address to listen on (e.g., "0.0.0.0:443")
    pub listen_addr: String,
    /// Backend address to forward to (e.g., "127.0.0.1:8443")
    pub backend_addr: String,
    /// Connection timeout
    pub connect_timeout: Duration,
    /// Read timeout for ClientHello
    pub read_timeout: Duration,
}

impl Default for TlsInterceptConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:443".to_string(),
            backend_addr: "127.0.0.1:8443".to_string(),
            connect_timeout: Duration::from_secs(10),
            read_timeout: Duration::from_secs(5),
        }
    }
}

/// TLS Intercept Proxy
///
/// Sits in front of the main proxy, captures ClientHello for fingerprinting,
/// then transparently forwards the complete TLS handshake to the backend.
pub struct TlsInterceptProxy {
    config: TlsInterceptConfig,
    fingerprint_store: Arc<FingerprintStore>,
}

impl TlsInterceptProxy {
    /// Create new TLS intercept proxy
    pub fn new(config: TlsInterceptConfig, fingerprint_store: Arc<FingerprintStore>) -> Self {
        Self {
            config,
            fingerprint_store,
        }
    }

    /// Run the intercept proxy
    pub async fn run(&self) -> Result<()> {
        let listener = TcpListener::bind(&self.config.listen_addr)
            .await
            .context(format!("Failed to bind to {}", self.config.listen_addr))?;

        log::info!(
            "TLS Intercept Proxy listening on {}, forwarding to {}",
            self.config.listen_addr,
            self.config.backend_addr
        );

        loop {
            match listener.accept().await {
                Ok((client_stream, client_addr)) => {
                    let config = self.config.clone();
                    let store = self.fingerprint_store.clone();

                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(client_stream, client_addr, config, store).await {
                            log::debug!("Connection error from {}: {}", client_addr, e);
                        }
                    });
                }
                Err(e) => {
                    log::error!("Accept error: {}", e);
                }
            }
        }
    }
}

/// Handle a single connection: extract ClientHello then forward
async fn handle_connection(
    client: TcpStream,
    client_addr: SocketAddr,
    config: TlsInterceptConfig,
    store: Arc<FingerprintStore>,
) -> Result<()> {
    // Buffer for reading ClientHello
    let mut buffer = vec![0u8; MAX_CLIENT_HELLO_SIZE];

    // Set read timeout
    let read_result = tokio::time::timeout(
        config.read_timeout,
        client.peek(&mut buffer),
    ).await;

    let bytes_peeked = match read_result {
        Ok(Ok(n)) => n,
        Ok(Err(e)) => {
            return Err(anyhow::anyhow!("Peek error: {}", e));
        }
        Err(_) => {
            return Err(anyhow::anyhow!("Read timeout"));
        }
    };

    if bytes_peeked == 0 {
        return Err(anyhow::anyhow!("Empty connection"));
    }

    // Try to extract ClientHello and compute fingerprint
    let data = &buffer[..bytes_peeked];

    if data[0] == TLS_HANDSHAKE {
        // This looks like a TLS handshake, try to parse ClientHello
        if let Some(client_hello) = ClientHello::parse(data) {
            let fingerprint = TlsFingerprint::from_client_hello(&client_hello);

            log::debug!(
                "TLS fingerprint for {}: JA3={}, JA4={}, SNI={:?}",
                client_addr,
                fingerprint.ja3,
                fingerprint.ja4,
                client_hello.sni
            );

            // Store fingerprint for later lookup by the proxy
            store.store(client_addr, fingerprint).await;
        } else {
            log::debug!("Could not parse ClientHello from {} ({} bytes)", client_addr, bytes_peeked);
        }
    } else {
        log::debug!("Non-TLS connection from {} (first byte: 0x{:02x})", client_addr, data[0]);
    }

    // Connect to backend
    let backend = tokio::time::timeout(
        config.connect_timeout,
        TcpStream::connect(&config.backend_addr),
    )
    .await
    .context("Backend connect timeout")?
    .context("Failed to connect to backend")?;

    // Forward traffic bidirectionally
    let (mut client_read, mut client_write) = client.into_split();
    let (mut backend_read, mut backend_write) = backend.into_split();

    let client_to_backend = async {
        tokio::io::copy(&mut client_read, &mut backend_write).await
    };

    let backend_to_client = async {
        tokio::io::copy(&mut backend_read, &mut client_write).await
    };

    // Run both directions concurrently, finish when either completes
    tokio::select! {
        result = client_to_backend => {
            if let Err(e) = result {
                log::debug!("Client->Backend copy error: {}", e);
            }
        }
        result = backend_to_client => {
            if let Err(e) = result {
                log::debug!("Backend->Client copy error: {}", e);
            }
        }
    }

    Ok(())
}

/// Helper to create a fingerprint store that can be shared between
/// the intercept proxy and the main proxy
pub fn create_shared_store() -> Arc<FingerprintStore> {
    Arc::new(FingerprintStore::new())
}

/// Extract fingerprint header name for passing via HTTP headers (alternative approach)
pub const FINGERPRINT_HEADER_JA3: &str = "X-TLS-JA3";
pub const FINGERPRINT_HEADER_JA4: &str = "X-TLS-JA4";
pub const FINGERPRINT_HEADER_VERSION: &str = "X-TLS-Version";

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[tokio::test]
    async fn test_fingerprint_store_basic() {
        let store = FingerprintStore::new();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 12345);

        // Create a test fingerprint
        let fp = TlsFingerprint {
            ja3: "test_ja3_hash".to_string(),
            ja3_raw: "771,4865-4866,0-23,29,0".to_string(),
            ja4: "t3d050400_abc_def".to_string(),
            tls_version: crate::tls_fingerprint::TlsVersion::Tls13,
            cipher_count: 5,
            extension_count: 4,
            has_sni: true,
            has_alpn: true,
        };

        // Store and retrieve
        store.store(addr, fp.clone()).await;
        let retrieved = store.get(&addr).await;

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().ja3, "test_ja3_hash");
    }

    #[tokio::test]
    async fn test_fingerprint_store_take() {
        let store = FingerprintStore::new();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2)), 12346);

        let fp = TlsFingerprint {
            ja3: "take_test".to_string(),
            ja3_raw: String::new(),
            ja4: String::new(),
            tls_version: crate::tls_fingerprint::TlsVersion::Tls12,
            cipher_count: 3,
            extension_count: 2,
            has_sni: false,
            has_alpn: false,
        };

        store.store(addr, fp).await;

        // Take should return and remove
        let taken = store.take(&addr).await;
        assert!(taken.is_some());
        assert_eq!(taken.unwrap().ja3, "take_test");

        // Second take should return None
        let taken_again = store.take(&addr).await;
        assert!(taken_again.is_none());
    }

    #[tokio::test]
    async fn test_fingerprint_store_expiration() {
        let store = FingerprintStore::new();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 3)), 12347);

        let fp = TlsFingerprint {
            ja3: "expiry_test".to_string(),
            ja3_raw: String::new(),
            ja4: String::new(),
            tls_version: crate::tls_fingerprint::TlsVersion::Tls12,
            cipher_count: 1,
            extension_count: 1,
            has_sni: false,
            has_alpn: false,
        };

        // Store with expired entry (manually create)
        {
            let mut map = store.fingerprints.write().await;
            map.insert(addr, CachedTlsFingerprint {
                fingerprint: fp,
                client_addr: addr,
                expires_at: Instant::now() - Duration::from_secs(1), // Already expired
            });
        }

        // Should not return expired entry
        let result = store.get(&addr).await;
        assert!(result.is_none());
    }

    #[test]
    fn test_config_defaults() {
        let config = TlsInterceptConfig::default();
        assert_eq!(config.listen_addr, "0.0.0.0:443");
        assert_eq!(config.backend_addr, "127.0.0.1:8443");
    }

    #[tokio::test]
    async fn test_store_clear() {
        let store = FingerprintStore::new();

        // Add some entries
        for i in 0..10 {
            let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, i)), 12345 + i as u16);
            let fp = TlsFingerprint {
                ja3: format!("test_{}", i),
                ja3_raw: String::new(),
                ja4: String::new(),
                tls_version: crate::tls_fingerprint::TlsVersion::Tls12,
                cipher_count: 1,
                extension_count: 1,
                has_sni: false,
                has_alpn: false,
            };
            store.store(addr, fp).await;
        }

        assert_eq!(store.len().await, 10);

        store.clear().await;

        assert_eq!(store.len().await, 0);
    }
}
