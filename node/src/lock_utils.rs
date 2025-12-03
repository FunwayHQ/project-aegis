//! SECURITY FIX (X2.6): Lock Poisoning Recovery Utilities
//!
//! This module provides macros and utilities for gracefully handling poisoned locks
//! in multi-threaded code. Lock poisoning occurs when a thread panics while holding
//! a lock, leaving the protected data in a potentially inconsistent state.
//!
//! Instead of propagating panics (which can crash the entire application), these
//! utilities allow recovery by:
//! 1. Logging the poisoning event for monitoring/alerting
//! 2. Recovering the guard to access the (potentially inconsistent) data
//! 3. Allowing the application to continue operating in a degraded mode
//!
//! # Security Considerations
//!
//! - Poisoned locks indicate a previous panic, which may have left data in an
//!   inconsistent state. Recovery should be used carefully.
//! - For security-critical data (e.g., authentication state), consider failing
//!   closed rather than recovering with potentially corrupt data.
//! - All poisoning events are logged at ERROR level for monitoring.

use std::sync::{Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};
use tracing::error;

// =============================================================================
// Mutex Lock Recovery
// =============================================================================

/// Acquire a Mutex lock, recovering from poisoning if necessary.
///
/// If the lock is poisoned, logs an error and returns the recovered guard.
/// This allows the application to continue operating even after a panic in
/// another thread.
///
/// # Arguments
/// * `mutex` - The Mutex to lock
/// * `context` - A description of what the lock protects (for logging)
///
/// # Returns
/// The MutexGuard, either fresh or recovered from poisoning
///
/// # Example
/// ```ignore
/// let data = lock_or_recover(&self.metrics, "bot metrics");
/// // Use data...
/// ```
pub fn lock_or_recover<'a, T>(mutex: &'a Mutex<T>, context: &str) -> MutexGuard<'a, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            error!(
                "SECURITY: Mutex poisoned for '{}' - recovering with potentially stale data",
                context
            );
            poisoned.into_inner()
        }
    }
}

/// Attempt to acquire a Mutex lock, returning an error on poisoning.
///
/// Use this for security-critical locks where recovery is not acceptable.
///
/// # Arguments
/// * `mutex` - The Mutex to lock
///
/// # Returns
/// * `Ok(MutexGuard)` - Lock acquired successfully
/// * `Err(PoisonError)` - Lock was poisoned
pub fn lock_or_fail<'a, T>(
    mutex: &'a Mutex<T>,
) -> Result<MutexGuard<'a, T>, PoisonError<MutexGuard<'a, T>>> {
    mutex.lock()
}

// =============================================================================
// RwLock Read Recovery
// =============================================================================

/// Acquire a RwLock read lock, recovering from poisoning if necessary.
///
/// # Arguments
/// * `rwlock` - The RwLock to read-lock
/// * `context` - A description of what the lock protects (for logging)
///
/// # Returns
/// The RwLockReadGuard, either fresh or recovered from poisoning
pub fn read_lock_or_recover<'a, T>(rwlock: &'a RwLock<T>, context: &str) -> RwLockReadGuard<'a, T> {
    match rwlock.read() {
        Ok(guard) => guard,
        Err(poisoned) => {
            error!(
                "SECURITY: RwLock (read) poisoned for '{}' - recovering with potentially stale data",
                context
            );
            poisoned.into_inner()
        }
    }
}

// =============================================================================
// RwLock Write Recovery
// =============================================================================

/// Acquire a RwLock write lock, recovering from poisoning if necessary.
///
/// # Arguments
/// * `rwlock` - The RwLock to write-lock
/// * `context` - A description of what the lock protects (for logging)
///
/// # Returns
/// The RwLockWriteGuard, either fresh or recovered from poisoning
pub fn write_lock_or_recover<'a, T>(
    rwlock: &'a RwLock<T>,
    context: &str,
) -> RwLockWriteGuard<'a, T> {
    match rwlock.write() {
        Ok(guard) => guard,
        Err(poisoned) => {
            error!(
                "SECURITY: RwLock (write) poisoned for '{}' - recovering with potentially stale data",
                context
            );
            poisoned.into_inner()
        }
    }
}

// =============================================================================
// Convenience Macros (for inline use without context parameter)
// =============================================================================

/// Macro for acquiring a Mutex lock with automatic context from variable name.
///
/// # Usage
/// ```ignore
/// let guard = lock_recover!(self.metrics);
/// ```
#[macro_export]
macro_rules! lock_recover {
    ($lock:expr) => {{
        use tracing::error;
        match $lock.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                error!(
                    "SECURITY: Mutex poisoned at {}:{} - recovering",
                    file!(),
                    line!()
                );
                poisoned.into_inner()
            }
        }
    }};
}

/// Macro for acquiring a RwLock read lock with automatic context.
///
/// # Usage
/// ```ignore
/// let guard = read_lock_recover!(self.data);
/// ```
#[macro_export]
macro_rules! read_lock_recover {
    ($lock:expr) => {{
        use tracing::error;
        match $lock.read() {
            Ok(guard) => guard,
            Err(poisoned) => {
                error!(
                    "SECURITY: RwLock (read) poisoned at {}:{} - recovering",
                    file!(),
                    line!()
                );
                poisoned.into_inner()
            }
        }
    }};
}

/// Macro for acquiring a RwLock write lock with automatic context.
///
/// # Usage
/// ```ignore
/// let guard = write_lock_recover!(self.data);
/// ```
#[macro_export]
macro_rules! write_lock_recover {
    ($lock:expr) => {{
        use tracing::error;
        match $lock.write() {
            Ok(guard) => guard,
            Err(poisoned) => {
                error!(
                    "SECURITY: RwLock (write) poisoned at {}:{} - recovering",
                    file!(),
                    line!()
                );
                poisoned.into_inner()
            }
        }
    }};
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_lock_or_recover_normal_operation() {
        let mutex = Mutex::new(42);
        let guard = lock_or_recover(&mutex, "test value");
        assert_eq!(*guard, 42);
    }

    #[test]
    fn test_read_lock_or_recover_normal_operation() {
        let rwlock = RwLock::new("test");
        let guard = read_lock_or_recover(&rwlock, "test string");
        assert_eq!(*guard, "test");
    }

    #[test]
    fn test_write_lock_or_recover_normal_operation() {
        let rwlock = RwLock::new(0);
        {
            let mut guard = write_lock_or_recover(&rwlock, "test counter");
            *guard = 100;
        }
        let guard = read_lock_or_recover(&rwlock, "test counter");
        assert_eq!(*guard, 100);
    }

    #[test]
    fn test_lock_recover_macro_normal_operation() {
        let mutex = Mutex::new(vec![1, 2, 3]);
        let guard = lock_recover!(mutex);
        assert_eq!(guard.len(), 3);
    }

    #[test]
    fn test_mutex_poisoning_recovery() {
        let mutex = Arc::new(Mutex::new(42));
        let mutex_clone = Arc::clone(&mutex);

        // Spawn a thread that will panic while holding the lock
        let handle = thread::spawn(move || {
            let _guard = mutex_clone.lock().unwrap();
            panic!("Intentional panic to poison the lock");
        });

        // Wait for the thread to panic
        let _ = handle.join();

        // The lock should now be poisoned, but we should be able to recover
        let guard = lock_or_recover(&mutex, "poisoned test");
        assert_eq!(*guard, 42);
    }

    #[test]
    fn test_rwlock_poisoning_recovery() {
        let rwlock = Arc::new(RwLock::new(String::from("original")));
        let rwlock_clone = Arc::clone(&rwlock);

        // Spawn a thread that will panic while holding the write lock
        let handle = thread::spawn(move || {
            let _guard = rwlock_clone.write().unwrap();
            panic!("Intentional panic to poison the lock");
        });

        // Wait for the thread to panic
        let _ = handle.join();

        // The lock should now be poisoned, but we should be able to recover
        let guard = read_lock_or_recover(&rwlock, "poisoned rwlock");
        assert_eq!(*guard, "original");
    }

    #[test]
    fn test_lock_or_fail_returns_error_on_poisoning() {
        let mutex = Arc::new(Mutex::new(0));
        let mutex_clone = Arc::clone(&mutex);

        // Poison the lock
        let handle = thread::spawn(move || {
            let _guard = mutex_clone.lock().unwrap();
            panic!("Poison!");
        });
        let _ = handle.join();

        // lock_or_fail should return an error
        let result = lock_or_fail(&mutex);
        assert!(result.is_err());
    }
}
