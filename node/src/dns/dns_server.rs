//! AEGIS DNS Server
//!
//! A custom DNS server implementation using Hickory DNS protocol parsing
//! with our own zone store and rate limiting integration.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use hickory_proto::op::{Header, Message, MessageType, OpCode, Query, ResponseCode};
use hickory_proto::rr::{DNSClass, Name, RData, Record, RecordType};
use hickory_proto::serialize::binary::{BinDecodable, BinEncodable};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::Semaphore;
use tokio::time::timeout;
use tracing::{debug, error, info, warn};

use super::{
    DnsConfig, DnsError, DnsRecord, DnsRecordType, DnsRecordValue, DnsRateLimiter,
    TcpConnectionTracker, Zone, ZoneStore,
};

/// Maximum UDP DNS message size
const MAX_UDP_SIZE: usize = 512;

/// Maximum EDNS UDP message size
const MAX_EDNS_SIZE: usize = 4096;

/// Maximum TCP DNS message size
const MAX_TCP_SIZE: usize = 65535;

/// TCP read timeout
const TCP_TIMEOUT: Duration = Duration::from_secs(10);

/// AEGIS DNS Server
pub struct DnsServer {
    /// Server configuration
    config: DnsConfig,
    /// Zone storage
    zone_store: Arc<ZoneStore>,
    /// Rate limiter
    rate_limiter: Arc<DnsRateLimiter>,
    /// TCP connection tracker
    tcp_tracker: Arc<TcpConnectionTracker>,
    /// Semaphore for limiting concurrent TCP connections
    tcp_semaphore: Arc<Semaphore>,
}

impl DnsServer {
    /// Create a new DNS server
    pub fn new(config: DnsConfig, zone_store: Arc<ZoneStore>) -> Result<Self, DnsError> {
        config.validate()?;

        let rate_limiter = Arc::new(DnsRateLimiter::new(&config.rate_limit));
        let tcp_tracker = Arc::new(TcpConnectionTracker::new(
            config.tcp_limits.max_per_ip,
            config.tcp_limits.max_connections,
        ));
        let tcp_semaphore = Arc::new(Semaphore::new(config.tcp_limits.max_connections));

        Ok(Self {
            config,
            zone_store,
            rate_limiter,
            tcp_tracker,
            tcp_semaphore,
        })
    }

    /// Start the DNS server (runs until shutdown)
    pub async fn run(&self) -> Result<(), DnsError> {
        info!(
            "Starting AEGIS DNS server on UDP {} and TCP {}",
            self.config.udp_addr, self.config.tcp_addr
        );

        // Bind UDP socket
        let udp_socket = UdpSocket::bind(&self.config.udp_addr)
            .await
            .map_err(|e| DnsError::ServerError(format!("Failed to bind UDP: {}", e)))?;

        // Bind TCP listener
        let tcp_listener = TcpListener::bind(&self.config.tcp_addr)
            .await
            .map_err(|e| DnsError::ServerError(format!("Failed to bind TCP: {}", e)))?;

        info!("DNS server listening on UDP and TCP port 53");

        // Run UDP and TCP servers concurrently
        tokio::select! {
            result = self.run_udp_server(udp_socket) => {
                error!("UDP server stopped: {:?}", result);
                result
            }
            result = self.run_tcp_server(tcp_listener) => {
                error!("TCP server stopped: {:?}", result);
                result
            }
        }
    }

    /// Run the UDP DNS server
    async fn run_udp_server(&self, socket: UdpSocket) -> Result<(), DnsError> {
        let socket = Arc::new(socket);
        let mut buf = vec![0u8; MAX_EDNS_SIZE];

        loop {
            let (len, addr) = socket
                .recv_from(&mut buf)
                .await
                .map_err(|e| DnsError::ServerError(format!("UDP recv error: {}", e)))?;

            let query_bytes = buf[..len].to_vec();
            let socket = Arc::clone(&socket);
            let zone_store = Arc::clone(&self.zone_store);
            let rate_limiter = Arc::clone(&self.rate_limiter);
            let config = self.config.clone();

            // Handle query in background task
            tokio::spawn(async move {
                if let Err(e) = Self::handle_udp_query(
                    socket,
                    addr,
                    query_bytes,
                    zone_store,
                    rate_limiter,
                    &config,
                )
                .await
                {
                    debug!("UDP query error from {}: {}", addr, e);
                }
            });
        }
    }

    /// Handle a single UDP query
    async fn handle_udp_query(
        socket: Arc<UdpSocket>,
        addr: SocketAddr,
        query_bytes: Vec<u8>,
        zone_store: Arc<ZoneStore>,
        rate_limiter: Arc<DnsRateLimiter>,
        config: &DnsConfig,
    ) -> Result<(), DnsError> {
        // Rate limiting
        if !rate_limiter.check(addr.ip()).await {
            debug!("Rate limited UDP query from {}", addr);
            return Ok(()); // Silently drop rate-limited queries
        }

        // Parse query
        let query = Message::from_vec(&query_bytes)
            .map_err(|e| DnsError::ServerError(format!("Failed to parse query: {}", e)))?;

        // Build response
        let response = Self::build_response(&query, &zone_store, config, addr.ip()).await;

        // Serialize and send
        let response_bytes = response
            .to_vec()
            .map_err(|e| DnsError::ServerError(format!("Failed to serialize response: {}", e)))?;

        socket
            .send_to(&response_bytes, addr)
            .await
            .map_err(|e| DnsError::ServerError(format!("Failed to send response: {}", e)))?;

        Ok(())
    }

    /// Run the TCP DNS server
    async fn run_tcp_server(&self, listener: TcpListener) -> Result<(), DnsError> {
        loop {
            let (stream, addr) = listener
                .accept()
                .await
                .map_err(|e| DnsError::ServerError(format!("TCP accept error: {}", e)))?;

            // Check connection limits
            if !self.tcp_tracker.try_accept(addr.ip()).await {
                debug!("TCP connection limit reached for {}", addr);
                continue;
            }

            // Acquire semaphore permit
            let permit = match self.tcp_semaphore.clone().try_acquire_owned() {
                Ok(permit) => permit,
                Err(_) => {
                    debug!("TCP semaphore full, rejecting connection from {}", addr);
                    self.tcp_tracker.release(addr.ip()).await;
                    continue;
                }
            };

            let zone_store = Arc::clone(&self.zone_store);
            let rate_limiter = Arc::clone(&self.rate_limiter);
            let tcp_tracker = Arc::clone(&self.tcp_tracker);
            let config = self.config.clone();

            tokio::spawn(async move {
                if let Err(e) = Self::handle_tcp_connection(
                    stream,
                    addr,
                    zone_store,
                    rate_limiter,
                    &config,
                )
                .await
                {
                    debug!("TCP connection error from {}: {}", addr, e);
                }

                tcp_tracker.release(addr.ip()).await;
                drop(permit);
            });
        }
    }

    /// Handle a TCP connection (may include multiple queries)
    async fn handle_tcp_connection(
        mut stream: TcpStream,
        addr: SocketAddr,
        zone_store: Arc<ZoneStore>,
        rate_limiter: Arc<DnsRateLimiter>,
        config: &DnsConfig,
    ) -> Result<(), DnsError> {
        loop {
            // Read 2-byte length prefix
            let mut len_buf = [0u8; 2];
            match timeout(TCP_TIMEOUT, stream.read_exact(&mut len_buf)).await {
                Ok(Ok(_)) => {}
                Ok(Err(_)) | Err(_) => break, // Connection closed or timeout
            }

            let msg_len = u16::from_be_bytes(len_buf) as usize;
            if msg_len > MAX_TCP_SIZE {
                warn!("TCP message too large from {}: {} bytes", addr, msg_len);
                break;
            }

            // Read message
            let mut msg_buf = vec![0u8; msg_len];
            match timeout(TCP_TIMEOUT, stream.read_exact(&mut msg_buf)).await {
                Ok(Ok(_)) => {}
                Ok(Err(_)) | Err(_) => break,
            }

            // Rate limiting
            if !rate_limiter.check(addr.ip()).await {
                debug!("Rate limited TCP query from {}", addr);
                // Send REFUSED response
                if let Ok(query) = Message::from_vec(&msg_buf) {
                    let response = Self::build_refused_response(&query);
                    let response_bytes = response.to_vec().unwrap_or_default();
                    let len_prefix = (response_bytes.len() as u16).to_be_bytes();
                    let _ = stream.write_all(&len_prefix).await;
                    let _ = stream.write_all(&response_bytes).await;
                }
                continue;
            }

            // Parse and handle query
            let query = match Message::from_vec(&msg_buf) {
                Ok(q) => q,
                Err(e) => {
                    debug!("Failed to parse TCP query from {}: {}", addr, e);
                    continue;
                }
            };

            let response = Self::build_response(&query, &zone_store, config, addr.ip()).await;

            // Send response with length prefix
            let response_bytes = response.to_vec().unwrap_or_default();
            let len_prefix = (response_bytes.len() as u16).to_be_bytes();

            if stream.write_all(&len_prefix).await.is_err() {
                break;
            }
            if stream.write_all(&response_bytes).await.is_err() {
                break;
            }
        }

        Ok(())
    }

    /// Build a DNS response message
    async fn build_response(
        query: &Message,
        zone_store: &ZoneStore,
        config: &DnsConfig,
        client_ip: IpAddr,
    ) -> Message {
        let mut response = Message::new();
        response.set_id(query.id());
        response.set_message_type(MessageType::Response);
        response.set_op_code(OpCode::Query);
        response.set_recursion_desired(query.recursion_desired());
        response.set_recursion_available(false); // We're authoritative only
        response.set_authentic_data(false);
        response.set_checking_disabled(query.checking_disabled());

        // Copy questions
        for question in query.queries() {
            response.add_query(question.clone());
        }

        // We only handle standard queries
        if query.op_code() != OpCode::Query {
            response.set_response_code(ResponseCode::NotImp);
            return response;
        }

        // Process each question
        for question in query.queries() {
            Self::process_question(&mut response, question, zone_store, config, client_ip).await;
        }

        response
    }

    /// Process a single DNS question
    async fn process_question(
        response: &mut Message,
        question: &Query,
        zone_store: &ZoneStore,
        config: &DnsConfig,
        _client_ip: IpAddr,
    ) {
        let qname = question.name().to_string();
        let qname = qname.trim_end_matches('.');
        let qtype = convert_record_type(question.query_type());

        debug!("Processing query for {} {:?}", qname, question.query_type());

        // Try to resolve from zone store
        match zone_store.resolve(qname, qtype).await {
            Some((zone, records)) => {
                // Set authoritative answer flag
                response.set_authoritative(true);

                if records.is_empty() {
                    // NXDOMAIN - name exists in zone but no records of requested type
                    // Actually this should be NODATA (NOERROR with no answers)
                    response.set_response_code(ResponseCode::NoError);

                    // Add SOA to authority section for negative caching
                    if let Some(soa) = zone.soa_record() {
                        if let Some(rr) = convert_to_hickory_record(soa, &zone.domain) {
                            response.add_name_server(rr);
                        }
                    }
                } else {
                    response.set_response_code(ResponseCode::NoError);

                    // Convert and add records
                    for record in &records {
                        // Check if record should be proxied (return AEGIS anycast IP)
                        let record_to_use = if record.proxied && zone.proxied {
                            Self::create_proxied_record(record, config, &zone.domain)
                        } else {
                            convert_to_hickory_record(record, &zone.domain)
                        };

                        if let Some(rr) = record_to_use {
                            response.add_answer(rr);
                        }
                    }

                    // Add NS records to authority section
                    for ns in zone.ns_records() {
                        if let Some(rr) = convert_to_hickory_record(ns, &zone.domain) {
                            response.add_name_server(rr);
                        }
                    }
                }
            }
            None => {
                // Zone not found - we're not authoritative
                response.set_response_code(ResponseCode::Refused);
            }
        }
    }

    /// Create a proxied record (return AEGIS anycast IP)
    fn create_proxied_record(
        record: &DnsRecord,
        config: &DnsConfig,
        zone_domain: &str,
    ) -> Option<Record<RData>> {
        let name = if record.name == "@" || record.name.is_empty() {
            Name::from_ascii(zone_domain).ok()?
        } else {
            Name::from_ascii(&format!("{}.{}", record.name, zone_domain)).ok()?
        };

        match record.record_type {
            DnsRecordType::A => {
                let ip: Ipv4Addr = config
                    .edge
                    .anycast_ipv4
                    .as_ref()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_else(|| match &record.value {
                        DnsRecordValue::A(ip) => *ip,
                        _ => Ipv4Addr::UNSPECIFIED,
                    });

                let rdata = RData::A(hickory_proto::rr::rdata::A(ip));
                Some(Record::from_rdata(name, record.ttl, rdata))
            }
            DnsRecordType::AAAA => {
                let ip: Ipv6Addr = config
                    .edge
                    .anycast_ipv6
                    .as_ref()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_else(|| match &record.value {
                        DnsRecordValue::AAAA(ip) => *ip,
                        _ => Ipv6Addr::UNSPECIFIED,
                    });

                let rdata = RData::AAAA(hickory_proto::rr::rdata::AAAA(ip));
                Some(Record::from_rdata(name, record.ttl, rdata))
            }
            _ => convert_to_hickory_record(record, zone_domain),
        }
    }

    /// Build a REFUSED response
    fn build_refused_response(query: &Message) -> Message {
        let mut response = Message::new();
        response.set_id(query.id());
        response.set_message_type(MessageType::Response);
        response.set_op_code(OpCode::Query);
        response.set_response_code(ResponseCode::Refused);

        for question in query.queries() {
            response.add_query(question.clone());
        }

        response
    }

    /// Get reference to zone store
    pub fn zone_store(&self) -> &Arc<ZoneStore> {
        &self.zone_store
    }

    /// Get reference to rate limiter
    pub fn rate_limiter(&self) -> &Arc<DnsRateLimiter> {
        &self.rate_limiter
    }

    /// Get reference to TCP tracker
    pub fn tcp_tracker(&self) -> &Arc<TcpConnectionTracker> {
        &self.tcp_tracker
    }

    /// Get server configuration
    pub fn config(&self) -> &DnsConfig {
        &self.config
    }
}

/// Convert AEGIS record type to Hickory record type
fn convert_record_type(rt: RecordType) -> DnsRecordType {
    match rt {
        RecordType::A => DnsRecordType::A,
        RecordType::AAAA => DnsRecordType::AAAA,
        RecordType::CNAME => DnsRecordType::CNAME,
        RecordType::MX => DnsRecordType::MX,
        RecordType::TXT => DnsRecordType::TXT,
        RecordType::NS => DnsRecordType::NS,
        RecordType::SOA => DnsRecordType::SOA,
        RecordType::CAA => DnsRecordType::CAA,
        RecordType::SRV => DnsRecordType::SRV,
        RecordType::PTR => DnsRecordType::PTR,
        _ => DnsRecordType::A, // Default fallback
    }
}

/// Convert AEGIS DNS record to Hickory DNS record
fn convert_to_hickory_record(record: &DnsRecord, zone_domain: &str) -> Option<Record<RData>> {
    let name = if record.name == "@" || record.name.is_empty() {
        Name::from_ascii(zone_domain).ok()?
    } else {
        Name::from_ascii(&format!("{}.{}", record.name, zone_domain)).ok()?
    };

    let rdata = match &record.value {
        DnsRecordValue::A(ip) => {
            RData::A(hickory_proto::rr::rdata::A(*ip))
        }
        DnsRecordValue::AAAA(ip) => {
            RData::AAAA(hickory_proto::rr::rdata::AAAA(*ip))
        }
        DnsRecordValue::CNAME(target) => {
            let target_name = Name::from_ascii(target).ok()?;
            RData::CNAME(hickory_proto::rr::rdata::CNAME(target_name))
        }
        DnsRecordValue::MX { exchange } => {
            let exchange_name = Name::from_ascii(exchange).ok()?;
            let priority = record.priority.unwrap_or(10);
            RData::MX(hickory_proto::rr::rdata::MX::new(priority, exchange_name))
        }
        DnsRecordValue::TXT(text) => {
            RData::TXT(hickory_proto::rr::rdata::TXT::new(vec![text.clone()]))
        }
        DnsRecordValue::NS(nameserver) => {
            let ns_name = Name::from_ascii(nameserver).ok()?;
            RData::NS(hickory_proto::rr::rdata::NS(ns_name))
        }
        DnsRecordValue::SOA {
            mname,
            rname,
            serial,
            refresh,
            retry,
            expire,
            minimum,
        } => {
            let mname = Name::from_ascii(mname).ok()?;
            let rname = Name::from_ascii(rname).ok()?;
            RData::SOA(hickory_proto::rr::rdata::SOA::new(
                mname, rname, *serial, *refresh as i32, *retry as i32, *expire as i32, *minimum,
            ))
        }
        DnsRecordValue::CAA { flags: _, tag: _, value: _ } => {
            // CAA records require hickory_proto::rr::rdata::caa::KeyValue
            // which has a complex API. Skip for now - will be implemented in Sprint 30.4
            return None;
        }
        DnsRecordValue::SRV {
            weight,
            port,
            target,
        } => {
            let target_name = Name::from_ascii(target).ok()?;
            let priority = record.priority.unwrap_or(0);
            RData::SRV(hickory_proto::rr::rdata::SRV::new(
                priority, *weight, *port, target_name,
            ))
        }
        DnsRecordValue::PTR(target) => {
            let target_name = Name::from_ascii(target).ok()?;
            RData::PTR(hickory_proto::rr::rdata::PTR(target_name))
        }
        // DNSSEC records would be handled here in Sprint 30.4
        _ => return None,
    };

    Some(Record::from_rdata(name, record.ttl, rdata))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_record_type() {
        assert_eq!(convert_record_type(RecordType::A), DnsRecordType::A);
        assert_eq!(convert_record_type(RecordType::AAAA), DnsRecordType::AAAA);
        assert_eq!(convert_record_type(RecordType::CNAME), DnsRecordType::CNAME);
        assert_eq!(convert_record_type(RecordType::MX), DnsRecordType::MX);
        assert_eq!(convert_record_type(RecordType::TXT), DnsRecordType::TXT);
        assert_eq!(convert_record_type(RecordType::NS), DnsRecordType::NS);
        assert_eq!(convert_record_type(RecordType::SOA), DnsRecordType::SOA);
    }

    #[test]
    fn test_convert_a_record() {
        let record = DnsRecord::a("www", "192.168.1.1".parse().unwrap(), 300);
        let rr = convert_to_hickory_record(&record, "example.com").unwrap();

        // Hickory DNS Name::from_ascii doesn't include trailing dot in to_string()
        assert_eq!(rr.name().to_string(), "www.example.com");
        assert_eq!(rr.ttl(), 300);
        assert_eq!(rr.record_type(), RecordType::A);
    }

    #[test]
    fn test_convert_root_record() {
        let record = DnsRecord::a("@", "192.168.1.1".parse().unwrap(), 300);
        let rr = convert_to_hickory_record(&record, "example.com").unwrap();

        assert_eq!(rr.name().to_string(), "example.com");
    }

    #[test]
    fn test_convert_cname_record() {
        let record = DnsRecord::cname("www", "example.com", 300);
        let rr = convert_to_hickory_record(&record, "example.com").unwrap();

        assert_eq!(rr.record_type(), RecordType::CNAME);
    }

    #[test]
    fn test_convert_mx_record() {
        let record = DnsRecord::mx("@", "mail.example.com", 10, 300);
        let rr = convert_to_hickory_record(&record, "example.com").unwrap();

        assert_eq!(rr.record_type(), RecordType::MX);
    }

    #[test]
    fn test_convert_txt_record() {
        let record = DnsRecord::txt("@", "v=spf1 include:_spf.google.com ~all", 300);
        let rr = convert_to_hickory_record(&record, "example.com").unwrap();

        assert_eq!(rr.record_type(), RecordType::TXT);
    }

    #[tokio::test]
    async fn test_build_response_zone_not_found() {
        let zone_store = Arc::new(ZoneStore::new());
        let config = DnsConfig::default();

        let mut query = Message::new();
        query.set_id(1234);
        query.set_message_type(MessageType::Query);
        query.set_op_code(OpCode::Query);
        query.add_query(Query::query(
            Name::from_ascii("nonexistent.com").unwrap(),
            RecordType::A,
        ));

        let response =
            DnsServer::build_response(&query, &zone_store, &config, "127.0.0.1".parse().unwrap())
                .await;

        assert_eq!(response.response_code(), ResponseCode::Refused);
    }

    #[tokio::test]
    async fn test_build_response_with_records() {
        let zone_store = Arc::new(ZoneStore::new());

        // Create zone with record
        let mut zone = Zone::new("example.com", false);
        zone.add_record(DnsRecord::a("www", "192.168.1.1".parse().unwrap(), 300));
        zone_store.upsert_zone(zone).await.unwrap();

        let config = DnsConfig::default();

        let mut query = Message::new();
        query.set_id(1234);
        query.set_message_type(MessageType::Query);
        query.set_op_code(OpCode::Query);
        query.add_query(Query::query(
            Name::from_ascii("www.example.com").unwrap(),
            RecordType::A,
        ));

        let response =
            DnsServer::build_response(&query, &zone_store, &config, "127.0.0.1".parse().unwrap())
                .await;

        assert_eq!(response.response_code(), ResponseCode::NoError);
        assert_eq!(response.answers().len(), 1);
        assert!(response.authoritative());
    }

    #[tokio::test]
    async fn test_build_response_no_records() {
        let zone_store = Arc::new(ZoneStore::new());

        // Create zone without www record
        let mut zone = Zone::new("example.com", false);
        zone.create_default_records(&["ns1.aegis.network".to_string()]);
        zone_store.upsert_zone(zone).await.unwrap();

        let config = DnsConfig::default();

        let mut query = Message::new();
        query.set_id(1234);
        query.set_message_type(MessageType::Query);
        query.set_op_code(OpCode::Query);
        query.add_query(Query::query(
            Name::from_ascii("www.example.com").unwrap(),
            RecordType::A,
        ));

        let response =
            DnsServer::build_response(&query, &zone_store, &config, "127.0.0.1".parse().unwrap())
                .await;

        // NODATA response (zone exists but record doesn't)
        assert_eq!(response.response_code(), ResponseCode::NoError);
        assert_eq!(response.answers().len(), 0);
        // Should have SOA in authority section
        assert!(!response.name_servers().is_empty());
    }

    #[test]
    fn test_build_refused_response() {
        let mut query = Message::new();
        query.set_id(5678);
        query.set_message_type(MessageType::Query);
        query.add_query(Query::query(
            Name::from_ascii("test.com").unwrap(),
            RecordType::A,
        ));

        let response = DnsServer::build_refused_response(&query);

        assert_eq!(response.id(), 5678);
        assert_eq!(response.response_code(), ResponseCode::Refused);
        assert_eq!(response.queries().len(), 1);
    }
}
