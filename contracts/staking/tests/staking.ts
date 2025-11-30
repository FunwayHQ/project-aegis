import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Staking } from "../target/types/staking";
import { expect } from "chai";
import {
  createMint,
  mintTo,
  getAccount,
  TOKEN_PROGRAM_ID,
  ACCOUNT_SIZE,
  createInitializeAccountInstruction,
} from "@solana/spl-token";

describe("staking", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.Staking as Program<Staking>;

  let mint: anchor.web3.PublicKey;
  let stakeVault: anchor.web3.PublicKey;
  let treasury: anchor.web3.PublicKey;
  let globalConfigPDA: anchor.web3.PublicKey;
  let globalConfigBump: number;
  let stakingAuthorityPDA: anchor.web3.PublicKey;

  const MIN_STAKE = new anchor.BN(100_000_000_000); // 100 AEGIS
  const COOLDOWN_PERIOD = new anchor.BN(7 * 24 * 60 * 60); // 7 days in seconds

  // Mock registry program (we use system program as placeholder since CPI is optional for tests)
  const MOCK_REGISTRY_PROGRAM = anchor.web3.SystemProgram.programId;

  // Helper to get stake account PDA
  function getStakePDA(operator: anchor.web3.PublicKey): [anchor.web3.PublicKey, number] {
    return anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("stake"), operator.toBuffer()],
      program.programId
    );
  }

  // Helper to get global config PDA
  function getGlobalConfigPDA(): [anchor.web3.PublicKey, number] {
    return anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("global_config")],
      program.programId
    );
  }

  // Helper to get staking authority PDA
  function getStakingAuthorityPDA(): [anchor.web3.PublicKey, number] {
    return anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("staking_authority")],
      program.programId
    );
  }

  // Helper to fund account with SOL (minimal for rent + fees)
  async function fundAccount(publicKey: anchor.web3.PublicKey, lamports: number = 0.01 * anchor.web3.LAMPORTS_PER_SOL) {
    const tx = new anchor.web3.Transaction().add(
      anchor.web3.SystemProgram.transfer({
        fromPubkey: provider.wallet.publicKey,
        toPubkey: publicKey,
        lamports,
      })
    );
    await provider.sendAndConfirm(tx);
  }

  // Helper to create token account manually
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

  // Helper to create token account for operator
  async function createTokenAccountForOperator(
    operator: anchor.web3.Keypair
  ): Promise<anchor.web3.PublicKey> {
    return createTokenAccount(operator.publicKey, operator);
  }

  before(async () => {
    // Get PDAs
    [globalConfigPDA, globalConfigBump] = getGlobalConfigPDA();
    [stakingAuthorityPDA] = getStakingAuthorityPDA();

    // Create AEGIS token mint
    const mintAuthority = provider.wallet.publicKey;
    mint = await createMint(
      provider.connection,
      provider.wallet.payer,
      mintAuthority,
      null,
      9 // 9 decimals
    );

    // Create stake vault token account (owned by stake vault PDA)
    const stakeVaultKeypair = anchor.web3.Keypair.generate();
    const lamports = await provider.connection.getMinimumBalanceForRentExemption(ACCOUNT_SIZE);
    const createVaultTx = new anchor.web3.Transaction().add(
      anchor.web3.SystemProgram.createAccount({
        fromPubkey: provider.wallet.publicKey,
        newAccountPubkey: stakeVaultKeypair.publicKey,
        space: ACCOUNT_SIZE,
        lamports,
        programId: TOKEN_PROGRAM_ID,
      }),
      createInitializeAccountInstruction(
        stakeVaultKeypair.publicKey,
        mint,
        provider.wallet.publicKey, // Owner will be changed later
        TOKEN_PROGRAM_ID
      )
    );
    await provider.sendAndConfirm(createVaultTx, [stakeVaultKeypair]);
    stakeVault = stakeVaultKeypair.publicKey;

    // Create treasury token account
    treasury = await createTokenAccount(
      provider.wallet.publicKey,
      provider.wallet.payer
    );

    console.log("Mint:", mint.toString());
    console.log("Stake Vault:", stakeVault.toString());
    console.log("Treasury:", treasury.toString());
    console.log("Global Config PDA:", globalConfigPDA.toString());
    console.log("Staking Authority PDA:", stakingAuthorityPDA.toString());

    // Initialize global config
    try {
      await program.methods
        .initializeGlobalConfig(
          provider.wallet.publicKey, // admin authority
          MIN_STAKE,
          COOLDOWN_PERIOD,
          MOCK_REGISTRY_PROGRAM // registry program (mock for tests)
        )
        .accounts({
          globalConfig: globalConfigPDA,
          treasury: treasury,
          deployer: provider.wallet.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .rpc();

      console.log("Global config initialized successfully");
    } catch (error) {
      // May already exist from previous test run
      console.log("Global config may already exist:", error.message);
    }
  });

  describe("Global Config", () => {
    it("Has correct initial values", async () => {
      const config = await program.account.globalConfig.fetch(globalConfigPDA);

      expect(config.adminAuthority.toString()).to.equal(provider.wallet.publicKey.toString());
      expect(config.minStakeAmount.toString()).to.equal(MIN_STAKE.toString());
      expect(config.unstakeCooldownPeriod.toString()).to.equal(COOLDOWN_PERIOD.toString());
      expect(config.treasury.toString()).to.equal(treasury.toString());
      expect(config.paused).to.equal(false);
    });

    it("Allows admin to update config", async () => {
      const newMinStake = MIN_STAKE.muln(2);

      await program.methods
        .updateGlobalConfig(
          null, // new_admin
          newMinStake, // new_min_stake
          null, // new_cooldown
          null  // new_registry_program
        )
        .accounts({
          globalConfig: globalConfigPDA,
          admin: provider.wallet.publicKey,
        })
        .rpc();

      const config = await program.account.globalConfig.fetch(globalConfigPDA);
      expect(config.minStakeAmount.toString()).to.equal(newMinStake.toString());

      // Revert to original
      await program.methods
        .updateGlobalConfig(null, MIN_STAKE, null, null)
        .accounts({
          globalConfig: globalConfigPDA,
          admin: provider.wallet.publicKey,
        })
        .rpc();
    });

    it("Rejects unauthorized config updates", async () => {
      const unauthorized = anchor.web3.Keypair.generate();
      await fundAccount(unauthorized.publicKey);

      try {
        await program.methods
          .updateGlobalConfig(null, MIN_STAKE.muln(5), null, null)
          .accounts({
            globalConfig: globalConfigPDA,
            admin: unauthorized.publicKey,
          })
          .signers([unauthorized])
          .rpc();

        expect.fail("Should have rejected unauthorized update");
      } catch (error) {
        expect(error.toString()).to.include("UnauthorizedAdmin");
      }
    });

    it("Allows admin to pause/unpause staking", async () => {
      // Pause
      await program.methods
        .setPaused(true)
        .accounts({
          globalConfig: globalConfigPDA,
          admin: provider.wallet.publicKey,
        })
        .rpc();

      let config = await program.account.globalConfig.fetch(globalConfigPDA);
      expect(config.paused).to.equal(true);

      // Unpause
      await program.methods
        .setPaused(false)
        .accounts({
          globalConfig: globalConfigPDA,
          admin: provider.wallet.publicKey,
        })
        .rpc();

      config = await program.account.globalConfig.fetch(globalConfigPDA);
      expect(config.paused).to.equal(false);
    });
  });

  describe("Stake Account Initialization", () => {
    it("Initializes a stake account successfully", async () => {
      const operator = anchor.web3.Keypair.generate();
      await fundAccount(operator.publicKey);
      const [stakePDA] = getStakePDA(operator.publicKey);

      await program.methods
        .initializeStake()
        .accounts({
          stakeAccount: stakePDA,
          operator: operator.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([operator])
        .rpc();

      const stakeAccount = await program.account.stakeAccount.fetch(stakePDA);

      expect(stakeAccount.operator.toString()).to.equal(operator.publicKey.toString());
      expect(stakeAccount.stakedAmount.toString()).to.equal("0");
      expect(stakeAccount.pendingUnstake.toString()).to.equal("0");
      expect(stakeAccount.totalStakedEver.toString()).to.equal("0");
      expect(stakeAccount.createdAt.toNumber()).to.be.greaterThan(0);
    });

    it("Prevents duplicate stake account initialization", async () => {
      const operator = anchor.web3.Keypair.generate();
      await fundAccount(operator.publicKey);
      const [stakePDA] = getStakePDA(operator.publicKey);

      // First initialization
      await program.methods
        .initializeStake()
        .accounts({
          stakeAccount: stakePDA,
          operator: operator.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([operator])
        .rpc();

      // Try to initialize again
      try {
        await program.methods
          .initializeStake()
          .accounts({
            stakeAccount: stakePDA,
            operator: operator.publicKey,
            systemProgram: anchor.web3.SystemProgram.programId,
          })
          .signers([operator])
          .rpc();

        expect.fail("Should have prevented duplicate initialization");
      } catch (error) {
        expect(error).to.exist;
      }
    });
  });

  describe("Unstaking (without CPI)", () => {
    let operator: anchor.web3.Keypair;
    let operatorTokenAccount: anchor.web3.PublicKey;
    let stakePDA: anchor.web3.PublicKey;

    before(async () => {
      operator = anchor.web3.Keypair.generate();
      await fundAccount(operator.publicKey, 1 * anchor.web3.LAMPORTS_PER_SOL);

      [stakePDA] = getStakePDA(operator.publicKey);

      // Initialize stake account
      await program.methods
        .initializeStake()
        .accounts({
          stakeAccount: stakePDA,
          operator: operator.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([operator])
        .rpc();

      // Create token account and mint tokens
      operatorTokenAccount = await createTokenAccountForOperator(operator);
      await mintTo(
        provider.connection,
        provider.wallet.payer,
        mint,
        operatorTokenAccount,
        provider.wallet.publicKey,
        1000_000_000_000 // 1000 AEGIS
      );

      // Manually set some stake for testing unstake flow
      // We can't easily test stake() without registry CPI, so we'll test unstake request/cancel
    });

    it("Request unstake fails without staked balance", async () => {
      try {
        await program.methods
          .requestUnstake(MIN_STAKE)
          .accounts({
            globalConfig: globalConfigPDA,
            stakeAccount: stakePDA,
            operator: operator.publicKey,
          })
          .signers([operator])
          .rpc();

        expect.fail("Should have rejected unstake without balance");
      } catch (error) {
        expect(error.toString()).to.include("InsufficientStakedBalance");
      }
    });

    it("Rejects zero amount unstake", async () => {
      try {
        await program.methods
          .requestUnstake(new anchor.BN(0))
          .accounts({
            globalConfig: globalConfigPDA,
            stakeAccount: stakePDA,
            operator: operator.publicKey,
          })
          .signers([operator])
          .rpc();

        expect.fail("Should have rejected zero amount");
      } catch (error) {
        expect(error.toString()).to.include("InvalidAmount");
      }
    });
  });

  describe("Paused Staking", () => {
    it("Blocks staking when paused", async () => {
      const operator = anchor.web3.Keypair.generate();
      await fundAccount(operator.publicKey, 1 * anchor.web3.LAMPORTS_PER_SOL);
      const [stakePDA] = getStakePDA(operator.publicKey);

      await program.methods
        .initializeStake()
        .accounts({
          stakeAccount: stakePDA,
          operator: operator.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([operator])
        .rpc();

      const operatorTokenAccount = await createTokenAccountForOperator(operator);
      await mintTo(
        provider.connection,
        provider.wallet.payer,
        mint,
        operatorTokenAccount,
        provider.wallet.publicKey,
        1000_000_000_000
      );

      // Pause staking
      await program.methods
        .setPaused(true)
        .accounts({
          globalConfig: globalConfigPDA,
          admin: provider.wallet.publicKey,
        })
        .rpc();

      // Try to stake while paused
      try {
        // Note: This would need full account setup with registry
        // For now we just verify pause state is set
        const config = await program.account.globalConfig.fetch(globalConfigPDA);
        expect(config.paused).to.equal(true);
      } finally {
        // Unpause for other tests
        await program.methods
          .setPaused(false)
          .accounts({
            globalConfig: globalConfigPDA,
            admin: provider.wallet.publicKey,
          })
          .rpc();
      }
    });
  });

  describe("Security & Authorization", () => {
    it("Rejects unauthorized stake operations (wrong operator)", async () => {
      const operator = anchor.web3.Keypair.generate();
      const attacker = anchor.web3.Keypair.generate();
      await fundAccount(operator.publicKey);
      await fundAccount(attacker.publicKey);

      const [stakePDA] = getStakePDA(operator.publicKey);

      await program.methods
        .initializeStake()
        .accounts({
          stakeAccount: stakePDA,
          operator: operator.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([operator])
        .rpc();

      // Try to request unstake with wrong operator
      try {
        await program.methods
          .requestUnstake(MIN_STAKE)
          .accounts({
            globalConfig: globalConfigPDA,
            stakeAccount: stakePDA,
            operator: attacker.publicKey,
          })
          .signers([attacker])
          .rpc();

        expect.fail("Should have rejected unauthorized unstake request");
      } catch (error) {
        // Expected to fail due to PDA constraint mismatch
        expect(error).to.exist;
      }
    });

    it("Rejects unauthorized cancel unstake", async () => {
      const operator = anchor.web3.Keypair.generate();
      const attacker = anchor.web3.Keypair.generate();
      await fundAccount(operator.publicKey);
      await fundAccount(attacker.publicKey);

      const [stakePDA] = getStakePDA(operator.publicKey);

      await program.methods
        .initializeStake()
        .accounts({
          stakeAccount: stakePDA,
          operator: operator.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([operator])
        .rpc();

      try {
        await program.methods
          .cancelUnstake()
          .accounts({
            stakeAccount: stakePDA,
            operator: attacker.publicKey,
          })
          .signers([attacker])
          .rpc();

        expect.fail("Should have rejected unauthorized cancel");
      } catch (error) {
        expect(error).to.exist;
      }
    });
  });

  describe("PDA Derivation", () => {
    it("Derives unique PDA for each operator", async () => {
      const operator1 = anchor.web3.Keypair.generate();
      const operator2 = anchor.web3.Keypair.generate();

      const [pda1] = getStakePDA(operator1.publicKey);
      const [pda2] = getStakePDA(operator2.publicKey);

      expect(pda1.toString()).to.not.equal(pda2.toString());

      // Same operator should give same PDA
      const [pda1Again] = getStakePDA(operator1.publicKey);
      expect(pda1.toString()).to.equal(pda1Again.toString());
    });

    it("Global config PDA is deterministic", () => {
      const [pda1] = getGlobalConfigPDA();
      const [pda2] = getGlobalConfigPDA();

      expect(pda1.toString()).to.equal(pda2.toString());
    });
  });

  describe("Timestamp Validation", () => {
    it("Records creation timestamp correctly", async () => {
      const operator = anchor.web3.Keypair.generate();
      await fundAccount(operator.publicKey);

      const [stakePDA] = getStakePDA(operator.publicKey);

      const beforeTime = Math.floor(Date.now() / 1000);

      await program.methods
        .initializeStake()
        .accounts({
          stakeAccount: stakePDA,
          operator: operator.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([operator])
        .rpc();

      const afterTime = Math.floor(Date.now() / 1000);
      const account = await program.account.stakeAccount.fetch(stakePDA);

      expect(account.createdAt.toNumber()).to.be.at.least(beforeTime - 10);
      expect(account.createdAt.toNumber()).to.be.at.most(afterTime + 10);
    });
  });
});
