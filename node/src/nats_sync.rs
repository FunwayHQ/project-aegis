use anyhow::{Context, Result};
use async_nats::jetstream;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::distributed_counter::{CounterOp, DistributedCounter};

/// Configuration for NATS connection
#[derive(Debug, Clone)]
pub struct NatsConfig {
    /// NATS server URL (e.g., "nats://localhost:4222")
    pub server_url: String,
    /// Stream name for CRDT operations
    pub stream_name: String,
    /// Subject for counter operations
    pub counter_subject: String,
    /// Consumer durable name
    pub consumer_name: String,
}

impl Default for NatsConfig {
    fn default() -> Self {
        Self {
            server_url: "nats://localhost:4222".to_string(),
            stream_name: "AEGIS_STATE".to_string(),
            counter_subject: "aegis.state.counter".to_string(),
            consumer_name: "aegis-counter-consumer".to_string(),
        }
    }
}

/// Message wrapper for CRDT operations over NATS
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrdtMessage {
    /// Actor ID that originated this operation
    pub actor_id: u64,
    /// The CRDT operation
    pub operation: CounterOp,
    /// Timestamp (Unix epoch milliseconds)
    pub timestamp: u64,
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
        }
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
}

impl NatsSync {
    /// Connect to NATS server and set up JetStream
    pub async fn connect(config: NatsConfig) -> Result<Self> {
        info!("Connecting to NATS server: {}", config.server_url);

        let client = async_nats::connect(&config.server_url)
            .await
            .context("Failed to connect to NATS server")?;

        info!("Connected to NATS server");

        let jetstream = jetstream::new(client.clone());

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
        })
    }

    /// Publish a CRDT operation to NATS
    pub async fn publish(&self, actor_id: u64, operation: CounterOp) -> Result<()> {
        let message = CrdtMessage::new(actor_id, operation);
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

    /// Subscribe to CRDT operations and merge into local state
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

    #[test]
    fn test_nats_config_default() {
        let config = NatsConfig::default();
        assert_eq!(config.server_url, "nats://localhost:4222");
        assert_eq!(config.stream_name, "AEGIS_STATE");
        assert_eq!(config.counter_subject, "aegis.state.counter");
        assert_eq!(config.consumer_name, "aegis-counter-consumer");
    }

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
}
