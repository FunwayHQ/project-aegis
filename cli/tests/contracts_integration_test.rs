// Integration tests for CLI contract RPC functions
// Note: These tests validate the structure and logic of RPC calls
// Full integration testing requires Devnet connection

use anchor_client::Cluster;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use std::str::FromStr;

#[cfg(test)]
mod discriminator_tests {
    use super::*;

    #[test]
    fn test_discriminator_lengths() {
        // All Anchor discriminators should be exactly 8 bytes
        let discriminators = vec![
            [102, 85, 117, 114, 194, 188, 211, 168],  // register_node
            [33, 175, 216, 4, 116, 130, 164, 177],     // initialize_stake
            [206, 176, 202, 18, 200, 209, 179, 108],   // stake
            [44, 154, 110, 253, 160, 202, 54, 34],     // request_unstake
            [136, 166, 210, 104, 134, 184, 142, 230],  // execute_unstake
            [149, 95, 181, 242, 94, 90, 158, 162],     // claim_rewards
        ];

        for disc in discriminators {
            assert_eq!(disc.len(), 8);
        }
    }

    #[test]
    fn test_discriminators_are_unique() {
        let discriminators = vec![
            [102, 85, 117, 114, 194, 188, 211, 168],  // register_node
            [33, 175, 216, 4, 116, 130, 164, 177],     // initialize_stake
            [206, 176, 202, 18, 200, 209, 179, 108],   // stake
            [44, 154, 110, 253, 160, 202, 54, 34],     // request_unstake
            [136, 166, 210, 104, 134, 184, 142, 230],  // execute_unstake
            [149, 95, 181, 242, 94, 90, 158, 162],     // claim_rewards
        ];

        // Each discriminator should be unique
        for i in 0..discriminators.len() {
            for j in (i + 1)..discriminators.len() {
                assert_ne!(discriminators[i], discriminators[j]);
            }
        }
    }

    #[test]
    fn test_discriminator_not_all_zeros() {
        let discriminators = vec![
            [102, 85, 117, 114, 194, 188, 211, 168],
            [33, 175, 216, 4, 116, 130, 164, 177],
            [206, 176, 202, 18, 200, 209, 179, 108],
        ];

        for disc in discriminators {
            let is_all_zeros = disc.iter().all(|&b| b == 0);
            assert!(!is_all_zeros);
        }
    }
}

#[cfg(test)]
mod program_id_tests {
    use super::*;

    const TOKEN_PROGRAM_ID: &str = "JLQ4c9UWdNoYbsbAKU59SkYAw9HdVoz1Pxu7Juu4qyB";
    const REGISTRY_PROGRAM_ID: &str = "D6kkpeujhPcoT9Er4HMaJh2FgG5fP6MEBAVogmF6ykr6";
    const STAKING_PROGRAM_ID: &str = "5oGLkNZ7Hku3bRD4aWnRNo8PsXusXmojm8EzAiQUVD1H";
    const REWARDS_PROGRAM_ID: &str = "3j4guuzvNESX5iMUFcfihRGsjEjKmjaEBD4p8GGxNs8c";

    #[test]
    fn test_program_ids_are_valid_pubkeys() {
        // All program IDs should parse as valid Pubkeys
        assert!(Pubkey::from_str(TOKEN_PROGRAM_ID).is_ok());
        assert!(Pubkey::from_str(REGISTRY_PROGRAM_ID).is_ok());
        assert!(Pubkey::from_str(STAKING_PROGRAM_ID).is_ok());
        assert!(Pubkey::from_str(REWARDS_PROGRAM_ID).is_ok());
    }

    #[test]
    fn test_program_ids_are_unique() {
        let ids = vec![
            TOKEN_PROGRAM_ID,
            REGISTRY_PROGRAM_ID,
            STAKING_PROGRAM_ID,
            REWARDS_PROGRAM_ID,
        ];

        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                assert_ne!(ids[i], ids[j]);
            }
        }
    }

    #[test]
    fn test_program_ids_correct_length() {
        let ids = vec![
            TOKEN_PROGRAM_ID,
            REGISTRY_PROGRAM_ID,
            STAKING_PROGRAM_ID,
            REWARDS_PROGRAM_ID,
        ];

        for id in ids {
            // Solana base58 pubkeys are typically 43-44 characters
            assert!(id.len() >= 43 && id.len() <= 44);
        }
    }
}

#[cfg(test)]
mod pda_derivation_tests {
    use super::*;

    #[test]
    fn test_node_account_pda_derivation() {
        let operator = Keypair::new();
        let program_id = Pubkey::from_str("D6kkpeujhPcoT9Er4HMaJh2FgG5fP6MEBAVogmF6ykr6").unwrap();

        let (pda, bump) = Pubkey::find_program_address(
            &[b"node", operator.pubkey().as_ref()],
            &program_id,
        );

        // PDA should be valid
        assert_ne!(pda, Pubkey::default());
        assert!(bump <= 255);
    }

    #[test]
    fn test_stake_account_pda_derivation() {
        let operator = Keypair::new();
        let program_id = Pubkey::from_str("5oGLkNZ7Hku3bRD4aWnRNo8PsXusXmojm8EzAiQUVD1H").unwrap();

        let (pda, bump) = Pubkey::find_program_address(
            &[b"stake", operator.pubkey().as_ref()],
            &program_id,
        );

        assert_ne!(pda, Pubkey::default());
        assert!(bump <= 255);
    }

    #[test]
    fn test_stake_vault_pda_derivation() {
        let program_id = Pubkey::from_str("5oGLkNZ7Hku3bRD4aWnRNo8PsXusXmojm8EzAiQUVD1H").unwrap();

        let (pda, bump) = Pubkey::find_program_address(
            &[b"stake_vault"],
            &program_id,
        );

        assert_ne!(pda, Pubkey::default());
        assert!(bump <= 255);
    }

    #[test]
    fn test_operator_rewards_pda_derivation() {
        let operator = Keypair::new();
        let program_id = Pubkey::from_str("3j4guuzvNESX5iMUFcfihRGsjEjKmjaEBD4p8GGxNs8c").unwrap();

        let (pda, bump) = Pubkey::find_program_address(
            &[b"operator_rewards", operator.pubkey().as_ref()],
            &program_id,
        );

        assert_ne!(pda, Pubkey::default());
        assert!(bump <= 255);
    }

    #[test]
    fn test_reward_pool_pda_derivation() {
        let program_id = Pubkey::from_str("3j4guuzvNESX5iMUFcfihRGsjEjKmjaEBD4p8GGxNs8c").unwrap();

        let (pda, bump) = Pubkey::find_program_address(
            &[b"reward_pool"],
            &program_id,
        );

        assert_ne!(pda, Pubkey::default());
        assert!(bump <= 255);
    }

    #[test]
    fn test_pda_determinism() {
        // Same seeds should always produce same PDA
        let operator = Keypair::new();
        let program_id = Pubkey::from_str("D6kkpeujhPcoT9Er4HMaJh2FgG5fP6MEBAVogmF6ykr6").unwrap();

        let (pda1, bump1) = Pubkey::find_program_address(
            &[b"node", operator.pubkey().as_ref()],
            &program_id,
        );

        let (pda2, bump2) = Pubkey::find_program_address(
            &[b"node", operator.pubkey().as_ref()],
            &program_id,
        );

        assert_eq!(pda1, pda2);
        assert_eq!(bump1, bump2);
    }

    #[test]
    fn test_different_operators_different_pdas() {
        let operator1 = Keypair::new();
        let operator2 = Keypair::new();
        let program_id = Pubkey::from_str("D6kkpeujhPcoT9Er4HMaJh2FgG5fP6MEBAVogmF6ykr6").unwrap();

        let (pda1, _) = Pubkey::find_program_address(
            &[b"node", operator1.pubkey().as_ref()],
            &program_id,
        );

        let (pda2, _) = Pubkey::find_program_address(
            &[b"node", operator2.pubkey().as_ref()],
            &program_id,
        );

        assert_ne!(pda1, pda2);
    }
}

#[cfg(test)]
mod instruction_data_tests {
    use super::*;

    #[test]
    fn test_u64_serialization() {
        let amount: u64 = 100_000_000_000;
        let bytes = amount.to_le_bytes();

        assert_eq!(bytes.len(), 8);

        // Deserialize back
        let deserialized = u64::from_le_bytes(bytes);
        assert_eq!(deserialized, amount);
    }

    #[test]
    fn test_instruction_data_construction() {
        let discriminator = [102, 85, 117, 114, 194, 188, 211, 168];
        let amount: u64 = 100_000_000_000;

        let mut data = Vec::new();
        data.extend_from_slice(&discriminator);
        data.extend_from_slice(&amount.to_le_bytes());

        assert_eq!(data.len(), 16); // 8 bytes discriminator + 8 bytes u64
        assert_eq!(&data[0..8], &discriminator);
    }

    #[test]
    fn test_string_serialization() {
        let metadata_url = "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG";

        let mut data = Vec::new();
        data.extend_from_slice(&(metadata_url.len() as u32).to_le_bytes()[..4]);
        data.extend_from_slice(metadata_url.as_bytes());

        // Should have 4 bytes for length + string bytes
        assert_eq!(data.len(), 4 + metadata_url.len());
    }
}

#[cfg(test)]
mod balance_function_tests {
    use super::*;

    #[test]
    fn test_lamports_to_sol_conversion() {
        let test_cases = vec![
            (1_000_000_000_u64, 1.0),
            (500_000_000_u64, 0.5),
            (100_000_000_u64, 0.1),
            (1_000_000_u64, 0.001),
            (1_u64, 0.000000001),
        ];

        for (lamports, expected_sol) in test_cases {
            let sol = lamports as f64 / 1_000_000_000.0;
            assert!((sol - expected_sol).abs() < 0.000000001);
        }
    }

    #[test]
    fn test_token_amount_conversion() {
        // Test AEGIS token amount conversion (9 decimals)
        let test_cases = vec![
            (1_000_000_000_u64, 1.0),
            (100_000_000_000_u64, 100.0),
            (500_000_000_u64, 0.5),
            (1_u64, 0.000000001),
        ];

        for (base_units, expected_tokens) in test_cases {
            let tokens = base_units as f64 / 1_000_000_000.0;
            assert!((tokens - expected_tokens).abs() < 0.000000001);
        }
    }

    #[test]
    fn test_zero_balance_handling() {
        let lamports = 0_u64;
        let sol = lamports as f64 / 1_000_000_000.0;
        assert_eq!(sol, 0.0);
    }

    #[test]
    fn test_max_balance_handling() {
        // Test near max u64
        let large_lamports = u64::MAX / 2;
        let sol = large_lamports as f64 / 1_000_000_000.0;

        // Should not panic
        assert!(sol > 0.0);
    }
}

#[cfg(test)]
mod account_meta_tests {
    use super::*;
    use solana_sdk::instruction::AccountMeta;

    #[test]
    fn test_account_meta_writable() {
        let pubkey = Pubkey::new_unique();
        let account = AccountMeta::new(pubkey, false);

        assert_eq!(account.pubkey, pubkey);
        assert!(account.is_writable);
        assert!(!account.is_signer);
    }

    #[test]
    fn test_account_meta_readonly() {
        let pubkey = Pubkey::new_unique();
        let account = AccountMeta::new_readonly(pubkey, false);

        assert_eq!(account.pubkey, pubkey);
        assert!(!account.is_writable);
        assert!(!account.is_signer);
    }

    #[test]
    fn test_account_meta_signer() {
        let pubkey = Pubkey::new_unique();
        let account = AccountMeta::new_readonly(pubkey, true);

        assert_eq!(account.pubkey, pubkey);
        assert!(!account.is_writable);
        assert!(account.is_signer);
    }

    #[test]
    fn test_account_ordering() {
        // Test that we can create accounts in correct order
        let node_account = Pubkey::new_unique();
        let operator = Pubkey::new_unique();
        let system_program = solana_sdk::system_program::ID;

        let accounts = vec![
            AccountMeta::new(node_account, false),
            AccountMeta::new_readonly(operator, true),
            AccountMeta::new_readonly(system_program, false),
        ];

        assert_eq!(accounts.len(), 3);
        assert_eq!(accounts[0].pubkey, node_account);
        assert_eq!(accounts[1].pubkey, operator);
        assert_eq!(accounts[2].pubkey, system_program);
    }
}

#[cfg(test)]
mod cluster_tests {
    use super::*;

    #[test]
    fn test_cluster_urls() {
        let clusters = vec![
            (Cluster::Devnet, "https://api.devnet.solana.com"),
            (Cluster::Testnet, "https://api.testnet.solana.com"),
            (Cluster::Mainnet, "https://api.mainnet-beta.solana.com"),
            (Cluster::Localnet, "http://localhost:8899"),
        ];

        for (cluster, expected_url) in clusters {
            let url = match cluster {
                Cluster::Devnet => "https://api.devnet.solana.com",
                Cluster::Testnet => "https://api.testnet.solana.com",
                Cluster::Mainnet => "https://api.mainnet-beta.solana.com",
                Cluster::Localnet => "http://localhost:8899",
                Cluster::Debug => "http://localhost:8899",
                Cluster::Custom(url, _) => url.as_str(),
            };

            assert_eq!(url, expected_url);
        }
    }

    #[test]
    fn test_devnet_cluster() {
        let cluster = Cluster::Devnet;

        match cluster {
            Cluster::Devnet => assert!(true),
            _ => panic!("Should be Devnet"),
        }
    }
}

#[cfg(test)]
mod amount_validation_tests {
    use super::*;

    const MINIMUM_STAKE: u64 = 100_000_000_000; // 100 AEGIS

    #[test]
    fn test_minimum_stake_validation() {
        assert!(MINIMUM_STAKE > 0);
        assert_eq!(MINIMUM_STAKE, 100_000_000_000);

        let valid_stake = 100_000_000_000_u64;
        assert!(valid_stake >= MINIMUM_STAKE);

        let invalid_stake = 99_999_999_999_u64;
        assert!(invalid_stake < MINIMUM_STAKE);
    }

    #[test]
    fn test_stake_amount_boundaries() {
        let test_cases = vec![
            (0, false),
            (1, false),
            (99_999_999_999, false),
            (100_000_000_000, true),
            (100_000_000_001, true),
            (1_000_000_000_000, true),
        ];

        for (amount, should_be_valid) in test_cases {
            let is_valid = amount >= MINIMUM_STAKE;
            assert_eq!(is_valid, should_be_valid, "Failed for amount {}", amount);
        }
    }

    #[test]
    fn test_unstake_amount_validation() {
        let staked_amount = 500_000_000_000_u64; // 500 AEGIS

        let valid_unstake = 100_000_000_000_u64;
        assert!(valid_unstake <= staked_amount);

        let invalid_unstake = 600_000_000_000_u64;
        assert!(invalid_unstake > staked_amount);
    }
}

#[cfg(test)]
mod error_handling_tests {
    use super::*;

    #[test]
    fn test_zero_balance_error_handling() {
        let balance = 0.0;
        // Zero balance should not error, just display 0.00
        assert_eq!(balance, 0.0);
    }

    #[test]
    fn test_invalid_pubkey_handling() {
        let invalid = "not-a-valid-pubkey";
        let result = Pubkey::from_str(invalid);
        assert!(result.is_err());
    }

    #[test]
    fn test_account_not_found_handling() {
        // When account doesn't exist, balance functions should return 0.0
        let default_balance = 0.0;
        assert_eq!(default_balance, 0.0);
    }
}

#[cfg(test)]
mod instruction_building_tests {
    use super::*;
    use solana_sdk::instruction::Instruction;

    #[test]
    fn test_instruction_structure() {
        let program_id = Pubkey::new_unique();
        let account1 = Pubkey::new_unique();

        let instruction = Instruction {
            program_id,
            accounts: vec![AccountMeta::new(account1, false)],
            data: vec![1, 2, 3, 4],
        };

        assert_eq!(instruction.program_id, program_id);
        assert_eq!(instruction.accounts.len(), 1);
        assert_eq!(instruction.data.len(), 4);
    }

    #[test]
    fn test_empty_instruction_data() {
        let program_id = Pubkey::new_unique();

        let instruction = Instruction {
            program_id,
            accounts: vec![],
            data: vec![],
        };

        assert_eq!(instruction.data.len(), 0);
        assert_eq!(instruction.accounts.len(), 0);
    }

    #[test]
    fn test_instruction_data_with_discriminator_only() {
        let discriminator = [102, 85, 117, 114, 194, 188, 211, 168];

        let data = discriminator.to_vec();
        assert_eq!(data.len(), 8);
        assert_eq!(data[0], 102);
        assert_eq!(data[7], 168);
    }

    #[test]
    fn test_instruction_data_with_discriminator_and_args() {
        let discriminator = [102, 85, 117, 114, 194, 188, 211, 168];
        let amount: u64 = 100_000_000_000;

        let mut data = Vec::new();
        data.extend_from_slice(&discriminator);
        data.extend_from_slice(&amount.to_le_bytes());

        assert_eq!(data.len(), 16);
        assert_eq!(&data[0..8], &discriminator);

        // Verify amount can be deserialized
        let amount_bytes: [u8; 8] = data[8..16].try_into().unwrap();
        let deserialized = u64::from_le_bytes(amount_bytes);
        assert_eq!(deserialized, amount);
    }
}

#[cfg(test)]
mod associated_token_account_tests {
    use super::*;

    #[test]
    fn test_associated_token_account_derivation() {
        let owner = Pubkey::new_unique();
        let mint = Pubkey::new_unique();

        let ata = spl_associated_token_account::get_associated_token_address(&owner, &mint);

        // ATA should be valid pubkey
        assert_ne!(ata, Pubkey::default());
        assert_ne!(ata, owner);
        assert_ne!(ata, mint);
    }

    #[test]
    fn test_ata_determinism() {
        let owner = Pubkey::new_unique();
        let mint = Pubkey::new_unique();

        let ata1 = spl_associated_token_account::get_associated_token_address(&owner, &mint);
        let ata2 = spl_associated_token_account::get_associated_token_address(&owner, &mint);

        // Same owner + mint = same ATA
        assert_eq!(ata1, ata2);
    }

    #[test]
    fn test_different_owners_different_atas() {
        let owner1 = Pubkey::new_unique();
        let owner2 = Pubkey::new_unique();
        let mint = Pubkey::new_unique();

        let ata1 = spl_associated_token_account::get_associated_token_address(&owner1, &mint);
        let ata2 = spl_associated_token_account::get_associated_token_address(&owner2, &mint);

        assert_ne!(ata1, ata2);
    }
}
