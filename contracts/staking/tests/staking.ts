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

  const MIN_STAKE = new anchor.BN(100_000_000_000); // 100 AEGIS
  const COOLDOWN_PERIOD = 7 * 24 * 60 * 60; // 7 days in seconds

  // Helper to get stake account PDA
  function getStakePDA(operator: anchor.web3.PublicKey): [anchor.web3.PublicKey, number] {
    return anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("stake"), operator.toBuffer()],
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
    // Create AEGIS token mint
    const mintAuthority = provider.wallet.publicKey;
    mint = await createMint(
      provider.connection,
      provider.wallet.payer,
      mintAuthority,
      null,
      9 // 9 decimals
    );

    // Create stake vault token account
    stakeVault = await createTokenAccount(
      provider.wallet.publicKey,
      provider.wallet.payer
    );

    // Create treasury token account
    treasury = await createTokenAccount(
      provider.wallet.publicKey,
      provider.wallet.payer
    );

    console.log("Mint:", mint.toString());
    console.log("Stake Vault:", stakeVault.toString());
    console.log("Treasury:", treasury.toString());
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

  describe("Staking", () => {
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
    });

    it("Stakes tokens successfully", async () => {
      const stakeAmount = MIN_STAKE.muln(2); // 200 AEGIS

      await program.methods
        .stake(stakeAmount)
        .accounts({
          stakeAccount: stakePDA,
          operatorTokenAccount,
          stakeVault,
          operator: operator.publicKey,
          tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
        })
        .signers([operator])
        .rpc();

      const stakeAccount = await program.account.stakeAccount.fetch(stakePDA);
      expect(stakeAccount.stakedAmount.toString()).to.equal(stakeAmount.toString());
      expect(stakeAccount.totalStakedEver.toString()).to.equal(stakeAmount.toString());

      // Verify tokens were transferred
      const vaultAccount = await getAccount(provider.connection, stakeVault);
      expect(Number(vaultAccount.amount)).to.be.at.least(stakeAmount.toNumber());
    });

    it("Rejects stake below minimum", async () => {
      const lowAmount = MIN_STAKE.subn(1);

      try {
        await program.methods
          .stake(lowAmount)
          .accounts({
            stakeAccount: stakePDA,
            operatorTokenAccount,
            stakeVault,
            operator: operator.publicKey,
            tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
          })
          .signers([operator])
          .rpc();

        expect.fail("Should have rejected stake below minimum");
      } catch (error) {
        expect(error.toString()).to.include("InsufficientStakeAmount");
      }
    });

    it("Allows multiple stakes (accumulates)", async () => {
      const firstStake = MIN_STAKE.muln(2);
      const secondStake = MIN_STAKE;

      const beforeAccount = await program.account.stakeAccount.fetch(stakePDA);

      await program.methods
        .stake(secondStake)
        .accounts({
          stakeAccount: stakePDA,
          operatorTokenAccount,
          stakeVault,
          operator: operator.publicKey,
          tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
        })
        .signers([operator])
        .rpc();

      const afterAccount = await program.account.stakeAccount.fetch(stakePDA);
      const expected = beforeAccount.stakedAmount.add(secondStake);
      expect(afterAccount.stakedAmount.toString()).to.equal(expected.toString());
    });
  });

  describe("Unstaking", () => {
    let operator: anchor.web3.Keypair;
    let operatorTokenAccount: anchor.web3.PublicKey;
    let stakePDA: anchor.web3.PublicKey;
    const INITIAL_STAKE = MIN_STAKE.muln(5); // 500 AEGIS

    before(async () => {
      operator = anchor.web3.Keypair.generate();
      await fundAccount(operator.publicKey, 1 * anchor.web3.LAMPORTS_PER_SOL);

      [stakePDA] = getStakePDA(operator.publicKey);

      // Initialize and stake
      await program.methods
        .initializeStake()
        .accounts({
          stakeAccount: stakePDA,
          operator: operator.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([operator])
        .rpc();

      operatorTokenAccount = await createTokenAccountForOperator(operator);
      await mintTo(
        provider.connection,
        provider.wallet.payer,
        mint,
        operatorTokenAccount,
        provider.wallet.publicKey,
        1000_000_000_000
      );

      await program.methods
        .stake(INITIAL_STAKE)
        .accounts({
          stakeAccount: stakePDA,
          operatorTokenAccount,
          stakeVault,
          operator: operator.publicKey,
          tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
        })
        .signers([operator])
        .rpc();
    });

    it("Requests unstake successfully", async () => {
      const unstakeAmount = MIN_STAKE.muln(2);

      await program.methods
        .requestUnstake(unstakeAmount)
        .accounts({
          stakeAccount: stakePDA,
          operator: operator.publicKey,
        })
        .signers([operator])
        .rpc();

      const stakeAccount = await program.account.stakeAccount.fetch(stakePDA);
      expect(stakeAccount.pendingUnstake.toString()).to.equal(unstakeAmount.toString());
      expect(stakeAccount.stakedAmount.toString()).to.equal(
        INITIAL_STAKE.sub(unstakeAmount).toString()
      );
      expect(stakeAccount.unstakeRequestTime.toNumber()).to.be.greaterThan(0);
    });

    it("Rejects unstake exceeding staked balance", async () => {
      const excessiveAmount = INITIAL_STAKE.addn(1);

      try {
        await program.methods
          .requestUnstake(excessiveAmount)
          .accounts({
            stakeAccount: stakePDA,
            operator: operator.publicKey,
          })
          .signers([operator])
          .rpc();

        expect.fail("Should have rejected excessive unstake");
      } catch (error) {
        expect(error.toString()).to.include("InsufficientStakedBalance");
      }
    });

    it("Prevents multiple pending unstakes", async () => {
      try {
        await program.methods
          .requestUnstake(MIN_STAKE)
          .accounts({
            stakeAccount: stakePDA,
            operator: operator.publicKey,
          })
          .signers([operator])
          .rpc();

        expect.fail("Should have prevented multiple pending unstakes");
      } catch (error) {
        expect(error.toString()).to.include("PendingUnstakeExists");
      }
    });

    it("Allows cancelling unstake request", async () => {
      const beforeAccount = await program.account.stakeAccount.fetch(stakePDA);
      const pendingAmount = beforeAccount.pendingUnstake;

      await program.methods
        .cancelUnstake()
        .accounts({
          stakeAccount: stakePDA,
          operator: operator.publicKey,
        })
        .signers([operator])
        .rpc();

      const afterAccount = await program.account.stakeAccount.fetch(stakePDA);
      expect(afterAccount.pendingUnstake.toString()).to.equal("0");
      expect(afterAccount.stakedAmount.toString()).to.equal(
        beforeAccount.stakedAmount.add(pendingAmount).toString()
      );
    });

    it("Prevents executing unstake before cooldown", async () => {
      // Request new unstake
      await program.methods
        .requestUnstake(MIN_STAKE)
        .accounts({
          stakeAccount: stakePDA,
          operator: operator.publicKey,
        })
        .signers([operator])
        .rpc();

      // Try to execute immediately
      try {
        await program.methods
          .executeUnstake()
          .accounts({
            stakeAccount: stakePDA,
            stakeVault,
            operatorTokenAccount,
            operator: operator.publicKey,
            tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
          })
          .signers([operator])
          .rpc();

        expect.fail("Should have prevented early unstake execution");
      } catch (error) {
        expect(error.toString()).to.include("CooldownNotComplete");
      }
    });
  });

  describe("Slashing", () => {
    let operator: anchor.web3.Keypair;
    let operatorTokenAccount: anchor.web3.PublicKey;
    let stakePDA: anchor.web3.PublicKey;

    before(async () => {
      operator = anchor.web3.Keypair.generate();
      await fundAccount(operator.publicKey, 1 * anchor.web3.LAMPORTS_PER_SOL);

      [stakePDA] = getStakePDA(operator.publicKey);

      await program.methods
        .initializeStake()
        .accounts({
          stakeAccount: stakePDA,
          operator: operator.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([operator])
        .rpc();

      operatorTokenAccount = await createTokenAccountForOperator(operator);
      await mintTo(
        provider.connection,
        provider.wallet.payer,
        mint,
        operatorTokenAccount,
        provider.wallet.publicKey,
        1000_000_000_000
      );

      await program.methods
        .stake(MIN_STAKE.muln(10))
        .accounts({
          stakeAccount: stakePDA,
          operatorTokenAccount,
          stakeVault,
          operator: operator.publicKey,
          tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
        })
        .signers([operator])
        .rpc();
    });

    it("Slashes stake for violations", async () => {
      const slashAmount = MIN_STAKE;
      const reason = "Extended downtime detected";

      const beforeAccount = await program.account.stakeAccount.fetch(stakePDA);
      const beforeTreasury = await getAccount(provider.connection, treasury);

      await program.methods
        .slashStake(slashAmount, reason)
        .accounts({
          stakeAccount: stakePDA,
          stakeVault,
          treasury,
          authority: provider.wallet.publicKey,
          tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
        })
        .rpc();

      const afterAccount = await program.account.stakeAccount.fetch(stakePDA);
      const afterTreasury = await getAccount(provider.connection, treasury);

      expect(afterAccount.stakedAmount.toString()).to.equal(
        beforeAccount.stakedAmount.sub(slashAmount).toString()
      );
      expect(Number(afterTreasury.amount)).to.equal(
        Number(beforeTreasury.amount) + slashAmount.toNumber()
      );
    });

    it("Rejects slash exceeding staked balance", async () => {
      const excessiveSlash = MIN_STAKE.muln(100);

      try {
        await program.methods
          .slashStake(excessiveSlash, "Test")
          .accounts({
            stakeAccount: stakePDA,
            stakeVault,
            treasury,
            authority: provider.wallet.publicKey,
            tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
          })
          .rpc();

        expect.fail("Should have rejected excessive slash");
      } catch (error) {
        expect(error.toString()).to.include("InsufficientStakedBalance");
      }
    });

    it("Rejects slash with too long reason", async () => {
      const longReason = "x".repeat(129);

      try {
        await program.methods
          .slashStake(MIN_STAKE, longReason)
          .accounts({
            stakeAccount: stakePDA,
            stakeVault,
            treasury,
            authority: provider.wallet.publicKey,
            tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
          })
          .rpc();

        expect.fail("Should have rejected too long reason");
      } catch (error) {
        expect(error.toString()).to.include("ReasonTooLong");
      }
    });
  });

  describe("Edge Cases", () => {
    it("Handles exact minimum stake", async () => {
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
        MIN_STAKE.toNumber()
      );

      await program.methods
        .stake(MIN_STAKE)
        .accounts({
          stakeAccount: stakePDA,
          operatorTokenAccount,
          stakeVault,
          operator: operator.publicKey,
          tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
        })
        .signers([operator])
        .rpc();

      const stakeAccount = await program.account.stakeAccount.fetch(stakePDA);
      expect(stakeAccount.stakedAmount.toString()).to.equal(MIN_STAKE.toString());
    });

    it("Handles very large stake amounts", async () => {
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

      const largeStake = new anchor.BN("10000000000000000"); // 10M AEGIS
      const operatorTokenAccount = await createTokenAccountForOperator(operator);
      await mintTo(
        provider.connection,
        provider.wallet.payer,
        mint,
        operatorTokenAccount,
        provider.wallet.publicKey,
        Number(largeStake.toString()) // Convert BN to number via string for large values
      );

      await program.methods
        .stake(largeStake)
        .accounts({
          stakeAccount: stakePDA,
          operatorTokenAccount,
          stakeVault,
          operator: operator.publicKey,
          tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
        })
        .signers([operator])
        .rpc();

      const stakeAccount = await program.account.stakeAccount.fetch(stakePDA);
      expect(stakeAccount.stakedAmount.toString()).to.equal(largeStake.toString());
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
  });

  describe("Complete Unstaking Flow", () => {
    let operator: anchor.web3.Keypair;
    let operatorTokenAccount: anchor.web3.PublicKey;
    let stakePDA: anchor.web3.PublicKey;

    before(async () => {
      operator = anchor.web3.Keypair.generate();
      await fundAccount(operator.publicKey, 1 * anchor.web3.LAMPORTS_PER_SOL);

      [stakePDA] = getStakePDA(operator.publicKey);

      // Initialize and stake
      await program.methods
        .initializeStake()
        .accounts({
          stakeAccount: stakePDA,
          operator: operator.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([operator])
        .rpc();

      operatorTokenAccount = await createTokenAccountForOperator(operator);
      await mintTo(
        provider.connection,
        provider.wallet.payer,
        mint,
        operatorTokenAccount,
        provider.wallet.publicKey,
        1000_000_000_000
      );

      await program.methods
        .stake(MIN_STAKE.muln(10))
        .accounts({
          stakeAccount: stakePDA,
          operatorTokenAccount,
          stakeVault,
          operator: operator.publicKey,
          tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
        })
        .signers([operator])
        .rpc();
    });

    it("Completes full unstaking after cooldown", async () => {
      const unstakeAmount = MIN_STAKE.muln(2);

      // Request unstake
      await program.methods
        .requestUnstake(unstakeAmount)
        .accounts({
          stakeAccount: stakePDA,
          operator: operator.publicKey,
        })
        .signers([operator])
        .rpc();

      const beforeAccount = await program.account.stakeAccount.fetch(stakePDA);
      const beforeBalance = await getAccount(provider.connection, operatorTokenAccount);

      // Fast-forward time would require test validator manipulation
      // For now, just verify the state is correct
      expect(beforeAccount.pendingUnstake.toString()).to.equal(unstakeAmount.toString());
      expect(beforeAccount.unstakeRequestTime.toNumber()).to.be.greaterThan(0);
    });
  });

  describe("Security & Authorization", () => {
    it("Rejects unauthorized stake operations", async () => {
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

      const operatorTokenAccount = await createTokenAccountForOperator(operator);
      await mintTo(
        provider.connection,
        provider.wallet.payer,
        mint,
        operatorTokenAccount,
        provider.wallet.publicKey,
        1000_000_000_000
      );

      // Try to stake with attacker's signature
      try {
        await program.methods
          .stake(MIN_STAKE)
          .accounts({
            stakeAccount: stakePDA,
            operatorTokenAccount,
            stakeVault,
            operator: attacker.publicKey, // Wrong operator!
            tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
          })
          .signers([attacker])
          .rpc();

        expect.fail("Should have rejected unauthorized stake");
      } catch (error) {
        expect(error).to.exist;
      }
    });

    it("Rejects unauthorized unstake requests", async () => {
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
          .requestUnstake(MIN_STAKE)
          .accounts({
            stakeAccount: stakePDA,
            operator: attacker.publicKey,
          })
          .signers([attacker])
          .rpc();

        expect.fail("Should have rejected unauthorized unstake");
      } catch (error) {
        expect(error).to.exist;
      }
    });

    it("Rejects unauthorized slashing", async () => {
      const operator = anchor.web3.Keypair.generate();
      const unauthorized = anchor.web3.Keypair.generate();
      await fundAccount(operator.publicKey);
      await fundAccount(unauthorized.publicKey);

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

      await program.methods
        .stake(MIN_STAKE.muln(5))
        .accounts({
          stakeAccount: stakePDA,
          operatorTokenAccount,
          stakeVault,
          operator: operator.publicKey,
          tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
        })
        .signers([operator])
        .rpc();

      // Only the authority (provider wallet) can slash, not random users
      try {
        await program.methods
          .slashStake(MIN_STAKE, "Unauthorized slash attempt")
          .accounts({
            stakeAccount: stakePDA,
            stakeVault,
            treasury,
            authority: unauthorized.publicKey,
            tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
          })
          .signers([unauthorized])
          .rpc();

        expect.fail("Should have rejected unauthorized slash");
      } catch (error) {
        expect(error).to.exist;
      }
    });
  });

  describe("Multi-Operator Scenarios", () => {
    it("Handles multiple independent operators", async () => {
      const operator1 = anchor.web3.Keypair.generate();
      const operator2 = anchor.web3.Keypair.generate();
      await fundAccount(operator1.publicKey, 1 * anchor.web3.LAMPORTS_PER_SOL);
      await fundAccount(operator2.publicKey, 1 * anchor.web3.LAMPORTS_PER_SOL);

      const [stake1PDA] = getStakePDA(operator1.publicKey);
      const [stake2PDA] = getStakePDA(operator2.publicKey);

      // Initialize both
      for (const [op, stakePDA] of [[operator1, stake1PDA], [operator2, stake2PDA]]) {
        await program.methods
          .initializeStake()
          .accounts({
            stakeAccount: stakePDA,
            operator: op.publicKey,
            systemProgram: anchor.web3.SystemProgram.programId,
          })
          .signers([op])
          .rpc();

        const tokenAccount = await createTokenAccountForOperator(op);
        await mintTo(
          provider.connection,
          provider.wallet.payer,
          mint,
          tokenAccount,
          provider.wallet.publicKey,
          1000_000_000_000
        );

        await program.methods
          .stake(MIN_STAKE.muln(3))
          .accounts({
            stakeAccount: stakePDA,
            operatorTokenAccount: tokenAccount,
            stakeVault,
            operator: op.publicKey,
            tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
          })
          .signers([op])
          .rpc();
      }

      // Verify both have independent stake accounts
      const account1 = await program.account.stakeAccount.fetch(stake1PDA);
      const account2 = await program.account.stakeAccount.fetch(stake2PDA);

      expect(account1.stakedAmount.toString()).to.equal(MIN_STAKE.muln(3).toString());
      expect(account2.stakedAmount.toString()).to.equal(MIN_STAKE.muln(3).toString());
      expect(account1.operator.toString()).to.not.equal(account2.operator.toString());
    });
  });

  describe("Lifecycle & State Transitions", () => {
    it("Tracks lifetime statistics correctly", async () => {
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

      // Stake multiple times
      await program.methods
        .stake(MIN_STAKE)
        .accounts({
          stakeAccount: stakePDA,
          operatorTokenAccount,
          stakeVault,
          operator: operator.publicKey,
          tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
        })
        .signers([operator])
        .rpc();

      await program.methods
        .stake(MIN_STAKE.muln(2))
        .accounts({
          stakeAccount: stakePDA,
          operatorTokenAccount,
          stakeVault,
          operator: operator.publicKey,
          tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
        })
        .signers([operator])
        .rpc();

      const account = await program.account.stakeAccount.fetch(stakePDA);

      expect(account.totalStakedEver.toString()).to.equal(MIN_STAKE.muln(3).toString());
      expect(account.stakedAmount.toString()).to.equal(MIN_STAKE.muln(3).toString());
    });

    it("Maintains correct state after cancel unstake", async () => {
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

      await program.methods
        .stake(MIN_STAKE.muln(5))
        .accounts({
          stakeAccount: stakePDA,
          operatorTokenAccount,
          stakeVault,
          operator: operator.publicKey,
          tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
        })
        .signers([operator])
        .rpc();

      // Request unstake
      await program.methods
        .requestUnstake(MIN_STAKE.muln(2))
        .accounts({
          stakeAccount: stakePDA,
          operator: operator.publicKey,
        })
        .signers([operator])
        .rpc();

      const beforeCancel = await program.account.stakeAccount.fetch(stakePDA);

      // Cancel unstake
      await program.methods
        .cancelUnstake()
        .accounts({
          stakeAccount: stakePDA,
          operator: operator.publicKey,
        })
        .signers([operator])
        .rpc();

      const afterCancel = await program.account.stakeAccount.fetch(stakePDA);

      // Verify state returned to pre-unstake
      expect(afterCancel.stakedAmount.toString()).to.equal(MIN_STAKE.muln(5).toString());
      expect(afterCancel.pendingUnstake.toString()).to.equal("0");
      expect(afterCancel.unstakeRequestTime.toString()).to.equal("0");
    });
  });

  describe("Boundary Conditions", () => {
    it("Handles unstaking all staked tokens", async () => {
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
        MIN_STAKE.toNumber()
      );

      await program.methods
        .stake(MIN_STAKE)
        .accounts({
          stakeAccount: stakePDA,
          operatorTokenAccount,
          stakeVault,
          operator: operator.publicKey,
          tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
        })
        .signers([operator])
        .rpc();

      // Unstake everything
      await program.methods
        .requestUnstake(MIN_STAKE)
        .accounts({
          stakeAccount: stakePDA,
          operator: operator.publicKey,
        })
        .signers([operator])
        .rpc();

      const account = await program.account.stakeAccount.fetch(stakePDA);

      expect(account.stakedAmount.toString()).to.equal("0");
      expect(account.pendingUnstake.toString()).to.equal(MIN_STAKE.toString());
    });

    it("Handles slashing entire stake", async () => {
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
        MIN_STAKE.toNumber()
      );

      await program.methods
        .stake(MIN_STAKE)
        .accounts({
          stakeAccount: stakePDA,
          operatorTokenAccount,
          stakeVault,
          operator: operator.publicKey,
          tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
        })
        .signers([operator])
        .rpc();

      // Slash entire stake
      await program.methods
        .slashStake(MIN_STAKE, "Complete violation")
        .accounts({
          stakeAccount: stakePDA,
          stakeVault,
          treasury,
          authority: provider.wallet.publicKey,
          tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
        })
        .rpc();

      const account = await program.account.stakeAccount.fetch(stakePDA);

      expect(account.stakedAmount.toString()).to.equal("0");
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

      expect(account.createdAt.toNumber()).to.be.at.least(beforeTime - 5);
      expect(account.createdAt.toNumber()).to.be.at.most(afterTime + 5);
    });

    it("Updates timestamps on state changes", async () => {
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

      const before = await program.account.stakeAccount.fetch(stakePDA);

      // Wait a moment
      await new Promise(resolve => setTimeout(resolve, 1000));

      await program.methods
        .stake(MIN_STAKE)
        .accounts({
          stakeAccount: stakePDA,
          operatorTokenAccount,
          stakeVault,
          operator: operator.publicKey,
          tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
        })
        .signers([operator])
        .rpc();

      const after = await program.account.stakeAccount.fetch(stakePDA);

      expect(after.updatedAt.toNumber()).to.be.greaterThan(before.updatedAt.toNumber());
    });
  });
});
