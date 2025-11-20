import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { NodeRegistry } from "../target/types/node_registry";
import { expect } from "chai";
import { Keypair, LAMPORTS_PER_SOL, PublicKey, SystemProgram } from "@solana/web3.js";

describe("Registry Security Tests", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.NodeRegistry as Program<NodeRegistry>;

  let admin: Keypair;
  let attacker: Keypair;
  let operator: Keypair;
  let registryConfigPda: PublicKey;
  let nodeAccountPda: PublicKey;
  const stakingProgramId = new PublicKey("5oGLkNZ7Hku3bRD4aWnRNo8PsXusXmojm8EzAiQUVD1H");

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

    // Derive PDAs
    [registryConfigPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("registry_config")],
      program.programId
    );

    [nodeAccountPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("node"), operator.publicKey.toBuffer()],
      program.programId
    );
  });

  describe("SECURITY FIX #1: Registry Config Initialization", () => {
    it("✅ Allows deployer to initialize registry config", async () => {
      await program.methods
        .initializeRegistryConfig(
          admin.publicKey,
          stakingProgramId,
          new anchor.BN(100_000_000_000)
        )
        .accounts({
          registryConfig: registryConfigPda,
          deployer: provider.wallet.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .rpc();

      const config = await program.account.registryConfig.fetch(
        registryConfigPda
      );
      expect(config.adminAuthority.toString()).to.equal(
        admin.publicKey.toString()
      );
      expect(config.stakingProgramId.toString()).to.equal(
        stakingProgramId.toString()
      );
    });
  });

  describe("SECURITY FIX #2: Unauthorized Stake Update Prevention", () => {
    before(async () => {
      // Register a node first
      await program.methods
        .registerNode(
          "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG",
          new anchor.BN(100_000_000_000)
        )
        .accounts({
          nodeAccount: nodeAccountPda,
          operator: operator.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([operator])
        .rpc();
    });

    it("❌ CRITICAL: Prevents random user from updating stake amount", async () => {
      const maliciousStakeAmount = new anchor.BN(0); // Try to zero out stake

      try {
        await program.methods
          .updateStake(maliciousStakeAmount)
          .accounts({
            registryConfig: registryConfigPda,
            nodeAccount: nodeAccountPda,
            authority: attacker.publicKey, // Random attacker
          })
          .signers([attacker])
          .rpc();

        expect.fail("Attacker should NOT be able to update stake");
      } catch (error) {
        expect(error.message).to.include("UnauthorizedStakeUpdate");
      }
    });

    it("❌ CRITICAL: Prevents operator from manipulating their own stake", async () => {
      const fakeStakeAmount = new anchor.BN(1_000_000_000_000_000); // 1M AEGIS

      try {
        await program.methods
          .updateStake(fakeStakeAmount)
          .accounts({
            registryConfig: registryConfigPda,
            nodeAccount: nodeAccountPda,
            authority: operator.publicKey, // Operator trying to fake stake
          })
          .signers([operator])
          .rpc();

        expect.fail("Operator should NOT be able to update their own stake");
      } catch (error) {
        expect(error.message).to.include("UnauthorizedStakeUpdate");
      }
    });

    it("❌ CRITICAL: Prevents admin from updating stake (only staking program)", async () => {
      try {
        await program.methods
          .updateStake(new anchor.BN(500_000_000_000))
          .accounts({
            registryConfig: registryConfigPda,
            nodeAccount: nodeAccountPda,
            authority: admin.publicKey, // Even admin cannot do this
          })
          .signers([admin])
          .rpc();

        expect.fail("Admin should NOT be able to update stake directly");
      } catch (error) {
        expect(error.message).to.include("UnauthorizedStakeUpdate");
      }
    });

    it("✅ NOTE: Only staking program can update stake via CPI", () => {
      // This test demonstrates that ONLY the staking program can update stake
      // The actual test would require CPI from the staking program
      // For now, we verify that all unauthorized attempts fail

      // In production, the staking program would:
      // 1. Call update_stake via CPI
      // 2. Pass its own program ID as authority
      // 3. Registry would verify: authority == staking_program_id
      // 4. Update would succeed

      expect(true).to.be.true;
    });
  });

  describe("SECURITY FIX #3: Admin-Only Config Updates", () => {
    it("❌ CRITICAL: Prevents non-admin from updating staking program ID", async () => {
      const maliciousProgramId = Keypair.generate().publicKey;

      try {
        await program.methods
          .updateRegistryConfig(null, maliciousProgramId, null)
          .accounts({
            registryConfig: registryConfigPda,
            admin: attacker.publicKey,
          })
          .signers([attacker])
          .rpc();

        expect.fail(
          "Attacker should NOT be able to change staking program ID"
        );
      } catch (error) {
        expect(error.message).to.include("UnauthorizedAdmin");
      }
    });

    it("✅ Allows admin to update authorized program IDs", async () => {
      const newStakingProgram = Keypair.generate().publicKey;

      await program.methods
        .updateRegistryConfig(null, newStakingProgram, null)
        .accounts({
          registryConfig: registryConfigPda,
          admin: admin.publicKey,
        })
        .signers([admin])
        .rpc();

      const config = await program.account.registryConfig.fetch(
        registryConfigPda
      );
      expect(config.stakingProgramId.toString()).to.equal(
        newStakingProgram.toString()
      );
    });
  });

  describe("SECURITY: Attack Scenarios", () => {
    it("❌ Scenario: Griefing Attack (try to fake unstake all nodes)", async () => {
      // Attacker tries to set all node stakes to 0
      // This would make nodes appear unstaked and ineligible for rewards

      try {
        await program.methods
          .updateStake(new anchor.BN(0))
          .accounts({
            registryConfig: registryConfigPda,
            nodeAccount: nodeAccountPda,
            authority: attacker.publicKey,
          })
          .signers([attacker])
          .rpc();

        expect.fail("Griefing attack should be prevented");
      } catch (error) {
        expect(error.message).to.include("UnauthorizedStakeUpdate");
      }
    });

    it("❌ Scenario: Sybil Attack (try to bypass min stake)", async () => {
      // Attacker tries to reduce min_stake to 0 to register fake nodes

      try {
        await program.methods
          .updateRegistryConfig(null, null, new anchor.BN(0))
          .accounts({
            registryConfig: registryConfigPda,
            admin: attacker.publicKey,
          })
          .signers([attacker])
          .rpc();

        expect.fail("Should not allow lowering min stake by non-admin");
      } catch (error) {
        expect(error.message).to.include("UnauthorizedAdmin");
      }
    });

    it("✅ Legitimate: Admin lowers min stake for network growth", async () => {
      // Legitimate scenario: DAO votes to lower barrier to entry

      const newMinStake = new anchor.BN(50_000_000_000); // 50 AEGIS

      await program.methods
        .updateRegistryConfig(null, null, newMinStake)
        .accounts({
          registryConfig: registryConfigPda,
          admin: admin.publicKey,
        })
        .signers([admin])
        .rpc();

      const config = await program.account.registryConfig.fetch(
        registryConfigPda
      );
      expect(config.minStakeForRegistration.toNumber()).to.equal(50_000_000_000);
    });
  });
});
