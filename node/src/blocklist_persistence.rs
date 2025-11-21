use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

#[cfg(target_os = "linux")]
use crate::ebpf_loader::EbpfLoader;

/// Blocklist entry for persistence
#[derive(Debug, Clone)]
pub struct BlocklistEntry {
    pub ip: String,
    pub blocked_until_us: u64, // Microseconds since UNIX epoch
    pub reason: String,
    pub created_at: u64, // Unix timestamp in seconds
}

impl BlocklistEntry {
    /// Create a new blocklist entry
    pub fn new(ip: String, duration_secs: u64, reason: String) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let blocked_until_us = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64
            + (duration_secs * 1_000_000);

        Self {
            ip,
            blocked_until_us,
            reason,
            created_at: now,
        }
    }

    /// Check if this entry has expired
    pub fn is_expired(&self) -> bool {
        let now_us = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        now_us > self.blocked_until_us
    }

    /// Get remaining duration in seconds
    pub fn remaining_secs(&self) -> u64 {
        let now_us = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        if now_us >= self.blocked_until_us {
            0
        } else {
            (self.blocked_until_us - now_us) / 1_000_000
        }
    }
}

/// Persistent storage for eBPF blocklist using SQLite
pub struct BlocklistPersistence {
    conn: Connection,
}

impl BlocklistPersistence {
    /// Create or open blocklist database
    pub fn new(db_path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(db_path.as_ref())
            .context("Failed to open blocklist database")?;

        // Create table if it doesn't exist
        conn.execute(
            "CREATE TABLE IF NOT EXISTS blocklist (
                ip TEXT PRIMARY KEY,
                blocked_until_us INTEGER NOT NULL,
                reason TEXT NOT NULL,
                created_at INTEGER NOT NULL
            )",
            [],
        )
        .context("Failed to create blocklist table")?;

        // Create index on expiration time for efficient cleanup
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_blocked_until
             ON blocklist(blocked_until_us)",
            [],
        )
        .context("Failed to create index")?;

        info!(
            "Opened blocklist persistence database: {:?}",
            db_path.as_ref()
        );

        Ok(Self { conn })
    }

    /// Add an IP to the persistent blocklist
    pub fn add_entry(&self, entry: &BlocklistEntry) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO blocklist (ip, blocked_until_us, reason, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                entry.ip,
                entry.blocked_until_us,
                entry.reason,
                entry.created_at
            ],
        )?;

        info!(
            "Added {} to persistent blocklist (expires in {}s)",
            entry.ip,
            entry.remaining_secs()
        );
        Ok(())
    }

    /// Remove an IP from the persistent blocklist
    pub fn remove_entry(&self, ip: &str) -> Result<()> {
        let rows = self
            .conn
            .execute("DELETE FROM blocklist WHERE ip = ?1", params![ip])?;

        if rows > 0 {
            info!("Removed {} from persistent blocklist", ip);
        }
        Ok(())
    }

    /// Get all non-expired blocklist entries
    pub fn get_active_entries(&self) -> Result<Vec<BlocklistEntry>> {
        let now_us = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        let mut stmt = self.conn.prepare(
            "SELECT ip, blocked_until_us, reason, created_at
             FROM blocklist
             WHERE blocked_until_us > ?1
             ORDER BY created_at DESC",
        )?;

        let entries = stmt
            .query_map(params![now_us], |row| {
                Ok(BlocklistEntry {
                    ip: row.get(0)?,
                    blocked_until_us: row.get(1)?,
                    reason: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(entries)
    }

    /// Get all entries (including expired)
    pub fn get_all_entries(&self) -> Result<Vec<BlocklistEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT ip, blocked_until_us, reason, created_at
             FROM blocklist
             ORDER BY created_at DESC",
        )?;

        let entries = stmt
            .query_map([], |row| {
                Ok(BlocklistEntry {
                    ip: row.get(0)?,
                    blocked_until_us: row.get(1)?,
                    reason: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(entries)
    }

    /// Clean up expired entries from the database
    pub fn cleanup_expired(&self) -> Result<usize> {
        let now_us = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        let deleted = self.conn.execute(
            "DELETE FROM blocklist WHERE blocked_until_us <= ?1",
            params![now_us],
        )?;

        if deleted > 0 {
            info!("Cleaned up {} expired entries from blocklist", deleted);
        }

        Ok(deleted)
    }

    /// Get total count of entries
    pub fn count(&self) -> Result<usize> {
        let count: usize = self
            .conn
            .query_row("SELECT COUNT(*) FROM blocklist", [], |row| row.get(0))?;
        Ok(count)
    }

    /// Get count of active (non-expired) entries
    pub fn count_active(&self) -> Result<usize> {
        let now_us = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        let count: usize = self.conn.query_row(
            "SELECT COUNT(*) FROM blocklist WHERE blocked_until_us > ?1",
            params![now_us],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Restore blocklist to eBPF from persistent storage (Linux only)
    #[cfg(target_os = "linux")]
    pub fn restore_to_ebpf(&self, ebpf_loader: &mut EbpfLoader) -> Result<usize> {
        let entries = self.get_active_entries()?;
        let mut restored = 0;

        for entry in entries {
            let duration_secs = entry.remaining_secs();
            if duration_secs > 0 {
                match ebpf_loader.blocklist_ip(&entry.ip, duration_secs) {
                    Ok(_) => {
                        restored += 1;
                    }
                    Err(e) => {
                        warn!("Failed to restore {} to eBPF blocklist: {}", entry.ip, e);
                    }
                }
            }
        }

        info!(
            "Restored {} entries from persistent storage to eBPF blocklist",
            restored
        );
        Ok(restored)
    }

    /// Save eBPF blocklist to persistent storage (Linux only)
    #[cfg(target_os = "linux")]
    pub fn save_from_ebpf(&self, ebpf_loader: &EbpfLoader) -> Result<usize> {
        let blocklist = ebpf_loader.get_blocklist()?;
        let mut saved = 0;

        for (ip, blocked_until_us) in blocklist {
            let entry = BlocklistEntry {
                ip: ip.clone(),
                blocked_until_us,
                reason: "Restored from eBPF".to_string(),
                created_at: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            };

            if !entry.is_expired() {
                match self.add_entry(&entry) {
                    Ok(_) => {
                        saved += 1;
                    }
                    Err(e) => {
                        warn!("Failed to save {} to persistent blocklist: {}", ip, e);
                    }
                }
            }
        }

        info!(
            "Saved {} entries from eBPF blocklist to persistent storage",
            saved
        );
        Ok(saved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_create_database() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test_blocklist.db");

        let persistence = BlocklistPersistence::new(&db_path).unwrap();
        assert_eq!(persistence.count().unwrap(), 0);
    }

    #[test]
    fn test_add_and_retrieve_entry() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test_blocklist.db");
        let persistence = BlocklistPersistence::new(&db_path).unwrap();

        let entry = BlocklistEntry::new("192.168.1.100".to_string(), 60, "Test block".to_string());

        persistence.add_entry(&entry).unwrap();
        assert_eq!(persistence.count().unwrap(), 1);

        let entries = persistence.get_active_entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].ip, "192.168.1.100");
    }

    #[test]
    fn test_remove_entry() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test_blocklist.db");
        let persistence = BlocklistPersistence::new(&db_path).unwrap();

        let entry = BlocklistEntry::new("192.168.1.100".to_string(), 60, "Test block".to_string());

        persistence.add_entry(&entry).unwrap();
        assert_eq!(persistence.count().unwrap(), 1);

        persistence.remove_entry("192.168.1.100").unwrap();
        assert_eq!(persistence.count().unwrap(), 0);
    }

    #[test]
    fn test_cleanup_expired() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test_blocklist.db");
        let persistence = BlocklistPersistence::new(&db_path).unwrap();

        // Add entry that expires in 1 second
        let entry =
            BlocklistEntry::new("192.168.1.100".to_string(), 1, "Short block".to_string());
        persistence.add_entry(&entry).unwrap();

        // Should have 1 active entry
        assert_eq!(persistence.count_active().unwrap(), 1);

        // Wait for expiration
        thread::sleep(Duration::from_secs(2));

        // Should have 0 active entries
        assert_eq!(persistence.count_active().unwrap(), 0);

        // Cleanup should remove expired entry
        let deleted = persistence.cleanup_expired().unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(persistence.count().unwrap(), 0);
    }

    #[test]
    fn test_entry_expiration() {
        let entry = BlocklistEntry::new("192.168.1.100".to_string(), 1, "Test".to_string());

        assert!(!entry.is_expired());
        assert!(entry.remaining_secs() > 0);

        thread::sleep(Duration::from_secs(2));

        assert!(entry.is_expired());
        assert_eq!(entry.remaining_secs(), 0);
    }

    #[test]
    fn test_multiple_entries() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test_blocklist.db");
        let persistence = BlocklistPersistence::new(&db_path).unwrap();

        for i in 1..=5 {
            let entry = BlocklistEntry::new(
                format!("192.168.1.{}", i),
                60,
                format!("Block {}", i),
            );
            persistence.add_entry(&entry).unwrap();
        }

        assert_eq!(persistence.count().unwrap(), 5);

        let entries = persistence.get_active_entries().unwrap();
        assert_eq!(entries.len(), 5);
    }
}
