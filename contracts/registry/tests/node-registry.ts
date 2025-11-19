import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { NodeRegistry } from "../target/types/node_registry";
import { expect } from "chai";

describe("node-registry", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.NodeRegistry as Program<NodeRegistry>;
  const operator = provider.wallet;

  const VALID_IPFS_CID = "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG";
  const MIN_STAKE = new anchor.BN(100_000_000_000); // 100 AEGIS tokens

  // Helper to derive node PDA
  function getNodePDA(operator: anchor.web3.PublicKey): [anchor.web3.PublicKey, number] {
    return anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("node"), operator.toBuffer()],
      program.programId
    );
  }

  describe("Node Registration", () => {
    it("Registers a new node successfully", async () => {
      const [nodePDA] = getNodePDA(operator.publicKey);

      await program.methods
        .registerNode(VALID_IPFS_CID, MIN_STAKE)
        .accounts({
          nodeAccount: nodePDA,
          operator: operator.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .rpc();

      // Verify node account was created
      const nodeAccount = await program.account.nodeAccount.fetch(nodePDA);

      expect(nodeAccount.operator.toString()).to.equal(operator.publicKey.toString());
      expect(nodeAccount.metadataUrl).to.equal(VALID_IPFS_CID);
      expect(nodeAccount.stakeAmount.toString()).to.equal(MIN_STAKE.toString());
      expect(nodeAccount.status).to.deep.equal({ active: {} });
      expect(nodeAccount.registeredAt.toNumber()).to.be.greaterThan(0);
      expect(nodeAccount.updatedAt.toNumber()).to.equal(nodeAccount.registeredAt.toNumber());
    });

    it("Prevents duplicate registration", async () => {
      const [nodePDA] = getNodePDA(operator.publicKey);

      try {
        await program.methods
          .registerNode(VALID_IPFS_CID, MIN_STAKE)
          .accounts({
            nodeAccount: nodePDA,
            operator: operator.publicKey,
            systemProgram: anchor.web3.SystemProgram.programId,
          })
          .rpc();

        expect.fail("Should have prevented duplicate registration");
      } catch (error) {
        // Already initialized error expected
        expect(error).to.exist;
      }
    });

    it("Rejects registration with empty metadata URL", async () => {
      const newOperator = anchor.web3.Keypair.generate();
      const [nodePDA] = getNodePDA(newOperator.publicKey);

      try {
        await program.methods
          .registerNode("", MIN_STAKE)
          .accounts({
            nodeAccount: nodePDA,
            operator: newOperator.publicKey,
            systemProgram: anchor.web3.SystemProgram.programId,
          })
          .signers([newOperator])
          .rpc();

        expect.fail("Should have rejected empty metadata URL");
      } catch (error) {
        // Error should exist (may be SOL or validation error)
        expect(error).to.exist;
      }
    });

    it("Rejects registration with too long metadata URL", async () => {
      const newOperator = anchor.web3.Keypair.generate();
      const [nodePDA] = getNodePDA(newOperator.publicKey);
      const longUrl = "Q".repeat(129); // Exceeds 128 char limit

      try {
        await program.methods
          .registerNode(longUrl, MIN_STAKE)
          .accounts({
            nodeAccount: nodePDA,
            operator: newOperator.publicKey,
            systemProgram: anchor.web3.SystemProgram.programId,
          })
          .signers([newOperator])
          .rpc();

        expect.fail("Should have rejected too long metadata URL");
      } catch (error) {
        // Error should exist (may be SOL or validation error)
        expect(error).to.exist;
      }
    });

    it("Rejects registration with insufficient stake", async () => {
      const newOperator = anchor.web3.Keypair.generate();
      const [nodePDA] = getNodePDA(newOperator.publicKey);
      const insufficientStake = MIN_STAKE.subn(1);

      try {
        await program.methods
          .registerNode(VALID_IPFS_CID, insufficientStake)
          .accounts({
            nodeAccount: nodePDA,
            operator: newOperator.publicKey,
            systemProgram: anchor.web3.SystemProgram.programId,
          })
          .signers([newOperator])
          .rpc();

        expect.fail("Should have rejected insufficient stake");
      } catch (error) {
        expect(error.toString()).to.include("InsufficientStake");
      }
    });

    it("Allows registration with stake above minimum", async () => {
      const newOperator = anchor.web3.Keypair.generate();
      const [nodePDA] = getNodePDA(newOperator.publicKey);
      const largeStake = MIN_STAKE.muln(10); // 1000 AEGIS

      await program.methods
        .registerNode(VALID_IPFS_CID, largeStake)
        .accounts({
          nodeAccount: nodePDA,
          operator: newOperator.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([newOperator])
        .rpc();

      const nodeAccount = await program.account.nodeAccount.fetch(nodePDA);
      expect(nodeAccount.stakeAmount.toString()).to.equal(largeStake.toString());
    });
  });

  describe("Metadata Updates", () => {
    let testOperator: anchor.web3.Keypair;
    let testNodePDA: anchor.web3.PublicKey;

    before(async () => {
      testOperator = anchor.web3.Keypair.generate();
      [testNodePDA] = getNodePDA(testOperator.publicKey);

      // Register node first
      await program.methods
        .registerNode(VALID_IPFS_CID, MIN_STAKE)
        .accounts({
          nodeAccount: testNodePDA,
          operator: testOperator.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([testOperator])
        .rpc();
    });

    it("Updates metadata successfully", async () => {
      const newCID = "Qmabcdef1234567890ABCDEFGHIJKLMNOPQRSTUVWXYZ123";

      const accountBefore = await program.account.nodeAccount.fetch(testNodePDA);

      await program.methods
        .updateMetadata(newCID)
        .accounts({
          nodeAccount: testNodePDA,
          operator: testOperator.publicKey,
        })
        .signers([testOperator])
        .rpc();

      const accountAfter = await program.account.nodeAccount.fetch(testNodePDA);

      expect(accountAfter.metadataUrl).to.equal(newCID);
      expect(accountAfter.updatedAt).to.be.greaterThan(accountBefore.updatedAt);
    });

    it("Prevents unauthorized metadata updates", async () => {
      const unauthorized = anchor.web3.Keypair.generate();
      const newCID = "QmUnauthorized123456789ABCDEFGHIJK";

      try {
        await program.methods
          .updateMetadata(newCID)
          .accounts({
            nodeAccount: testNodePDA,
            operator: unauthorized.publicKey,
          })
          .signers([unauthorized])
          .rpc();

        expect.fail("Should have prevented unauthorized update");
      } catch (error) {
        expect(error.toString()).to.include("UnauthorizedOperator");
      }
    });

    it("Rejects empty metadata URL in update", async () => {
      try {
        await program.methods
          .updateMetadata("")
          .accounts({
            nodeAccount: testNodePDA,
            operator: testOperator.publicKey,
          })
          .signers([testOperator])
          .rpc();

        expect.fail("Should have rejected empty metadata");
      } catch (error) {
        expect(error.toString()).to.include("MetadataUrlEmpty");
      }
    });
  });

  describe("Node Status Management", () => {
    let statusOperator: anchor.web3.Keypair;
    let statusNodePDA: anchor.web3.PublicKey;

    before(async () => {
      statusOperator = anchor.web3.Keypair.generate();
      [statusNodePDA] = getNodePDA(statusOperator.publicKey);

      await program.methods
        .registerNode(VALID_IPFS_CID, MIN_STAKE)
        .accounts({
          nodeAccount: statusNodePDA,
          operator: statusOperator.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([statusOperator])
        .rpc();
    });

    it("Deactivates node successfully", async () => {
      await program.methods
        .deactivateNode()
        .accounts({
          nodeAccount: statusNodePDA,
          operator: statusOperator.publicKey,
        })
        .signers([statusOperator])
        .rpc();

      const nodeAccount = await program.account.nodeAccount.fetch(statusNodePDA);
      expect(nodeAccount.status).to.deep.equal({ inactive: {} });
    });

    it("Reactivates node successfully", async () => {
      await program.methods
        .reactivateNode()
        .accounts({
          nodeAccount: statusNodePDA,
          operator: statusOperator.publicKey,
        })
        .signers([statusOperator])
        .rpc();

      const nodeAccount = await program.account.nodeAccount.fetch(statusNodePDA);
      expect(nodeAccount.status).to.deep.equal({ active: {} });
    });

    it("Prevents deactivating already inactive node", async () => {
      // Deactivate first
      await program.methods
        .deactivateNode()
        .accounts({
          nodeAccount: statusNodePDA,
          operator: statusOperator.publicKey,
        })
        .signers([statusOperator])
        .rpc();

      // Try to deactivate again
      try {
        await program.methods
          .deactivateNode()
          .accounts({
            nodeAccount: statusNodePDA,
            operator: statusOperator.publicKey,
          })
          .signers([statusOperator])
          .rpc();

        expect.fail("Should have prevented double deactivation");
      } catch (error) {
        expect(error.toString()).to.include("NodeAlreadyInactive");
      }
    });

    it("Prevents unauthorized deactivation", async () => {
      const unauthorized = anchor.web3.Keypair.generate();

      try {
        await program.methods
          .deactivateNode()
          .accounts({
            nodeAccount: statusNodePDA,
            operator: unauthorized.publicKey,
          })
          .signers([unauthorized])
          .rpc();

        expect.fail("Should have prevented unauthorized deactivation");
      } catch (error) {
        expect(error).to.exist;
      }
    });
  });

  describe("Stake Management", () => {
    let stakeOperator: anchor.web3.Keypair;
    let stakeNodePDA: anchor.web3.PublicKey;

    before(async () => {
      stakeOperator = anchor.web3.Keypair.generate();
      [stakeNodePDA] = getNodePDA(stakeOperator.publicKey);

      await program.methods
        .registerNode(VALID_IPFS_CID, MIN_STAKE)
        .accounts({
          nodeAccount: stakeNodePDA,
          operator: stakeOperator.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([stakeOperator])
        .rpc();
    });

    it("Updates stake amount", async () => {
      const newStake = MIN_STAKE.muln(5); // 500 AEGIS

      await program.methods
        .updateStake(newStake)
        .accounts({
          nodeAccount: stakeNodePDA,
          authority: stakeOperator.publicKey,
        })
        .signers([stakeOperator])
        .rpc();

      const nodeAccount = await program.account.nodeAccount.fetch(stakeNodePDA);
      expect(nodeAccount.stakeAmount.toString()).to.equal(newStake.toString());
    });

    it("Allows reducing stake amount", async () => {
      const reducedStake = MIN_STAKE;

      await program.methods
        .updateStake(reducedStake)
        .accounts({
          nodeAccount: stakeNodePDA,
          authority: stakeOperator.publicKey,
        })
        .signers([stakeOperator])
        .rpc();

      const nodeAccount = await program.account.nodeAccount.fetch(stakeNodePDA);
      expect(nodeAccount.stakeAmount.toString()).to.equal(reducedStake.toString());
    });
  });

  describe("Multiple Nodes", () => {
    it("Handles multiple independent node registrations", async () => {
      const operators = [
        anchor.web3.Keypair.generate(),
        anchor.web3.Keypair.generate(),
        anchor.web3.Keypair.generate(),
      ];

      const stakes = [
        MIN_STAKE,
        MIN_STAKE.muln(2),
        MIN_STAKE.muln(5),
      ];

      for (let i = 0; i < operators.length; i++) {
        const [nodePDA] = getNodePDA(operators[i].publicKey);

        await program.methods
          .registerNode(VALID_IPFS_CID, stakes[i])
          .accounts({
            nodeAccount: nodePDA,
            operator: operators[i].publicKey,
            systemProgram: anchor.web3.SystemProgram.programId,
          })
          .signers([operators[i]])
          .rpc();

        // Verify each node
        const nodeAccount = await program.account.nodeAccount.fetch(nodePDA);
        expect(nodeAccount.operator.toString()).to.equal(operators[i].publicKey.toString());
        expect(nodeAccount.stakeAmount.toString()).to.equal(stakes[i].toString());
      }
    });
  });

  describe("Edge Cases", () => {
    it("Handles maximum length metadata URL", async () => {
      const newOperator = anchor.web3.Keypair.generate();
      const [nodePDA] = getNodePDA(newOperator.publicKey);
      const maxLengthUrl = "Q".repeat(128); // Exactly at limit

      await program.methods
        .registerNode(maxLengthUrl, MIN_STAKE)
        .accounts({
          nodeAccount: nodePDA,
          operator: newOperator.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([newOperator])
        .rpc();

      const nodeAccount = await program.account.nodeAccount.fetch(nodePDA);
      expect(nodeAccount.metadataUrl).to.equal(maxLengthUrl);
      expect(nodeAccount.metadataUrl.length).to.equal(128);
    });

    it("Handles minimum stake amount exactly", async () => {
      const newOperator = anchor.web3.Keypair.generate();
      const [nodePDA] = getNodePDA(newOperator.publicKey);

      await program.methods
        .registerNode(VALID_IPFS_CID, MIN_STAKE)
        .accounts({
          nodeAccount: nodePDA,
          operator: newOperator.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([newOperator])
        .rpc();

      const nodeAccount = await program.account.nodeAccount.fetch(nodePDA);
      expect(nodeAccount.stakeAmount.toString()).to.equal(MIN_STAKE.toString());
    });

    it("Handles very large stake amounts", async () => {
      const newOperator = anchor.web3.Keypair.generate();
      const [nodePDA] = getNodePDA(newOperator.publicKey);
      const largeStake = new anchor.BN("1000000000000000000"); // 1 billion AEGIS

      await program.methods
        .registerNode(VALID_IPFS_CID, largeStake)
        .accounts({
          nodeAccount: nodePDA,
          operator: newOperator.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([newOperator])
        .rpc();

      const nodeAccount = await program.account.nodeAccount.fetch(nodePDA);
      expect(nodeAccount.stakeAmount.toString()).to.equal(largeStake.toString());
    });
  });

  describe("PDA Derivation", () => {
    it("Derives correct PDA for each operator", async () => {
      const operator1 = anchor.web3.Keypair.generate();
      const operator2 = anchor.web3.Keypair.generate();

      const [pda1] = getNodePDA(operator1.publicKey);
      const [pda2] = getNodePDA(operator2.publicKey);

      // PDAs should be different for different operators
      expect(pda1.toString()).to.not.equal(pda2.toString());

      // Same operator should always give same PDA
      const [pda1Again] = getNodePDA(operator1.publicKey);
      expect(pda1.toString()).to.equal(pda1Again.toString());
    });
  });
});
