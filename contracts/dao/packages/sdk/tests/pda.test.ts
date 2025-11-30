import { describe, it, expect } from "vitest";
import { PublicKey } from "@solana/web3.js";
import BN from "bn.js";
import {
  getDaoConfigPDA,
  getProposalPDA,
  getVoteEscrowPDA,
  getVoteRecordPDA,
} from "../src/pda";
import { DAO_PROGRAM_ID } from "../src/constants";

describe("PDA Derivation", () => {
  const testVoter = new PublicKey("4WGq9QqzycHaTZvLWRAQmzXWybm6SARHbFaCqLWptDe4");

  describe("getDaoConfigPDA", () => {
    it("derives DAO config PDA deterministically", () => {
      const [pda1, bump1] = getDaoConfigPDA();
      const [pda2, bump2] = getDaoConfigPDA();

      expect(pda1.equals(pda2)).toBe(true);
      expect(bump1).toBe(bump2);
    });

    it("uses correct program ID", () => {
      const [pda] = getDaoConfigPDA();
      // Verify it's a valid PDA on the curve
      expect(PublicKey.isOnCurve(pda.toBytes())).toBe(false);
    });

    it("accepts custom program ID", () => {
      const customProgramId = new PublicKey("11111111111111111111111111111111");
      const [pda1] = getDaoConfigPDA(DAO_PROGRAM_ID);
      const [pda2] = getDaoConfigPDA(customProgramId);

      expect(pda1.equals(pda2)).toBe(false);
    });

    it("matches known devnet PDA", () => {
      const [pda] = getDaoConfigPDA();
      // This is the actual DAO config PDA on devnet
      expect(pda.toString()).toBe("H4kFduR1jEkASReb9zhNqsCYdcdtbwxjQgveVuzkLduo");
    });
  });

  describe("getProposalPDA", () => {
    it("derives unique PDAs for different proposal IDs", () => {
      const [pda1] = getProposalPDA(1);
      const [pda2] = getProposalPDA(2);
      const [pda3] = getProposalPDA(new BN(3));

      expect(pda1.equals(pda2)).toBe(false);
      expect(pda2.equals(pda3)).toBe(false);
    });

    it("accepts BN, number, and bigint", () => {
      const [pdaNumber] = getProposalPDA(1);
      const [pdaBN] = getProposalPDA(new BN(1));
      const [pdaBigint] = getProposalPDA(BigInt(1));

      expect(pdaNumber.equals(pdaBN)).toBe(true);
      expect(pdaBN.equals(pdaBigint)).toBe(true);
    });

    it("handles large proposal IDs", () => {
      const largeId = new BN("18446744073709551615"); // u64 max
      const [pda, bump] = getProposalPDA(largeId);

      expect(PublicKey.isOnCurve(pda.toBytes())).toBe(false);
      expect(bump).toBeLessThanOrEqual(255);
    });
  });

  describe("getVoteEscrowPDA", () => {
    it("derives unique PDAs for different voters", () => {
      const voter1 = new PublicKey("4WGq9QqzycHaTZvLWRAQmzXWybm6SARHbFaCqLWptDe4");
      const voter2 = new PublicKey("11111111111111111111111111111111");

      const [pda1] = getVoteEscrowPDA(1, voter1);
      const [pda2] = getVoteEscrowPDA(1, voter2);

      expect(pda1.equals(pda2)).toBe(false);
    });

    it("derives unique PDAs for different proposals", () => {
      const [pda1] = getVoteEscrowPDA(1, testVoter);
      const [pda2] = getVoteEscrowPDA(2, testVoter);

      expect(pda1.equals(pda2)).toBe(false);
    });

    it("is deterministic for same inputs", () => {
      const [pda1] = getVoteEscrowPDA(1, testVoter);
      const [pda2] = getVoteEscrowPDA(new BN(1), testVoter);

      expect(pda1.equals(pda2)).toBe(true);
    });
  });

  describe("getVoteRecordPDA", () => {
    it("derives unique PDAs for different voters", () => {
      const voter1 = new PublicKey("4WGq9QqzycHaTZvLWRAQmzXWybm6SARHbFaCqLWptDe4");
      const voter2 = new PublicKey("11111111111111111111111111111111");

      const [pda1] = getVoteRecordPDA(1, voter1);
      const [pda2] = getVoteRecordPDA(1, voter2);

      expect(pda1.equals(pda2)).toBe(false);
    });

    it("vote escrow and vote record PDAs are different", () => {
      const [escrowPDA] = getVoteEscrowPDA(1, testVoter);
      const [recordPDA] = getVoteRecordPDA(1, testVoter);

      expect(escrowPDA.equals(recordPDA)).toBe(false);
    });
  });
});
