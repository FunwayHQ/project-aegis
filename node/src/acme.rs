//! ACME/Let's Encrypt Certificate Automation
//!
//! This module provides automatic TLS certificate management using the ACME protocol
//! (RFC 8555), compatible with Let's Encrypt and other ACME-compliant CAs.
//!
//! ## Features
//! - HTTP-01 challenge handling for domain validation
//! - Automatic certificate renewal (30 days before expiration)
//! - Certificate/key storage in local files
//! - Support for staging (testing) and production environments
//!
//! ## Usage
//! ```ignore
//! let config = AcmeConfig {
//!     email: "admin@example.com".to_string(),
//!     domains: vec!["example.com".to_string(), "www.example.com".to_string()],
//!     staging: false,  // Use production Let's Encrypt
//!     storage_dir: PathBuf::from("/etc/aegis/certs"),
//! };
//!
//! let mut client = AcmeClient::new(config).await?;
//! let (cert_pem, key_pem) = client.obtain_certificate().await?;
//! ```
//!
//! ## Security Notes
//! - Private keys are stored with 0600 permissions (owner read/write only)
//! - Account keys are separate from certificate keys
//! - Supports ECDSA P-256 keys for modern security

use anyhow::{Context, Result};
use instant_acme::{
    Account, AccountCredentials, AuthorizationStatus, ChallengeType,
    Identifier, NewAccount, NewOrder, OrderStatus,
};
use log::{debug, info, warn};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::fs;
use tokio::sync::RwLock;

/// Let's Encrypt production ACME directory URL
pub const LETS_ENCRYPT_PRODUCTION: &str = "https://acme-v02.api.letsencrypt.org/directory";

/// Let's Encrypt staging ACME directory URL (for testing)
pub const LETS_ENCRYPT_STAGING: &str = "https://acme-staging-v02.api.letsencrypt.org/directory";

/// Days before expiration to renew certificates
const RENEWAL_DAYS_BEFORE_EXPIRY: i64 = 30;

/// Maximum retries for ACME operations
const MAX_RETRIES: usize = 10;

/// Delay between challenge status checks
const CHALLENGE_POLL_DELAY: Duration = Duration::from_secs(5);

/// ACME client configuration
#[derive(Debug, Clone)]
pub struct AcmeConfig {
    /// Contact email for the ACME account
    pub email: String,
    /// Domains to obtain certificates for (first is primary)
    pub domains: Vec<String>,
    /// Use Let's Encrypt staging environment (for testing)
    pub staging: bool,
    /// Directory to store certificates and account keys
    pub storage_dir: PathBuf,
    /// Terms of service agreement (required for account creation)
    pub agree_tos: bool,
}

impl Default for AcmeConfig {
    fn default() -> Self {
        Self {
            email: String::new(),
            domains: Vec::new(),
            staging: true, // Default to staging for safety
            storage_dir: PathBuf::from("/var/lib/aegis/acme"),
            agree_tos: false,
        }
    }
}

/// Pending HTTP-01 challenge token and response
#[derive(Debug, Clone)]
pub struct PendingChallenge {
    /// Challenge token (appears in URL path)
    pub token: String,
    /// Key authorization (response body)
    pub key_authorization: String,
    /// Domain being validated
    pub domain: String,
}

/// Certificate and private key pair
#[derive(Debug, Clone)]
pub struct CertificatePair {
    /// PEM-encoded certificate chain
    pub cert_pem: String,
    /// PEM-encoded private key
    pub key_pem: String,
    /// Primary domain name
    pub domain: String,
    /// Certificate expiration time (Unix timestamp)
    pub expires_at: i64,
}

/// ACME client for automatic certificate management
pub struct AcmeClient {
    config: AcmeConfig,
    /// ACME account (created or loaded)
    account: Option<Account>,
    /// Pending HTTP-01 challenges (token -> key_authorization)
    pending_challenges: Arc<RwLock<HashMap<String, PendingChallenge>>>,
}

impl AcmeClient {
    /// Create a new ACME client
    ///
    /// This will load or create an ACME account based on stored credentials.
    pub async fn new(config: AcmeConfig) -> Result<Self> {
        // Validate configuration
        if config.email.is_empty() {
            anyhow::bail!("ACME email is required");
        }
        if config.domains.is_empty() {
            anyhow::bail!("At least one domain is required");
        }
        if !config.agree_tos {
            anyhow::bail!("You must agree to the Let's Encrypt Terms of Service");
        }

        // Create storage directory
        fs::create_dir_all(&config.storage_dir)
            .await
            .context("Failed to create ACME storage directory")?;

        Ok(Self {
            config,
            account: None,
            pending_challenges: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Get or create ACME account
    async fn get_or_create_account(&mut self) -> Result<&Account> {
        if self.account.is_some() {
            return Ok(self.account.as_ref().unwrap());
        }

        let account_path = self.config.storage_dir.join("account.json");
        let directory_url = if self.config.staging {
            LETS_ENCRYPT_STAGING
        } else {
            LETS_ENCRYPT_PRODUCTION
        };

        // Try to load existing account
        if account_path.exists() {
            info!("Loading existing ACME account from {:?}", account_path);
            let account_json = fs::read_to_string(&account_path)
                .await
                .context("Failed to read account file")?;
            let credentials: AccountCredentials = serde_json::from_str(&account_json)
                .context("Failed to parse account credentials")?;

            let builder = Account::builder()
                .context("Failed to create account builder")?;
            let account = builder
                .from_credentials(credentials)
                .await
                .context("Failed to restore ACME account")?;

            self.account = Some(account);
            info!("ACME account loaded successfully");
        } else {
            // Create new account
            info!("Creating new ACME account for {}", self.config.email);
            let builder = Account::builder()
                .context("Failed to create account builder")?;
            let (account, credentials) = builder
                .create(
                    &NewAccount {
                        contact: &[&format!("mailto:{}", self.config.email)],
                        terms_of_service_agreed: true,
                        only_return_existing: false,
                    },
                    directory_url.to_string(),
                    None,
                )
                .await
                .context("Failed to create ACME account")?;

            // Save account credentials
            let account_json = serde_json::to_string_pretty(&credentials)
                .context("Failed to serialize account credentials")?;
            fs::write(&account_path, &account_json)
                .await
                .context("Failed to save account credentials")?;

            // Set file permissions (Unix only)
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&account_path).await?.permissions();
                perms.set_mode(0o600);
                fs::set_permissions(&account_path, perms).await?;
            }

            info!("ACME account created and saved to {:?}", account_path);
            self.account = Some(account);
        }

        Ok(self.account.as_ref().unwrap())
    }

    /// Get pending challenges for HTTP-01 validation
    ///
    /// Returns a clone of the pending challenges map for use by the
    /// challenge responder.
    pub async fn get_pending_challenges(&self) -> HashMap<String, PendingChallenge> {
        self.pending_challenges.read().await.clone()
    }

    /// Handle HTTP-01 challenge request
    ///
    /// Call this from your HTTP server when receiving a request to:
    /// `/.well-known/acme-challenge/{token}`
    ///
    /// Returns the key authorization if the token is valid, None otherwise.
    pub async fn handle_challenge(&self, token: &str) -> Option<String> {
        let challenges = self.pending_challenges.read().await;
        challenges.get(token).map(|c| c.key_authorization.clone())
    }

    /// Obtain or renew a certificate
    ///
    /// This method:
    /// 1. Creates a new order for the configured domains
    /// 2. Handles HTTP-01 challenges (caller must serve them)
    /// 3. Finalizes the order with a CSR
    /// 4. Returns the certificate and private key
    ///
    /// # Challenge Handling
    /// The caller MUST serve HTTP-01 challenges while this method runs.
    /// Use `handle_challenge()` to get the response for challenge URLs.
    pub async fn obtain_certificate(&mut self) -> Result<CertificatePair> {
        // Clone configuration values before borrowing self mutably
        let domains = self.config.domains.clone();
        let primary_domain = domains.first()
            .ok_or_else(|| anyhow::anyhow!("No domains configured"))?
            .clone();
        let storage_dir = self.config.storage_dir.clone();

        info!("Requesting certificate for domains: {:?}", domains);

        // Create identifiers for all domains
        let identifiers: Vec<Identifier> = domains
            .iter()
            .map(|d| Identifier::Dns(d.clone()))
            .collect();

        // Get or create the account
        self.get_or_create_account().await?;
        let account = self.account.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Account not initialized"))?;

        // Create a new order
        let new_order = NewOrder::new(&identifiers);
        let mut order = account
            .new_order(&new_order)
            .await
            .context("Failed to create ACME order")?;

        // Get authorizations and process challenges
        let mut authorizations = order.authorizations();

        while let Some(auth_result) = authorizations.next().await {
            let mut auth = auth_result.context("Failed to get authorization")?;

            // Get identifier and status first
            let ident = auth.identifier();
            let domain = ident.to_string();
            let status = auth.status;

            // AuthorizationHandle derefs to AuthorizationState
            match status {
                AuthorizationStatus::Valid => {
                    debug!("Authorization already valid for {}", domain);
                    continue;
                }
                AuthorizationStatus::Pending => {
                    // Get HTTP-01 challenge handle
                    let mut challenge_handle = auth
                        .challenge(ChallengeType::Http01)
                        .ok_or_else(|| anyhow::anyhow!("No HTTP-01 challenge found"))?;

                    info!("Setting up HTTP-01 challenge for {}", domain);

                    // Get key authorization
                    let key_auth = challenge_handle.key_authorization();

                    // Store the challenge for the HTTP server to serve
                    let pending = PendingChallenge {
                        token: challenge_handle.token.clone(),
                        key_authorization: key_auth.as_str().to_string(),
                        domain: domain.clone(),
                    };

                    {
                        let mut challenges = self.pending_challenges.write().await;
                        challenges.insert(challenge_handle.token.clone(), pending);
                    }

                    // Tell ACME server we're ready
                    challenge_handle
                        .set_ready()
                        .await
                        .context("Failed to set challenge ready")?;

                    info!("Challenge set ready for {}, waiting for validation...", domain);
                }
                _ => {
                    anyhow::bail!("Unexpected authorization status: {:?}", status);
                }
            }
        }

        // Wait for order to become ready
        let mut attempts = 0;
        loop {
            tokio::time::sleep(CHALLENGE_POLL_DELAY).await;

            let state = order.refresh().await.context("Failed to refresh order")?;

            match state.status {
                OrderStatus::Ready => {
                    info!("Order is ready for finalization");
                    break;
                }
                OrderStatus::Pending => {
                    attempts += 1;
                    if attempts >= MAX_RETRIES {
                        anyhow::bail!("Order stuck in pending state after {} attempts", attempts);
                    }
                    debug!("Order still pending, waiting... (attempt {})", attempts);
                }
                OrderStatus::Invalid => {
                    anyhow::bail!("Order became invalid - check challenge configuration");
                }
                OrderStatus::Valid => {
                    // Already finalized
                    break;
                }
                OrderStatus::Processing => {
                    debug!("Order is processing...");
                }
            }
        }

        // Clear pending challenges
        {
            let mut challenges = self.pending_challenges.write().await;
            challenges.clear();
        }

        // Finalize order - this generates the key and CSR automatically
        let key_pem = order
            .finalize()
            .await
            .context("Failed to finalize order")?;

        // Wait for certificate
        let cert_pem = loop {
            tokio::time::sleep(CHALLENGE_POLL_DELAY).await;

            match order.certificate().await {
                Ok(Some(cert)) => break cert,
                Ok(None) => {
                    debug!("Certificate not ready yet, waiting...");
                }
                Err(e) => {
                    warn!("Error fetching certificate: {}", e);
                }
            }
        };

        // Calculate expiration (Let's Encrypt certs are 90 days)
        let expires_at = chrono::Utc::now().timestamp() + (90 * 24 * 60 * 60);

        // Save certificate and key
        let cert_path = storage_dir.join(format!("{}.crt", primary_domain));
        let key_path = storage_dir.join(format!("{}.key", primary_domain));

        fs::write(&cert_path, &cert_pem)
            .await
            .context("Failed to save certificate")?;
        fs::write(&key_path, &key_pem)
            .await
            .context("Failed to save private key")?;

        // Set key file permissions (Unix only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&key_path).await?.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(&key_path, perms).await?;
        }

        info!(
            "Certificate obtained and saved for {}",
            primary_domain
        );

        Ok(CertificatePair {
            cert_pem,
            key_pem,
            domain: primary_domain.clone(),
            expires_at,
        })
    }

    /// Check if certificate needs renewal
    ///
    /// Returns true if:
    /// - No certificate exists
    /// - Certificate expires within RENEWAL_DAYS_BEFORE_EXPIRY days
    pub async fn needs_renewal(&self) -> Result<bool> {
        let primary_domain = &self.config.domains[0];
        let cert_path = self.config.storage_dir.join(format!("{}.crt", primary_domain));

        if !cert_path.exists() {
            info!("No certificate found, renewal needed");
            return Ok(true);
        }

        // Read certificate to verify it exists
        let _cert_pem = fs::read_to_string(&cert_path)
            .await
            .context("Failed to read certificate")?;

        // Parse PEM to get expiration
        // For simplicity, we use a file modification time approach
        // A more robust implementation would parse X.509 notAfter
        let metadata = fs::metadata(&cert_path).await?;
        let modified = metadata.modified()?;
        let age = modified.elapsed().unwrap_or(Duration::ZERO);

        // Let's Encrypt certs are 90 days, renew at 60 days old
        let renewal_threshold = Duration::from_secs((90 - RENEWAL_DAYS_BEFORE_EXPIRY) as u64 * 24 * 60 * 60);

        if age > renewal_threshold {
            info!(
                "Certificate is {} days old, renewal needed",
                age.as_secs() / (24 * 60 * 60)
            );
            return Ok(true);
        }

        debug!(
            "Certificate is {} days old, no renewal needed yet",
            age.as_secs() / (24 * 60 * 60)
        );
        Ok(false)
    }

    /// Load existing certificate from storage
    pub async fn load_certificate(&self) -> Result<Option<CertificatePair>> {
        let primary_domain = &self.config.domains[0];
        let cert_path = self.config.storage_dir.join(format!("{}.crt", primary_domain));
        let key_path = self.config.storage_dir.join(format!("{}.key", primary_domain));

        if !cert_path.exists() || !key_path.exists() {
            return Ok(None);
        }

        let cert_pem = fs::read_to_string(&cert_path)
            .await
            .context("Failed to read certificate")?;
        let key_pem = fs::read_to_string(&key_path)
            .await
            .context("Failed to read private key")?;

        // Estimate expiration from file modification time
        let metadata = fs::metadata(&cert_path).await?;
        let modified = metadata.modified()?;
        let cert_age = modified.elapsed().unwrap_or(Duration::ZERO);
        let expires_at = chrono::Utc::now().timestamp() +
            ((90 * 24 * 60 * 60) - cert_age.as_secs() as i64);

        Ok(Some(CertificatePair {
            cert_pem,
            key_pem,
            domain: primary_domain.clone(),
            expires_at,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acme_config_default() {
        let config = AcmeConfig::default();
        assert!(config.staging);
        assert!(config.domains.is_empty());
        assert!(!config.agree_tos);
    }

    #[test]
    fn test_lets_encrypt_urls() {
        assert!(LETS_ENCRYPT_PRODUCTION.contains("letsencrypt.org"));
        assert!(LETS_ENCRYPT_STAGING.contains("staging"));
    }

    #[tokio::test]
    async fn test_acme_client_validation() {
        // Missing email
        let config = AcmeConfig {
            email: "".to_string(),
            domains: vec!["example.com".to_string()],
            staging: true,
            storage_dir: PathBuf::from("/tmp/test"),
            agree_tos: true,
        };
        assert!(AcmeClient::new(config).await.is_err());

        // Missing domains
        let config = AcmeConfig {
            email: "test@example.com".to_string(),
            domains: vec![],
            staging: true,
            storage_dir: PathBuf::from("/tmp/test"),
            agree_tos: true,
        };
        assert!(AcmeClient::new(config).await.is_err());

        // Missing TOS agreement
        let config = AcmeConfig {
            email: "test@example.com".to_string(),
            domains: vec!["example.com".to_string()],
            staging: true,
            storage_dir: PathBuf::from("/tmp/test"),
            agree_tos: false,
        };
        assert!(AcmeClient::new(config).await.is_err());
    }

    #[tokio::test]
    async fn test_acme_client_creation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config = AcmeConfig {
            email: "test@example.com".to_string(),
            domains: vec!["example.com".to_string()],
            staging: true,
            storage_dir: temp_dir.path().to_path_buf(),
            agree_tos: true,
        };

        let client = AcmeClient::new(config).await;
        assert!(client.is_ok());
    }

    #[tokio::test]
    async fn test_pending_challenges() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config = AcmeConfig {
            email: "test@example.com".to_string(),
            domains: vec!["example.com".to_string()],
            staging: true,
            storage_dir: temp_dir.path().to_path_buf(),
            agree_tos: true,
        };

        let client = AcmeClient::new(config).await.unwrap();

        // No pending challenges initially
        let challenges = client.get_pending_challenges().await;
        assert!(challenges.is_empty());

        // Handle unknown token returns None
        let result = client.handle_challenge("unknown-token").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_load_nonexistent_certificate() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config = AcmeConfig {
            email: "test@example.com".to_string(),
            domains: vec!["example.com".to_string()],
            staging: true,
            storage_dir: temp_dir.path().to_path_buf(),
            agree_tos: true,
        };

        let client = AcmeClient::new(config).await.unwrap();
        let cert = client.load_certificate().await.unwrap();
        assert!(cert.is_none());
    }

    #[tokio::test]
    async fn test_needs_renewal_no_cert() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config = AcmeConfig {
            email: "test@example.com".to_string(),
            domains: vec!["example.com".to_string()],
            staging: true,
            storage_dir: temp_dir.path().to_path_buf(),
            agree_tos: true,
        };

        let client = AcmeClient::new(config).await.unwrap();
        assert!(client.needs_renewal().await.unwrap());
    }
}
