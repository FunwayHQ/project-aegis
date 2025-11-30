import { describe, it, expect } from "vitest";
import { PublicKey } from "@solana/web3.js";
import {
  DAO_PROGRAM_ID,
  SEEDS,
  DEFAULTS,
  AEGIS_DECIMALS,
  CLUSTERS,
} from "../src/constants";

describe("Constants", () => {
  describe("DAO_PROGRAM_ID", () => {
    it("is a valid PublicKey", () => {
      expect(DAO_PROGRAM_ID).toBeInstanceOf(PublicKey);
    });

    it("matches the deployed devnet program", () => {
      expect(DAO_PROGRAM_ID.toString()).toBe(
        "9zQDZPNyDqVxevUAwaWTGGvCGwLSpfvkMn6aDKx7x6hz"
      );
    });
  });

  describe("SEEDS", () => {
    it("DAO_CONFIG seed is correct", () => {
      expect(SEEDS.DAO_CONFIG.toString()).toBe("dao_config");
    });

    it("PROPOSAL seed is correct", () => {
      expect(SEEDS.PROPOSAL.toString()).toBe("proposal");
    });

    it("VOTE_ESCROW seed is correct", () => {
      expect(SEEDS.VOTE_ESCROW.toString()).toBe("vote_escrow");
    });

    it("VOTE_RECORD seed is correct", () => {
      expect(SEEDS.VOTE_RECORD.toString()).toBe("vote");
    });

    it("all seeds are Buffers", () => {
      expect(Buffer.isBuffer(SEEDS.DAO_CONFIG)).toBe(true);
      expect(Buffer.isBuffer(SEEDS.PROPOSAL)).toBe(true);
      expect(Buffer.isBuffer(SEEDS.VOTE_ESCROW)).toBe(true);
      expect(Buffer.isBuffer(SEEDS.VOTE_RECORD)).toBe(true);
    });
  });

  describe("DEFAULTS", () => {
    it("has reasonable voting period in days", () => {
      expect(DEFAULTS.VOTING_PERIOD_DAYS).toBe(3);
    });

    it("has reasonable voting period in seconds", () => {
      // 3 days in seconds
      expect(DEFAULTS.VOTING_PERIOD_SECONDS).toBe(259200);
    });

    it("has minimum voting period of 1 day", () => {
      expect(DEFAULTS.MIN_VOTING_PERIOD_SECONDS).toBe(86400);
    });

    it("has maximum voting period of 14 days", () => {
      expect(DEFAULTS.MAX_VOTING_PERIOD_SECONDS).toBe(1209600);
    });

    it("has reasonable proposal bond", () => {
      // 100 tokens with 9 decimals
      expect(DEFAULTS.PROPOSAL_BOND.toString()).toBe("100000000000");
    });

    it("has minimum proposal bond of 1 token", () => {
      expect(DEFAULTS.MIN_PROPOSAL_BOND.toString()).toBe("1000000000");
    });

    it("has reasonable quorum percentage", () => {
      // 10%
      expect(DEFAULTS.QUORUM_PERCENTAGE).toBe(10);
    });

    it("has reasonable approval threshold", () => {
      // 51%
      expect(DEFAULTS.APPROVAL_THRESHOLD).toBe(51);
    });

    it("has 48-hour config timelock", () => {
      expect(DEFAULTS.CONFIG_TIMELOCK_SECONDS).toBe(172800);
    });

    it("has title length limits", () => {
      expect(DEFAULTS.MAX_TITLE_LENGTH).toBe(128);
    });

    it("has description CID length limits", () => {
      expect(DEFAULTS.MAX_DESCRIPTION_CID_LENGTH).toBe(64);
    });
  });

  describe("AEGIS_DECIMALS", () => {
    it("is 9 decimals like most Solana tokens", () => {
      expect(AEGIS_DECIMALS).toBe(9);
    });
  });

  describe("CLUSTERS", () => {
    it("has devnet endpoint", () => {
      expect(CLUSTERS.devnet).toBe("https://api.devnet.solana.com");
    });

    it("has mainnet endpoint", () => {
      expect(CLUSTERS.mainnet).toBe("https://api.mainnet-beta.solana.com");
    });

    it("has localnet endpoint", () => {
      expect(CLUSTERS.localnet).toBe("http://localhost:8899");
    });
  });
});
