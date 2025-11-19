use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;
use serial_test::serial;
use std::env;
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Helper to create test command with isolated config
fn aegis_cmd() -> Command {
    let mut cmd = Command::cargo_bin("aegis-cli").unwrap();

    // Use unique temporary directory for each test
    let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let temp_dir = env::temp_dir().join(format!("aegis-test-{}-{}", std::process::id(), test_id));
    cmd.env("HOME", temp_dir.to_str().unwrap());
    cmd.env("USERPROFILE", temp_dir.to_str().unwrap());

    cmd
}

#[test]
fn test_cli_runs() {
    aegis_cmd()
        .arg("--version")
        .assert()
        .success();
}

#[test]
fn test_cli_shows_help() {
    aegis_cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("AEGIS"));
}

#[test]
fn test_config_show() {
    aegis_cmd()
        .arg("config")
        .arg("show")
        .assert()
        .success()
        .stdout(predicate::str::contains("Configuration"));
}

#[test]
fn test_config_set_cluster_devnet() {
    aegis_cmd()
        .arg("config")
        .arg("set-cluster")
        .arg("devnet")
        .assert()
        .success()
        .stdout(predicate::str::contains("devnet"));
}

#[test]
fn test_config_set_cluster_invalid() {
    aegis_cmd()
        .arg("config")
        .arg("set-cluster")
        .arg("invalid-cluster")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid cluster"));
}

#[test]
fn test_register_requires_metadata_url() {
    aegis_cmd()
        .arg("register")
        .assert()
        .failure();
}

#[test]
fn test_register_with_invalid_metadata() {
    aegis_cmd()
        .arg("register")
        .arg("--metadata-url")
        .arg("invalid")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid metadata"));
}

#[test]
fn test_register_with_valid_cid() {
    // Without wallet, should fail
    aegis_cmd()
        .arg("register")
        .arg("--metadata-url")
        .arg("QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Wallet not found"));
}

#[test]
fn test_stake_requires_amount() {
    aegis_cmd()
        .arg("stake")
        .assert()
        .failure();
}

#[test]
fn test_stake_with_amount() {
    // Stake command requires wallet
    aegis_cmd()
        .arg("stake")
        .arg("--amount")
        .arg("1000000000000")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Wallet not found"));
}

#[test]
fn test_unstake_command() {
    // Unstake command requires wallet
    aegis_cmd()
        .arg("unstake")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Wallet not found"));
}

#[test]
fn test_status_command() {
    // Status command requires wallet
    aegis_cmd()
        .arg("status")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Wallet not found"));
}

#[test]
fn test_balance_command() {
    // Balance command requires wallet
    aegis_cmd()
        .arg("balance")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Wallet not found"));
}

#[test]
fn test_claim_rewards_command() {
    // Claim rewards requires wallet
    aegis_cmd()
        .arg("claim-rewards")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Wallet not found"));
}

#[test]
fn test_wallet_create_command() {
    aegis_cmd()
        .arg("wallet")
        .arg("create")
        .assert()
        .success()
        .stdout(predicate::str::contains("wallet created"));
}

#[test]
fn test_wallet_import_requires_path() {
    aegis_cmd()
        .arg("wallet")
        .arg("import")
        .assert()
        .failure();
}

#[test]
fn test_wallet_import_nonexistent_file() {
    aegis_cmd()
        .arg("wallet")
        .arg("import")
        .arg("--keypair")
        .arg("/nonexistent/path.json")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn test_invalid_command() {
    aegis_cmd()
        .arg("nonexistent-command")
        .assert()
        .failure();
}

#[test]
fn test_register_with_stake() {
    // Register command requires wallet
    aegis_cmd()
        .arg("register")
        .arg("--metadata-url")
        .arg("QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG")
        .arg("--stake")
        .arg("100000000000")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Wallet not found"));
}

#[test]
fn test_register_with_insufficient_stake() {
    aegis_cmd()
        .arg("register")
        .arg("--metadata-url")
        .arg("QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG")
        .arg("--stake")
        .arg("1000") // Too low
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid"));
}
