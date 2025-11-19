use anchor_client::{Client, Cluster, Program};
use anyhow::{anyhow, Result};
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_program,
};
use std::rc::Rc;
use std::str::FromStr;

// Program IDs (update these after deployment)
pub const REGISTRY_PROGRAM_ID: &str = "GLpPpGCANeD7mLuY7XdJ2mAXX7MSLEdaLr91MMjoscno";
pub const STAKING_PROGRAM_ID: &str = "Ba5sohaR6jH1t8ukfxbW3XEcpZJaoQ446F8HmeVTjXie";
pub const REWARDS_PROGRAM_ID: &str = "3j4guuzvNESX5iMUFcfihRGsjEjKmjaEBD4p8GGxNs8c";

/// Get Solana client
pub fn get_client(cluster: Cluster, payer: Rc<Keypair>) -> Result<Client> {
    let client = Client::new(cluster, payer);
    Ok(client)
}

/// Register node with the registry contract
pub async fn register_node(
    keypair: &Keypair,
    cluster: Cluster,
    metadata_url: String,
    initial_stake: u64,
) -> Result<String> {
    let payer = Rc::new(Keypair::from_bytes(&keypair.to_bytes())?);
    let client = get_client(cluster, payer)?;

    let program_id = Pubkey::from_str(REGISTRY_PROGRAM_ID)?;
    let program = client.program(program_id)?;

    // Derive node account PDA
    let (node_account, _bump) = Pubkey::find_program_address(
        &[b"node", keypair.pubkey().as_ref()],
        &program_id,
    );

    // Call register_node instruction
    let sig = program
        .request()
        .accounts(node_registry::accounts::RegisterNode {
            node_account,
            operator: keypair.pubkey(),
            system_program: system_program::ID,
        })
        .args(node_registry::instruction::RegisterNode {
            metadata_url,
            stake_amount: initial_stake,
        })
        .signer(keypair)
        .send()?;

    Ok(sig.to_string())
}

/// Initialize stake account
pub async fn initialize_stake_account(
    keypair: &Keypair,
    cluster: Cluster,
) -> Result<String> {
    let payer = Rc::new(Keypair::from_bytes(&keypair.to_bytes())?);
    let client = get_client(cluster, payer)?;

    let program_id = Pubkey::from_str(STAKING_PROGRAM_ID)?;
    let program = client.program(program_id)?;

    let (stake_account, _bump) = Pubkey::find_program_address(
        &[b"stake", keypair.pubkey().as_ref()],
        &program_id,
    );

    let sig = program
        .request()
        .accounts(staking::accounts::InitializeStake {
            stake_account,
            operator: keypair.pubkey(),
            system_program: system_program::ID,
        })
        .args(staking::instruction::InitializeStake {})
        .signer(keypair)
        .send()?;

    Ok(sig.to_string())
}

/// Stake tokens
pub async fn stake_tokens(
    keypair: &Keypair,
    cluster: Cluster,
    amount: u64,
    operator_token_account: Pubkey,
    stake_vault: Pubkey,
) -> Result<String> {
    let payer = Rc::new(Keypair::from_bytes(&keypair.to_bytes())?);
    let client = get_client(cluster, payer)?;

    let program_id = Pubkey::from_str(STAKING_PROGRAM_ID)?;
    let program = client.program(program_id)?;

    let (stake_account, _bump) = Pubkey::find_program_address(
        &[b"stake", keypair.pubkey().as_ref()],
        &program_id,
    );

    let sig = program
        .request()
        .accounts(staking::accounts::Stake {
            stake_account,
            operator_token_account,
            stake_vault,
            operator: keypair.pubkey(),
            token_program: anchor_spl::token::ID,
        })
        .args(staking::instruction::Stake { amount })
        .signer(keypair)
        .send()?;

    Ok(sig.to_string())
}

/// Request unstake
pub async fn request_unstake(
    keypair: &Keypair,
    cluster: Cluster,
    amount: u64,
) -> Result<String> {
    let payer = Rc::new(Keypair::from_bytes(&keypair.to_bytes())?);
    let client = get_client(cluster, payer)?;

    let program_id = Pubkey::from_str(STAKING_PROGRAM_ID)?;
    let program = client.program(program_id)?;

    let (stake_account, _bump) = Pubkey::find_program_address(
        &[b"stake", keypair.pubkey().as_ref()],
        &program_id,
    );

    let sig = program
        .request()
        .accounts(staking::accounts::RequestUnstake {
            stake_account,
            operator: keypair.pubkey(),
        })
        .args(staking::instruction::RequestUnstake { amount })
        .signer(keypair)
        .send()?;

    Ok(sig.to_string())
}

/// Claim rewards
pub async fn claim_rewards(
    keypair: &Keypair,
    cluster: Cluster,
    operator_token_account: Pubkey,
) -> Result<String> {
    let payer = Rc::new(Keypair::from_bytes(&keypair.to_bytes())?);
    let client = get_client(cluster, payer)?;

    let program_id = Pubkey::from_str(REWARDS_PROGRAM_ID)?;
    let program = client.program(program_id)?;

    let (reward_pool, _) = Pubkey::find_program_address(
        &[b"reward_pool"],
        &program_id,
    );

    let (operator_rewards, _) = Pubkey::find_program_address(
        &[b"operator_rewards", keypair.pubkey().as_ref()],
        &program_id,
    );

    // Get reward pool account to find reward vault
    let pool_account: rewards::RewardPool = program.account(reward_pool)?;
    let reward_vault = pool_account.reward_vault;

    let sig = program
        .request()
        .accounts(rewards::accounts::ClaimRewards {
            reward_pool,
            operator_rewards,
            reward_vault,
            operator_token_account,
            operator: keypair.pubkey(),
            token_program: anchor_spl::token::ID,
        })
        .args(rewards::instruction::ClaimRewards {})
        .signer(keypair)
        .send()?;

    Ok(sig.to_string())
}

/// Get node status from registry
pub async fn get_node_status(
    operator: &Pubkey,
    cluster: Cluster,
) -> Result<NodeStatus> {
    let payer = Rc::new(Keypair::new());
    let client = get_client(cluster, payer)?;

    let program_id = Pubkey::from_str(REGISTRY_PROGRAM_ID)?;
    let program = client.program(program_id)?;

    let (node_account, _) = Pubkey::find_program_address(
        &[b"node", operator.as_ref()],
        &program_id,
    );

    let account: node_registry::NodeAccount = program.account(node_account)?;

    Ok(NodeStatus {
        operator: account.operator,
        metadata_url: account.metadata_url,
        stake_amount: account.stake_amount,
        is_active: account.is_active,
        registered_at: account.registered_at,
    })
}

/// Get stake account info
pub async fn get_stake_info(
    operator: &Pubkey,
    cluster: Cluster,
) -> Result<StakeInfo> {
    let payer = Rc::new(Keypair::new());
    let client = get_client(cluster, payer)?;

    let program_id = Pubkey::from_str(STAKING_PROGRAM_ID)?;
    let program = client.program(program_id)?;

    let (stake_account, _) = Pubkey::find_program_address(
        &[b"stake", operator.as_ref()],
        &program_id,
    );

    let account: staking::StakeAccount = program.account(stake_account)?;

    Ok(StakeInfo {
        staked_amount: account.staked_amount,
        pending_unstake: account.pending_unstake,
        unstake_request_time: account.unstake_request_time,
        total_staked_ever: account.total_staked_ever,
    })
}

/// Get rewards info
pub async fn get_rewards_info(
    operator: &Pubkey,
    cluster: Cluster,
) -> Result<RewardsInfo> {
    let payer = Rc::new(Keypair::new());
    let client = get_client(cluster, payer)?;

    let program_id = Pubkey::from_str(REWARDS_PROGRAM_ID)?;
    let program = client.program(program_id)?;

    let (operator_rewards, _) = Pubkey::find_program_address(
        &[b"operator_rewards", operator.as_ref()],
        &program_id,
    );

    let account: rewards::OperatorRewards = program.account(operator_rewards)?;

    Ok(RewardsInfo {
        total_earned: account.total_earned,
        total_claimed: account.total_claimed,
        unclaimed_rewards: account.unclaimed_rewards,
        last_claim_time: account.last_claim_time,
    })
}

// Response structs
#[derive(Debug)]
pub struct NodeStatus {
    pub operator: Pubkey,
    pub metadata_url: String,
    pub stake_amount: u64,
    pub is_active: bool,
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

// Mock modules for compilation (these would normally come from generated IDL code)
mod node_registry {
    use super::*;

    pub mod accounts {
        use super::*;
        pub struct RegisterNode {
            pub node_account: Pubkey,
            pub operator: Pubkey,
            pub system_program: Pubkey,
        }
    }

    pub mod instruction {
        pub struct RegisterNode {
            pub metadata_url: String,
            pub stake_amount: u64,
        }
    }

    #[derive(Debug)]
    pub struct NodeAccount {
        pub operator: Pubkey,
        pub metadata_url: String,
        pub stake_amount: u64,
        pub is_active: bool,
        pub registered_at: i64,
    }
}

mod staking {
    use super::*;

    pub mod accounts {
        use super::*;
        pub struct InitializeStake {
            pub stake_account: Pubkey,
            pub operator: Pubkey,
            pub system_program: Pubkey,
        }

        pub struct Stake {
            pub stake_account: Pubkey,
            pub operator_token_account: Pubkey,
            pub stake_vault: Pubkey,
            pub operator: Pubkey,
            pub token_program: Pubkey,
        }

        pub struct RequestUnstake {
            pub stake_account: Pubkey,
            pub operator: Pubkey,
        }
    }

    pub mod instruction {
        pub struct InitializeStake {}

        pub struct Stake {
            pub amount: u64,
        }

        pub struct RequestUnstake {
            pub amount: u64,
        }
    }

    #[derive(Debug)]
    pub struct StakeAccount {
        pub operator: Pubkey,
        pub staked_amount: u64,
        pub pending_unstake: u64,
        pub unstake_request_time: i64,
        pub total_staked_ever: u64,
        pub total_unstaked_ever: u64,
    }
}

mod rewards {
    use super::*;

    pub mod accounts {
        use super::*;
        pub struct ClaimRewards {
            pub reward_pool: Pubkey,
            pub operator_rewards: Pubkey,
            pub reward_vault: Pubkey,
            pub operator_token_account: Pubkey,
            pub operator: Pubkey,
            pub token_program: Pubkey,
        }
    }

    pub mod instruction {
        pub struct ClaimRewards {}
    }

    #[derive(Debug)]
    pub struct RewardPool {
        pub authority: Pubkey,
        pub reward_vault: Pubkey,
        pub total_distributed: u64,
        pub reward_rate_per_epoch: u64,
    }

    #[derive(Debug)]
    pub struct OperatorRewards {
        pub operator: Pubkey,
        pub total_earned: u64,
        pub total_claimed: u64,
        pub unclaimed_rewards: u64,
        pub last_claim_time: i64,
    }
}
