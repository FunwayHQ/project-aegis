import { describe, it, expect } from "vitest";
import {
  ProposalType,
  ProposalStatus,
  VoteChoice,
  parseProposalType,
  parseProposalStatus,
  parseVoteChoice,
  toAnchorProposalType,
  toAnchorVoteChoice,
} from "../src/types";

describe("Types", () => {
  describe("ProposalType", () => {
    it("has correct enum values", () => {
      expect(ProposalType.General).toBe("general");
      expect(ProposalType.TreasuryWithdrawal).toBe("treasuryWithdrawal");
      expect(ProposalType.ParameterChange).toBe("parameterChange");
    });
  });

  describe("ProposalStatus", () => {
    it("has correct enum values", () => {
      expect(ProposalStatus.Active).toBe("active");
      expect(ProposalStatus.Passed).toBe("passed");
      expect(ProposalStatus.Defeated).toBe("defeated");
      expect(ProposalStatus.Executed).toBe("executed");
      expect(ProposalStatus.Cancelled).toBe("cancelled");
    });
  });

  describe("VoteChoice", () => {
    it("has correct enum values", () => {
      expect(VoteChoice.For).toBe("for");
      expect(VoteChoice.Against).toBe("against");
      expect(VoteChoice.Abstain).toBe("abstain");
    });
  });

  describe("parseProposalType", () => {
    it("parses general type", () => {
      expect(parseProposalType({ general: {} })).toBe(ProposalType.General);
    });

    it("parses treasuryWithdrawal type", () => {
      expect(parseProposalType({ treasuryWithdrawal: {} })).toBe(
        ProposalType.TreasuryWithdrawal
      );
    });

    it("parses parameterChange type", () => {
      expect(parseProposalType({ parameterChange: {} })).toBe(
        ProposalType.ParameterChange
      );
    });

    it("throws on unknown type", () => {
      expect(() => parseProposalType({ unknown: {} })).toThrow(
        "Unknown proposal type"
      );
    });
  });

  describe("parseProposalStatus", () => {
    it("parses active status", () => {
      expect(parseProposalStatus({ active: {} })).toBe(ProposalStatus.Active);
    });

    it("parses passed status", () => {
      expect(parseProposalStatus({ passed: {} })).toBe(ProposalStatus.Passed);
    });

    it("parses defeated status", () => {
      expect(parseProposalStatus({ defeated: {} })).toBe(ProposalStatus.Defeated);
    });

    it("parses executed status", () => {
      expect(parseProposalStatus({ executed: {} })).toBe(ProposalStatus.Executed);
    });

    it("parses cancelled status", () => {
      expect(parseProposalStatus({ cancelled: {} })).toBe(
        ProposalStatus.Cancelled
      );
    });

    it("throws on unknown status", () => {
      expect(() => parseProposalStatus({ unknown: {} })).toThrow(
        "Unknown proposal status"
      );
    });
  });

  describe("parseVoteChoice", () => {
    it("parses for choice", () => {
      expect(parseVoteChoice({ for: {} })).toBe(VoteChoice.For);
    });

    it("parses against choice", () => {
      expect(parseVoteChoice({ against: {} })).toBe(VoteChoice.Against);
    });

    it("parses abstain choice", () => {
      expect(parseVoteChoice({ abstain: {} })).toBe(VoteChoice.Abstain);
    });

    it("throws on unknown choice", () => {
      expect(() => parseVoteChoice({ unknown: {} })).toThrow(
        "Unknown vote choice"
      );
    });
  });

  describe("toAnchorProposalType", () => {
    it("converts General to anchor format", () => {
      expect(toAnchorProposalType(ProposalType.General)).toEqual({ general: {} });
    });

    it("converts TreasuryWithdrawal to anchor format", () => {
      expect(toAnchorProposalType(ProposalType.TreasuryWithdrawal)).toEqual({
        treasuryWithdrawal: {},
      });
    });

    it("converts ParameterChange to anchor format", () => {
      expect(toAnchorProposalType(ProposalType.ParameterChange)).toEqual({
        parameterChange: {},
      });
    });
  });

  describe("toAnchorVoteChoice", () => {
    it("converts For to anchor format", () => {
      expect(toAnchorVoteChoice(VoteChoice.For)).toEqual({ for: {} });
    });

    it("converts Against to anchor format", () => {
      expect(toAnchorVoteChoice(VoteChoice.Against)).toEqual({ against: {} });
    });

    it("converts Abstain to anchor format", () => {
      expect(toAnchorVoteChoice(VoteChoice.Abstain)).toEqual({ abstain: {} });
    });
  });

  describe("roundtrip conversions", () => {
    it("ProposalType roundtrip", () => {
      for (const type of Object.values(ProposalType)) {
        const anchor = toAnchorProposalType(type);
        const parsed = parseProposalType(anchor);
        expect(parsed).toBe(type);
      }
    });

    it("VoteChoice roundtrip", () => {
      for (const choice of Object.values(VoteChoice)) {
        const anchor = toAnchorVoteChoice(choice);
        const parsed = parseVoteChoice(anchor);
        expect(parsed).toBe(choice);
      }
    });
  });
});
