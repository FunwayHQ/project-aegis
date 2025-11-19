import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Staking } from "../target/types/staking";
import { expect } from "chai";
import {
  createMint,
  createAccount,
  mintTo,
  getAccount,
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

  // Helper to fund account with SOL
  async function fundAccount(publicKey: anchor.web3.PublicKey, lamports: number = 0.1 * anchor.web3.LAMPORTS_PER_SOL) {
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
  async function createTokenAccountForOperator(
    operator: anchor.web3.Keypair
  ): Promise<anchor.web3.PublicKey> {
    const tokenAccount = await createAccount(
      provider.connection,
      operator,
      mint,
      operator.publicKey
    );
    return tokenAccount;
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

    // Create stake vault (program-owned token account)
    stakeVault = await createAccount(
      provider.connection,
      provider.wallet.payer,
      mint,
      provider.wallet.publicKey // Will be transferred to PDA authority
    );

    // Create treasury
    treasury = await createAccount(
      provider.connection,
      provider.wallet.payer,
      mint,
      provider.wallet.publicKey
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
        largeStake.toNumber()
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
});
