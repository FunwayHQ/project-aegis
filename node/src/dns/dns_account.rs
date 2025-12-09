//! DNS Account Management
//!
//! Manages account tiers and feature access based on $AEGIS staking.
//! Implements the freemium pricing model:
//! - Free: Up to 5 zones, standard resolution
//! - Paid (staking): Unlimited zones, DNSSEC, advanced analytics, priority support

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

/// Account tier based on $AEGIS staking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccountTier {
    /// Free tier: 5 zones, standard resolution
    Free,
    /// Basic staking tier: Unlimited zones
    Basic,
    /// Professional tier: + DNSSEC
    Professional,
    /// Business tier: + Advanced analytics
    Business,
    /// Enterprise tier: + Priority support, custom nameservers
    Enterprise,
}

impl AccountTier {
    /// Get tier from staking amount (in $AEGIS tokens)
    pub fn from_staking_amount(amount: u64) -> Self {
        match amount {
            0..=999 => AccountTier::Free,
            1000..=2499 => AccountTier::Basic,
            2500..=4999 => AccountTier::Professional,
            5000..=9999 => AccountTier::Business,
            _ => AccountTier::Enterprise,
        }
    }

    /// Get minimum staking amount for this tier
    pub fn min_staking_amount(&self) -> u64 {
        match self {
            AccountTier::Free => 0,
            AccountTier::Basic => 1000,
            AccountTier::Professional => 2500,
            AccountTier::Business => 5000,
            AccountTier::Enterprise => 10000,
        }
    }

    /// Get maximum zones allowed for this tier
    pub fn max_zones(&self) -> usize {
        match self {
            AccountTier::Free => 5,
            _ => usize::MAX, // Unlimited for paid tiers
        }
    }

    /// Check if DNSSEC is enabled for this tier
    pub fn has_dnssec(&self) -> bool {
        matches!(
            self,
            AccountTier::Professional | AccountTier::Business | AccountTier::Enterprise
        )
    }

    /// Check if advanced analytics is enabled for this tier
    pub fn has_advanced_analytics(&self) -> bool {
        matches!(self, AccountTier::Business | AccountTier::Enterprise)
    }

    /// Check if priority support is enabled for this tier
    pub fn has_priority_support(&self) -> bool {
        matches!(self, AccountTier::Enterprise)
    }

    /// Check if custom nameservers are enabled for this tier
    pub fn has_custom_nameservers(&self) -> bool {
        matches!(self, AccountTier::Enterprise)
    }

    /// Get rate limit (queries per second per zone)
    pub fn rate_limit_qps(&self) -> u32 {
        match self {
            AccountTier::Free => 1000,
            AccountTier::Basic => 5000,
            AccountTier::Professional => 10000,
            AccountTier::Business => 50000,
            AccountTier::Enterprise => 100000,
        }
    }

    /// Get analytics retention period in days
    pub fn analytics_retention_days(&self) -> u32 {
        match self {
            AccountTier::Free => 1,         // 24 hours
            AccountTier::Basic => 7,        // 1 week
            AccountTier::Professional => 30, // 1 month
            AccountTier::Business => 90,    // 3 months
            AccountTier::Enterprise => 365, // 1 year
        }
    }

    /// Get tier display name
    pub fn display_name(&self) -> &'static str {
        match self {
            AccountTier::Free => "Free",
            AccountTier::Basic => "Basic",
            AccountTier::Professional => "Professional",
            AccountTier::Business => "Business",
            AccountTier::Enterprise => "Enterprise",
        }
    }
}

impl std::fmt::Display for AccountTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

impl Default for AccountTier {
    fn default() -> Self {
        AccountTier::Free
    }
}

/// Features enabled for an account
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountFeatures {
    /// Maximum number of zones allowed
    pub max_zones: usize,
    /// DNSSEC signing enabled
    pub dnssec_enabled: bool,
    /// Advanced analytics enabled
    pub advanced_analytics: bool,
    /// Priority support enabled
    pub priority_support: bool,
    /// Custom nameservers enabled
    pub custom_nameservers: bool,
    /// Rate limit (queries per second per zone)
    pub rate_limit_qps: u32,
    /// Analytics retention in days
    pub analytics_retention_days: u32,
}

impl AccountFeatures {
    /// Create features from a tier
    pub fn from_tier(tier: AccountTier) -> Self {
        Self {
            max_zones: tier.max_zones(),
            dnssec_enabled: tier.has_dnssec(),
            advanced_analytics: tier.has_advanced_analytics(),
            priority_support: tier.has_priority_support(),
            custom_nameservers: tier.has_custom_nameservers(),
            rate_limit_qps: tier.rate_limit_qps(),
            analytics_retention_days: tier.analytics_retention_days(),
        }
    }
}

impl Default for AccountFeatures {
    fn default() -> Self {
        Self::from_tier(AccountTier::Free)
    }
}

/// DNS Account
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsAccount {
    /// Unique account identifier (wallet address or API key)
    pub account_id: String,
    /// Current staking amount in $AEGIS tokens
    pub staked_amount: u64,
    /// Current tier
    pub tier: AccountTier,
    /// Features enabled for this account
    pub features: AccountFeatures,
    /// Number of zones owned
    pub zone_count: usize,
    /// Account creation timestamp
    pub created_at: u64,
    /// Last update timestamp
    pub updated_at: u64,
}

impl DnsAccount {
    /// Create a new account
    pub fn new(account_id: impl Into<String>) -> Self {
        let now = current_timestamp();
        let tier = AccountTier::Free;

        Self {
            account_id: account_id.into(),
            staked_amount: 0,
            tier,
            features: AccountFeatures::from_tier(tier),
            zone_count: 0,
            created_at: now,
            updated_at: now,
        }
    }

    /// Update staking amount and recalculate tier
    pub fn update_staking(&mut self, amount: u64) {
        self.staked_amount = amount;
        self.tier = AccountTier::from_staking_amount(amount);
        self.features = AccountFeatures::from_tier(self.tier);
        self.updated_at = current_timestamp();
    }

    /// Check if account can create a new zone
    pub fn can_create_zone(&self) -> bool {
        self.zone_count < self.features.max_zones
    }

    /// Check if account can enable DNSSEC
    pub fn can_enable_dnssec(&self) -> bool {
        self.features.dnssec_enabled
    }

    /// Check if account can access advanced analytics
    pub fn can_access_analytics(&self) -> bool {
        self.features.advanced_analytics
    }

    /// Increment zone count
    pub fn increment_zone_count(&mut self) {
        self.zone_count += 1;
        self.updated_at = current_timestamp();
    }

    /// Decrement zone count
    pub fn decrement_zone_count(&mut self) {
        self.zone_count = self.zone_count.saturating_sub(1);
        self.updated_at = current_timestamp();
    }
}

/// Account manager for DNS service
pub struct AccountManager {
    /// In-memory account cache
    accounts: Arc<RwLock<HashMap<String, DnsAccount>>>,
    /// Default account for unauthenticated requests
    default_account: DnsAccount,
}

impl AccountManager {
    /// Create a new account manager
    pub fn new() -> Self {
        Self {
            accounts: Arc::new(RwLock::new(HashMap::new())),
            default_account: DnsAccount::new("anonymous"),
        }
    }

    /// Get or create an account
    pub async fn get_or_create(&self, account_id: &str) -> DnsAccount {
        let accounts = self.accounts.read().await;

        if let Some(account) = accounts.get(account_id) {
            return account.clone();
        }
        drop(accounts);

        // Create new account
        let account = DnsAccount::new(account_id);
        let mut accounts = self.accounts.write().await;
        accounts.insert(account_id.to_string(), account.clone());
        account
    }

    /// Get an account by ID
    pub async fn get(&self, account_id: &str) -> Option<DnsAccount> {
        let accounts = self.accounts.read().await;
        accounts.get(account_id).cloned()
    }

    /// Update an account
    pub async fn update(&self, account: DnsAccount) {
        let mut accounts = self.accounts.write().await;
        accounts.insert(account.account_id.clone(), account);
    }

    /// Update staking for an account
    pub async fn update_staking(&self, account_id: &str, amount: u64) -> DnsAccount {
        let mut accounts = self.accounts.write().await;

        let account = accounts
            .entry(account_id.to_string())
            .or_insert_with(|| DnsAccount::new(account_id));

        account.update_staking(amount);
        account.clone()
    }

    /// Check if account can create a zone
    pub async fn can_create_zone(&self, account_id: &str) -> bool {
        let account = self.get_or_create(account_id).await;
        account.can_create_zone()
    }

    /// Check if account can enable DNSSEC
    pub async fn can_enable_dnssec(&self, account_id: &str) -> bool {
        let account = self.get_or_create(account_id).await;
        account.can_enable_dnssec()
    }

    /// Record zone creation for an account
    pub async fn record_zone_created(&self, account_id: &str) {
        let mut accounts = self.accounts.write().await;
        if let Some(account) = accounts.get_mut(account_id) {
            account.increment_zone_count();
        }
    }

    /// Record zone deletion for an account
    pub async fn record_zone_deleted(&self, account_id: &str) {
        let mut accounts = self.accounts.write().await;
        if let Some(account) = accounts.get_mut(account_id) {
            account.decrement_zone_count();
        }
    }

    /// Get rate limit for an account
    pub async fn get_rate_limit(&self, account_id: &str) -> u32 {
        let account = self.get_or_create(account_id).await;
        account.features.rate_limit_qps
    }

    /// Get analytics retention for an account
    pub async fn get_analytics_retention(&self, account_id: &str) -> u32 {
        let account = self.get_or_create(account_id).await;
        account.features.analytics_retention_days
    }

    /// Get the default account for unauthenticated requests
    pub fn default_account(&self) -> &DnsAccount {
        &self.default_account
    }

    /// List all accounts
    pub async fn list_accounts(&self) -> Vec<DnsAccount> {
        let accounts = self.accounts.read().await;
        accounts.values().cloned().collect()
    }

    /// Delete an account
    pub async fn delete(&self, account_id: &str) -> bool {
        let mut accounts = self.accounts.write().await;
        accounts.remove(account_id).is_some()
    }
}

impl Default for AccountManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Error when tier requirements are not met
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierRequirementError {
    /// Feature that was denied
    pub feature: String,
    /// Current tier
    pub current_tier: AccountTier,
    /// Required tier
    pub required_tier: AccountTier,
    /// Required staking amount
    pub required_staking: u64,
    /// Human-readable message
    pub message: String,
}

impl TierRequirementError {
    /// Create error for zone limit exceeded
    pub fn zone_limit_exceeded(current_tier: AccountTier, current_count: usize) -> Self {
        Self {
            feature: "zones".to_string(),
            current_tier,
            required_tier: AccountTier::Basic,
            required_staking: AccountTier::Basic.min_staking_amount(),
            message: format!(
                "Zone limit exceeded ({}/{}). Stake {} $AEGIS for unlimited zones.",
                current_count,
                current_tier.max_zones(),
                AccountTier::Basic.min_staking_amount()
            ),
        }
    }

    /// Create error for DNSSEC not available
    pub fn dnssec_unavailable(current_tier: AccountTier) -> Self {
        Self {
            feature: "dnssec".to_string(),
            current_tier,
            required_tier: AccountTier::Professional,
            required_staking: AccountTier::Professional.min_staking_amount(),
            message: format!(
                "DNSSEC requires Professional tier. Stake {} $AEGIS to enable.",
                AccountTier::Professional.min_staking_amount()
            ),
        }
    }

    /// Create error for advanced analytics not available
    pub fn analytics_unavailable(current_tier: AccountTier) -> Self {
        Self {
            feature: "advanced_analytics".to_string(),
            current_tier,
            required_tier: AccountTier::Business,
            required_staking: AccountTier::Business.min_staking_amount(),
            message: format!(
                "Advanced analytics requires Business tier. Stake {} $AEGIS to enable.",
                AccountTier::Business.min_staking_amount()
            ),
        }
    }

    /// Create error for custom nameservers not available
    pub fn custom_nameservers_unavailable(current_tier: AccountTier) -> Self {
        Self {
            feature: "custom_nameservers".to_string(),
            current_tier,
            required_tier: AccountTier::Enterprise,
            required_staking: AccountTier::Enterprise.min_staking_amount(),
            message: format!(
                "Custom nameservers require Enterprise tier. Stake {} $AEGIS to enable.",
                AccountTier::Enterprise.min_staking_amount()
            ),
        }
    }
}

impl std::fmt::Display for TierRequirementError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for TierRequirementError {}

/// Get current Unix timestamp
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier_from_staking_amount() {
        assert_eq!(AccountTier::from_staking_amount(0), AccountTier::Free);
        assert_eq!(AccountTier::from_staking_amount(500), AccountTier::Free);
        assert_eq!(AccountTier::from_staking_amount(999), AccountTier::Free);
        assert_eq!(AccountTier::from_staking_amount(1000), AccountTier::Basic);
        assert_eq!(AccountTier::from_staking_amount(2499), AccountTier::Basic);
        assert_eq!(AccountTier::from_staking_amount(2500), AccountTier::Professional);
        assert_eq!(AccountTier::from_staking_amount(4999), AccountTier::Professional);
        assert_eq!(AccountTier::from_staking_amount(5000), AccountTier::Business);
        assert_eq!(AccountTier::from_staking_amount(9999), AccountTier::Business);
        assert_eq!(AccountTier::from_staking_amount(10000), AccountTier::Enterprise);
        assert_eq!(AccountTier::from_staking_amount(100000), AccountTier::Enterprise);
    }

    #[test]
    fn test_tier_max_zones() {
        assert_eq!(AccountTier::Free.max_zones(), 5);
        assert_eq!(AccountTier::Basic.max_zones(), usize::MAX);
        assert_eq!(AccountTier::Enterprise.max_zones(), usize::MAX);
    }

    #[test]
    fn test_tier_features() {
        // Free tier
        assert!(!AccountTier::Free.has_dnssec());
        assert!(!AccountTier::Free.has_advanced_analytics());
        assert!(!AccountTier::Free.has_priority_support());
        assert!(!AccountTier::Free.has_custom_nameservers());

        // Professional tier
        assert!(AccountTier::Professional.has_dnssec());
        assert!(!AccountTier::Professional.has_advanced_analytics());

        // Business tier
        assert!(AccountTier::Business.has_dnssec());
        assert!(AccountTier::Business.has_advanced_analytics());
        assert!(!AccountTier::Business.has_priority_support());

        // Enterprise tier
        assert!(AccountTier::Enterprise.has_dnssec());
        assert!(AccountTier::Enterprise.has_advanced_analytics());
        assert!(AccountTier::Enterprise.has_priority_support());
        assert!(AccountTier::Enterprise.has_custom_nameservers());
    }

    #[test]
    fn test_tier_rate_limits() {
        assert_eq!(AccountTier::Free.rate_limit_qps(), 1000);
        assert_eq!(AccountTier::Basic.rate_limit_qps(), 5000);
        assert_eq!(AccountTier::Professional.rate_limit_qps(), 10000);
        assert_eq!(AccountTier::Business.rate_limit_qps(), 50000);
        assert_eq!(AccountTier::Enterprise.rate_limit_qps(), 100000);
    }

    #[test]
    fn test_tier_analytics_retention() {
        assert_eq!(AccountTier::Free.analytics_retention_days(), 1);
        assert_eq!(AccountTier::Basic.analytics_retention_days(), 7);
        assert_eq!(AccountTier::Professional.analytics_retention_days(), 30);
        assert_eq!(AccountTier::Business.analytics_retention_days(), 90);
        assert_eq!(AccountTier::Enterprise.analytics_retention_days(), 365);
    }

    #[test]
    fn test_account_creation() {
        let account = DnsAccount::new("test_user");
        assert_eq!(account.account_id, "test_user");
        assert_eq!(account.tier, AccountTier::Free);
        assert_eq!(account.staked_amount, 0);
        assert_eq!(account.zone_count, 0);
        assert!(account.can_create_zone());
        assert!(!account.can_enable_dnssec());
    }

    #[test]
    fn test_account_update_staking() {
        let mut account = DnsAccount::new("test_user");

        // Upgrade to Professional
        account.update_staking(2500);
        assert_eq!(account.tier, AccountTier::Professional);
        assert!(account.can_enable_dnssec());
        assert!(!account.can_access_analytics());

        // Upgrade to Business
        account.update_staking(5000);
        assert_eq!(account.tier, AccountTier::Business);
        assert!(account.can_enable_dnssec());
        assert!(account.can_access_analytics());

        // Downgrade
        account.update_staking(0);
        assert_eq!(account.tier, AccountTier::Free);
        assert!(!account.can_enable_dnssec());
    }

    #[test]
    fn test_account_zone_limit() {
        let mut account = DnsAccount::new("test_user");

        // Free tier: 5 zones max
        for _ in 0..5 {
            assert!(account.can_create_zone());
            account.increment_zone_count();
        }
        assert!(!account.can_create_zone());

        // Upgrade to Basic - unlimited zones
        account.update_staking(1000);
        assert!(account.can_create_zone());
    }

    #[test]
    fn test_account_zone_count() {
        let mut account = DnsAccount::new("test_user");

        account.increment_zone_count();
        account.increment_zone_count();
        assert_eq!(account.zone_count, 2);

        account.decrement_zone_count();
        assert_eq!(account.zone_count, 1);

        account.decrement_zone_count();
        account.decrement_zone_count(); // Should not go below 0
        assert_eq!(account.zone_count, 0);
    }

    #[test]
    fn test_account_features() {
        let features = AccountFeatures::from_tier(AccountTier::Business);

        assert_eq!(features.max_zones, usize::MAX);
        assert!(features.dnssec_enabled);
        assert!(features.advanced_analytics);
        assert!(!features.priority_support);
        assert!(!features.custom_nameservers);
        assert_eq!(features.rate_limit_qps, 50000);
        assert_eq!(features.analytics_retention_days, 90);
    }

    #[tokio::test]
    async fn test_account_manager_get_or_create() {
        let manager = AccountManager::new();

        let account1 = manager.get_or_create("user1").await;
        assert_eq!(account1.account_id, "user1");
        assert_eq!(account1.tier, AccountTier::Free);

        // Should return existing account
        let account2 = manager.get_or_create("user1").await;
        assert_eq!(account2.account_id, "user1");
    }

    #[tokio::test]
    async fn test_account_manager_update_staking() {
        let manager = AccountManager::new();

        let account = manager.update_staking("user1", 5000).await;
        assert_eq!(account.tier, AccountTier::Business);

        let retrieved = manager.get("user1").await.unwrap();
        assert_eq!(retrieved.tier, AccountTier::Business);
    }

    #[tokio::test]
    async fn test_account_manager_zone_tracking() {
        let manager = AccountManager::new();

        manager.get_or_create("user1").await;

        manager.record_zone_created("user1").await;
        manager.record_zone_created("user1").await;

        let account = manager.get("user1").await.unwrap();
        assert_eq!(account.zone_count, 2);

        manager.record_zone_deleted("user1").await;
        let account = manager.get("user1").await.unwrap();
        assert_eq!(account.zone_count, 1);
    }

    #[tokio::test]
    async fn test_account_manager_can_create_zone() {
        let manager = AccountManager::new();

        // Free tier user
        let mut account = manager.get_or_create("user1").await;
        for _ in 0..5 {
            account.increment_zone_count();
        }
        manager.update(account).await;

        assert!(!manager.can_create_zone("user1").await);

        // Upgrade user
        manager.update_staking("user1", 1000).await;
        assert!(manager.can_create_zone("user1").await);
    }

    #[tokio::test]
    async fn test_account_manager_list_accounts() {
        let manager = AccountManager::new();

        manager.get_or_create("user1").await;
        manager.get_or_create("user2").await;
        manager.get_or_create("user3").await;

        let accounts = manager.list_accounts().await;
        assert_eq!(accounts.len(), 3);
    }

    #[tokio::test]
    async fn test_account_manager_delete() {
        let manager = AccountManager::new();

        manager.get_or_create("user1").await;
        assert!(manager.get("user1").await.is_some());

        assert!(manager.delete("user1").await);
        assert!(manager.get("user1").await.is_none());

        assert!(!manager.delete("nonexistent").await);
    }

    #[test]
    fn test_tier_requirement_error() {
        let error = TierRequirementError::zone_limit_exceeded(AccountTier::Free, 5);
        assert!(error.message.contains("Zone limit exceeded"));
        assert_eq!(error.required_tier, AccountTier::Basic);

        let error = TierRequirementError::dnssec_unavailable(AccountTier::Free);
        assert!(error.message.contains("DNSSEC"));
        assert_eq!(error.required_tier, AccountTier::Professional);

        let error = TierRequirementError::analytics_unavailable(AccountTier::Basic);
        assert!(error.message.contains("analytics"));
        assert_eq!(error.required_tier, AccountTier::Business);
    }

    #[test]
    fn test_tier_display() {
        assert_eq!(AccountTier::Free.to_string(), "Free");
        assert_eq!(AccountTier::Basic.to_string(), "Basic");
        assert_eq!(AccountTier::Professional.to_string(), "Professional");
        assert_eq!(AccountTier::Business.to_string(), "Business");
        assert_eq!(AccountTier::Enterprise.to_string(), "Enterprise");
    }

    #[test]
    fn test_tier_min_staking_amount() {
        assert_eq!(AccountTier::Free.min_staking_amount(), 0);
        assert_eq!(AccountTier::Basic.min_staking_amount(), 1000);
        assert_eq!(AccountTier::Professional.min_staking_amount(), 2500);
        assert_eq!(AccountTier::Business.min_staking_amount(), 5000);
        assert_eq!(AccountTier::Enterprise.min_staking_amount(), 10000);
    }
}
