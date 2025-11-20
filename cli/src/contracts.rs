use anchor_client::anchor_lang::prelude::*;
use anchor_client::anchor_lang::{AccountDeserialize, AnchorDeserialize, AnchorSerialize};
use anchor_client::{Client, Cluster};
use anyhow::Result;
use solana_client::rpc_client::RpcClient;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::signature::{Keypair, Signer};
use solana_sdk::system_program;
use std::rc::Rc;
use std::str::FromStr;

// Program IDs (deployed to devnet)
pub const TOKEN_PROGRAM_ID: &str = "JLQ4c9UWdNoYbsbAKU59SkYAw9HdVoz1Pxu7Juu4qyB";
pub const REGISTRY_PROGRAM_ID: &str = "D6kkpeujhPcoT9Er4HMaJh2FgG5fP6MEBAVogmF6ykr6";
pub const STAKING_PROGRAM_ID: &str = "5oGLkNZ7Hku3bRD4aWnRNo8PsXusXmojm8EzAiQUVD1H";
pub const REWARDS_PROGRAM_ID: &str = "3j4guuzvNESX5iMUFcfihRGsjEjKmjaEBD4p8GGxNs8c";

/// Get Solana client
pub fn get_client(cluster: Cluster, payer: Rc<Keypair>) -> Result<Client<Rc<Keypair>>> {
    let client = Client::new(cluster, payer);
    Ok(client)
}

/// Get RPC client for cluster
fn get_rpc_client(cluster: &Cluster) -> RpcClient {
    let url = match cluster {
        Cluster::Devnet => "https://api.devnet.solana.com",
        Cluster::Testnet => "https://api.testnet.solana.com",
        Cluster::Mainnet => "https://api.mainnet-beta.solana.com",
        Cluster::Localnet => "http://localhost:8899",
        Cluster::Debug => "http://localhost:8899",
        Cluster::Custom(url, _) => url.as_str(),
    };
    RpcClient::new_with_commitment(url.to_string(), CommitmentConfig::confirmed())
}

/// Get node status from registry
pub async fn get_node_status(
    operator: &Pubkey,
    cluster: Cluster,
) -> Result<NodeStatus> {
    let rpc_client = get_rpc_client(&cluster);
    let program_id = Pubkey::from_str(REGISTRY_PROGRAM_ID)?;
    let (node_account, _) = Pubkey::find_program_address(
        &[b"node", operator.as_ref()],
        &program_id,
    );

    // Fetch account data directly using RPC
    let account = rpc_client.get_account(&node_account)?;
    let mut data_slice = &account.data[8..]; // Skip 8-byte discriminator
    let account_data = NodeAccount::deserialize(&mut data_slice)?;

    Ok(NodeStatus {
        operator: account_data.operator,
        metadata_url: account_data.metadata_url,
        stake_amount: account_data.stake_amount,
        status: account_data.status,
        registered_at: account_data.registered_at,
    })
}

/// Get stake account info
pub async fn get_stake_info(
    operator: &Pubkey,
    cluster: Cluster,
) -> Result<StakeInfo> {
    let rpc_client = get_rpc_client(&cluster);
    let program_id = Pubkey::from_str(STAKING_PROGRAM_ID)?;
    let (stake_account, _) = Pubkey::find_program_address(
        &[b"stake", operator.as_ref()],
        &program_id,
    );

    // Fetch account data directly using RPC
    let account = rpc_client.get_account(&stake_account)?;
    let mut data_slice = &account.data[8..]; // Skip 8-byte discriminator
    let account_data = StakeAccount::deserialize(&mut data_slice)?;

    Ok(StakeInfo {
        staked_amount: account_data.staked_amount,
        pending_unstake: account_data.pending_unstake,
        unstake_request_time: account_data.unstake_request_time,
        total_staked_ever: account_data.total_staked_ever,
    })
}

/// Get rewards info
pub async fn get_rewards_info(
    operator: &Pubkey,
    cluster: Cluster,
) -> Result<RewardsInfo> {
    let rpc_client = get_rpc_client(&cluster);
    let program_id = Pubkey::from_str(REWARDS_PROGRAM_ID)?;
    let (operator_rewards, _) = Pubkey::find_program_address(
        &[b"operator_rewards", operator.as_ref()],
        &program_id,
    );

    // Fetch account data directly using RPC
    let account = rpc_client.get_account(&operator_rewards)?;
    let mut data_slice = &account.data[8..]; // Skip 8-byte discriminator
    let account_data = OperatorRewards::deserialize(&mut data_slice)?;

    Ok(RewardsInfo {
        total_earned: account_data.total_earned,
        total_claimed: account_data.total_claimed,
        unclaimed_rewards: account_data.unclaimed_rewards,
        last_claim_time: account_data.last_claim_time,
    })
}

// Response structs
#[derive(Debug)]
pub struct NodeStatus {
    pub operator: Pubkey,
    pub metadata_url: String,
    pub stake_amount: u64,
    pub status: NodeStatusEnum,
    pub registered_at: i64,
}

#[derive(Debug)]
pub struct StakeInfo {
    pub staked_amount: u64,
    pub pending_unstake: u64,
    pub unstake_request_time: i64,
    pub total_staked_ever: u64,
}

#[derive(Debug)]
pub struct RewardsInfo {
    pub total_earned: u64,
    pub total_claimed: u64,
    pub unclaimed_rewards: u64,
    pub last_claim_time: i64,
}

// Account type definitions (must match IDL)
#[derive(Debug, AnchorDeserialize, AnchorSerialize)]
pub struct NodeAccount {
    pub operator: Pubkey,
    pub metadata_url: String,
    pub status: NodeStatusEnum,
    pub stake_amount: u64,
    pub registered_at: i64,
    pub updated_at: i64,
    pub bump: u8,
}

#[derive(Debug, AnchorDeserialize, AnchorSerialize)]
pub enum NodeStatusEnum {
    Active,
    Inactive,
    Slashed,
}

#[derive(Debug, AnchorDeserialize, AnchorSerialize)]
pub struct StakeAccount {
    pub operator: Pubkey,
    pub staked_amount: u64,
    pub pending_unstake: u64,
    pub unstake_request_time: i64,
    pub total_staked_ever: u64,
    pub total_unstaked_ever: u64,
}

#[derive(Debug, AnchorDeserialize, AnchorSerialize)]
pub struct OperatorRewards {
    pub operator: Pubkey,
    pub total_earned: u64,
    pub total_claimed: u64,
    pub unclaimed_rewards: u64,
    pub last_claim_time: i64,
}
