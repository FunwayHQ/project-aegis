import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Staking } from "../target/types/staking";
import { expect } from "chai";
import {
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SystemProgram,
} from "@solana/web3.js";
import {
  createMint,
  getOrCreateAssociatedTokenAccount,
  mintTo,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";

describe("Staking Security Tests", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.Staking as Program<Staking>;

  let admin: Keypair;
  let attacker: Keypair;
  let operator: Keypair;
  let mint: PublicKey;
  let treasury: PublicKey;
  let globalConfigPda: PublicKey;
  let stakeAccountPda: PublicKey;
  let stakeVaultPda: PublicKey;

  before(async () => {
    // Create test keypairs
    admin = Keypair.generate();
    attacker = Keypair.generate();
    operator = Keypair.generate();

    // Fund accounts
    await provider.connection.requestAirdrop(
      admin.publicKey,
      2 * LAMPORTS_PER_SOL
    );
    await provider.connection.requestAirdrop(
      attacker.publicKey,
      2 * LAMPORTS_PER_SOL
    );
    await provider.connection.requestAirdrop(
      operator.publicKey,
      2 * LAMPORTS_PER_SOL
    );

    await new Promise((resolve) => setTimeout(resolve, 1000));

    // Create token mint
    mint = await createMint(
      provider.connection,
      admin,
      admin.publicKey,
      null,
      9
    );

    // Create treasury token account
    const treasuryAccount = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      admin,
      mint,
      admin.publicKey
    );
    treasury = treasuryAccount.address;

    // Derive PDAs
    [globalConfigPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("global_config")],
      program.programId
    );

    [stakeAccountPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("stake"), operator.publicKey.toBuffer()],
      program.programId
    );

    [stakeVaultPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("stake_vault")],
      program.programId
    );
  });

  describe("SECURITY FIX #1: Global Config Initialization", () => {
    it("✅ Allows deployer to initialize global config", async () => {
      await program.methods
        .initializeGlobalConfig(
          admin.publicKey,
          new anchor.BN(100_000_000_000), // 100 AEGIS min stake
          new anchor.BN(604800) // 7 days cooldown
        )
        .accounts({
          globalConfig: globalConfigPda,
          treasury: treasury,
          deployer: provider.wallet.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .rpc();

      const config = await program.account.globalConfig.fetch(globalConfigPda);
      expect(config.adminAuthority.toString()).to.equal(
        admin.publicKey.toString()
      );
      expect(config.minStakeAmount.toNumber()).to.equal(100_000_000_000);
      expect(config.unstakeCooldownPeriod.toNumber()).to.equal(604800);
    });

    it("❌ Prevents second initialization", async () => {
      try {
        await program.methods
          .initializeGlobalConfig(
            attacker.publicKey,
            new anchor.BN(1),
            new anchor.BN(1)
          )
          .accounts({
            globalConfig: globalConfigPda,
            treasury: treasury,
            deployer: provider.wallet.publicKey,
            systemProgram: SystemProgram.programId,
          })
          .rpc();

        expect.fail("Should have failed to re-initialize");
      } catch (error) {
        expect(error.message).to.include("already in use");
      }
    });
  });

  describe("SECURITY FIX #2: Admin-Only Config Updates", () => {
    it("✅ Allows admin to update config", async () => {
      const newMinStake = new anchor.BN(200_000_000_000); // 200 AEGIS

      await program.methods
        .updateGlobalConfig(null, newMinStake, null)
        .accounts({
          globalConfig: globalConfigPda,
          admin: admin.publicKey,
        })
        .signers([admin])
        .rpc();

      const config = await program.account.globalConfig.fetch(globalConfigPda);
      expect(config.minStakeAmount.toNumber()).to.equal(200_000_000_000);
    });

    it("❌ CRITICAL: Prevents non-admin from updating config", async () => {
      try {
        await program.methods
          .updateGlobalConfig(
            attacker.publicKey, // Try to steal admin
            new anchor.BN(1),
            new anchor.BN(1)
          )
          .accounts({
            globalConfig: globalConfigPda,
            admin: attacker.publicKey,
          })
          .signers([attacker])
          .rpc();

        expect.fail("Attacker should NOT be able to update config");
      } catch (error) {
        expect(error.message).to.include("UnauthorizedAdmin");
      }
    });
  });

  describe("SECURITY FIX #3: Admin-Only Slashing", () => {
    before(async () => {
      // Setup: Create stake account and stake tokens
      await program.methods
        .initializeStake()
        .accounts({
          stakeAccount: stakeAccountPda,
          operator: operator.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([operator])
        .rpc();

      // Create operator token account and stake
      const operatorTokenAccount = await getOrCreateAssociatedTokenAccount(
        provider.connection,
        operator,
        mint,
        operator.publicKey
      );

      // Mint tokens to operator
      await mintTo(
        provider.connection,
        admin,
        mint,
        operatorTokenAccount.address,
        admin.publicKey,
        500_000_000_000 // 500 AEGIS
      );

      // Stake tokens
      await program.methods
        .stake(new anchor.BN(300_000_000_000))
        .accounts({
          globalConfig: globalConfigPda,
          stakeAccount: stakeAccountPda,
          operatorTokenAccount: operatorTokenAccount.address,
          stakeVault: stakeVaultPda,
          operator: operator.publicKey,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .signers([operator])
        .rpc();
    });

    it("❌ CRITICAL: Prevents random user from slashing", async () => {
      try {
        await program.methods
          .slashStake(new anchor.BN(100_000_000_000), "Malicious slash")
          .accounts({
            globalConfig: globalConfigPda,
            stakeAccount: stakeAccountPda,
            stakeVault: stakeVaultPda,
            treasury: treasury,
            authority: attacker.publicKey, // Random attacker
            tokenProgram: TOKEN_PROGRAM_ID,
          })
          .signers([attacker])
          .rpc();

        expect.fail("Attacker should NOT be able to slash");
      } catch (error) {
        expect(error.message).to.include("UnauthorizedSlashing");
      }
    });

    it("❌ CRITICAL: Prevents operator from slashing themselves", async () => {
      try {
        await program.methods
          .slashStake(new anchor.BN(100_000_000_000), "Self slash")
          .accounts({
            globalConfig: globalConfigPda,
            stakeAccount: stakeAccountPda,
            stakeVault: stakeVaultPda,
            treasury: treasury,
            authority: operator.publicKey, // Operator trying to slash self
            tokenProgram: TOKEN_PROGRAM_ID,
          })
          .signers([operator])
          .rpc();

        expect.fail("Operator should NOT be able to slash themselves");
      } catch (error) {
        expect(error.message).to.include("UnauthorizedSlashing");
      }
    });

    it("✅ Allows authorized admin to slash", async () => {
      const beforeStake = await program.account.stakeAccount.fetch(
        stakeAccountPda
      );
      const slashAmount = new anchor.BN(50_000_000_000); // 50 AEGIS

      await program.methods
        .slashStake(slashAmount, "Terms of service violation")
        .accounts({
          globalConfig: globalConfigPda,
          stakeAccount: stakeAccountPda,
          stakeVault: stakeVaultPda,
          treasury: treasury,
          authority: admin.publicKey, // Authorized admin
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .signers([admin])
        .rpc();

      const afterStake = await program.account.stakeAccount.fetch(
        stakeAccountPda
      );

      expect(
        beforeStake.stakedAmount.sub(afterStake.stakedAmount).toNumber()
      ).to.equal(slashAmount.toNumber());
    });
  });

  describe("SECURITY FIX #4: Pause Functionality", () => {
    it("✅ Allows admin to pause staking", async () => {
      await program.methods
        .setPaused(true)
        .accounts({
          globalConfig: globalConfigPda,
          admin: admin.publicKey,
        })
        .signers([admin])
        .rpc();

      const config = await program.account.globalConfig.fetch(globalConfigPda);
      expect(config.paused).to.be.true;
    });

    it("❌ Prevents staking when paused", async () => {
      const operatorTokenAccount = await getOrCreateAssociatedTokenAccount(
        provider.connection,
        operator,
        mint,
        operator.publicKey
      );

      try {
        await program.methods
          .stake(new anchor.BN(100_000_000_000))
          .accounts({
            globalConfig: globalConfigPda,
            stakeAccount: stakeAccountPda,
            operatorTokenAccount: operatorTokenAccount.address,
            stakeVault: stakeVaultPda,
            operator: operator.publicKey,
            tokenProgram: TOKEN_PROGRAM_ID,
          })
          .signers([operator])
          .rpc();

        expect.fail("Should not allow staking when paused");
      } catch (error) {
        expect(error.message).to.include("StakingPaused");
      }
    });

    it("❌ CRITICAL: Prevents non-admin from unpausing", async () => {
      try {
        await program.methods
          .setPaused(false)
          .accounts({
            globalConfig: globalConfigPda,
            admin: attacker.publicKey,
          })
          .signers([attacker])
          .rpc();

        expect.fail("Attacker should NOT be able to unpause");
      } catch (error) {
        expect(error.message).to.include("UnauthorizedAdmin");
      }
    });

    it("✅ Allows admin to unpause", async () => {
      await program.methods
        .setPaused(false)
        .accounts({
          globalConfig: globalConfigPda,
          admin: admin.publicKey,
        })
        .signers([admin])
        .rpc();

      const config = await program.account.globalConfig.fetch(globalConfigPda);
      expect(config.paused).to.be.false;
    });
  });

  describe("SECURITY: Admin Transfer", () => {
    it("✅ Allows admin to transfer authority to DAO", async () => {
      const dao = Keypair.generate();

      await program.methods
        .updateGlobalConfig(dao.publicKey, null, null)
        .accounts({
          globalConfig: globalConfigPda,
          admin: admin.publicKey,
        })
        .signers([admin])
        .rpc();

      const config = await program.account.globalConfig.fetch(globalConfigPda);
      expect(config.adminAuthority.toString()).to.equal(
        dao.publicKey.toString()
      );

      // Original admin should no longer have access
      try {
        await program.methods
          .setPaused(true)
          .accounts({
            globalConfig: globalConfigPda,
            admin: admin.publicKey,
          })
          .signers([admin])
          .rpc();

        expect.fail("Old admin should no longer have access");
      } catch (error) {
        expect(error.message).to.include("UnauthorizedAdmin");
      }

      // New DAO admin should have access
      await program.methods
        .setPaused(true)
        .accounts({
          globalConfig: globalConfigPda,
          admin: dao.publicKey,
        })
        .signers([dao])
        .rpc();

      const updatedConfig = await program.account.globalConfig.fetch(
        globalConfigPda
      );
      expect(updatedConfig.paused).to.be.true;
    });
  });
});
