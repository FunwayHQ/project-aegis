//! DNS Persistence Layer
//!
//! SQLite storage for DNS zones and records. Provides durability across
//! restarts and supports the tiered account system.

use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::zone_store::{Zone, ZoneStore};
use super::{DnsError, DnsRecord, DnsRecordType, DnsRecordValue};

/// SQLite persistence for DNS zones and records
pub struct DnsPersistence {
    conn: Arc<Mutex<Connection>>,
}

impl DnsPersistence {
    /// Create a new persistence layer with the given database path
    pub fn new(db_path: &str) -> Result<Self, DnsError> {
        let conn = Connection::open(db_path)
            .map_err(|e| DnsError::ServerError(format!("Failed to open database: {}", e)))?;

        // Initialize database schema synchronously
        Self::create_tables(&conn)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Create an in-memory database (useful for testing)
    pub fn in_memory() -> Result<Self, DnsError> {
        let conn = Connection::open_in_memory()
            .map_err(|e| DnsError::ServerError(format!("Failed to open in-memory database: {}", e)))?;

        // Initialize database schema synchronously
        Self::create_tables(&conn)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Create database tables
    fn create_tables(conn: &Connection) -> Result<(), DnsError> {
        // Enable foreign keys
        conn.execute("PRAGMA foreign_keys = ON", [])
            .map_err(|e| DnsError::ServerError(format!("Failed to enable foreign keys: {}", e)))?;

        // Create zones table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS zones (
                domain TEXT PRIMARY KEY,
                proxied INTEGER NOT NULL DEFAULT 0,
                dnssec_enabled INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
            [],
        )
        .map_err(|e| DnsError::ServerError(format!("Failed to create zones table: {}", e)))?;

        // Create records table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS records (
                id TEXT PRIMARY KEY,
                domain TEXT NOT NULL,
                name TEXT NOT NULL,
                record_type TEXT NOT NULL,
                ttl INTEGER NOT NULL,
                value_json TEXT NOT NULL,
                priority INTEGER,
                proxied INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY (domain) REFERENCES zones(domain) ON DELETE CASCADE
            )",
            [],
        )
        .map_err(|e| DnsError::ServerError(format!("Failed to create records table: {}", e)))?;

        // Create index on domain for records
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_records_domain ON records(domain)",
            [],
        )
        .map_err(|e| DnsError::ServerError(format!("Failed to create index: {}", e)))?;

        // Create accounts table for tier tracking
        conn.execute(
            "CREATE TABLE IF NOT EXISTS accounts (
                account_id TEXT PRIMARY KEY,
                staked_amount INTEGER NOT NULL DEFAULT 0,
                tier TEXT NOT NULL DEFAULT 'free',
                features_json TEXT NOT NULL DEFAULT '{}',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
            [],
        )
        .map_err(|e| DnsError::ServerError(format!("Failed to create accounts table: {}", e)))?;

        // Create zone_accounts mapping (which account owns which zone)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS zone_accounts (
                domain TEXT PRIMARY KEY,
                account_id TEXT NOT NULL,
                FOREIGN KEY (domain) REFERENCES zones(domain) ON DELETE CASCADE
            )",
            [],
        )
        .map_err(|e| DnsError::ServerError(format!("Failed to create zone_accounts table: {}", e)))?;

        Ok(())
    }

    /// Save a zone to the database
    pub async fn save_zone(&self, zone: &Zone) -> Result<(), DnsError> {
        let conn = self.conn.lock().await;

        conn.execute(
            "INSERT INTO zones (domain, proxied, dnssec_enabled, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(domain) DO UPDATE SET
                proxied = excluded.proxied,
                dnssec_enabled = excluded.dnssec_enabled,
                updated_at = excluded.updated_at",
            params![
                zone.domain,
                zone.proxied as i32,
                zone.dnssec_enabled as i32,
                zone.created_at as i64,
                zone.updated_at as i64,
            ],
        )
        .map_err(|e| DnsError::ServerError(format!("Failed to save zone: {}", e)))?;

        // Save all records
        for record in &zone.records {
            self.save_record_internal(&conn, &zone.domain, record)?;
        }

        Ok(())
    }

    /// Save a record to the database (internal, uses existing connection)
    fn save_record_internal(
        &self,
        conn: &Connection,
        domain: &str,
        record: &DnsRecord,
    ) -> Result<(), DnsError> {
        let value_json = serde_json::to_string(&record.value)
            .map_err(|e| DnsError::ServerError(format!("Failed to serialize record value: {}", e)))?;

        conn.execute(
            "INSERT INTO records (id, domain, name, record_type, ttl, value_json, priority, proxied)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                record_type = excluded.record_type,
                ttl = excluded.ttl,
                value_json = excluded.value_json,
                priority = excluded.priority,
                proxied = excluded.proxied",
            params![
                record.id,
                domain,
                record.name,
                record.record_type.to_string(),
                record.ttl as i32,
                value_json,
                record.priority.map(|p| p as i32),
                record.proxied as i32,
            ],
        )
        .map_err(|e| DnsError::ServerError(format!("Failed to save record: {}", e)))?;

        Ok(())
    }

    /// Save a single record
    pub async fn save_record(&self, domain: &str, record: &DnsRecord) -> Result<(), DnsError> {
        let conn = self.conn.lock().await;
        self.save_record_internal(&conn, domain, record)
    }

    /// Load a zone from the database
    pub async fn load_zone(&self, domain: &str) -> Result<Option<Zone>, DnsError> {
        let conn = self.conn.lock().await;

        // Load zone metadata
        let zone_row: Option<(String, i32, i32, i64, i64)> = conn
            .query_row(
                "SELECT domain, proxied, dnssec_enabled, created_at, updated_at
                 FROM zones WHERE domain = ?1",
                params![domain],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .optional()
            .map_err(|e| DnsError::ServerError(format!("Failed to load zone: {}", e)))?;

        let zone_row = match zone_row {
            Some(r) => r,
            None => return Ok(None),
        };

        // Load records
        let records = self.load_records_internal(&conn, domain)?;

        Ok(Some(Zone {
            domain: zone_row.0,
            records,
            proxied: zone_row.1 != 0,
            dnssec_enabled: zone_row.2 != 0,
            created_at: zone_row.3 as u64,
            updated_at: zone_row.4 as u64,
        }))
    }

    /// Load records for a domain (internal, uses existing connection)
    fn load_records_internal(&self, conn: &Connection, domain: &str) -> Result<Vec<DnsRecord>, DnsError> {
        let mut stmt = conn
            .prepare(
                "SELECT id, name, record_type, ttl, value_json, priority, proxied
                 FROM records WHERE domain = ?1",
            )
            .map_err(|e| DnsError::ServerError(format!("Failed to prepare statement: {}", e)))?;

        let records = stmt
            .query_map(params![domain], |row| {
                let id: String = row.get(0)?;
                let name: String = row.get(1)?;
                let record_type_str: String = row.get(2)?;
                let ttl: i32 = row.get(3)?;
                let value_json: String = row.get(4)?;
                let priority: Option<i32> = row.get(5)?;
                let proxied: i32 = row.get(6)?;

                Ok((id, name, record_type_str, ttl, value_json, priority, proxied))
            })
            .map_err(|e| DnsError::ServerError(format!("Failed to query records: {}", e)))?
            .filter_map(|r| r.ok())
            .filter_map(|(id, name, record_type_str, ttl, value_json, priority, proxied)| {
                let record_type = record_type_str.parse::<DnsRecordType>().ok()?;
                let value: DnsRecordValue = serde_json::from_str(&value_json).ok()?;

                Some(DnsRecord {
                    id,
                    name,
                    record_type,
                    ttl: ttl as u32,
                    value,
                    priority: priority.map(|p| p as u16),
                    proxied: proxied != 0,
                })
            })
            .collect();

        Ok(records)
    }

    /// Delete a zone from the database
    pub async fn delete_zone(&self, domain: &str) -> Result<bool, DnsError> {
        let conn = self.conn.lock().await;

        // Delete records first (should cascade, but be explicit)
        conn.execute("DELETE FROM records WHERE domain = ?1", params![domain])
            .map_err(|e| DnsError::ServerError(format!("Failed to delete records: {}", e)))?;

        // Delete zone
        let rows_affected = conn
            .execute("DELETE FROM zones WHERE domain = ?1", params![domain])
            .map_err(|e| DnsError::ServerError(format!("Failed to delete zone: {}", e)))?;

        Ok(rows_affected > 0)
    }

    /// Delete a record from the database
    pub async fn delete_record(&self, record_id: &str) -> Result<bool, DnsError> {
        let conn = self.conn.lock().await;

        let rows_affected = conn
            .execute("DELETE FROM records WHERE id = ?1", params![record_id])
            .map_err(|e| DnsError::ServerError(format!("Failed to delete record: {}", e)))?;

        Ok(rows_affected > 0)
    }

    /// List all zones
    pub async fn list_zones(&self) -> Result<Vec<Zone>, DnsError> {
        let conn = self.conn.lock().await;

        let mut stmt = conn
            .prepare(
                "SELECT domain, proxied, dnssec_enabled, created_at, updated_at FROM zones",
            )
            .map_err(|e| DnsError::ServerError(format!("Failed to prepare statement: {}", e)))?;

        let zone_rows: Vec<(String, i32, i32, i64, i64)> = stmt
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
            })
            .map_err(|e| DnsError::ServerError(format!("Failed to query zones: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        let mut zones = Vec::new();
        for (domain, proxied, dnssec_enabled, created_at, updated_at) in zone_rows {
            let records = self.load_records_internal(&conn, &domain)?;
            zones.push(Zone {
                domain,
                records,
                proxied: proxied != 0,
                dnssec_enabled: dnssec_enabled != 0,
                created_at: created_at as u64,
                updated_at: updated_at as u64,
            });
        }

        Ok(zones)
    }

    /// List records for a domain
    pub async fn list_records(&self, domain: &str) -> Result<Vec<DnsRecord>, DnsError> {
        let conn = self.conn.lock().await;
        self.load_records_internal(&conn, domain)
    }

    /// Get zone count
    pub async fn zone_count(&self) -> Result<usize, DnsError> {
        let conn = self.conn.lock().await;

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM zones", [], |row| row.get(0))
            .map_err(|e| DnsError::ServerError(format!("Failed to count zones: {}", e)))?;

        Ok(count as usize)
    }

    /// Get zone count for an account
    pub async fn zone_count_for_account(&self, account_id: &str) -> Result<usize, DnsError> {
        let conn = self.conn.lock().await;

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM zone_accounts WHERE account_id = ?1",
                params![account_id],
                |row| row.get(0),
            )
            .map_err(|e| DnsError::ServerError(format!("Failed to count zones for account: {}", e)))?;

        Ok(count as usize)
    }

    /// Associate a zone with an account
    pub async fn associate_zone_account(&self, domain: &str, account_id: &str) -> Result<(), DnsError> {
        let conn = self.conn.lock().await;

        conn.execute(
            "INSERT INTO zone_accounts (domain, account_id) VALUES (?1, ?2)
             ON CONFLICT(domain) DO UPDATE SET account_id = excluded.account_id",
            params![domain, account_id],
        )
        .map_err(|e| DnsError::ServerError(format!("Failed to associate zone with account: {}", e)))?;

        Ok(())
    }

    /// Get account for a zone
    pub async fn get_zone_account(&self, domain: &str) -> Result<Option<String>, DnsError> {
        let conn = self.conn.lock().await;

        let account_id: Option<String> = conn
            .query_row(
                "SELECT account_id FROM zone_accounts WHERE domain = ?1",
                params![domain],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| DnsError::ServerError(format!("Failed to get zone account: {}", e)))?;

        Ok(account_id)
    }

    /// Restore all zones to a ZoneStore on startup
    pub async fn restore_to_store(&self, store: &ZoneStore) -> Result<usize, DnsError> {
        let zones = self.list_zones().await?;
        let count = zones.len();

        for zone in zones {
            store.upsert_zone(zone).await?;
        }

        Ok(count)
    }

    /// Update zone settings
    pub async fn update_zone_settings(
        &self,
        domain: &str,
        proxied: Option<bool>,
        dnssec_enabled: Option<bool>,
    ) -> Result<(), DnsError> {
        let conn = self.conn.lock().await;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        if let Some(p) = proxied {
            conn.execute(
                "UPDATE zones SET proxied = ?1, updated_at = ?2 WHERE domain = ?3",
                params![p as i32, now, domain],
            )
            .map_err(|e| DnsError::ServerError(format!("Failed to update zone proxied: {}", e)))?;
        }

        if let Some(d) = dnssec_enabled {
            conn.execute(
                "UPDATE zones SET dnssec_enabled = ?1, updated_at = ?2 WHERE domain = ?3",
                params![d as i32, now, domain],
            )
            .map_err(|e| DnsError::ServerError(format!("Failed to update zone dnssec: {}", e)))?;
        }

        Ok(())
    }

    /// Check if a zone exists
    pub async fn zone_exists(&self, domain: &str) -> Result<bool, DnsError> {
        let conn = self.conn.lock().await;

        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM zones WHERE domain = ?1",
                params![domain],
                |row| row.get(0),
            )
            .map_err(|e| DnsError::ServerError(format!("Failed to check zone existence: {}", e)))?;

        Ok(exists > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn create_test_zone() -> Zone {
        let mut zone = Zone::new("example.com", true);
        zone.dnssec_enabled = false;
        zone.add_record(DnsRecord::a("www", Ipv4Addr::new(192, 168, 1, 1), 300));
        zone.add_record(DnsRecord::a("mail", Ipv4Addr::new(192, 168, 1, 2), 300));
        zone.add_record(DnsRecord::txt("@", "v=spf1 -all", 3600));
        zone
    }

    #[tokio::test]
    async fn test_save_and_load_zone() {
        let persistence = DnsPersistence::in_memory().unwrap();

        let zone = create_test_zone();
        persistence.save_zone(&zone).await.unwrap();

        let loaded = persistence.load_zone("example.com").await.unwrap().unwrap();
        assert_eq!(loaded.domain, "example.com");
        assert!(loaded.proxied);
        assert!(!loaded.dnssec_enabled);
        assert_eq!(loaded.records.len(), 3);
    }

    #[tokio::test]
    async fn test_zone_not_found() {
        let persistence = DnsPersistence::in_memory().unwrap();

        let loaded = persistence.load_zone("nonexistent.com").await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_delete_zone() {
        let persistence = DnsPersistence::in_memory().unwrap();

        let zone = create_test_zone();
        persistence.save_zone(&zone).await.unwrap();

        assert!(persistence.delete_zone("example.com").await.unwrap());
        assert!(persistence.load_zone("example.com").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_zone() {
        let persistence = DnsPersistence::in_memory().unwrap();

        assert!(!persistence.delete_zone("nonexistent.com").await.unwrap());
    }

    #[tokio::test]
    async fn test_list_zones() {
        let persistence = DnsPersistence::in_memory().unwrap();

        let zone1 = Zone::new("example.com", true);
        let zone2 = Zone::new("test.com", false);

        persistence.save_zone(&zone1).await.unwrap();
        persistence.save_zone(&zone2).await.unwrap();

        let zones = persistence.list_zones().await.unwrap();
        assert_eq!(zones.len(), 2);
    }

    #[tokio::test]
    async fn test_zone_count() {
        let persistence = DnsPersistence::in_memory().unwrap();

        assert_eq!(persistence.zone_count().await.unwrap(), 0);

        let zone = Zone::new("example.com", true);
        persistence.save_zone(&zone).await.unwrap();

        assert_eq!(persistence.zone_count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_save_and_delete_record() {
        let persistence = DnsPersistence::in_memory().unwrap();

        let zone = Zone::new("example.com", true);
        persistence.save_zone(&zone).await.unwrap();

        let record = DnsRecord::a("www", Ipv4Addr::new(192, 168, 1, 1), 300);
        let record_id = record.id.clone();
        persistence.save_record("example.com", &record).await.unwrap();

        let records = persistence.list_records("example.com").await.unwrap();
        assert_eq!(records.len(), 1);

        assert!(persistence.delete_record(&record_id).await.unwrap());
        let records = persistence.list_records("example.com").await.unwrap();
        assert!(records.is_empty());
    }

    #[tokio::test]
    async fn test_update_zone_settings() {
        let persistence = DnsPersistence::in_memory().unwrap();

        let zone = Zone::new("example.com", false);
        persistence.save_zone(&zone).await.unwrap();

        persistence
            .update_zone_settings("example.com", Some(true), Some(true))
            .await
            .unwrap();

        let loaded = persistence.load_zone("example.com").await.unwrap().unwrap();
        assert!(loaded.proxied);
        assert!(loaded.dnssec_enabled);
    }

    #[tokio::test]
    async fn test_zone_exists() {
        let persistence = DnsPersistence::in_memory().unwrap();

        assert!(!persistence.zone_exists("example.com").await.unwrap());

        let zone = Zone::new("example.com", false);
        persistence.save_zone(&zone).await.unwrap();

        assert!(persistence.zone_exists("example.com").await.unwrap());
    }

    #[tokio::test]
    async fn test_zone_account_association() {
        let persistence = DnsPersistence::in_memory().unwrap();

        let zone = Zone::new("example.com", false);
        persistence.save_zone(&zone).await.unwrap();

        persistence
            .associate_zone_account("example.com", "account_123")
            .await
            .unwrap();

        let account = persistence
            .get_zone_account("example.com")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(account, "account_123");
    }

    #[tokio::test]
    async fn test_zone_count_for_account() {
        let persistence = DnsPersistence::in_memory().unwrap();

        // Create zones
        let zone1 = Zone::new("example.com", false);
        let zone2 = Zone::new("test.com", false);
        let zone3 = Zone::new("other.com", false);

        persistence.save_zone(&zone1).await.unwrap();
        persistence.save_zone(&zone2).await.unwrap();
        persistence.save_zone(&zone3).await.unwrap();

        // Associate with accounts
        persistence
            .associate_zone_account("example.com", "account_a")
            .await
            .unwrap();
        persistence
            .associate_zone_account("test.com", "account_a")
            .await
            .unwrap();
        persistence
            .associate_zone_account("other.com", "account_b")
            .await
            .unwrap();

        assert_eq!(persistence.zone_count_for_account("account_a").await.unwrap(), 2);
        assert_eq!(persistence.zone_count_for_account("account_b").await.unwrap(), 1);
        assert_eq!(persistence.zone_count_for_account("account_c").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_restore_to_store() {
        let persistence = DnsPersistence::in_memory().unwrap();

        let zone1 = create_test_zone();
        let mut zone2 = Zone::new("test.com", false);
        zone2.add_record(DnsRecord::a("@", Ipv4Addr::new(10, 0, 0, 1), 300));

        persistence.save_zone(&zone1).await.unwrap();
        persistence.save_zone(&zone2).await.unwrap();

        let store = ZoneStore::new();
        let count = persistence.restore_to_store(&store).await.unwrap();
        assert_eq!(count, 2);

        let restored = store.get_zone("example.com").await.unwrap();
        assert_eq!(restored.domain, "example.com");
        assert_eq!(restored.records.len(), 3);
    }

    #[tokio::test]
    async fn test_upsert_zone() {
        let persistence = DnsPersistence::in_memory().unwrap();

        // Initial save
        let zone = Zone::new("example.com", false);
        persistence.save_zone(&zone).await.unwrap();

        // Update (upsert)
        let mut updated_zone = Zone::new("example.com", true);
        updated_zone.dnssec_enabled = true;
        persistence.save_zone(&updated_zone).await.unwrap();

        let loaded = persistence.load_zone("example.com").await.unwrap().unwrap();
        assert!(loaded.proxied);
        assert!(loaded.dnssec_enabled);

        // Should still be only one zone
        assert_eq!(persistence.zone_count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_record_types_serialization() {
        let persistence = DnsPersistence::in_memory().unwrap();

        let mut zone = Zone::new("example.com", false);

        // Add various record types
        zone.add_record(DnsRecord::a("www", Ipv4Addr::new(192, 168, 1, 1), 300));
        zone.add_record(DnsRecord::aaaa("www", "2001:db8::1".parse().unwrap(), 300));
        zone.add_record(DnsRecord::cname("blog", "www.example.com", 300));
        zone.add_record(DnsRecord::mx("@", "mail.example.com", 10, 3600));
        zone.add_record(DnsRecord::txt("@", "v=spf1 -all", 3600));
        zone.add_record(DnsRecord::ns("@", "ns1.aegis.network", 86400));

        persistence.save_zone(&zone).await.unwrap();

        let loaded = persistence.load_zone("example.com").await.unwrap().unwrap();
        assert_eq!(loaded.records.len(), 6);

        // Check that types are correctly preserved
        let record_types: Vec<DnsRecordType> = loaded.records.iter().map(|r| r.record_type).collect();
        assert!(record_types.contains(&DnsRecordType::A));
        assert!(record_types.contains(&DnsRecordType::AAAA));
        assert!(record_types.contains(&DnsRecordType::CNAME));
        assert!(record_types.contains(&DnsRecordType::MX));
        assert!(record_types.contains(&DnsRecordType::TXT));
        assert!(record_types.contains(&DnsRecordType::NS));
    }

    #[tokio::test]
    async fn test_cascade_delete() {
        let persistence = DnsPersistence::in_memory().unwrap();

        let zone = create_test_zone();
        persistence.save_zone(&zone).await.unwrap();

        // Associate with account
        persistence
            .associate_zone_account("example.com", "account_123")
            .await
            .unwrap();

        // Delete zone - should cascade to records
        persistence.delete_zone("example.com").await.unwrap();

        // Records should be gone
        let records = persistence.list_records("example.com").await.unwrap();
        assert!(records.is_empty());
    }
}
