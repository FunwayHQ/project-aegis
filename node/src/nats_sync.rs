use anyhow::{Context, Result};
use async_nats::jetstream;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

use crate::distributed_counter::{CounterOp, DistributedCounter};

/// Authentication method for NATS connections
/// Y1.4: Add NATS authentication config
#[derive(Debug, Clone)]
pub enum NatsAuth {
    /// No authentication (NOT recommended for production)
    None,
    /// Username/password authentication
    UserPassword { username: String, password: String },
    /// Token-based authentication
    Token(String),
    /// NKey-based authentication (Ed25519)
    NKey { seed: String },
}

impl Default for NatsAuth {
    fn default() -> Self {
        Self::None
    }
}

/// Configuration for NATS connection
#[derive(Debug, Clone)]
pub struct NatsConfig {
    /// NATS server URL (e.g., "nats://localhost:4222" or "tls://localhost:4222")
    pub server_url: String,
    /// Stream name for CRDT operations
    pub stream_name: String,
    /// Subject for counter operations
    pub counter_subject: String,
    /// Consumer durable name
    pub consumer_name: String,
    /// Y1.4: Authentication method
    pub auth: NatsAuth,
    /// Y1.6: Require TLS for all connections (should be true in production)
    pub require_tls: bool,
    /// Ed25519 signing key for CRDT message signatures (hex encoded seed)
    pub signing_key_seed: Option<String>,
    /// Set of trusted public keys (hex encoded) for verifying CRDT messages
    pub trusted_keys: HashSet<String>,
}

impl Default for NatsConfig {
    fn default() -> Self {
        Self {
            server_url: "nats://localhost:4222".to_string(),
            stream_name: "AEGIS_STATE".to_string(),
            counter_subject: "aegis.state.counter".to_string(),
            consumer_name: "aegis-counter-consumer".to_string(),
            auth: NatsAuth::None,
            require_tls: false, // Default false for local dev, must be true in production
            signing_key_seed: None,
            trusted_keys: HashSet::new(),
        }
    }
}

impl NatsConfig {
    /// Create a production-ready config with TLS and authentication
    pub fn production(
        server_url: String,
        auth: NatsAuth,
        signing_key_seed: String,
        trusted_keys: HashSet<String>,
    ) -> Self {
        Self {
            server_url,
            stream_name: "AEGIS_STATE".to_string(),
            counter_subject: "aegis.state.counter".to_string(),
            consumer_name: "aegis-counter-consumer".to_string(),
            auth,
            require_tls: true,
            signing_key_seed: Some(signing_key_seed),
            trusted_keys,
        }
    }

    /// Validate configuration for production use
    pub fn validate_for_production(&self) -> Result<()> {
        if !self.require_tls {
            anyhow::bail!("TLS must be required in production");
        }

        if matches!(self.auth, NatsAuth::None) {
            anyhow::bail!("Authentication must be configured in production");
        }

        if self.signing_key_seed.is_none() {
            anyhow::bail!("Signing key must be configured for CRDT message authentication");
        }

        if self.trusted_keys.is_empty() {
            anyhow::bail!("At least one trusted public key must be configured");
        }

        Ok(())
    }
}

// =============================================================================
// Y6.4: Vector Clocks for Causality Detection
// =============================================================================

/// Vector clock for causality tracking
///
/// Tracks logical timestamps across multiple nodes to detect
/// concurrent events and causality violations.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct VectorClock {
    /// Clock entries: actor_id -> logical timestamp
    clocks: HashMap<u64, u64>,
}

impl VectorClock {
    /// Create a new empty vector clock
    pub fn new() -> Self {
        Self {
            clocks: HashMap::new(),
        }
    }

    /// Increment this node's clock
    pub fn increment(&mut self, actor_id: u64) {
        let counter = self.clocks.entry(actor_id).or_insert(0);
        *counter += 1;
    }

    /// Get the logical timestamp for a specific actor
    pub fn get(&self, actor_id: u64) -> u64 {
        *self.clocks.get(&actor_id).unwrap_or(&0)
    }

    /// Merge with another vector clock (take max of each entry)
    pub fn merge(&mut self, other: &VectorClock) {
        for (actor_id, &timestamp) in &other.clocks {
            let entry = self.clocks.entry(*actor_id).or_insert(0);
            if timestamp > *entry {
                *entry = timestamp;
            }
        }
    }

    /// Check if this clock happens-before another
    /// Returns true if all entries in self are <= corresponding entries in other
    /// and at least one entry is strictly less
    pub fn happens_before(&self, other: &VectorClock) -> bool {
        let mut has_less = false;

        // Check all entries in self
        for (&actor_id, &ts) in &self.clocks {
            let other_ts = other.get(actor_id);
            if ts > other_ts {
                return false;
            }
            if ts < other_ts {
                has_less = true;
            }
        }

        // Check entries in other that aren't in self
        for (&actor_id, &ts) in &other.clocks {
            if !self.clocks.contains_key(&actor_id) && ts > 0 {
                has_less = true;
            }
        }

        has_less
    }

    /// Check if two events are concurrent (neither happens-before the other)
    pub fn is_concurrent(&self, other: &VectorClock) -> bool {
        !self.happens_before(other) && !other.happens_before(self) && self != other
    }

    /// Detect causality violation: returns true if other's clock
    /// has entries that are behind self (would indicate out-of-order delivery)
    pub fn detect_causality_violation(&self, other: &VectorClock) -> Option<CausalityViolation> {
        let mut violations = Vec::new();

        for (&actor_id, &self_ts) in &self.clocks {
            let other_ts = other.get(actor_id);
            // If other claims a timestamp lower than what we've seen
            // AND other is the sender for that actor, it's a violation
            if other_ts < self_ts {
                violations.push((actor_id, self_ts, other_ts));
            }
        }

        if violations.is_empty() {
            None
        } else {
            Some(CausalityViolation { violations })
        }
    }

    /// Get all actors in this clock
    pub fn actors(&self) -> Vec<u64> {
        self.clocks.keys().copied().collect()
    }

    /// Get total number of actors tracked
    pub fn len(&self) -> usize {
        self.clocks.len()
    }

    /// Check if clock is empty
    pub fn is_empty(&self) -> bool {
        self.clocks.is_empty()
    }
}

/// Represents a detected causality violation
#[derive(Debug, Clone)]
pub struct CausalityViolation {
    /// (actor_id, expected_timestamp, received_timestamp)
    pub violations: Vec<(u64, u64, u64)>,
}

impl std::fmt::Display for CausalityViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Causality violations: ")?;
        for (actor, expected, received) in &self.violations {
            write!(f, "[actor {} expected >= {} got {}] ", actor, expected, received)?;
        }
        Ok(())
    }
}

/// Message wrapper for CRDT operations over NATS
/// Y1.1: Updated with Ed25519 signature fields
/// Y6.4: Added vector clock for causality detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrdtMessage {
    /// Actor ID that originated this operation
    pub actor_id: u64,
    /// The CRDT operation
    pub operation: CounterOp,
    /// Timestamp (Unix epoch milliseconds)
    pub timestamp: u64,
    /// Y1.1: Ed25519 signature (base64 encoded)
    #[serde(default)]
    pub signature: Option<String>,
    /// Y1.1: Signer's public key (hex encoded)
    #[serde(default)]
    pub public_key: Option<String>,
    /// Y6.4: Vector clock for causality tracking
    #[serde(default)]
    pub vector_clock: Option<VectorClock>,
}

impl CrdtMessage {
    pub fn new(actor_id: u64, operation: CounterOp) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        Self {
            actor_id,
            operation,
            timestamp,
            signature: None,
            public_key: None,
            vector_clock: None,
        }
    }

    /// Y6.4: Create message with vector clock
    pub fn with_vector_clock(actor_id: u64, operation: CounterOp, vector_clock: VectorClock) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        Self {
            actor_id,
            operation,
            timestamp,
            signature: None,
            public_key: None,
            vector_clock: Some(vector_clock),
        }
    }

    /// Y6.4: Set vector clock on existing message
    pub fn set_vector_clock(&mut self, vector_clock: VectorClock) {
        self.vector_clock = Some(vector_clock);
    }

    /// Y6.4: Get vector clock if present
    pub fn get_vector_clock(&self) -> Option<&VectorClock> {
        self.vector_clock.as_ref()
    }

    /// Y1.2: Get the payload used for signing (excludes signature fields)
    fn signing_payload(&self) -> String {
        format!(
            "{}:{}:{}",
            self.actor_id,
            self.timestamp,
            serde_json::to_string(&self.operation).unwrap_or_default()
        )
    }

    /// Y1.2: Sign the message with an Ed25519 key
    pub fn sign(&mut self, signing_key: &SigningKey) {
        let payload = self.signing_payload();
        let signature = signing_key.sign(payload.as_bytes());

        // Store signature as base64
        self.signature = Some(STANDARD.encode(signature.to_bytes()));

        // Store public key as hex
        let verifying_key = signing_key.verifying_key();
        self.public_key = Some(hex::encode(verifying_key.as_bytes()));
    }

    /// Y1.2: Verify the message signature against the embedded public key
    pub fn verify(&self) -> Result<bool> {
        let Some(sig_str) = &self.signature else {
            return Ok(false);
        };
        let Some(pub_key_hex) = &self.public_key else {
            return Ok(false);
        };

        // Decode signature from base64
        let sig_bytes = STANDARD
            .decode(sig_str)
            .context("Invalid signature encoding")?;
        let signature = Signature::from_slice(&sig_bytes)
            .map_err(|e| anyhow::anyhow!("Invalid signature format: {}", e))?;

        // Decode public key from hex
        let pub_key_bytes = hex::decode(pub_key_hex).context("Invalid public key encoding")?;
        let pub_key_array: [u8; 32] = pub_key_bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("Public key must be 32 bytes"))?;
        let verifying_key = VerifyingKey::from_bytes(&pub_key_array)
            .map_err(|e| anyhow::anyhow!("Invalid public key: {}", e))?;

        // Verify signature
        let payload = self.signing_payload();
        Ok(verifying_key.verify(payload.as_bytes(), &signature).is_ok())
    }

    /// Y1.2: Verify the message signature against a specific public key
    pub fn verify_with_key(&self, expected_key: &VerifyingKey) -> Result<bool> {
        let Some(sig_str) = &self.signature else {
            return Ok(false);
        };
        let Some(pub_key_hex) = &self.public_key else {
            return Ok(false);
        };

        // Verify the embedded public key matches the expected key
        let expected_hex = hex::encode(expected_key.as_bytes());
        if pub_key_hex != &expected_hex {
            debug!(
                "Public key mismatch: expected {}, got {}",
                expected_hex, pub_key_hex
            );
            return Ok(false);
        }

        // Decode signature from base64
        let sig_bytes = STANDARD
            .decode(sig_str)
            .context("Invalid signature encoding")?;
        let signature = Signature::from_slice(&sig_bytes)
            .map_err(|e| anyhow::anyhow!("Invalid signature format: {}", e))?;

        // Verify signature
        let payload = self.signing_payload();
        Ok(expected_key.verify(payload.as_bytes(), &signature).is_ok())
    }

    /// Y1.3: Check if the message is signed by a trusted key
    pub fn is_trusted(&self, trusted_keys: &HashSet<String>) -> Result<bool> {
        let Some(pub_key_hex) = &self.public_key else {
            return Ok(false);
        };

        // Check if the public key is in the trusted set
        if !trusted_keys.contains(pub_key_hex) {
            debug!("Message from untrusted key: {}", pub_key_hex);
            return Ok(false);
        }

        // Verify the signature is valid
        self.verify()
    }

    /// Serialize to JSON
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).context("Failed to serialize CRDT message")
    }

    /// Deserialize from JSON
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).context("Failed to deserialize CRDT message")
    }
}

/// NATS JetStream synchronization service
pub struct NatsSync {
    client: async_nats::Client,
    jetstream: jetstream::Context,
    config: NatsConfig,
    /// Y1.2: Signing key for outgoing messages
    signing_key: Option<SigningKey>,
    /// Y1.3: Trusted public keys for verifying incoming messages
    trusted_keys: Arc<RwLock<HashSet<String>>>,
}

impl NatsSync {
    /// Connect to NATS server and set up JetStream
    /// Y1.4-Y1.6: Updated with authentication and TLS support
    pub async fn connect(config: NatsConfig) -> Result<Self> {
        info!("Connecting to NATS server: {}", config.server_url);

        // Y1.6: Validate TLS requirement
        if config.require_tls && !config.server_url.starts_with("tls://") {
            anyhow::bail!(
                "TLS is required but server URL does not use tls:// scheme: {}",
                config.server_url
            );
        }

        // Y1.5: Build connection options with authentication
        let connect_options = Self::build_connect_options(&config)?;

        let client = connect_options
            .connect(&config.server_url)
            .await
            .context("Failed to connect to NATS server")?;

        info!("Connected to NATS server (TLS: {})", config.require_tls);

        let jetstream = jetstream::new(client.clone());

        // Parse signing key from config if provided
        let signing_key = if let Some(ref seed_hex) = config.signing_key_seed {
            let seed_bytes = hex::decode(seed_hex).context("Invalid signing key seed hex")?;
            let seed_array: [u8; 32] = seed_bytes
                .try_into()
                .map_err(|_| anyhow::anyhow!("Signing key seed must be 32 bytes"))?;
            Some(SigningKey::from_bytes(&seed_array))
        } else {
            None
        };

        // Clone trusted keys into Arc<RwLock<>>
        let trusted_keys = Arc::new(RwLock::new(config.trusted_keys.clone()));

        // Create or get stream
        let stream_config = jetstream::stream::Config {
            name: config.stream_name.clone(),
            subjects: vec![format!("{}.*", config.counter_subject)],
            max_messages: 10_000,
            max_bytes: 10_000_000, // 10 MB
            max_age: std::time::Duration::from_secs(3600), // 1 hour retention
            storage: jetstream::stream::StorageType::File,
            num_replicas: 1,
            ..Default::default()
        };

        match jetstream.get_stream(&config.stream_name).await {
            Ok(stream) => {
                info!("Using existing stream: {}", config.stream_name);
                debug!("Stream info: {:?}", stream.cached_info());
            }
            Err(_) => {
                info!("Creating new stream: {}", config.stream_name);
                jetstream
                    .create_stream(stream_config)
                    .await
                    .context("Failed to create JetStream stream")?;
            }
        }

        Ok(Self {
            client,
            jetstream,
            config,
            signing_key,
            trusted_keys,
        })
    }

    /// Y1.5: Build connection options with authentication
    fn build_connect_options(config: &NatsConfig) -> Result<async_nats::ConnectOptions> {
        let mut options = async_nats::ConnectOptions::new();

        match &config.auth {
            NatsAuth::None => {
                if config.require_tls {
                    warn!("No authentication configured but TLS is required - consider adding authentication");
                }
            }
            NatsAuth::UserPassword { username, password } => {
                options = options.user_and_password(username.clone(), password.clone());
                info!("Using username/password authentication");
            }
            NatsAuth::Token(token) => {
                options = options.token(token.clone());
                info!("Using token authentication");
            }
            NatsAuth::NKey { seed } => {
                // Validate the NKey seed format before using it
                let _key_pair = nkeys::KeyPair::from_seed(seed)
                    .map_err(|e| anyhow::anyhow!("Invalid NKey seed: {}", e))?;

                // async-nats nkey() method takes the seed string directly
                options = options.nkey(seed.clone());
                info!("Using NKey authentication");
            }
        }

        // Y1.6: Configure TLS if required
        if config.require_tls {
            options = options.require_tls(true);
            info!("TLS required for connection");
        }

        Ok(options)
    }

    /// Publish a CRDT operation to NATS
    /// Y1.2: Messages are now signed with Ed25519
    pub async fn publish(&self, actor_id: u64, operation: CounterOp) -> Result<()> {
        let mut message = CrdtMessage::new(actor_id, operation);

        // Sign the message if we have a signing key
        if let Some(ref signing_key) = self.signing_key {
            message.sign(signing_key);
            debug!("Signed CRDT message with key: {:?}", message.public_key);
        } else {
            warn!("Publishing unsigned CRDT message - this should not happen in production");
        }

        let json = message.to_json()?;
        let subject = format!("{}.{}", self.config.counter_subject, actor_id);

        self.jetstream
            .publish(subject.clone(), json.into())
            .await
            .context("Failed to publish message")?
            .await
            .context("Failed to confirm publish")?;

        debug!("Published operation to {}", subject);
        Ok(())
    }

    /// Get our public key (hex encoded) if we have a signing key
    pub fn public_key_hex(&self) -> Option<String> {
        self.signing_key
            .as_ref()
            .map(|sk| hex::encode(sk.verifying_key().as_bytes()))
    }

    /// Add a trusted public key
    pub async fn add_trusted_key(&self, public_key_hex: String) {
        let mut trusted = self.trusted_keys.write().await;
        trusted.insert(public_key_hex);
    }

    /// Remove a trusted public key
    pub async fn remove_trusted_key(&self, public_key_hex: &str) -> bool {
        let mut trusted = self.trusted_keys.write().await;
        trusted.remove(public_key_hex)
    }

    /// Check if a public key is trusted
    pub async fn is_key_trusted(&self, public_key_hex: &str) -> bool {
        let trusted = self.trusted_keys.read().await;
        trusted.contains(public_key_hex)
    }

    /// Subscribe to CRDT operations and merge into local state
    /// Y1.3: Now verifies signatures before merging operations
    pub async fn subscribe_and_sync(
        &self,
        counter: Arc<DistributedCounter>,
    ) -> Result<mpsc::UnboundedReceiver<String>> {
        info!(
            "Subscribing to counter operations on subject: {}.*",
            self.config.counter_subject
        );

        let stream = self
            .jetstream
            .get_stream(&self.config.stream_name)
            .await
            .context("Failed to get stream")?;

        // Create or get consumer
        let consumer = stream
            .create_consumer(jetstream::consumer::pull::Config {
                durable_name: Some(self.config.consumer_name.clone()),
                filter_subject: format!("{}.*", self.config.counter_subject),
                ack_policy: jetstream::consumer::AckPolicy::Explicit,
                ..Default::default()
            })
            .await
            .context("Failed to create consumer")?;

        let (status_tx, status_rx) = mpsc::unbounded_channel();

        // Spawn task to handle incoming messages
        let counter_clone = counter.clone();
        let local_actor_id = counter.actor_id();
        let status_tx_clone = status_tx.clone();
        let trusted_keys_clone = self.trusted_keys.clone();
        let require_signatures = self.config.signing_key_seed.is_some();

        tokio::spawn(async move {
            let mut messages = consumer.messages().await.unwrap();

            while let Some(msg) = messages.next().await {
                match msg {
                    Ok(msg) => {
                        // Parse message
                        match String::from_utf8(msg.payload.to_vec()) {
                            Ok(json) => match CrdtMessage::from_json(&json) {
                                Ok(crdt_msg) => {
                                    // Don't process our own messages
                                    if crdt_msg.actor_id == local_actor_id {
                                        debug!("Skipping own message from actor {}", local_actor_id);
                                        if let Err(e) = msg.ack().await {
                                            warn!("Failed to ack message: {}", e);
                                        }
                                        continue;
                                    }

                                    // Y1.3: Verify signature before merging
                                    if require_signatures {
                                        let trusted_keys = trusted_keys_clone.read().await;

                                        // Check if message is signed
                                        if crdt_msg.signature.is_none() {
                                            warn!(
                                                "Rejecting unsigned CRDT message from actor {}",
                                                crdt_msg.actor_id
                                            );
                                            let _ = status_tx_clone.send(format!(
                                                "Rejected unsigned message from actor {}",
                                                crdt_msg.actor_id
                                            ));
                                            if let Err(e) = msg.ack().await {
                                                warn!("Failed to ack message: {}", e);
                                            }
                                            continue;
                                        }

                                        // Verify signature and trust
                                        match crdt_msg.is_trusted(&trusted_keys) {
                                            Ok(true) => {
                                                debug!(
                                                    "Verified signature from trusted key: {:?}",
                                                    crdt_msg.public_key
                                                );
                                            }
                                            Ok(false) => {
                                                warn!(
                                                    "Rejecting message with invalid/untrusted signature from actor {}",
                                                    crdt_msg.actor_id
                                                );
                                                let _ = status_tx_clone.send(format!(
                                                    "Rejected untrusted message from actor {}",
                                                    crdt_msg.actor_id
                                                ));
                                                if let Err(e) = msg.ack().await {
                                                    warn!("Failed to ack message: {}", e);
                                                }
                                                continue;
                                            }
                                            Err(e) => {
                                                warn!(
                                                    "Error verifying signature from actor {}: {}",
                                                    crdt_msg.actor_id, e
                                                );
                                                let _ = status_tx_clone.send(format!(
                                                    "Signature verification error from actor {}: {}",
                                                    crdt_msg.actor_id, e
                                                ));
                                                if let Err(e) = msg.ack().await {
                                                    warn!("Failed to ack message: {}", e);
                                                }
                                                continue;
                                            }
                                        }
                                    }

                                    debug!(
                                        "Received operation from actor {}: {:?}",
                                        crdt_msg.actor_id, crdt_msg.operation
                                    );

                                    // Merge operation into local counter
                                    match counter_clone.merge_op(crdt_msg.operation.clone()) {
                                        Ok(_) => {
                                            info!(
                                                "Merged operation from actor {}",
                                                crdt_msg.actor_id
                                            );
                                            let _ = status_tx_clone.send(format!(
                                                "Merged operation from actor {}",
                                                crdt_msg.actor_id
                                            ));
                                        }
                                        Err(e) => {
                                            error!("Failed to merge operation: {}", e);
                                            let _ = status_tx_clone
                                                .send(format!("Error merging operation: {}", e));
                                        }
                                    }

                                    // Acknowledge message
                                    if let Err(e) = msg.ack().await {
                                        warn!("Failed to ack message: {}", e);
                                    }
                                }
                                Err(e) => {
                                    warn!("Failed to parse CRDT message: {}", e);
                                }
                            },
                            Err(e) => {
                                warn!("Invalid UTF-8 in message: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Error receiving message: {}", e);
                    }
                }
            }
        });

        info!("Subscription started");
        Ok(status_rx)
    }

    /// Get NATS client for direct access
    pub fn client(&self) -> &async_nats::Client {
        &self.client
    }

    /// Get JetStream context for direct access
    pub fn jetstream(&self) -> &jetstream::Context {
        &self.jetstream
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::RngCore;

    // Helper to create a test signing key
    fn create_test_signing_key() -> SigningKey {
        let mut secret_key_bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut secret_key_bytes);
        SigningKey::from_bytes(&secret_key_bytes)
    }

    // ==================== NatsConfig Tests ====================

    #[test]
    fn test_nats_config_default() {
        let config = NatsConfig::default();
        assert_eq!(config.server_url, "nats://localhost:4222");
        assert_eq!(config.stream_name, "AEGIS_STATE");
        assert_eq!(config.counter_subject, "aegis.state.counter");
        assert_eq!(config.consumer_name, "aegis-counter-consumer");
        // Y1.4-Y1.6: New fields
        assert!(matches!(config.auth, NatsAuth::None));
        assert!(!config.require_tls);
        assert!(config.signing_key_seed.is_none());
        assert!(config.trusted_keys.is_empty());
    }

    #[test]
    fn test_nats_config_production() {
        let mut trusted = HashSet::new();
        trusted.insert("abc123".to_string());

        let config = NatsConfig::production(
            "tls://nats.example.com:4222".to_string(),
            NatsAuth::Token("secret".to_string()),
            "0".repeat(64), // 32 bytes in hex
            trusted,
        );

        assert_eq!(config.server_url, "tls://nats.example.com:4222");
        assert!(config.require_tls);
        assert!(config.signing_key_seed.is_some());
        assert_eq!(config.trusted_keys.len(), 1);
    }

    #[test]
    fn test_nats_config_validate_for_production_success() {
        let mut trusted = HashSet::new();
        trusted.insert("abc123".to_string());

        let config = NatsConfig::production(
            "tls://nats.example.com:4222".to_string(),
            NatsAuth::Token("secret".to_string()),
            "0".repeat(64),
            trusted,
        );

        assert!(config.validate_for_production().is_ok());
    }

    #[test]
    fn test_nats_config_validate_for_production_no_tls() {
        let mut trusted = HashSet::new();
        trusted.insert("abc123".to_string());

        let mut config = NatsConfig::production(
            "tls://nats.example.com:4222".to_string(),
            NatsAuth::Token("secret".to_string()),
            "0".repeat(64),
            trusted,
        );
        config.require_tls = false;

        let result = config.validate_for_production();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("TLS"));
    }

    #[test]
    fn test_nats_config_validate_for_production_no_auth() {
        let mut trusted = HashSet::new();
        trusted.insert("abc123".to_string());

        let mut config = NatsConfig::production(
            "tls://nats.example.com:4222".to_string(),
            NatsAuth::Token("secret".to_string()),
            "0".repeat(64),
            trusted,
        );
        config.auth = NatsAuth::None;

        let result = config.validate_for_production();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Authentication"));
    }

    #[test]
    fn test_nats_config_validate_for_production_no_signing_key() {
        let mut trusted = HashSet::new();
        trusted.insert("abc123".to_string());

        let mut config = NatsConfig::production(
            "tls://nats.example.com:4222".to_string(),
            NatsAuth::Token("secret".to_string()),
            "0".repeat(64),
            trusted,
        );
        config.signing_key_seed = None;

        let result = config.validate_for_production();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Signing key"));
    }

    #[test]
    fn test_nats_config_validate_for_production_no_trusted_keys() {
        let config = NatsConfig::production(
            "tls://nats.example.com:4222".to_string(),
            NatsAuth::Token("secret".to_string()),
            "0".repeat(64),
            HashSet::new(), // Empty trusted keys
        );

        let result = config.validate_for_production();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("trusted public key"));
    }

    // ==================== CrdtMessage Tests ====================

    #[test]
    fn test_crdt_message_creation() {
        let op = CounterOp::Increment {
            actor: 123,
            value: 5,
        };

        let msg = CrdtMessage::new(123, op.clone());
        assert_eq!(msg.actor_id, 123);
        assert_eq!(msg.operation, op);
        assert!(msg.timestamp > 0);
        // Y1.1: New signature fields should be None initially
        assert!(msg.signature.is_none());
        assert!(msg.public_key.is_none());
    }

    #[test]
    fn test_crdt_message_serialization() {
        let op = CounterOp::Increment {
            actor: 456,
            value: 10,
        };

        let msg = CrdtMessage::new(456, op);
        let json = msg.to_json().unwrap();

        assert!(json.contains("456"));
        assert!(json.contains("\"value\":10"));

        let deserialized = CrdtMessage::from_json(&json).unwrap();
        assert_eq!(deserialized.actor_id, 456);
    }

    #[test]
    fn test_crdt_message_full_state() {
        let state = vec![1, 2, 3, 4, 5];
        let op = CounterOp::FullState {
            state: state.clone(),
        };

        let msg = CrdtMessage::new(789, op);
        let json = msg.to_json().unwrap();

        let deserialized = CrdtMessage::from_json(&json).unwrap();
        if let CounterOp::FullState { state: s } = deserialized.operation {
            assert_eq!(s, state);
        } else {
            panic!("Expected FullState operation");
        }
    }

    // ==================== Y1.2: Signature Tests ====================

    #[test]
    fn test_crdt_message_sign() {
        let signing_key = create_test_signing_key();
        let op = CounterOp::Increment {
            actor: 123,
            value: 5,
        };

        let mut msg = CrdtMessage::new(123, op);
        assert!(msg.signature.is_none());

        msg.sign(&signing_key);

        assert!(msg.signature.is_some());
        assert!(msg.public_key.is_some());

        // Verify public key matches
        let expected_pub_key = hex::encode(signing_key.verifying_key().as_bytes());
        assert_eq!(msg.public_key.as_ref().unwrap(), &expected_pub_key);
    }

    #[test]
    fn test_crdt_message_verify_valid() {
        let signing_key = create_test_signing_key();
        let op = CounterOp::Increment {
            actor: 123,
            value: 5,
        };

        let mut msg = CrdtMessage::new(123, op);
        msg.sign(&signing_key);

        let verified = msg.verify().unwrap();
        assert!(verified);
    }

    #[test]
    fn test_crdt_message_verify_unsigned() {
        let op = CounterOp::Increment {
            actor: 123,
            value: 5,
        };

        let msg = CrdtMessage::new(123, op);
        let verified = msg.verify().unwrap();
        assert!(!verified);
    }

    #[test]
    fn test_crdt_message_verify_tampered() {
        let signing_key = create_test_signing_key();
        let op = CounterOp::Increment {
            actor: 123,
            value: 5,
        };

        let mut msg = CrdtMessage::new(123, op);
        msg.sign(&signing_key);

        // Tamper with the message
        msg.actor_id = 999;

        let verified = msg.verify().unwrap();
        assert!(!verified);
    }

    #[test]
    fn test_crdt_message_verify_with_key() {
        let signing_key = create_test_signing_key();
        let op = CounterOp::Increment {
            actor: 123,
            value: 5,
        };

        let mut msg = CrdtMessage::new(123, op);
        msg.sign(&signing_key);

        // Verify with the correct key
        let verified = msg.verify_with_key(&signing_key.verifying_key()).unwrap();
        assert!(verified);

        // Verify with a different key should fail
        let other_key = create_test_signing_key();
        let verified_other = msg.verify_with_key(&other_key.verifying_key()).unwrap();
        assert!(!verified_other);
    }

    // ==================== Y1.3: Trust Tests ====================

    #[test]
    fn test_crdt_message_is_trusted_valid() {
        let signing_key = create_test_signing_key();
        let pub_key_hex = hex::encode(signing_key.verifying_key().as_bytes());

        let mut trusted_keys = HashSet::new();
        trusted_keys.insert(pub_key_hex);

        let op = CounterOp::Increment {
            actor: 123,
            value: 5,
        };

        let mut msg = CrdtMessage::new(123, op);
        msg.sign(&signing_key);

        let is_trusted = msg.is_trusted(&trusted_keys).unwrap();
        assert!(is_trusted);
    }

    #[test]
    fn test_crdt_message_is_trusted_unknown_key() {
        let signing_key = create_test_signing_key();

        let mut trusted_keys = HashSet::new();
        trusted_keys.insert("unknown_key".to_string());

        let op = CounterOp::Increment {
            actor: 123,
            value: 5,
        };

        let mut msg = CrdtMessage::new(123, op);
        msg.sign(&signing_key);

        let is_trusted = msg.is_trusted(&trusted_keys).unwrap();
        assert!(!is_trusted);
    }

    #[test]
    fn test_crdt_message_is_trusted_unsigned() {
        let trusted_keys = HashSet::new();

        let op = CounterOp::Increment {
            actor: 123,
            value: 5,
        };

        let msg = CrdtMessage::new(123, op);
        let is_trusted = msg.is_trusted(&trusted_keys).unwrap();
        assert!(!is_trusted);
    }

    #[test]
    fn test_crdt_message_serialization_with_signature() {
        let signing_key = create_test_signing_key();
        let op = CounterOp::Increment {
            actor: 123,
            value: 5,
        };

        let mut msg = CrdtMessage::new(123, op);
        msg.sign(&signing_key);

        // Serialize and deserialize
        let json = msg.to_json().unwrap();
        let deserialized = CrdtMessage::from_json(&json).unwrap();

        // Verify signature survives serialization
        assert!(deserialized.signature.is_some());
        assert!(deserialized.public_key.is_some());
        assert!(deserialized.verify().unwrap());
    }

    // ==================== NatsAuth Tests ====================

    #[test]
    fn test_nats_auth_default() {
        let auth = NatsAuth::default();
        assert!(matches!(auth, NatsAuth::None));
    }

    #[test]
    fn test_nats_auth_user_password() {
        let auth = NatsAuth::UserPassword {
            username: "user".to_string(),
            password: "pass".to_string(),
        };
        if let NatsAuth::UserPassword { username, password } = auth {
            assert_eq!(username, "user");
            assert_eq!(password, "pass");
        } else {
            panic!("Expected UserPassword variant");
        }
    }

    #[test]
    fn test_nats_auth_token() {
        let auth = NatsAuth::Token("my_token".to_string());
        if let NatsAuth::Token(token) = auth {
            assert_eq!(token, "my_token");
        } else {
            panic!("Expected Token variant");
        }
    }

    #[test]
    fn test_nats_auth_nkey() {
        let auth = NatsAuth::NKey {
            seed: "SUAM4WVHRTJHPRG36XJMGUXFKGGLXQ45S6BTKNQFLJH3PLCNLVD3Q7P5SY".to_string(),
        };
        if let NatsAuth::NKey { seed } = auth {
            assert!(seed.starts_with("SUA"));
        } else {
            panic!("Expected NKey variant");
        }
    }

    // ==================== Y6.4: Vector Clock Tests ====================

    #[test]
    fn test_y64_vector_clock_new() {
        let vc = VectorClock::new();
        assert!(vc.is_empty());
        assert_eq!(vc.len(), 0);
    }

    #[test]
    fn test_y64_vector_clock_increment() {
        let mut vc = VectorClock::new();
        vc.increment(1);
        assert_eq!(vc.get(1), 1);
        vc.increment(1);
        assert_eq!(vc.get(1), 2);
        vc.increment(2);
        assert_eq!(vc.get(2), 1);
        assert_eq!(vc.len(), 2);
    }

    #[test]
    fn test_y64_vector_clock_merge() {
        let mut vc1 = VectorClock::new();
        vc1.increment(1);
        vc1.increment(1);
        vc1.increment(2);

        let mut vc2 = VectorClock::new();
        vc2.increment(2);
        vc2.increment(2);
        vc2.increment(3);

        // Merge vc2 into vc1
        vc1.merge(&vc2);

        // Should have max of each
        assert_eq!(vc1.get(1), 2); // from vc1
        assert_eq!(vc1.get(2), 2); // max(1, 2) = 2
        assert_eq!(vc1.get(3), 1); // from vc2
    }

    #[test]
    fn test_y64_vector_clock_happens_before() {
        let mut vc1 = VectorClock::new();
        vc1.increment(1);

        let mut vc2 = VectorClock::new();
        vc2.increment(1);
        vc2.increment(1);

        // vc1 happens before vc2
        assert!(vc1.happens_before(&vc2));
        assert!(!vc2.happens_before(&vc1));
    }

    #[test]
    fn test_y64_vector_clock_concurrent() {
        let mut vc1 = VectorClock::new();
        vc1.increment(1);

        let mut vc2 = VectorClock::new();
        vc2.increment(2);

        // Neither happens before the other = concurrent
        assert!(vc1.is_concurrent(&vc2));
        assert!(vc2.is_concurrent(&vc1));
    }

    #[test]
    fn test_y64_vector_clock_causality_violation() {
        let mut local = VectorClock::new();
        local.increment(1);
        local.increment(1);
        local.increment(2);

        // Received clock claims actor 1's timestamp is 1, but we've seen 2
        // Actor 2 is also behind (0 vs 1)
        let mut received = VectorClock::new();
        received.increment(1); // Only 1, but local has 2

        let violation = local.detect_causality_violation(&received);
        assert!(violation.is_some());
        let v = violation.unwrap();
        // Two violations: actor 1 (2 > 1) and actor 2 (1 > 0)
        assert_eq!(v.violations.len(), 2);
        // Check both violations are present (order may vary)
        assert!(v.violations.iter().any(|&(a, e, r)| a == 1 && e == 2 && r == 1));
        assert!(v.violations.iter().any(|&(a, e, r)| a == 2 && e == 1 && r == 0));
    }

    #[test]
    fn test_y64_vector_clock_no_violation() {
        let mut local = VectorClock::new();
        local.increment(1);

        let mut received = VectorClock::new();
        received.increment(1);
        received.increment(1);

        // Received clock is ahead, no violation
        let violation = local.detect_causality_violation(&received);
        assert!(violation.is_none());
    }

    #[test]
    fn test_y64_crdt_message_with_vector_clock() {
        let mut vc = VectorClock::new();
        vc.increment(123);

        let op = CounterOp::Increment {
            actor: 123,
            value: 5,
        };

        let msg = CrdtMessage::with_vector_clock(123, op, vc.clone());
        assert!(msg.get_vector_clock().is_some());
        assert_eq!(msg.get_vector_clock().unwrap().get(123), 1);
    }

    #[test]
    fn test_y64_crdt_message_set_vector_clock() {
        let op = CounterOp::Increment {
            actor: 123,
            value: 5,
        };

        let mut msg = CrdtMessage::new(123, op);
        assert!(msg.get_vector_clock().is_none());

        let mut vc = VectorClock::new();
        vc.increment(123);
        msg.set_vector_clock(vc);

        assert!(msg.get_vector_clock().is_some());
    }
}
