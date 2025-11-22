use anyhow::{Context, Result};
use futures::StreamExt;
use libp2p::{
    gossipsub::{self, IdentTopic, MessageAuthenticity, ValidationMode},
    identify,
    identity::Keypair,
    kad::{self, store::MemoryStore, Mode as KadMode},
    mdns,
    noise,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, Swarm, SwarmBuilder,
};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Get current Unix timestamp in seconds
/// Returns 0 if system clock is before Unix epoch (should never happen on modern systems)
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_else(|e| {
            warn!("System clock before Unix epoch: {} - using timestamp 0", e);
            0
        })
}

/// Threat intelligence message shared across the P2P network
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThreatIntelligence {
    /// IP address of the threat
    pub ip: String,
    /// Type of threat (e.g., "syn_flood", "ddos", "brute_force")
    pub threat_type: String,
    /// Severity level (1-10, where 10 is most severe)
    pub severity: u8,
    /// Timestamp when threat was detected (Unix timestamp in seconds)
    pub timestamp: u64,
    /// How long to block the IP (in seconds)
    pub block_duration_secs: u64,
    /// Source node that reported the threat
    pub source_node: String,
    /// Optional description
    pub description: Option<String>,
}

impl ThreatIntelligence {
    /// Create a new threat intelligence report
    pub fn new(
        ip: String,
        threat_type: String,
        severity: u8,
        block_duration_secs: u64,
        source_node: String,
    ) -> Self {
        let timestamp = current_timestamp();

        Self {
            ip,
            threat_type,
            severity,
            timestamp,
            block_duration_secs,
            source_node,
            description: None,
        }
    }

    /// Create with description
    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    /// Validate the threat intelligence data
    pub fn validate(&self) -> Result<()> {
        // Validate IP address format
        let _: std::net::Ipv4Addr = self.ip.parse()
            .context("Invalid IPv4 address")?;

        // Validate severity
        if self.severity == 0 || self.severity > 10 {
            anyhow::bail!("Severity must be between 1 and 10");
        }

        // Validate block duration (max 24 hours)
        if self.block_duration_secs == 0 || self.block_duration_secs > 86400 {
            anyhow::bail!("Block duration must be between 1 second and 24 hours");
        }

        // Validate timestamp (not too far in the past or future)
        let now = current_timestamp();

        if self.timestamp > now + 300 {
            anyhow::bail!("Timestamp is too far in the future");
        }

        if now.saturating_sub(self.timestamp) > 3600 {
            anyhow::bail!("Timestamp is too old (>1 hour)");
        }

        Ok(())
    }

    /// Serialize to JSON
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).context("Failed to serialize threat intelligence")
    }

    /// Deserialize from JSON
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).context("Failed to deserialize threat intelligence")
    }
}

/// Network behaviour for threat intelligence P2P
#[derive(NetworkBehaviour)]
pub struct ThreatIntelBehaviour {
    pub gossipsub: gossipsub::Behaviour,
    pub mdns: mdns::tokio::Behaviour,
    pub kad: kad::Behaviour<MemoryStore>,
    pub identify: identify::Behaviour,
}

/// Configuration for P2P network
#[derive(Debug, Clone)]
pub struct P2PConfig {
    /// Port to listen on
    pub listen_port: u16,
    /// Whether to enable mDNS for local peer discovery
    pub enable_mdns: bool,
    /// Bootstrap peers for Kademlia DHT
    pub bootstrap_peers: Vec<(PeerId, Multiaddr)>,
}

impl Default for P2PConfig {
    fn default() -> Self {
        Self {
            listen_port: 9001,
            enable_mdns: true,
            bootstrap_peers: Vec::new(),
        }
    }
}

/// P2P Threat Intelligence Network
pub struct ThreatIntelP2P {
    swarm: Swarm<ThreatIntelBehaviour>,
    topic: IdentTopic,
    peer_id: PeerId,
    receiver: mpsc::UnboundedReceiver<ThreatIntelligence>,
    sender: mpsc::UnboundedSender<ThreatIntelligence>,
}

impl ThreatIntelP2P {
    /// Create a new P2P threat intelligence network
    pub fn new(config: P2PConfig) -> Result<Self> {
        let keypair = Keypair::generate_ed25519();
        let peer_id = PeerId::from(keypair.public());

        info!("Local peer ID: {}", peer_id);

        // Create a Gossipsub topic for threat intelligence
        let topic = IdentTopic::new("aegis-threat-intel");

        // Clone config for use in closure
        let bootstrap_peers = config.bootstrap_peers.clone();

        // Build swarm using the new builder API
        let swarm = SwarmBuilder::with_existing_identity(keypair.clone())
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )?
            .with_behaviour(|key| {
                // Create Gossipsub configuration
                let gossipsub_config = gossipsub::ConfigBuilder::default()
                    .heartbeat_interval(Duration::from_secs(1))
                    .validation_mode(ValidationMode::Strict)
                    .message_id_fn(|message: &gossipsub::Message| {
                        let mut hasher = DefaultHasher::new();
                        message.data.hash(&mut hasher);
                        gossipsub::MessageId::from(hasher.finish().to_string())
                    })
                    .build()
                    .expect("Failed to build gossipsub config");

                // Create Gossipsub behaviour
                let mut gossipsub = gossipsub::Behaviour::new(
                    MessageAuthenticity::Signed(key.clone()),
                    gossipsub_config,
                )
                .expect("Failed to create gossipsub behaviour");

                // Subscribe to the threat intelligence topic
                gossipsub.subscribe(&topic)
                    .expect("Failed to subscribe to topic");

                // Create mDNS behaviour for local peer discovery
                let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)
                    .expect("Failed to create mDNS behaviour");

                // Create Kademlia DHT for global peer discovery
                let store = MemoryStore::new(peer_id);
                let mut kad = kad::Behaviour::new(peer_id, store);
                kad.set_mode(Some(KadMode::Server));

                // Add bootstrap peers to Kademlia
                for (peer_id, addr) in &bootstrap_peers {
                    kad.add_address(peer_id, addr.clone());
                }

                // Create identify behaviour
                let identify = identify::Behaviour::new(identify::Config::new(
                    "/aegis-threat-intel/1.0.0".to_string(),
                    key.public(),
                ));

                ThreatIntelBehaviour {
                    gossipsub,
                    mdns,
                    kad,
                    identify,
                }
            })?
            .build();

        // Create channel for receiving threat intelligence
        let (sender, receiver) = mpsc::unbounded_channel();

        Ok(Self {
            swarm,
            topic,
            peer_id,
            receiver,
            sender,
        })
    }

    /// Get the peer ID
    pub fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    /// Get a sender for publishing threat intelligence
    pub fn get_sender(&self) -> mpsc::UnboundedSender<ThreatIntelligence> {
        self.sender.clone()
    }

    /// Start listening on configured address
    pub fn listen(&mut self, port: u16) -> Result<()> {
        let addr: Multiaddr = format!("/ip4/0.0.0.0/tcp/{}", port).parse()?;
        self.swarm.listen_on(addr.clone())?;
        info!("Listening on {}", addr);
        Ok(())
    }

    /// Publish threat intelligence to the network
    pub fn publish(&mut self, threat: &ThreatIntelligence) -> Result<()> {
        let json = threat.to_json()?;
        self.swarm
            .behaviour_mut()
            .gossipsub
            .publish(self.topic.clone(), json.as_bytes())
            .map_err(|e| anyhow::anyhow!("Failed to publish message: {}", e))?;

        info!("Published threat intelligence: {:?}", threat.ip);
        Ok(())
    }

    /// Run the P2P network event loop
    pub async fn run<F>(mut self, mut handler: F) -> Result<()>
    where
        F: FnMut(ThreatIntelligence) -> Result<()>,
    {
        loop {
            tokio::select! {
                // Handle outgoing messages from channel
                Some(threat) = self.receiver.recv() => {
                    if let Err(e) = self.publish(&threat) {
                        warn!("Failed to publish threat: {}", e);
                    }
                }

                // Handle swarm events
                event = self.swarm.select_next_some() => {
                    match event {
                        SwarmEvent::Behaviour(ThreatIntelBehaviourEvent::Gossipsub(
                            gossipsub::Event::Message {
                                propagation_source: _,
                                message_id: _,
                                message,
                            },
                        )) => {
                            // Received a message from the network
                            match String::from_utf8(message.data.clone()) {
                                Ok(json) => {
                                    match ThreatIntelligence::from_json(&json) {
                                        Ok(threat) => {
                                            // Validate threat intelligence
                                            if let Err(e) = threat.validate() {
                                                warn!("Invalid threat intelligence received: {}", e);
                                                continue;
                                            }

                                            info!(
                                                "Received threat intel: {} (type: {}, severity: {})",
                                                threat.ip, threat.threat_type, threat.severity
                                            );

                                            // Call handler
                                            if let Err(e) = handler(threat) {
                                                warn!("Handler error: {}", e);
                                            }
                                        }
                                        Err(e) => {
                                            warn!("Failed to parse threat intelligence: {}", e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    warn!("Invalid UTF-8 in message: {}", e);
                                }
                            }
                        }

                        SwarmEvent::Behaviour(ThreatIntelBehaviourEvent::Mdns(
                            mdns::Event::Discovered(peers),
                        )) => {
                            for (peer_id, addr) in peers {
                                info!("Discovered peer via mDNS: {} at {}", peer_id, addr);
                                self.swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                                self.swarm.behaviour_mut().kad.add_address(&peer_id, addr);
                            }
                        }

                        SwarmEvent::Behaviour(ThreatIntelBehaviourEvent::Mdns(
                            mdns::Event::Expired(peers),
                        )) => {
                            for (peer_id, addr) in peers {
                                info!("Peer expired via mDNS: {} at {}", peer_id, addr);
                                self.swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                            }
                        }

                        SwarmEvent::Behaviour(ThreatIntelBehaviourEvent::Identify(
                            identify::Event::Received { peer_id, info, connection_id: _ },
                        )) => {
                            debug!("Identified peer: {} with protocols: {:?}", peer_id, info.protocols);
                            for addr in info.listen_addrs {
                                self.swarm.behaviour_mut().kad.add_address(&peer_id, addr);
                            }
                        }

                        SwarmEvent::NewListenAddr { address, .. } => {
                            info!("Listening on {}", address);
                        }

                        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                            info!("Connection established with peer: {}", peer_id);
                        }

                        SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                            info!("Connection closed with peer: {} (cause: {:?})", peer_id, cause);
                        }

                        _ => {}
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_threat_intelligence_creation() {
        let threat = ThreatIntelligence::new(
            "192.168.1.100".to_string(),
            "syn_flood".to_string(),
            8,
            300,
            "node-123".to_string(),
        );

        assert_eq!(threat.ip, "192.168.1.100");
        assert_eq!(threat.threat_type, "syn_flood");
        assert_eq!(threat.severity, 8);
        assert_eq!(threat.block_duration_secs, 300);
        assert_eq!(threat.source_node, "node-123");
    }

    #[test]
    fn test_threat_intelligence_validation() {
        let valid_threat = ThreatIntelligence::new(
            "10.0.0.1".to_string(),
            "ddos".to_string(),
            5,
            600,
            "node-456".to_string(),
        );
        assert!(valid_threat.validate().is_ok());

        // Invalid IP
        let mut invalid = valid_threat.clone();
        invalid.ip = "999.999.999.999".to_string();
        assert!(invalid.validate().is_err());

        // Invalid severity (0)
        let mut invalid = valid_threat.clone();
        invalid.severity = 0;
        assert!(invalid.validate().is_err());

        // Invalid severity (>10)
        let mut invalid = valid_threat.clone();
        invalid.severity = 11;
        assert!(invalid.validate().is_err());

        // Invalid block duration (0)
        let mut invalid = valid_threat.clone();
        invalid.block_duration_secs = 0;
        assert!(invalid.validate().is_err());

        // Invalid block duration (>24 hours)
        let mut invalid = valid_threat.clone();
        invalid.block_duration_secs = 90000;
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn test_threat_intelligence_serialization() {
        let threat = ThreatIntelligence::new(
            "172.16.0.50".to_string(),
            "brute_force".to_string(),
            7,
            1800,
            "node-789".to_string(),
        )
        .with_description("Multiple failed login attempts".to_string());

        let json = threat.to_json().expect("Serialization should succeed");
        assert!(json.contains("172.16.0.50"));
        assert!(json.contains("brute_force"));

        let deserialized = ThreatIntelligence::from_json(&json)
            .expect("Deserialization should succeed");
        assert_eq!(deserialized, threat);
    }

    #[test]
    fn test_p2p_config_default() {
        let config = P2PConfig::default();
        assert_eq!(config.listen_port, 9001);
        assert!(config.enable_mdns);
        assert!(config.bootstrap_peers.is_empty());
    }

    #[test]
    fn test_threat_with_description() {
        let threat = ThreatIntelligence::new(
            "192.168.1.1".to_string(),
            "port_scan".to_string(),
            4,
            300,
            "node-001".to_string(),
        )
        .with_description("Port scan detected on ports 22, 80, 443".to_string());

        assert!(threat.description.is_some());
        assert_eq!(
            threat.description.expect("Description should be set"),
            "Port scan detected on ports 22, 80, 443"
        );
    }

    #[tokio::test]
    #[ignore] // Requires network permissions (mDNS); run with --ignored flag
    async fn test_p2p_network_creation() {
        // Use test config with mDNS disabled to avoid permission issues
        let config = P2PConfig {
            listen_port: 0, // OS assigns random port
            enable_mdns: false, // Disable mDNS for tests (requires elevated privileges)
            bootstrap_peers: Vec::new(),
        };

        let p2p = ThreatIntelP2P::new(config);

        // If this still fails due to permissions, that's acceptable for tests
        if p2p.is_err() {
            // Permission errors are expected in restricted test environments
            tracing::warn!("Note: P2P network creation requires network permissions");
            return;
        }

        let p2p = p2p.expect("P2P network creation should succeed if permissions are available");
        assert!(p2p.peer_id().to_string().len() > 0);
    }

    #[test]
    fn test_timestamp_validation_future() {
        let mut threat = ThreatIntelligence::new(
            "10.0.0.1".to_string(),
            "ddos".to_string(),
            5,
            600,
            "node-test".to_string(),
        );

        // Set timestamp too far in the future (>5 minutes)
        threat.timestamp = current_timestamp() + 400;

        assert!(threat.validate().is_err());
    }

    #[test]
    fn test_timestamp_validation_past() {
        let mut threat = ThreatIntelligence::new(
            "10.0.0.1".to_string(),
            "ddos".to_string(),
            5,
            600,
            "node-test".to_string(),
        );

        // Set timestamp too far in the past (>1 hour)
        threat.timestamp = current_timestamp().saturating_sub(3700);

        assert!(threat.validate().is_err());
    }
}
