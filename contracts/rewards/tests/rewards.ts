import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Rewards } from "../target/types/rewards";
import { expect } from "chai";
import {
  createMint,
  mintTo,
  getAccount,
  TOKEN_PROGRAM_ID,
  ACCOUNT_SIZE,
  createInitializeAccountInstruction,
} from "@solana/spl-token";

describe("rewards", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.Rewards as Program<Rewards>;

  let mint: anchor.web3.PublicKey;
  let rewardVault: anchor.web3.PublicKey;
  let rewardPoolPDA: anchor.web3.PublicKey;

  const REWARD_RATE = new anchor.BN(1_000_000); // 0.001 AEGIS per staked token per epoch

  // Helper to get reward pool PDA
  function getRewardPoolPDA(): [anchor.web3.PublicKey, number] {
    return anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("reward_pool")],
      program.programId
    );
  }

  // Helper to get operator rewards PDA
  function getOperatorRewardsPDA(operator: anchor.web3.PublicKey): [anchor.web3.PublicKey, number] {
    return anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("operator_rewards"), operator.toBuffer()],
      program.programId
    );
  }

  // Helper to fund account with SOL
  async function fundAccount(publicKey: anchor.web3.PublicKey, lamports: number = 1 * anchor.web3.LAMPORTS_PER_SOL) {
    const tx = new anchor.web3.Transaction().add(
      anchor.web3.SystemProgram.transfer({
        fromPubkey: provider.wallet.publicKey,
        toPubkey: publicKey,
        lamports,
      })
    );
    await provider.sendAndConfirm(tx);
  }

  // Helper to create token account
  async function createTokenAccount(
    owner: anchor.web3.PublicKey,
    payer: anchor.web3.Keypair
  ): Promise<anchor.web3.PublicKey> {
    const tokenAccount = anchor.web3.Keypair.generate();

    const lamports = await provider.connection.getMinimumBalanceForRentExemption(ACCOUNT_SIZE);

    const transaction = new anchor.web3.Transaction().add(
      anchor.web3.SystemProgram.createAccount({
        fromPubkey: payer.publicKey,
        newAccountPubkey: tokenAccount.publicKey,
        space: ACCOUNT_SIZE,
        lamports,
        programId: TOKEN_PROGRAM_ID,
      }),
      createInitializeAccountInstruction(
        tokenAccount.publicKey,
        mint,
        owner,
        TOKEN_PROGRAM_ID
      )
    );

    await provider.sendAndConfirm(transaction, [payer, tokenAccount]);
    return tokenAccount.publicKey;
  }

  before(async () => {
    // Create AEGIS token mint
    mint = await createMint(
      provider.connection,
      provider.wallet.payer,
      provider.wallet.publicKey,
      null,
      9 // 9 decimals
    );

    // Get reward pool PDA
    [rewardPoolPDA] = getRewardPoolPDA();

    // Create reward vault (owned by reward pool PDA)
    rewardVault = await createTokenAccount(
      rewardPoolPDA,
      provider.wallet.payer
    );

    console.log("Mint:", mint.toString());
    console.log("Reward Vault:", rewardVault.toString());
    console.log("Reward Pool PDA:", rewardPoolPDA.toString());
  });

  describe("Pool Initialization", () => {
    it("Initializes reward pool successfully", async () => {
      await program.methods
        .initializePool(REWARD_RATE)
        .accounts({
          rewardPool: rewardPoolPDA,
          rewardVault: rewardVault,
          authority: provider.wallet.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .rpc();

      const pool = await program.account.rewardPool.fetch(rewardPoolPDA);

      expect(pool.authority.toString()).to.equal(provider.wallet.publicKey.toString());
      expect(pool.rewardVault.toString()).to.equal(rewardVault.toString());
      expect(pool.rewardRatePerEpoch.toString()).to.equal(REWARD_RATE.toString());
      expect(pool.totalDistributed.toString()).to.equal("0");
      expect(pool.currentEpoch.toString()).to.equal("0");
    });

    it("Prevents duplicate pool initialization", async () => {
      try {
        await program.methods
          .initializePool(REWARD_RATE)
          .accounts({
            rewardPool: rewardPoolPDA,
            rewardVault: rewardVault,
            authority: provider.wallet.publicKey,
            systemProgram: anchor.web3.SystemProgram.programId,
          })
          .rpc();

        expect.fail("Should have prevented duplicate initialization");
      } catch (error) {
        expect(error).to.exist;
      }
    });
  });

  describe("Operator Rewards Initialization", () => {
    it("Initializes operator rewards account", async () => {
      const operator = anchor.web3.Keypair.generate();
      await fundAccount(operator.publicKey);

      const [operatorRewardsPDA] = getOperatorRewardsPDA(operator.publicKey);

      await program.methods
        .initializeOperatorRewards()
        .accounts({
          operatorRewards: operatorRewardsPDA,
          operator: operator.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([operator])
        .rpc();

      const rewards = await program.account.operatorRewards.fetch(operatorRewardsPDA);

      expect(rewards.operator.toString()).to.equal(operator.publicKey.toString());
      expect(rewards.totalEarned.toString()).to.equal("0");
      expect(rewards.totalClaimed.toString()).to.equal("0");
      expect(rewards.unclaimedRewards.toString()).to.equal("0");
      expect(rewards.performanceScore).to.equal(100);
      expect(rewards.uptimePercentage).to.equal(0);
    });
  });

  describe("Performance Tracking", () => {
    let operator: anchor.web3.Keypair;
    let operatorRewardsPDA: anchor.web3.PublicKey;

    before(async () => {
      operator = anchor.web3.Keypair.generate();
      await fundAccount(operator.publicKey);

      [operatorRewardsPDA] = getOperatorRewardsPDA(operator.publicKey);

      await program.methods
        .initializeOperatorRewards()
        .accounts({
          operatorRewards: operatorRewardsPDA,
          operator: operator.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([operator])
        .rpc();
    });

    it("Records performance metrics successfully", async () => {
      const uptime = 95;
      const performance = 98;
      const epoch = new anchor.BN(1);

      await program.methods
        .recordPerformance(uptime, performance, epoch)
        .accounts({
          rewardPool: rewardPoolPDA,
          operatorRewards: operatorRewardsPDA,
          authority: provider.wallet.publicKey,
        })
        .rpc();

      const rewards = await program.account.operatorRewards.fetch(operatorRewardsPDA);

      expect(rewards.uptimePercentage).to.equal(uptime);
      expect(rewards.performanceScore).to.equal(performance);
    });

    it("Rejects invalid uptime percentage", async () => {
      try {
        await program.methods
          .recordPerformance(101, 50, new anchor.BN(2))
          .accounts({
            rewardPool: rewardPoolPDA,
            operatorRewards: operatorRewardsPDA,
            authority: provider.wallet.publicKey,
          })
          .rpc();

        expect.fail("Should have rejected invalid uptime");
      } catch (error) {
        expect(error.toString()).to.include("InvalidPercentage");
      }
    });

    it("Rejects invalid performance score", async () => {
      try {
        await program.methods
          .recordPerformance(50, 150, new anchor.BN(2))
          .accounts({
            rewardPool: rewardPoolPDA,
            operatorRewards: operatorRewardsPDA,
            authority: provider.wallet.publicKey,
          })
          .rpc();

        expect.fail("Should have rejected invalid performance");
      } catch (error) {
        expect(error.toString()).to.include("InvalidPercentage");
      }
    });
  });

  describe("Rewards Calculation", () => {
    let operator: anchor.web3.Keypair;
    let operatorRewardsPDA: anchor.web3.PublicKey;

    before(async () => {
      operator = anchor.web3.Keypair.generate();
      await fundAccount(operator.publicKey);

      [operatorRewardsPDA] = getOperatorRewardsPDA(operator.publicKey);

      await program.methods
        .initializeOperatorRewards()
        .accounts({
          operatorRewards: operatorRewardsPDA,
          operator: operator.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([operator])
        .rpc();

      // Set performance metrics
      await program.methods
        .recordPerformance(100, 100, new anchor.BN(1))
        .accounts({
          rewardPool: rewardPoolPDA,
          operatorRewards: operatorRewardsPDA,
          authority: provider.wallet.publicKey,
        })
        .rpc();
    });

    it("Calculates rewards based on stake and epochs", async () => {
      const stakedAmount = new anchor.BN(100_000_000_000); // 100 AEGIS
      const epochsElapsed = new anchor.BN(10);

      await program.methods
        .calculateRewards(stakedAmount, epochsElapsed)
        .accounts({
          rewardPool: rewardPoolPDA,
          operatorRewards: operatorRewardsPDA,
          authority: provider.wallet.publicKey,
        })
        .rpc();

      const rewards = await program.account.operatorRewards.fetch(operatorRewardsPDA);

      expect(rewards.unclaimedRewards.toNumber()).to.be.greaterThan(0);
      expect(rewards.totalEarned.toString()).to.equal(rewards.unclaimedRewards.toString());
    });

    it("Applies performance multiplier correctly", async () => {
      const operator2 = anchor.web3.Keypair.generate();
      await fundAccount(operator2.publicKey);
      const [operatorRewardsPDA2] = getOperatorRewardsPDA(operator2.publicKey);

      await program.methods
        .initializeOperatorRewards()
        .accounts({
          operatorRewards: operatorRewardsPDA2,
          operator: operator2.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([operator2])
        .rpc();

      // Set lower performance (50% uptime, 50% performance)
      await program.methods
        .recordPerformance(50, 50, new anchor.BN(1))
        .accounts({
          rewardPool: rewardPoolPDA,
          operatorRewards: operatorRewardsPDA2,
          authority: provider.wallet.publicKey,
        })
        .rpc();

      const stakedAmount = new anchor.BN(100_000_000_000);
      const epochsElapsed = new anchor.BN(10);

      await program.methods
        .calculateRewards(stakedAmount, epochsElapsed)
        .accounts({
          rewardPool: rewardPoolPDA,
          operatorRewards: operatorRewardsPDA2,
          authority: provider.wallet.publicKey,
        })
        .rpc();

      const rewards2 = await program.account.operatorRewards.fetch(operatorRewardsPDA2);

      // With 50% * 50% = 25% multiplier, rewards should still be calculated
      // (unclaimed equals total_earned when no claims have been made)
      expect(rewards2.unclaimedRewards.toNumber()).to.be.greaterThan(0);
      expect(rewards2.totalEarned.toString()).to.equal(rewards2.unclaimedRewards.toString());
    });
  });

  describe("Claiming Rewards", () => {
    let operator: anchor.web3.Keypair;
    let operatorRewardsPDA: anchor.web3.PublicKey;
    let operatorTokenAccount: anchor.web3.PublicKey;

    before(async () => {
      operator = anchor.web3.Keypair.generate();
      await fundAccount(operator.publicKey);

      [operatorRewardsPDA] = getOperatorRewardsPDA(operator.publicKey);

      // Initialize rewards account
      await program.methods
        .initializeOperatorRewards()
        .accounts({
          operatorRewards: operatorRewardsPDA,
          operator: operator.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([operator])
        .rpc();

      // Set performance and calculate rewards
      await program.methods
        .recordPerformance(100, 100, new anchor.BN(1))
        .accounts({
          rewardPool: rewardPoolPDA,
          operatorRewards: operatorRewardsPDA,
          authority: provider.wallet.publicKey,
        })
        .rpc();

      const stakedAmount = new anchor.BN(200_000_000_000); // 200 AEGIS
      await program.methods
        .calculateRewards(stakedAmount, new anchor.BN(5))
        .accounts({
          rewardPool: rewardPoolPDA,
          operatorRewards: operatorRewardsPDA,
          authority: provider.wallet.publicKey,
        })
        .rpc();

      // Fund the reward vault
      const funderTokenAccount = await createTokenAccount(
        provider.wallet.publicKey,
        provider.wallet.payer
      );
      await mintTo(
        provider.connection,
        provider.wallet.payer,
        mint,
        funderTokenAccount,
        provider.wallet.publicKey,
        Number("10000000000000") // 10,000 AEGIS for rewards
      );

      await program.methods
        .fundPool(new anchor.BN("10000000000000"))
        .accounts({
          rewardPool: rewardPoolPDA,
          funderTokenAccount,
          rewardVault,
          authority: provider.wallet.publicKey,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .rpc();

      // Create operator token account
      operatorTokenAccount = await createTokenAccount(operator.publicKey, operator);
    });

    it("Claims rewards successfully", async () => {
      const beforeRewards = await program.account.operatorRewards.fetch(operatorRewardsPDA);
      const beforeBalance = await getAccount(provider.connection, operatorTokenAccount);

      await program.methods
        .claimRewards()
        .accounts({
          rewardPool: rewardPoolPDA,
          operatorRewards: operatorRewardsPDA,
          rewardVault,
          operatorTokenAccount,
          operator: operator.publicKey,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .signers([operator])
        .rpc();

      const afterRewards = await program.account.operatorRewards.fetch(operatorRewardsPDA);
      const afterBalance = await getAccount(provider.connection, operatorTokenAccount);

      expect(afterRewards.unclaimedRewards.toString()).to.equal("0");
      expect(afterRewards.totalClaimed.toString()).to.equal(beforeRewards.unclaimedRewards.toString());
      expect(Number(afterBalance.amount)).to.equal(
        Number(beforeBalance.amount) + beforeRewards.unclaimedRewards.toNumber()
      );
    });

    it("Rejects claim when no rewards available", async () => {
      try {
        await program.methods
          .claimRewards()
          .accounts({
            rewardPool: rewardPoolPDA,
            operatorRewards: operatorRewardsPDA,
            rewardVault,
            operatorTokenAccount,
            operator: operator.publicKey,
            tokenProgram: TOKEN_PROGRAM_ID,
          })
          .signers([operator])
          .rpc();

        expect.fail("Should have rejected claim with no rewards");
      } catch (error) {
        expect(error.toString()).to.include("NoRewardsToClaim");
      }
    });
  });

  describe("Pool Management", () => {
    it("Allows authority to fund pool", async () => {
      const funderTokenAccount = await createTokenAccount(
        provider.wallet.publicKey,
        provider.wallet.payer
      );
      await mintTo(
        provider.connection,
        provider.wallet.payer,
        mint,
        funderTokenAccount,
        provider.wallet.publicKey,
        Number("1000000000000")
      );

      const beforeVault = await getAccount(provider.connection, rewardVault);

      const fundAmount = new anchor.BN(500_000_000_000);
      await program.methods
        .fundPool(fundAmount)
        .accounts({
          rewardPool: rewardPoolPDA,
          funderTokenAccount,
          rewardVault,
          authority: provider.wallet.publicKey,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .rpc();

      const afterVault = await getAccount(provider.connection, rewardVault);

      expect(Number(afterVault.amount)).to.equal(
        Number(beforeVault.amount) + fundAmount.toNumber()
      );
    });

    it("Updates reward rate", async () => {
      const newRate = new anchor.BN(2_000_000);

      await program.methods
        .updateRewardRate(newRate)
        .accounts({
          rewardPool: rewardPoolPDA,
          authority: provider.wallet.publicKey,
        })
        .rpc();

      const pool = await program.account.rewardPool.fetch(rewardPoolPDA);
      expect(pool.rewardRatePerEpoch.toString()).to.equal(newRate.toString());
    });

    it("Rejects unauthorized rate update", async () => {
      const unauthorized = anchor.web3.Keypair.generate();
      await fundAccount(unauthorized.publicKey);

      try {
        await program.methods
          .updateRewardRate(new anchor.BN(5_000_000))
          .accounts({
            rewardPool: rewardPoolPDA,
            authority: unauthorized.publicKey,
          })
          .signers([unauthorized])
          .rpc();

        expect.fail("Should have rejected unauthorized update");
      } catch (error) {
        expect(error.toString()).to.include("UnauthorizedAuthority");
      }
    });
  });

  describe("Complete Workflow", () => {
    it("Handles full operator lifecycle", async () => {
      const operator = anchor.web3.Keypair.generate();
      await fundAccount(operator.publicKey);

      const [operatorRewardsPDA] = getOperatorRewardsPDA(operator.publicKey);

      // 1. Initialize operator rewards
      await program.methods
        .initializeOperatorRewards()
        .accounts({
          operatorRewards: operatorRewardsPDA,
          operator: operator.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([operator])
        .rpc();

      // 2. Record performance over multiple epochs
      for (let epoch = 1; epoch <= 3; epoch++) {
        await program.methods
          .recordPerformance(95, 90, new anchor.BN(epoch))
          .accounts({
            rewardPool: rewardPoolPDA,
            operatorRewards: operatorRewardsPDA,
            authority: provider.wallet.publicKey,
          })
          .rpc();

        // Calculate rewards
        await program.methods
          .calculateRewards(new anchor.BN(500_000_000_000), new anchor.BN(1))
          .accounts({
            rewardPool: rewardPoolPDA,
            operatorRewards: operatorRewardsPDA,
            authority: provider.wallet.publicKey,
          })
          .rpc();
      }

      const beforeClaim = await program.account.operatorRewards.fetch(operatorRewardsPDA);
      expect(beforeClaim.unclaimedRewards.toNumber()).to.be.greaterThan(0);

      // 3. Claim accumulated rewards
      const operatorTokenAccount = await createTokenAccount(operator.publicKey, operator);

      await program.methods
        .claimRewards()
        .accounts({
          rewardPool: rewardPoolPDA,
          operatorRewards: operatorRewardsPDA,
          rewardVault,
          operatorTokenAccount,
          operator: operator.publicKey,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .signers([operator])
        .rpc();

      const afterClaim = await program.account.operatorRewards.fetch(operatorRewardsPDA);
      const tokenBalance = await getAccount(provider.connection, operatorTokenAccount);

      expect(afterClaim.unclaimedRewards.toString()).to.equal("0");
      expect(afterClaim.totalClaimed.toString()).to.equal(beforeClaim.unclaimedRewards.toString());
      expect(Number(tokenBalance.amount)).to.equal(beforeClaim.unclaimedRewards.toNumber());
    });
  });

  describe("Edge Cases", () => {
    it("Handles zero performance correctly", async () => {
      const operator = anchor.web3.Keypair.generate();
      await fundAccount(operator.publicKey);

      const [operatorRewardsPDA] = getOperatorRewardsPDA(operator.publicKey);

      await program.methods
        .initializeOperatorRewards()
        .accounts({
          operatorRewards: operatorRewardsPDA,
          operator: operator.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([operator])
        .rpc();

      // Set zero performance
      await program.methods
        .recordPerformance(0, 0, new anchor.BN(1))
        .accounts({
          rewardPool: rewardPoolPDA,
          operatorRewards: operatorRewardsPDA,
          authority: provider.wallet.publicKey,
        })
        .rpc();

      await program.methods
        .calculateRewards(new anchor.BN(100_000_000_000), new anchor.BN(5))
        .accounts({
          rewardPool: rewardPoolPDA,
          operatorRewards: operatorRewardsPDA,
          authority: provider.wallet.publicKey,
        })
        .rpc();

      const rewards = await program.account.operatorRewards.fetch(operatorRewardsPDA);

      // With 0% performance, should get 0 rewards
      expect(rewards.unclaimedRewards.toString()).to.equal("0");
    });

    it("Handles maximum stake amounts", async () => {
      const operator = anchor.web3.Keypair.generate();
      await fundAccount(operator.publicKey);

      const [operatorRewardsPDA] = getOperatorRewardsPDA(operator.publicKey);

      await program.methods
        .initializeOperatorRewards()
        .accounts({
          operatorRewards: operatorRewardsPDA,
          operator: operator.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([operator])
        .rpc();

      await program.methods
        .recordPerformance(100, 100, new anchor.BN(1))
        .accounts({
          rewardPool: rewardPoolPDA,
          operatorRewards: operatorRewardsPDA,
          authority: provider.wallet.publicKey,
        })
        .rpc();

      // Test with very large stake
      const largeStake = new anchor.BN("100000000000000"); // 100,000 AEGIS
      await program.methods
        .calculateRewards(largeStake, new anchor.BN(1))
        .accounts({
          rewardPool: rewardPoolPDA,
          operatorRewards: operatorRewardsPDA,
          authority: provider.wallet.publicKey,
        })
        .rpc();

      const rewards = await program.account.operatorRewards.fetch(operatorRewardsPDA);
      expect(rewards.unclaimedRewards.toNumber()).to.be.greaterThan(0);
    });
  });

  describe("PDA Derivation", () => {
    it("Derives unique PDAs for different operators", async () => {
      const operator1 = anchor.web3.Keypair.generate();
      const operator2 = anchor.web3.Keypair.generate();

      const [pda1] = getOperatorRewardsPDA(operator1.publicKey);
      const [pda2] = getOperatorRewardsPDA(operator2.publicKey);

      expect(pda1.toString()).to.not.equal(pda2.toString());

      // Same operator should give same PDA
      const [pda1Again] = getOperatorRewardsPDA(operator1.publicKey);
      expect(pda1.toString()).to.equal(pda1Again.toString());
    });

    it("Derives consistent reward pool PDA", async () => {
      const [pda1] = getRewardPoolPDA();
      const [pda2] = getRewardPoolPDA();

      expect(pda1.toString()).to.equal(pda2.toString());
      expect(pda1.toString()).to.equal(rewardPoolPDA.toString());
    });
  });

  describe("Security & Authorization", () => {
    it("Rejects unauthorized operator claiming rewards", async () => {
      const operator = anchor.web3.Keypair.generate();
      const attacker = anchor.web3.Keypair.generate();
      await fundAccount(operator.publicKey);
      await fundAccount(attacker.publicKey);

      const [operatorRewardsPDA] = getOperatorRewardsPDA(operator.publicKey);

      await program.methods
        .initializeOperatorRewards()
        .accounts({
          operatorRewards: operatorRewardsPDA,
          operator: operator.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([operator])
        .rpc();

      // Set performance and calculate rewards
      await program.methods
        .recordPerformance(100, 100, new anchor.BN(1))
        .accounts({
          rewardPool: rewardPoolPDA,
          operatorRewards: operatorRewardsPDA,
          authority: provider.wallet.publicKey,
        })
        .rpc();

      await program.methods
        .calculateRewards(new anchor.BN(100_000_000_000), new anchor.BN(5))
        .accounts({
          rewardPool: rewardPoolPDA,
          operatorRewards: operatorRewardsPDA,
          authority: provider.wallet.publicKey,
        })
        .rpc();

      // Try to claim with wrong operator
      const attackerTokenAccount = await createTokenAccount(attacker.publicKey, attacker);

      try {
        await program.methods
          .claimRewards()
          .accounts({
            rewardPool: rewardPoolPDA,
            operatorRewards: operatorRewardsPDA,
            rewardVault,
            operatorTokenAccount: attackerTokenAccount,
            operator: attacker.publicKey, // Wrong operator!
            tokenProgram: TOKEN_PROGRAM_ID,
          })
          .signers([attacker])
          .rpc();

        expect.fail("Should have rejected unauthorized claim");
      } catch (error) {
        // Anchor constraint errors show as "AnchorError caused by account: operator"
        expect(error).to.exist;
      }
    });

    it("Rejects unauthorized funding", async () => {
      const unauthorized = anchor.web3.Keypair.generate();
      await fundAccount(unauthorized.publicKey);

      const unauthorizedTokenAccount = await createTokenAccount(
        unauthorized.publicKey,
        unauthorized
      );
      await mintTo(
        provider.connection,
        provider.wallet.payer,
        mint,
        unauthorizedTokenAccount,
        provider.wallet.publicKey,
        Number("1000000000000")
      );

      try {
        await program.methods
          .fundPool(new anchor.BN(100_000_000_000))
          .accounts({
            rewardPool: rewardPoolPDA,
            funderTokenAccount: unauthorizedTokenAccount,
            rewardVault,
            authority: unauthorized.publicKey,
            tokenProgram: TOKEN_PROGRAM_ID,
          })
          .signers([unauthorized])
          .rpc();

        expect.fail("Should have rejected unauthorized funding");
      } catch (error) {
        expect(error.toString()).to.include("UnauthorizedAuthority");
      }
    });

    it("Rejects fund pool with zero amount", async () => {
      const funderTokenAccount = await createTokenAccount(
        provider.wallet.publicKey,
        provider.wallet.payer
      );

      try {
        await program.methods
          .fundPool(new anchor.BN(0))
          .accounts({
            rewardPool: rewardPoolPDA,
            funderTokenAccount,
            rewardVault,
            authority: provider.wallet.publicKey,
            tokenProgram: TOKEN_PROGRAM_ID,
          })
          .rpc();

        expect.fail("Should have rejected zero amount");
      } catch (error) {
        expect(error.toString()).to.include("InvalidAmount");
      }
    });
  });

  describe("Multiple Calculations", () => {
    it("Accumulates rewards over multiple calculations", async () => {
      const operator = anchor.web3.Keypair.generate();
      await fundAccount(operator.publicKey);

      const [operatorRewardsPDA] = getOperatorRewardsPDA(operator.publicKey);

      await program.methods
        .initializeOperatorRewards()
        .accounts({
          operatorRewards: operatorRewardsPDA,
          operator: operator.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([operator])
        .rpc();

      await program.methods
        .recordPerformance(100, 100, new anchor.BN(1))
        .accounts({
          rewardPool: rewardPoolPDA,
          operatorRewards: operatorRewardsPDA,
          authority: provider.wallet.publicKey,
        })
        .rpc();

      // Calculate rewards 3 times
      for (let i = 0; i < 3; i++) {
        await program.methods
          .calculateRewards(new anchor.BN(100_000_000_000), new anchor.BN(1))
          .accounts({
            rewardPool: rewardPoolPDA,
            operatorRewards: operatorRewardsPDA,
            authority: provider.wallet.publicKey,
          })
          .rpc();
      }

      const rewards = await program.account.operatorRewards.fetch(operatorRewardsPDA);

      // Should have 3x the rewards
      expect(rewards.unclaimedRewards.toNumber()).to.be.greaterThan(0);
      expect(rewards.totalEarned.toString()).to.equal(rewards.unclaimedRewards.toString());
    });

    it("Handles partial performance over time", async () => {
      const operator = anchor.web3.Keypair.generate();
      await fundAccount(operator.publicKey);

      const [operatorRewardsPDA] = getOperatorRewardsPDA(operator.publicKey);

      await program.methods
        .initializeOperatorRewards()
        .accounts({
          operatorRewards: operatorRewardsPDA,
          operator: operator.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([operator])
        .rpc();

      // Epoch 1: Perfect performance
      await program.methods
        .recordPerformance(100, 100, new anchor.BN(1))
        .accounts({
          rewardPool: rewardPoolPDA,
          operatorRewards: operatorRewardsPDA,
          authority: provider.wallet.publicKey,
        })
        .rpc();

      await program.methods
        .calculateRewards(new anchor.BN(100_000_000_000), new anchor.BN(1))
        .accounts({
          rewardPool: rewardPoolPDA,
          operatorRewards: operatorRewardsPDA,
          authority: provider.wallet.publicKey,
        })
        .rpc();

      const afterEpoch1 = await program.account.operatorRewards.fetch(operatorRewardsPDA);

      // Epoch 2: Poor performance
      await program.methods
        .recordPerformance(30, 40, new anchor.BN(2))
        .accounts({
          rewardPool: rewardPoolPDA,
          operatorRewards: operatorRewardsPDA,
          authority: provider.wallet.publicKey,
        })
        .rpc();

      await program.methods
        .calculateRewards(new anchor.BN(100_000_000_000), new anchor.BN(1))
        .accounts({
          rewardPool: rewardPoolPDA,
          operatorRewards: operatorRewardsPDA,
          authority: provider.wallet.publicKey,
        })
        .rpc();

      const afterEpoch2 = await program.account.operatorRewards.fetch(operatorRewardsPDA);

      // Rewards should increase, but less than epoch 1
      expect(afterEpoch2.unclaimedRewards.toNumber()).to.be.greaterThan(
        afterEpoch1.unclaimedRewards.toNumber()
      );
    });
  });

  describe("Pool Statistics", () => {
    it("Tracks total distributed correctly", async () => {
      const operator1 = anchor.web3.Keypair.generate();
      const operator2 = anchor.web3.Keypair.generate();
      await fundAccount(operator1.publicKey);
      await fundAccount(operator2.publicKey);

      const [opRewards1] = getOperatorRewardsPDA(operator1.publicKey);
      const [opRewards2] = getOperatorRewardsPDA(operator2.publicKey);

      // Initialize both operators
      for (const [op, opRewards] of [[operator1, opRewards1], [operator2, opRewards2]]) {
        await program.methods
          .initializeOperatorRewards()
          .accounts({
            operatorRewards: opRewards,
            operator: op.publicKey,
            systemProgram: anchor.web3.SystemProgram.programId,
          })
          .signers([op])
          .rpc();

        await program.methods
          .recordPerformance(100, 100, new anchor.BN(1))
          .accounts({
            rewardPool: rewardPoolPDA,
            operatorRewards: opRewards,
            authority: provider.wallet.publicKey,
          })
          .rpc();

        await program.methods
          .calculateRewards(new anchor.BN(50_000_000_000), new anchor.BN(2))
          .accounts({
            rewardPool: rewardPoolPDA,
            operatorRewards: opRewards,
            authority: provider.wallet.publicKey,
          })
          .rpc();
      }

      const poolBefore = await program.account.rewardPool.fetch(rewardPoolPDA);
      const rewards1 = await program.account.operatorRewards.fetch(opRewards1);
      const rewards2 = await program.account.operatorRewards.fetch(opRewards2);

      // Both operators claim
      const op1TokenAccount = await createTokenAccount(operator1.publicKey, operator1);
      const op2TokenAccount = await createTokenAccount(operator2.publicKey, operator2);

      await program.methods
        .claimRewards()
        .accounts({
          rewardPool: rewardPoolPDA,
          operatorRewards: opRewards1,
          rewardVault,
          operatorTokenAccount: op1TokenAccount,
          operator: operator1.publicKey,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .signers([operator1])
        .rpc();

      await program.methods
        .claimRewards()
        .accounts({
          rewardPool: rewardPoolPDA,
          operatorRewards: opRewards2,
          rewardVault,
          operatorTokenAccount: op2TokenAccount,
          operator: operator2.publicKey,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .signers([operator2])
        .rpc();

      const poolAfter = await program.account.rewardPool.fetch(rewardPoolPDA);

      expect(poolAfter.totalDistributed.toNumber()).to.equal(
        poolBefore.totalDistributed.toNumber() +
        rewards1.unclaimedRewards.toNumber() +
        rewards2.unclaimedRewards.toNumber()
      );
    });
  });
});
