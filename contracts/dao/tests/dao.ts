import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Dao } from "../target/types/dao";
import { expect } from "chai";
import {
  createMint,
  mintTo,
  getAccount,
  TOKEN_PROGRAM_ID,
  ACCOUNT_SIZE,
  createInitializeAccountInstruction,
} from "@solana/spl-token";

describe("dao", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.Dao as Program<Dao>;

  let governanceTokenMint: anchor.web3.PublicKey;
  let treasury: anchor.web3.PublicKey;
  let bondEscrow: anchor.web3.PublicKey;

  // DAO Config parameters
  const VOTING_PERIOD = 3 * 24 * 60 * 60; // 3 days in seconds
  const PROPOSAL_BOND = new anchor.BN(100_000_000_000); // 100 AEGIS tokens
  const QUORUM_PERCENTAGE = 10; // 10%
  const APPROVAL_THRESHOLD = 51; // 51%

  // Helper to get DAO config PDA
  function getDaoConfigPDA(): [anchor.web3.PublicKey, number] {
    return anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("dao_config")],
      program.programId
    );
  }

  // Helper to get proposal PDA
  function getProposalPDA(proposalId: anchor.BN): [anchor.web3.PublicKey, number] {
    return anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("proposal"), proposalId.toArrayLike(Buffer, "le", 8)],
      program.programId
    );
  }

  // Helper to get vote record PDA
  function getVoteRecordPDA(
    proposalId: anchor.BN,
    voter: anchor.web3.PublicKey
  ): [anchor.web3.PublicKey, number] {
    return anchor.web3.PublicKey.findProgramAddressSync(
      [
        Buffer.from("vote"),
        proposalId.toArrayLike(Buffer, "le", 8),
        voter.toBuffer(),
      ],
      program.programId
    );
  }

  // Helper to fund account with SOL
  async function fundAccount(
    publicKey: anchor.web3.PublicKey,
    lamports: number = 0.1 * anchor.web3.LAMPORTS_PER_SOL
  ) {
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

    const lamports = await provider.connection.getMinimumBalanceForRentExemption(
      ACCOUNT_SIZE
    );

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
        governanceTokenMint,
        owner,
        TOKEN_PROGRAM_ID
      )
    );

    await provider.sendAndConfirm(transaction, [payer, tokenAccount]);
    return tokenAccount.publicKey;
  }

  // Helper to create token account for a user
  async function createTokenAccountForUser(
    user: anchor.web3.Keypair
  ): Promise<anchor.web3.PublicKey> {
    return createTokenAccount(user.publicKey, user);
  }

  before(async () => {
    // Create governance token mint ($AEGIS)
    const mintAuthority = provider.wallet.publicKey;
    governanceTokenMint = await createMint(
      provider.connection,
      provider.wallet.payer,
      mintAuthority,
      null,
      9 // 9 decimals
    );

    // Create treasury token account (owned by DAO PDA)
    const [daoConfigPda] = getDaoConfigPDA();
    treasury = await createTokenAccount(daoConfigPda, provider.wallet.payer);

    // Create bond escrow token account (owned by DAO PDA)
    bondEscrow = await createTokenAccount(daoConfigPda, provider.wallet.payer);

    // Mint some initial tokens to treasury for testing withdrawals
    await mintTo(
      provider.connection,
      provider.wallet.payer,
      governanceTokenMint,
      treasury,
      provider.wallet.publicKey,
      1_000_000_000_000 // 1000 AEGIS tokens
    );
  });

  describe("DAO Initialization", () => {
    it("should initialize the DAO configuration", async () => {
      const [daoConfigPda] = getDaoConfigPDA();

      await program.methods
        .initializeDao(
          new anchor.BN(VOTING_PERIOD),
          PROPOSAL_BOND,
          QUORUM_PERCENTAGE,
          APPROVAL_THRESHOLD
        )
        .accounts({
          daoConfig: daoConfigPda,
          treasury: treasury,
          governanceTokenMint: governanceTokenMint,
          authority: provider.wallet.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .rpc();

      const daoConfig = await program.account.daoConfig.fetch(daoConfigPda);

      expect(daoConfig.authority.toString()).to.equal(
        provider.wallet.publicKey.toString()
      );
      expect(daoConfig.treasury.toString()).to.equal(treasury.toString());
      expect(daoConfig.votingPeriod.toNumber()).to.equal(VOTING_PERIOD);
      expect(daoConfig.proposalBond.toString()).to.equal(
        PROPOSAL_BOND.toString()
      );
      expect(daoConfig.quorumPercentage).to.equal(QUORUM_PERCENTAGE);
      expect(daoConfig.approvalThreshold).to.equal(APPROVAL_THRESHOLD);
      expect(daoConfig.proposalCount.toNumber()).to.equal(0);
      expect(daoConfig.paused).to.equal(false);
    });

    it("should fail to initialize DAO with invalid voting period", async () => {
      const [daoConfigPda] = getDaoConfigPDA();

      try {
        await program.methods
          .initializeDao(
            new anchor.BN(100), // Too short (< 1 day)
            PROPOSAL_BOND,
            QUORUM_PERCENTAGE,
            APPROVAL_THRESHOLD
          )
          .accounts({
            daoConfig: daoConfigPda,
            treasury: treasury,
            governanceTokenMint: governanceTokenMint,
            authority: provider.wallet.publicKey,
            systemProgram: anchor.web3.SystemProgram.programId,
          })
          .rpc();
        expect.fail("Should have thrown an error");
      } catch (error: any) {
        // Handle both direct error and simulation error formats
        const hasExpectedError = error.message.includes("InvalidVotingPeriod") ||
          error.message.includes("custom program error: 0x1775") || // InvalidVotingPeriod error code
          error.logs?.some((log: string) => log.includes("InvalidVotingPeriod"));
        expect(hasExpectedError, `Expected InvalidVotingPeriod error but got: ${error.message}`).to.be.true;
      }
    });
  });

  describe("DAO Configuration Updates", () => {
    it("should update voting period", async () => {
      const [daoConfigPda] = getDaoConfigPDA();
      const newVotingPeriod = new anchor.BN(5 * 24 * 60 * 60); // 5 days

      await program.methods
        .updateDaoConfig(newVotingPeriod, null, null, null)
        .accounts({
          daoConfig: daoConfigPda,
          authority: provider.wallet.publicKey,
        })
        .rpc();

      const daoConfig = await program.account.daoConfig.fetch(daoConfigPda);
      expect(daoConfig.votingPeriod.toNumber()).to.equal(
        newVotingPeriod.toNumber()
      );
    });

    it("should pause and unpause the DAO", async () => {
      const [daoConfigPda] = getDaoConfigPDA();

      // Pause
      await program.methods
        .setDaoPaused(true)
        .accounts({
          daoConfig: daoConfigPda,
          authority: provider.wallet.publicKey,
        })
        .rpc();

      let daoConfig = await program.account.daoConfig.fetch(daoConfigPda);
      expect(daoConfig.paused).to.equal(true);

      // Unpause
      await program.methods
        .setDaoPaused(false)
        .accounts({
          daoConfig: daoConfigPda,
          authority: provider.wallet.publicKey,
        })
        .rpc();

      daoConfig = await program.account.daoConfig.fetch(daoConfigPda);
      expect(daoConfig.paused).to.equal(false);
    });

    it("should fail to update config by non-authority", async () => {
      const [daoConfigPda] = getDaoConfigPDA();
      const attacker = anchor.web3.Keypair.generate();

      await fundAccount(attacker.publicKey);

      try {
        await program.methods
          .updateDaoConfig(new anchor.BN(1 * 24 * 60 * 60), null, null, null)
          .accounts({
            daoConfig: daoConfigPda,
            authority: attacker.publicKey,
          })
          .signers([attacker])
          .rpc();
        expect.fail("Should have thrown an error");
      } catch (error: any) {
        expect(error.message).to.include("UnauthorizedAuthority");
      }
    });
  });

  describe("Proposal Creation", () => {
    let proposer: anchor.web3.Keypair;
    let proposerTokenAccount: anchor.web3.PublicKey;

    before(async () => {
      proposer = anchor.web3.Keypair.generate();
      await fundAccount(proposer.publicKey);
      proposerTokenAccount = await createTokenAccountForUser(proposer);

      // Mint tokens to proposer for bond
      await mintTo(
        provider.connection,
        provider.wallet.payer,
        governanceTokenMint,
        proposerTokenAccount,
        provider.wallet.publicKey,
        200_000_000_000 // 200 AEGIS tokens (enough for 2 proposals)
      );
    });

    it("should create a general proposal", async () => {
      const [daoConfigPda] = getDaoConfigPDA();
      const proposalId = new anchor.BN(1);
      const [proposalPda] = getProposalPDA(proposalId);

      await program.methods
        .createProposal(
          "Increase node rewards by 10%",
          "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG",
          { general: {} },
          null
        )
        .accounts({
          daoConfig: daoConfigPda,
          proposal: proposalPda,
          bondEscrow: bondEscrow,
          proposerTokenAccount: proposerTokenAccount,
          proposer: proposer.publicKey,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([proposer])
        .rpc();

      const proposal = await program.account.proposal.fetch(proposalPda);
      expect(proposal.proposalId.toNumber()).to.equal(1);
      expect(proposal.proposer.toString()).to.equal(proposer.publicKey.toString());
      expect(proposal.title).to.equal("Increase node rewards by 10%");
      expect(proposal.descriptionCid).to.equal(
        "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG"
      );
      expect(proposal.status).to.deep.equal({ active: {} });
      expect(proposal.forVotes.toNumber()).to.equal(0);
      expect(proposal.againstVotes.toNumber()).to.equal(0);
      expect(proposal.abstainVotes.toNumber()).to.equal(0);
    });

    it("should create a treasury withdrawal proposal", async () => {
      const [daoConfigPda] = getDaoConfigPDA();
      const proposalId = new anchor.BN(2);
      const [proposalPda] = getProposalPDA(proposalId);
      const recipient = anchor.web3.Keypair.generate().publicKey;

      await program.methods
        .createProposal(
          "Fund developer grant",
          "QmXoypizjW3WknFiJnKLwHCnL72vedxjQkDDP1mXWo6uco",
          { treasuryWithdrawal: {} },
          {
            recipient: recipient,
            amount: new anchor.BN(10_000_000_000), // 10 AEGIS
          }
        )
        .accounts({
          daoConfig: daoConfigPda,
          proposal: proposalPda,
          bondEscrow: bondEscrow,
          proposerTokenAccount: proposerTokenAccount,
          proposer: proposer.publicKey,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([proposer])
        .rpc();

      const proposal = await program.account.proposal.fetch(proposalPda);
      expect(proposal.proposalType).to.deep.equal({ treasuryWithdrawal: {} });
      expect(proposal.executionData).to.not.be.null;
      expect(proposal.executionData?.amount.toNumber()).to.equal(10_000_000_000);
      expect(proposal.executionData?.recipient.toString()).to.equal(
        recipient.toString()
      );
    });

    it("should fail to create proposal with invalid title", async () => {
      const [daoConfigPda] = getDaoConfigPDA();
      const proposalId = new anchor.BN(3);
      const [proposalPda] = getProposalPDA(proposalId);

      try {
        await program.methods
          .createProposal("", "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG", { general: {} }, null)
          .accounts({
            daoConfig: daoConfigPda,
            proposal: proposalPda,
            bondEscrow: bondEscrow,
            proposerTokenAccount: proposerTokenAccount,
            proposer: proposer.publicKey,
            tokenProgram: TOKEN_PROGRAM_ID,
            systemProgram: anchor.web3.SystemProgram.programId,
          })
          .signers([proposer])
          .rpc();
        expect.fail("Should have thrown an error");
      } catch (error: any) {
        expect(error.message).to.include("InvalidTitleLength");
      }
    });
  });

  describe("Voting", () => {
    let voter1: anchor.web3.Keypair;
    let voter2: anchor.web3.Keypair;
    let voter1TokenAccount: anchor.web3.PublicKey;
    let voter2TokenAccount: anchor.web3.PublicKey;

    before(async () => {
      voter1 = anchor.web3.Keypair.generate();
      voter2 = anchor.web3.Keypair.generate();

      await fundAccount(voter1.publicKey);
      await fundAccount(voter2.publicKey);

      voter1TokenAccount = await createTokenAccountForUser(voter1);
      voter2TokenAccount = await createTokenAccountForUser(voter2);

      // Mint tokens to voters (voting power)
      await mintTo(
        provider.connection,
        provider.wallet.payer,
        governanceTokenMint,
        voter1TokenAccount,
        provider.wallet.publicKey,
        500_000_000_000 // 500 AEGIS tokens
      );

      await mintTo(
        provider.connection,
        provider.wallet.payer,
        governanceTokenMint,
        voter2TokenAccount,
        provider.wallet.publicKey,
        300_000_000_000 // 300 AEGIS tokens
      );
    });

    it("should cast a FOR vote", async () => {
      const [daoConfigPda] = getDaoConfigPDA();
      const proposalId = new anchor.BN(1);
      const [proposalPda] = getProposalPDA(proposalId);
      const [voteRecordPda] = getVoteRecordPDA(proposalId, voter1.publicKey);

      await program.methods
        .castVote({ for: {} })
        .accounts({
          daoConfig: daoConfigPda,
          proposal: proposalPda,
          voteRecord: voteRecordPda,
          voterTokenAccount: voter1TokenAccount,
          voter: voter1.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([voter1])
        .rpc();

      const voteRecord = await program.account.voteRecord.fetch(voteRecordPda);
      expect(voteRecord.voter.toString()).to.equal(voter1.publicKey.toString());
      expect(voteRecord.voteChoice).to.deep.equal({ for: {} });
      expect(voteRecord.voteWeight.toNumber()).to.equal(500_000_000_000);

      const proposal = await program.account.proposal.fetch(proposalPda);
      expect(proposal.forVotes.toNumber()).to.equal(500_000_000_000);
    });

    it("should cast an AGAINST vote", async () => {
      const [daoConfigPda] = getDaoConfigPDA();
      const proposalId = new anchor.BN(1);
      const [proposalPda] = getProposalPDA(proposalId);
      const [voteRecordPda] = getVoteRecordPDA(proposalId, voter2.publicKey);

      await program.methods
        .castVote({ against: {} })
        .accounts({
          daoConfig: daoConfigPda,
          proposal: proposalPda,
          voteRecord: voteRecordPda,
          voterTokenAccount: voter2TokenAccount,
          voter: voter2.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([voter2])
        .rpc();

      const voteRecord = await program.account.voteRecord.fetch(voteRecordPda);
      expect(voteRecord.voteChoice).to.deep.equal({ against: {} });

      const proposal = await program.account.proposal.fetch(proposalPda);
      expect(proposal.againstVotes.toNumber()).to.equal(300_000_000_000);
    });

    it("should prevent double voting", async () => {
      const [daoConfigPda] = getDaoConfigPDA();
      const proposalId = new anchor.BN(1);
      const [proposalPda] = getProposalPDA(proposalId);
      const [voteRecordPda] = getVoteRecordPDA(proposalId, voter1.publicKey);

      try {
        await program.methods
          .castVote({ for: {} })
          .accounts({
            daoConfig: daoConfigPda,
            proposal: proposalPda,
            voteRecord: voteRecordPda,
            voterTokenAccount: voter1TokenAccount,
            voter: voter1.publicKey,
            systemProgram: anchor.web3.SystemProgram.programId,
          })
          .signers([voter1])
          .rpc();
        expect.fail("Should have thrown an error");
      } catch (error: any) {
        // Account already exists error (vote record PDA already initialized)
        expect(error.message).to.include("already in use");
      }
    });

    it("should prevent voting with no tokens", async () => {
      const [daoConfigPda] = getDaoConfigPDA();
      const proposalId = new anchor.BN(1);
      const [proposalPda] = getProposalPDA(proposalId);

      const noTokenVoter = anchor.web3.Keypair.generate();
      await fundAccount(noTokenVoter.publicKey);
      const noTokenAccount = await createTokenAccountForUser(noTokenVoter);

      const [voteRecordPda] = getVoteRecordPDA(proposalId, noTokenVoter.publicKey);

      try {
        await program.methods
          .castVote({ for: {} })
          .accounts({
            daoConfig: daoConfigPda,
            proposal: proposalPda,
            voteRecord: voteRecordPda,
            voterTokenAccount: noTokenAccount,
            voter: noTokenVoter.publicKey,
            systemProgram: anchor.web3.SystemProgram.programId,
          })
          .signers([noTokenVoter])
          .rpc();
        expect.fail("Should have thrown an error");
      } catch (error: any) {
        expect(error.message).to.include("NoVotingPower");
      }
    });
  });

  describe("Treasury Operations", () => {
    it("should accept deposits to treasury", async () => {
      const [daoConfigPda] = getDaoConfigPDA();

      const depositor = anchor.web3.Keypair.generate();
      await fundAccount(depositor.publicKey);
      const depositorTokenAccount = await createTokenAccountForUser(depositor);

      // Mint tokens to depositor
      await mintTo(
        provider.connection,
        provider.wallet.payer,
        governanceTokenMint,
        depositorTokenAccount,
        provider.wallet.publicKey,
        50_000_000_000 // 50 AEGIS tokens
      );

      const treasuryBefore = await getAccount(provider.connection, treasury);

      await program.methods
        .depositToTreasury(new anchor.BN(50_000_000_000))
        .accounts({
          daoConfig: daoConfigPda,
          treasury: treasury,
          depositorTokenAccount: depositorTokenAccount,
          depositor: depositor.publicKey,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .signers([depositor])
        .rpc();

      const treasuryAfter = await getAccount(provider.connection, treasury);
      expect(Number(treasuryAfter.amount)).to.equal(
        Number(treasuryBefore.amount) + 50_000_000_000
      );

      const daoConfig = await program.account.daoConfig.fetch(daoConfigPda);
      expect(daoConfig.totalTreasuryDeposits.toNumber()).to.be.greaterThan(0);
    });
  });

  describe("Proposal State Summary", () => {
    it("should show current proposal states", async () => {
      const [daoConfigPda] = getDaoConfigPDA();
      const daoConfig = await program.account.daoConfig.fetch(daoConfigPda);

      console.log("\n=== DAO Configuration ===");
      console.log(`Authority: ${daoConfig.authority.toString()}`);
      console.log(`Treasury: ${daoConfig.treasury.toString()}`);
      console.log(`Voting Period: ${daoConfig.votingPeriod.toNumber()} seconds`);
      console.log(`Proposal Bond: ${daoConfig.proposalBond.toString()} tokens`);
      console.log(`Quorum: ${daoConfig.quorumPercentage}%`);
      console.log(`Approval Threshold: ${daoConfig.approvalThreshold}%`);
      console.log(`Total Proposals: ${daoConfig.proposalCount.toNumber()}`);
      console.log(`Paused: ${daoConfig.paused}`);

      // Fetch and display proposal 1
      const proposalId = new anchor.BN(1);
      const [proposalPda] = getProposalPDA(proposalId);
      const proposal = await program.account.proposal.fetch(proposalPda);

      console.log("\n=== Proposal #1 ===");
      console.log(`Title: ${proposal.title}`);
      console.log(`Proposer: ${proposal.proposer.toString()}`);
      console.log(`Status: ${JSON.stringify(proposal.status)}`);
      console.log(`FOR votes: ${proposal.forVotes.toString()}`);
      console.log(`AGAINST votes: ${proposal.againstVotes.toString()}`);
      console.log(`ABSTAIN votes: ${proposal.abstainVotes.toString()}`);
    });
  });
});
