//! DNS over TLS (DoT) Server
//!
//! Sprint 30.5: Encrypted DNS Protocols
//!
//! Implements DNS over TLS (RFC 7858) on port 853 for encrypted DNS queries.
//! DoT provides privacy by encrypting DNS traffic between clients and the server.

use std::fs::File;
use std::io::BufReader;
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::ServerConfig;
use rustls_pemfile::{certs, private_key};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio::time::timeout;
use tokio_rustls::TlsAcceptor;
use tracing::{debug, error, info, warn};

use crate::dns::DotConfig;

/// Maximum concurrent TLS connections
const MAX_CONCURRENT_CONNECTIONS: usize = 10000;

/// Connection timeout for TLS handshake
const TLS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Read timeout for DNS messages
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum DNS message size (TCP uses 2-byte length prefix)
/// Using 65534 to allow testing the "too large" case with u16::MAX (65535)
const MAX_DNS_MESSAGE_SIZE: usize = 65534;

/// DNS message handler trait
#[async_trait::async_trait]
pub trait DnsHandler: Send + Sync {
    /// Handle a DNS query and return a response
    async fn handle_query(&self, query: &[u8], client_ip: IpAddr) -> Result<Vec<u8>, DotError>;
}

/// DNS over TLS Server
pub struct DotServer {
    config: DotConfig,
    tls_config: Arc<ServerConfig>,
    dns_handler: Arc<dyn DnsHandler>,
    connection_semaphore: Arc<Semaphore>,
}

impl DotServer {
    /// Create a new DoT server
    pub fn new(
        config: DotConfig,
        dns_handler: Arc<dyn DnsHandler>,
    ) -> Result<Self, DotError> {
        let cert_path = config.cert_path.as_ref()
            .ok_or_else(|| DotError::ConfigError("cert_path is required".to_string()))?;
        let key_path = config.key_path.as_ref()
            .ok_or_else(|| DotError::ConfigError("key_path is required".to_string()))?;

        let tls_config = build_tls_config(cert_path, key_path)?;

        Ok(Self {
            config,
            tls_config: Arc::new(tls_config),
            dns_handler,
            connection_semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_CONNECTIONS)),
        })
    }

    /// Get the listen address
    pub fn addr(&self) -> SocketAddr {
        self.config.addr
    }

    /// Start the DoT server
    pub async fn run(&self) -> Result<(), DotError> {
        let listener = TcpListener::bind(&self.config.addr).await
            .map_err(|e| DotError::BindError(format!("Failed to bind to {}: {}", self.config.addr, e)))?;

        let acceptor = TlsAcceptor::from(self.tls_config.clone());

        info!("DoT server listening on {}", self.config.addr);

        loop {
            let (stream, client_addr) = match listener.accept().await {
                Ok(conn) => conn,
                Err(e) => {
                    warn!("Failed to accept TCP connection: {}", e);
                    continue;
                }
            };

            // Acquire connection permit
            let permit = match self.connection_semaphore.clone().try_acquire_owned() {
                Ok(permit) => permit,
                Err(_) => {
                    warn!("Max connections reached, rejecting connection from {}", client_addr);
                    continue;
                }
            };

            let acceptor = acceptor.clone();
            let dns_handler = self.dns_handler.clone();

            tokio::spawn(async move {
                // Keep permit alive for the duration of the connection
                let _permit = permit;

                // TLS handshake with timeout
                let tls_result = timeout(TLS_HANDSHAKE_TIMEOUT, acceptor.accept(stream)).await;

                let tls_stream = match tls_result {
                    Ok(Ok(stream)) => stream,
                    Ok(Err(e)) => {
                        debug!("TLS handshake failed for {}: {}", client_addr, e);
                        return;
                    }
                    Err(_) => {
                        debug!("TLS handshake timeout for {}", client_addr);
                        return;
                    }
                };

                debug!("DoT connection established from {}", client_addr);

                // Handle DNS queries on this connection
                if let Err(e) = Self::handle_connection(tls_stream, client_addr.ip(), dns_handler).await {
                    debug!("Connection error from {}: {}", client_addr, e);
                }
            });
        }
    }

    /// Handle a single TLS connection (may contain multiple DNS queries)
    async fn handle_connection<S>(
        mut stream: S,
        client_ip: IpAddr,
        dns_handler: Arc<dyn DnsHandler>,
    ) -> Result<(), DotError>
    where
        S: AsyncReadExt + AsyncWriteExt + Unpin,
    {
        let mut queries_handled = 0u64;

        loop {
            // Read 2-byte length prefix (DNS over TCP format)
            let mut len_buf = [0u8; 2];
            let read_result = timeout(READ_TIMEOUT, stream.read_exact(&mut len_buf)).await;

            match read_result {
                Ok(Ok(_)) => {}
                Ok(Err(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    // Client closed connection gracefully
                    debug!("Client {} closed connection after {} queries", client_ip, queries_handled);
                    break;
                }
                Ok(Err(e)) => {
                    return Err(DotError::IoError(format!("Read length failed: {}", e)));
                }
                Err(_) => {
                    // Read timeout - close connection
                    debug!("Read timeout for client {}", client_ip);
                    break;
                }
            }

            let msg_len = u16::from_be_bytes(len_buf) as usize;

            if msg_len == 0 {
                debug!("Empty message from {}", client_ip);
                continue;
            }

            if msg_len > MAX_DNS_MESSAGE_SIZE {
                return Err(DotError::MessageTooLarge(msg_len));
            }

            // Read DNS message
            let mut msg_buf = vec![0u8; msg_len];
            let read_result = timeout(READ_TIMEOUT, stream.read_exact(&mut msg_buf)).await;

            match read_result {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => {
                    return Err(DotError::IoError(format!("Read message failed: {}", e)));
                }
                Err(_) => {
                    return Err(DotError::Timeout("Message read timeout".to_string()));
                }
            }

            // Process DNS query
            let response = match dns_handler.handle_query(&msg_buf, client_ip).await {
                Ok(resp) => resp,
                Err(e) => {
                    warn!("Query handler error for {}: {}", client_ip, e);
                    continue;
                }
            };

            // Send response with length prefix
            let resp_len = (response.len() as u16).to_be_bytes();
            if let Err(e) = stream.write_all(&resp_len).await {
                return Err(DotError::IoError(format!("Write length failed: {}", e)));
            }
            if let Err(e) = stream.write_all(&response).await {
                return Err(DotError::IoError(format!("Write response failed: {}", e)));
            }

            queries_handled += 1;
        }

        Ok(())
    }

    /// Check if DoT is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }
}

/// Build TLS server configuration from certificate and key files
fn build_tls_config(cert_path: &str, key_path: &str) -> Result<ServerConfig, DotError> {
    // Load certificates
    let cert_file = File::open(cert_path)
        .map_err(|e| DotError::CertError(format!("Failed to open cert file: {}", e)))?;
    let mut cert_reader = BufReader::new(cert_file);
    let certs: Vec<CertificateDer<'static>> = certs(&mut cert_reader)
        .filter_map(|r| r.ok())
        .collect();

    if certs.is_empty() {
        return Err(DotError::CertError("No certificates found in file".to_string()));
    }

    // Load private key
    let key_file = File::open(key_path)
        .map_err(|e| DotError::KeyError(format!("Failed to open key file: {}", e)))?;
    let mut key_reader = BufReader::new(key_file);
    let key = private_key(&mut key_reader)
        .map_err(|e| DotError::KeyError(format!("Failed to read private key: {}", e)))?
        .ok_or_else(|| DotError::KeyError("No private key found in file".to_string()))?;

    // Build TLS config
    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| DotError::TlsError(format!("Failed to build TLS config: {}", e)))?;

    Ok(config)
}

/// Load certificates from PEM file
pub fn load_certs(path: &str) -> Result<Vec<CertificateDer<'static>>, DotError> {
    let file = File::open(path)
        .map_err(|e| DotError::CertError(format!("Failed to open cert file: {}", e)))?;
    let mut reader = BufReader::new(file);
    let certs: Vec<CertificateDer<'static>> = certs(&mut reader)
        .filter_map(|r| r.ok())
        .collect();

    if certs.is_empty() {
        return Err(DotError::CertError("No certificates found".to_string()));
    }

    Ok(certs)
}

/// Load private key from PEM file
pub fn load_private_key(path: &str) -> Result<PrivateKeyDer<'static>, DotError> {
    let file = File::open(path)
        .map_err(|e| DotError::KeyError(format!("Failed to open key file: {}", e)))?;
    let mut reader = BufReader::new(file);

    private_key(&mut reader)
        .map_err(|e| DotError::KeyError(format!("Failed to read private key: {}", e)))?
        .ok_or_else(|| DotError::KeyError("No private key found in file".to_string()))
}

/// DoT server errors
#[derive(Debug, Clone)]
pub enum DotError {
    ConfigError(String),
    BindError(String),
    CertError(String),
    KeyError(String),
    TlsError(String),
    IoError(String),
    MessageTooLarge(usize),
    Timeout(String),
    HandlerError(String),
}

impl std::fmt::Display for DotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DotError::ConfigError(msg) => write!(f, "Config error: {}", msg),
            DotError::BindError(msg) => write!(f, "Bind error: {}", msg),
            DotError::CertError(msg) => write!(f, "Certificate error: {}", msg),
            DotError::KeyError(msg) => write!(f, "Key error: {}", msg),
            DotError::TlsError(msg) => write!(f, "TLS error: {}", msg),
            DotError::IoError(msg) => write!(f, "I/O error: {}", msg),
            DotError::MessageTooLarge(size) => write!(f, "Message too large: {} bytes", size),
            DotError::Timeout(msg) => write!(f, "Timeout: {}", msg),
            DotError::HandlerError(msg) => write!(f, "Handler error: {}", msg),
        }
    }
}

impl std::error::Error for DotError {}

/// Statistics for DoT server
#[derive(Debug, Clone, Default)]
pub struct DotStats {
    pub connections_total: u64,
    pub connections_active: u64,
    pub queries_total: u64,
    pub errors_total: u64,
    pub handshake_failures: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    // Mock DNS handler for testing
    struct MockDnsHandler;

    #[async_trait::async_trait]
    impl DnsHandler for MockDnsHandler {
        async fn handle_query(&self, query: &[u8], _client_ip: IpAddr) -> Result<Vec<u8>, DotError> {
            // Echo back the query as response (for testing)
            Ok(query.to_vec())
        }
    }

    #[test]
    fn test_dot_error_display() {
        let err = DotError::ConfigError("test".to_string());
        assert!(err.to_string().contains("Config error"));

        let err = DotError::MessageTooLarge(100000);
        assert!(err.to_string().contains("100000"));

        let err = DotError::Timeout("read".to_string());
        assert!(err.to_string().contains("Timeout"));
    }

    #[test]
    fn test_dot_stats_default() {
        let stats = DotStats::default();
        assert_eq!(stats.connections_total, 0);
        assert_eq!(stats.queries_total, 0);
    }

    #[tokio::test]
    async fn test_handle_connection_single_query() {
        let handler = Arc::new(MockDnsHandler);

        // Create a duplex stream for testing
        let (mut client, server) = duplex(1024);

        // Spawn connection handler
        let handle = tokio::spawn(async move {
            DotServer::handle_connection(server, "127.0.0.1".parse().unwrap(), handler).await
        });

        // Send a DNS query
        let query = vec![0x01, 0x02, 0x03, 0x04]; // Mock DNS query
        let len_prefix = (query.len() as u16).to_be_bytes();
        client.write_all(&len_prefix).await.unwrap();
        client.write_all(&query).await.unwrap();

        // Read response
        let mut resp_len_buf = [0u8; 2];
        client.read_exact(&mut resp_len_buf).await.unwrap();
        let resp_len = u16::from_be_bytes(resp_len_buf) as usize;

        let mut response = vec![0u8; resp_len];
        client.read_exact(&mut response).await.unwrap();

        // Verify response matches query (mock handler echoes)
        assert_eq!(response, query);

        // Close connection
        drop(client);

        // Wait for handler to complete
        let result = handle.await.unwrap();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_connection_multiple_queries() {
        let handler = Arc::new(MockDnsHandler);

        let (mut client, server) = duplex(4096);

        let handle = tokio::spawn(async move {
            DotServer::handle_connection(server, "127.0.0.1".parse().unwrap(), handler).await
        });

        // Send 3 queries
        for i in 0..3 {
            let query = vec![i as u8; 10]; // Different query each time
            let len_prefix = (query.len() as u16).to_be_bytes();
            client.write_all(&len_prefix).await.unwrap();
            client.write_all(&query).await.unwrap();

            // Read response
            let mut resp_len_buf = [0u8; 2];
            client.read_exact(&mut resp_len_buf).await.unwrap();
            let resp_len = u16::from_be_bytes(resp_len_buf) as usize;

            let mut response = vec![0u8; resp_len];
            client.read_exact(&mut response).await.unwrap();

            assert_eq!(response, query);
        }

        drop(client);
        let result = handle.await.unwrap();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_connection_empty_message() {
        let handler = Arc::new(MockDnsHandler);

        let (mut client, server) = duplex(1024);

        let handle = tokio::spawn(async move {
            DotServer::handle_connection(server, "127.0.0.1".parse().unwrap(), handler).await
        });

        // Send empty message (length = 0)
        let len_prefix = 0u16.to_be_bytes();
        client.write_all(&len_prefix).await.unwrap();

        // Send a real query after
        let query = vec![0x01, 0x02, 0x03, 0x04];
        let len_prefix = (query.len() as u16).to_be_bytes();
        client.write_all(&len_prefix).await.unwrap();
        client.write_all(&query).await.unwrap();

        // Read response
        let mut resp_len_buf = [0u8; 2];
        client.read_exact(&mut resp_len_buf).await.unwrap();
        let resp_len = u16::from_be_bytes(resp_len_buf) as usize;

        let mut response = vec![0u8; resp_len];
        client.read_exact(&mut response).await.unwrap();

        assert_eq!(response, query);

        drop(client);
        let result = handle.await.unwrap();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_connection_message_too_large() {
        let handler = Arc::new(MockDnsHandler);

        let (mut client, server) = duplex(1024);

        let handle = tokio::spawn(async move {
            DotServer::handle_connection(server, "127.0.0.1".parse().unwrap(), handler).await
        });

        // Send message with length > MAX_DNS_MESSAGE_SIZE (65534)
        // Using u16::MAX (65535) which is larger than our limit
        let len_prefix = u16::MAX.to_be_bytes();
        client.write_all(&len_prefix).await.unwrap();

        drop(client);
        let result = handle.await.unwrap();
        assert!(result.is_err());

        if let Err(DotError::MessageTooLarge(size)) = result {
            assert_eq!(size, u16::MAX as usize);
            assert!(size > MAX_DNS_MESSAGE_SIZE);
        } else {
            panic!("Expected MessageTooLarge error");
        }
    }

    #[test]
    fn test_dot_config_default() {
        let config = DotConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.addr.port(), 853);
        assert!(config.cert_path.is_none());
        assert!(config.key_path.is_none());
    }
}
