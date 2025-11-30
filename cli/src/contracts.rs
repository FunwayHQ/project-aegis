use anchor_client::anchor_lang::prelude::*;
use anchor_client::anchor_lang::{AnchorDeserialize, AnchorSerialize};
use anchor_client::{Client, Cluster};
use anchor_spl::token::ID as SPL_TOKEN_PROGRAM_ID;
use anyhow::Result;
use solana_client::rpc_client::RpcClient;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::signature::{Keypair, Signer};
#[allow(deprecated)]
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

// Instruction discriminators (extracted from deployed contract IDLs)
const REGISTER_NODE_DISCRIMINATOR: [u8; 8] = [102, 85, 117, 114, 194, 188, 211, 168]; // from registry.json
const INITIALIZE_STAKE_DISCRIMINATOR: [u8; 8] = [33, 175, 216, 4, 116, 130, 164, 177]; // from staking.json
const STAKE_DISCRIMINATOR: [u8; 8] = [206, 176, 202, 18, 200, 209, 179, 108]; // from staking.json
const REQUEST_UNSTAKE_DISCRIMINATOR: [u8; 8] = [44, 154, 110, 253, 160, 202, 54, 34]; // from staking.json
const EXECUTE_UNSTAKE_DISCRIMINATOR: [u8; 8] = [136, 166, 210, 104, 134, 184, 142, 230]; // from staking.json
const CLAIM_REWARDS_DISCRIMINATOR: [u8; 8] = [149, 95, 181, 242, 94, 90, 158, 162]; // SHA256("global:claim_rewards")[0..8]

/// Register a new node on the network
pub async fn register_node(
    operator: &Keypair,
    metadata_url: String,
    min_stake: u64,
    cluster: Cluster,
) -> Result<String> {
    let rpc_client = get_rpc_client(&cluster);
    let program_id = Pubkey::from_str(REGISTRY_PROGRAM_ID)?;

    // Derive PDA for node account
    let (node_account, _bump) = Pubkey::find_program_address(
        &[b"node", operator.pubkey().as_ref()],
        &program_id,
    );

    // Build instruction data
    let mut instruction_data = Vec::new();
    instruction_data.extend_from_slice(&REGISTER_NODE_DISCRIMINATOR);
    instruction_data.extend_from_slice(&metadata_url.len().to_le_bytes()[..4]);
    instruction_data.extend_from_slice(metadata_url.as_bytes());
    instruction_data.extend_from_slice(&min_stake.to_le_bytes());

    // Build accounts
    let accounts = vec![
        AccountMeta::new(node_account, false),
        AccountMeta::new_readonly(operator.pubkey(), true),
        AccountMeta::new_readonly(system_program::ID, false),
    ];

    // Create instruction
    let instruction = Instruction {
        program_id,
        accounts,
        data: instruction_data,
    };

    // Send transaction
    let recent_blockhash = rpc_client.get_latest_blockhash()?;
    let transaction = solana_sdk::transaction::Transaction::new_signed_with_payer(
        &[instruction],
        Some(&operator.pubkey()),
        &[operator],
        recent_blockhash,
    );

    let signature = rpc_client.send_and_confirm_transaction(&transaction)?;
    Ok(signature.to_string())
}

/// Initialize stake account for an operator
pub async fn initialize_stake_account(
    operator: &Keypair,
    cluster: Cluster,
) -> Result<String> {
    let rpc_client = get_rpc_client(&cluster);
    let program_id = Pubkey::from_str(STAKING_PROGRAM_ID)?;

    // Derive PDA for stake account
    let (stake_account, _bump) = Pubkey::find_program_address(
        &[b"stake", operator.pubkey().as_ref()],
        &program_id,
    );

    // Build instruction data
    let mut instruction_data = Vec::new();
    instruction_data.extend_from_slice(&INITIALIZE_STAKE_DISCRIMINATOR);

    // Build accounts
    let accounts = vec![
        AccountMeta::new(stake_account, false),
        AccountMeta::new_readonly(operator.pubkey(), true),
        AccountMeta::new_readonly(system_program::ID, false),
    ];

    // Create instruction
    let instruction = Instruction {
        program_id,
        accounts,
        data: instruction_data,
    };

    // Send transaction
    let recent_blockhash = rpc_client.get_latest_blockhash()?;
    let transaction = solana_sdk::transaction::Transaction::new_signed_with_payer(
        &[instruction],
        Some(&operator.pubkey()),
        &[operator],
        recent_blockhash,
    );

    let signature = rpc_client.send_and_confirm_transaction(&transaction)?;
    Ok(signature.to_string())
}

/// Stake AEGIS tokens
pub async fn stake_tokens(
    operator: &Keypair,
    amount: u64,
    cluster: Cluster,
) -> Result<String> {
    let rpc_client = get_rpc_client(&cluster);
    let program_id = Pubkey::from_str(STAKING_PROGRAM_ID)?;
    let token_program_id = Pubkey::from_str(TOKEN_PROGRAM_ID)?;

    // Derive PDA for stake account
    let (stake_account, _bump) = Pubkey::find_program_address(
        &[b"stake", operator.pubkey().as_ref()],
        &program_id,
    );

    // Derive stake vault PDA
    let (stake_vault, _vault_bump) = Pubkey::find_program_address(
        &[b"stake_vault"],
        &program_id,
    );

    // Get operator's token account (associated token account)
    let operator_token_account = spl_associated_token_account::get_associated_token_address(
        &operator.pubkey(),
        &token_program_id,
    );

    // Build instruction data
    let mut instruction_data = Vec::new();
    instruction_data.extend_from_slice(&STAKE_DISCRIMINATOR);
    instruction_data.extend_from_slice(&amount.to_le_bytes());

    // Build accounts
    let accounts = vec![
        AccountMeta::new(stake_account, false),
        AccountMeta::new(operator_token_account, false),
        AccountMeta::new(stake_vault, false),
        AccountMeta::new_readonly(operator.pubkey(), true),
        AccountMeta::new_readonly(SPL_TOKEN_PROGRAM_ID, false),
    ];

    // Create instruction
    let instruction = Instruction {
        program_id,
        accounts,
        data: instruction_data,
    };

    // Send transaction
    let recent_blockhash = rpc_client.get_latest_blockhash()?;
    let transaction = solana_sdk::transaction::Transaction::new_signed_with_payer(
        &[instruction],
        Some(&operator.pubkey()),
        &[operator],
        recent_blockhash,
    );

    let signature = rpc_client.send_and_confirm_transaction(&transaction)?;
    Ok(signature.to_string())
}

/// Request unstake
pub async fn request_unstake(
    operator: &Keypair,
    amount: u64,
    cluster: Cluster,
) -> Result<String> {
    let rpc_client = get_rpc_client(&cluster);
    let program_id = Pubkey::from_str(STAKING_PROGRAM_ID)?;

    // Derive PDA for stake account
    let (stake_account, _bump) = Pubkey::find_program_address(
        &[b"stake", operator.pubkey().as_ref()],
        &program_id,
    );

    // Build instruction data
    let mut instruction_data = Vec::new();
    instruction_data.extend_from_slice(&REQUEST_UNSTAKE_DISCRIMINATOR);
    instruction_data.extend_from_slice(&amount.to_le_bytes());

    // Build accounts
    let accounts = vec![
        AccountMeta::new(stake_account, false),
        AccountMeta::new_readonly(operator.pubkey(), true),
        AccountMeta::new_readonly(solana_sdk::sysvar::clock::ID, false),
    ];

    // Create instruction
    let instruction = Instruction {
        program_id,
        accounts,
        data: instruction_data,
    };

    // Send transaction
    let recent_blockhash = rpc_client.get_latest_blockhash()?;
    let transaction = solana_sdk::transaction::Transaction::new_signed_with_payer(
        &[instruction],
        Some(&operator.pubkey()),
        &[operator],
        recent_blockhash,
    );

    let signature = rpc_client.send_and_confirm_transaction(&transaction)?;
    Ok(signature.to_string())
}

/// Execute unstake after cooldown period
pub async fn execute_unstake(
    operator: &Keypair,
    cluster: Cluster,
) -> Result<String> {
    let rpc_client = get_rpc_client(&cluster);
    let program_id = Pubkey::from_str(STAKING_PROGRAM_ID)?;
    let token_program_id = Pubkey::from_str(TOKEN_PROGRAM_ID)?;

    // Derive PDA for stake account
    let (stake_account, _bump) = Pubkey::find_program_address(
        &[b"stake", operator.pubkey().as_ref()],
        &program_id,
    );

    // Derive stake vault PDA
    let (stake_vault, _vault_bump) = Pubkey::find_program_address(
        &[b"stake_vault"],
        &program_id,
    );

    // Get operator's token account
    let operator_token_account = spl_associated_token_account::get_associated_token_address(
        &operator.pubkey(),
        &token_program_id,
    );

    // Build instruction data
    let mut instruction_data = Vec::new();
    instruction_data.extend_from_slice(&EXECUTE_UNSTAKE_DISCRIMINATOR);

    // Build accounts
    let accounts = vec![
        AccountMeta::new(stake_account, false),
        AccountMeta::new(stake_vault, false),
        AccountMeta::new(operator_token_account, false),
        AccountMeta::new_readonly(operator.pubkey(), true),
        AccountMeta::new_readonly(SPL_TOKEN_PROGRAM_ID, false),
    ];

    // Create instruction
    let instruction = Instruction {
        program_id,
        accounts,
        data: instruction_data,
    };

    // Send transaction
    let recent_blockhash = rpc_client.get_latest_blockhash()?;
    let transaction = solana_sdk::transaction::Transaction::new_signed_with_payer(
        &[instruction],
        Some(&operator.pubkey()),
        &[operator],
        recent_blockhash,
    );

    let signature = rpc_client.send_and_confirm_transaction(&transaction)?;
    Ok(signature.to_string())
}

/// Get AEGIS token balance for a wallet
pub async fn get_token_balance(
    owner: &Pubkey,
    cluster: Cluster,
) -> Result<f64> {
    let rpc_client = get_rpc_client(&cluster);
    let token_mint = Pubkey::from_str(TOKEN_PROGRAM_ID)?;

    // Get associated token account
    let token_account = spl_associated_token_account::get_associated_token_address(
        owner,
        &token_mint,
    );

    // Try to get token account balance
    match rpc_client.get_token_account_balance(&token_account) {
        Ok(balance) => Ok(balance.ui_amount.unwrap_or(0.0)),
        Err(_) => Ok(0.0), // Account doesn't exist yet
    }
}

/// Get SOL balance for a wallet
pub async fn get_sol_balance(
    owner: &Pubkey,
    cluster: Cluster,
) -> Result<f64> {
    let rpc_client = get_rpc_client(&cluster);

    match rpc_client.get_balance(owner) {
        Ok(lamports) => Ok(lamports as f64 / 1_000_000_000.0), // Convert lamports to SOL
        Err(e) => Err(e.into()),
    }
}

/// Claim accumulated rewards
pub async fn claim_rewards(
    operator: &Keypair,
    cluster: Cluster,
) -> Result<String> {
    let rpc_client = get_rpc_client(&cluster);
    let program_id = Pubkey::from_str(REWARDS_PROGRAM_ID)?;
    let token_program_id = Pubkey::from_str(TOKEN_PROGRAM_ID)?;

    // Derive reward pool PDA
    let (reward_pool, _pool_bump) = Pubkey::find_program_address(
        &[b"reward_pool"],
        &program_id,
    );

    // Derive operator rewards PDA
    let (operator_rewards, _rewards_bump) = Pubkey::find_program_address(
        &[b"operator_rewards", operator.pubkey().as_ref()],
        &program_id,
    );

    // Derive reward vault PDA (token account)
    let (reward_vault, _vault_bump) = Pubkey::find_program_address(
        &[b"reward_vault"],
        &program_id,
    );

    // Get operator's token account
    let operator_token_account = spl_associated_token_account::get_associated_token_address(
        &operator.pubkey(),
        &token_program_id,
    );

    // Build instruction data
    let mut instruction_data = Vec::new();
    instruction_data.extend_from_slice(&CLAIM_REWARDS_DISCRIMINATOR);

    // Build accounts (order must match ClaimRewards struct)
    let accounts = vec![
        AccountMeta::new(reward_pool, false),
        AccountMeta::new(operator_rewards, false),
        AccountMeta::new(reward_vault, false),
        AccountMeta::new(operator_token_account, false),
        AccountMeta::new_readonly(operator.pubkey(), true),
        AccountMeta::new_readonly(SPL_TOKEN_PROGRAM_ID, false),
    ];

    // Create instruction
    let instruction = Instruction {
        program_id,
        accounts,
        data: instruction_data,
    };

    // Send transaction
    let recent_blockhash = rpc_client.get_latest_blockhash()?;
    let transaction = solana_sdk::transaction::Transaction::new_signed_with_payer(
        &[instruction],
        Some(&operator.pubkey()),
        &[operator],
        recent_blockhash,
    );

    let signature = rpc_client.send_and_confirm_transaction(&transaction)?;
    Ok(signature.to_string())
}
