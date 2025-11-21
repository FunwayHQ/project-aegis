use anyhow::{Context, Result};
use crdts::{CmRDT, GCounter, CvRDT};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use tracing::{debug, info};

/// Actor ID type for CRDT identification
pub type ActorId = u64;

/// Operation on a distributed counter
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CounterOp {
    /// Increment counter by value
    Increment { actor: ActorId, value: u64 },
    /// Full state synchronization
    FullState { state: Vec<u8> },
}

/// Distributed counter using G-Counter CRDT
/// G-Counter = Grow-only Counter, only supports increments
/// Perfect for rate limiting where we only count requests
#[derive(Debug, Clone)]
pub struct DistributedCounter {
    /// The underlying G-Counter CRDT
    counter: Arc<RwLock<GCounter<ActorId>>>,
    /// This node's actor ID
    actor_id: ActorId,
}

impl DistributedCounter {
    /// Create a new distributed counter
    pub fn new(actor_id: ActorId) -> Self {
        info!("Creating distributed counter for actor: {}", actor_id);
        Self {
            counter: Arc::new(RwLock::new(GCounter::new())),
            actor_id,
        }
    }

    /// Increment the counter locally
    pub fn increment(&self, value: u64) -> Result<CounterOp> {
        let mut counter = self.counter.write()
            .map_err(|e| anyhow::anyhow!("Failed to acquire write lock: {}", e))?;

        // Apply increments
        for _ in 0..value {
            let op = counter.inc(self.actor_id);
            counter.apply(op);
        }

        debug!("Incremented counter by {} (actor: {})", value, self.actor_id);

        Ok(CounterOp::Increment {
            actor: self.actor_id,
            value,
        })
    }

    /// Get the current counter value (sum across all actors)
    pub fn value(&self) -> Result<u64> {
        use num_traits::ToPrimitive;

        let counter = self.counter.read()
            .map_err(|e| anyhow::anyhow!("Failed to acquire read lock: {}", e))?;

        let big_value = counter.read();
        big_value.to_u64()
            .ok_or_else(|| anyhow::anyhow!("Counter value too large for u64"))
    }

    /// Merge an operation from another node
    pub fn merge_op(&self, op: CounterOp) -> Result<()> {
        match op {
            CounterOp::Increment { actor, value } => {
                let mut counter = self.counter.write()
                    .map_err(|e| anyhow::anyhow!("Failed to acquire write lock: {}", e))?;

                // Apply increments from remote actor
                for _ in 0..value {
                    let op = counter.inc(actor);
                    counter.apply(op);
                }

                debug!("Merged increment: actor={}, value={}", actor, value);
                Ok(())
            }
            CounterOp::FullState { state } => {
                self.merge_state(&state)
            }
        }
    }

    /// Serialize counter state for transmission
    pub fn serialize_state(&self) -> Result<Vec<u8>> {
        let counter = self.counter.read()
            .map_err(|e| anyhow::anyhow!("Failed to acquire read lock: {}", e))?;

        bincode::serialize(&*counter)
            .context("Failed to serialize counter state")
    }

    /// Merge serialized state from another node
    pub fn merge_state(&self, state: &[u8]) -> Result<()> {
        let remote_counter: GCounter<ActorId> = bincode::deserialize(state)
            .context("Failed to deserialize counter state")?;

        let mut counter = self.counter.write()
            .map_err(|e| anyhow::anyhow!("Failed to acquire write lock: {}", e))?;

        // Merge remote state using CRDT merge operation
        counter.merge(remote_counter);

        debug!("Merged full state, new value: {}", counter.read());
        Ok(())
    }

    /// Get the actor ID
    pub fn actor_id(&self) -> ActorId {
        self.actor_id
    }

    /// Reset counter (for testing only)
    #[cfg(test)]
    pub fn reset(&self) -> Result<()> {
        let mut counter = self.counter.write()
            .map_err(|e| anyhow::anyhow!("Failed to acquire write lock: {}", e))?;

        *counter = GCounter::new();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter_creation() {
        let counter = DistributedCounter::new(1);
        assert_eq!(counter.actor_id(), 1);
        assert_eq!(counter.value().unwrap(), 0);
    }

    #[test]
    fn test_counter_increment() {
        let counter = DistributedCounter::new(1);

        counter.increment(1).unwrap();
        assert_eq!(counter.value().unwrap(), 1);

        counter.increment(5).unwrap();
        assert_eq!(counter.value().unwrap(), 6);
    }

    #[test]
    fn test_counter_merge_increment() {
        let counter1 = DistributedCounter::new(1);
        let counter2 = DistributedCounter::new(2);

        // Counter 1 increments
        counter1.increment(3).unwrap();
        assert_eq!(counter1.value().unwrap(), 3);

        // Counter 2 increments
        let op = counter2.increment(5).unwrap();
        assert_eq!(counter2.value().unwrap(), 5);

        // Counter 1 merges Counter 2's operation
        counter1.merge_op(op).unwrap();
        assert_eq!(counter1.value().unwrap(), 8); // 3 + 5
    }

    #[test]
    fn test_counter_merge_state() {
        let counter1 = DistributedCounter::new(1);
        let counter2 = DistributedCounter::new(2);
        let counter3 = DistributedCounter::new(3);

        // Each counter increments independently
        counter1.increment(10).unwrap();
        counter2.increment(20).unwrap();
        counter3.increment(30).unwrap();

        // Serialize counter1's state
        let state1 = counter1.serialize_state().unwrap();
        let state2 = counter2.serialize_state().unwrap();

        // Counter3 merges both states
        counter3.merge_state(&state1).unwrap();
        counter3.merge_state(&state2).unwrap();

        // Should have sum of all increments
        assert_eq!(counter3.value().unwrap(), 60); // 10 + 20 + 30
    }

    #[test]
    fn test_counter_commutativity() {
        // Test that merge order doesn't matter (CRDT property)
        let counter_a1 = DistributedCounter::new(1);
        let counter_a2 = DistributedCounter::new(2);

        let counter_b1 = DistributedCounter::new(1);
        let counter_b2 = DistributedCounter::new(2);

        // Both increment
        counter_a1.increment(5).unwrap();
        counter_a2.increment(10).unwrap();

        counter_b1.increment(5).unwrap();
        counter_b2.increment(10).unwrap();

        // Merge in different orders
        let state_a1 = counter_a1.serialize_state().unwrap();
        let state_a2 = counter_a2.serialize_state().unwrap();

        let state_b1 = counter_b1.serialize_state().unwrap();
        let state_b2 = counter_b2.serialize_state().unwrap();

        let final_a = DistributedCounter::new(3);
        final_a.merge_state(&state_a1).unwrap();
        final_a.merge_state(&state_a2).unwrap();

        let final_b = DistributedCounter::new(3);
        final_b.merge_state(&state_b2).unwrap(); // Reverse order
        final_b.merge_state(&state_b1).unwrap();

        // Should have same final value regardless of order
        assert_eq!(final_a.value().unwrap(), final_b.value().unwrap());
        assert_eq!(final_a.value().unwrap(), 15); // 5 + 10
    }

    #[test]
    fn test_counter_idempotence() {
        // Test that merging same state multiple times has same effect as once
        let counter1 = DistributedCounter::new(1);
        let counter2 = DistributedCounter::new(2);

        counter1.increment(7).unwrap();
        let state = counter1.serialize_state().unwrap();

        // Merge same state multiple times
        counter2.merge_state(&state).unwrap();
        counter2.merge_state(&state).unwrap();
        counter2.merge_state(&state).unwrap();

        // Should be same as merging once
        assert_eq!(counter2.value().unwrap(), 7);
    }

    #[test]
    fn test_counter_concurrent_increments() {
        // Simulate concurrent increments from multiple actors
        let counter1 = DistributedCounter::new(1);
        let counter2 = DistributedCounter::new(2);
        let counter3 = DistributedCounter::new(3);

        // All increment concurrently
        let op1 = counter1.increment(1).unwrap();
        let op2 = counter2.increment(2).unwrap();
        let op3 = counter3.increment(3).unwrap();

        // All merge each other's operations
        counter1.merge_op(op2.clone()).unwrap();
        counter1.merge_op(op3.clone()).unwrap();

        counter2.merge_op(op1.clone()).unwrap();
        counter2.merge_op(op3.clone()).unwrap();

        counter3.merge_op(op1).unwrap();
        counter3.merge_op(op2).unwrap();

        // All should converge to same value
        assert_eq!(counter1.value().unwrap(), 6);
        assert_eq!(counter2.value().unwrap(), 6);
        assert_eq!(counter3.value().unwrap(), 6);
    }

    #[test]
    fn test_counter_op_serialization() {
        let op = CounterOp::Increment {
            actor: 42,
            value: 100,
        };

        let serialized = bincode::serialize(&op).unwrap();
        let deserialized: CounterOp = bincode::deserialize(&serialized).unwrap();

        assert_eq!(op, deserialized);
    }

    #[test]
    fn test_counter_large_values() {
        let counter = DistributedCounter::new(1);

        // Test with large increment
        counter.increment(1_000_000).unwrap();
        assert_eq!(counter.value().unwrap(), 1_000_000);

        // Test with multiple large increments
        counter.increment(500_000).unwrap();
        assert_eq!(counter.value().unwrap(), 1_500_000);
    }

    #[test]
    fn test_counter_multi_actor() {
        // Test with many actors
        let counters: Vec<_> = (1..=10)
            .map(|i| DistributedCounter::new(i))
            .collect();

        // Each increments by its actor ID
        for counter in &counters {
            counter.increment(counter.actor_id()).unwrap();
        }

        // Merge all states into first counter
        for counter in counters.iter().skip(1) {
            let state = counter.serialize_state().unwrap();
            counters[0].merge_state(&state).unwrap();
        }

        // Should be sum of 1+2+3+...+10 = 55
        assert_eq!(counters[0].value().unwrap(), 55);
    }
}
