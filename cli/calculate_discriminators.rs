// Helper script to calculate Anchor instruction discriminators
// Run with: rustc calculate_discriminators.rs && ./calculate_discriminators

use sha2::{Sha256, Digest};

fn calculate_discriminator(instruction_name: &str) -> [u8; 8] {
    let preimage = format!("global:{}", instruction_name);
    let hash = Sha256::digest(preimage.as_bytes());
    let mut discriminator = [0u8; 8];
    discriminator.copy_from_slice(&hash[..8]);
    discriminator
}

fn main() {
    println!("Anchor Instruction Discriminators\n");

    // Registry instructions
    println!("=== Node Registry ===");
    let registry_instructions = vec![
        "register_node",
        "update_metadata",
        "deactivate_node",
        "reactivate_node",
        "update_stake",
    ];

    for name in registry_instructions {
        let disc = calculate_discriminator(name);
        println!("const {}_DISCRIMINATOR: [u8; 8] = {:?};",
                 name.to_uppercase(), disc);
    }

    println!("\n=== Staking ===");
    let staking_instructions = vec![
        "initialize_stake",
        "stake",
        "request_unstake",
        "execute_unstake",
        "cancel_unstake",
        "slash_stake",
    ];

    for name in staking_instructions {
        let disc = calculate_discriminator(name);
        println!("const {}_DISCRIMINATOR: [u8; 8] = {:?};",
                 name.to_uppercase(), disc);
    }

    println!("\n=== Rewards ===");
    let rewards_instructions = vec![
        "initialize_pool",
        "initialize_operator_rewards",
        "record_performance",
        "calculate_rewards",
        "claim_rewards",
    ];

    for name in rewards_instructions {
        let disc = calculate_discriminator(name);
        println!("const {}_DISCRIMINATOR: [u8; 8] = {:?};",
                 name.to_uppercase(), disc);
    }
}
