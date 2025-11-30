import { describe, it, expect } from "vitest";
import BN from "bn.js";
import { PublicKey } from "@solana/web3.js";
import {
  formatTokenAmount,
  parseTokenAmount,
  formatDuration,
  shortAddress,
  formatPercentage,
  calculateVotePercentages,
  getTimeRemaining,
} from "../src/utils/format";

describe("formatTokenAmount", () => {
  it("formats whole numbers correctly", () => {
    const amount = new BN("1000000000"); // 1 AEGIS (9 decimals)
    expect(formatTokenAmount(amount)).toBe("1");
  });

  it("formats amounts with decimals correctly", () => {
    const amount = new BN("1500000000"); // 1.5 AEGIS
    expect(formatTokenAmount(amount)).toBe("1.5");
  });

  it("formats amounts less than 1 correctly", () => {
    const amount = new BN("500000000"); // 0.5 AEGIS
    expect(formatTokenAmount(amount)).toBe("0.5");
  });

  it("formats zero correctly", () => {
    const amount = new BN(0);
    expect(formatTokenAmount(amount)).toBe("0");
  });

  it("handles bigint input", () => {
    const amount = BigInt("2500000000"); // 2.5 AEGIS
    expect(formatTokenAmount(amount)).toBe("2.5");
  });

  it("handles number input", () => {
    const amount = 1000000000; // 1 AEGIS
    expect(formatTokenAmount(amount)).toBe("1");
  });
});

describe("parseTokenAmount", () => {
  it("parses whole numbers correctly", () => {
    const result = parseTokenAmount("1");
    expect(result.toString()).toBe("1000000000");
  });

  it("parses decimal amounts correctly", () => {
    const result = parseTokenAmount("1.5");
    expect(result.toString()).toBe("1500000000");
  });

  it("parses amounts less than 1", () => {
    const result = parseTokenAmount("0.5");
    expect(result.toString()).toBe("500000000");
  });

  it("handles extra decimal places by truncating", () => {
    const result = parseTokenAmount("1.123456789123");
    expect(result.toString()).toBe("1123456789");
  });
});

describe("formatDuration", () => {
  it("formats days correctly", () => {
    expect(formatDuration(86400)).toBe("1d");
    expect(formatDuration(172800)).toBe("2d");
  });

  it("formats hours correctly", () => {
    expect(formatDuration(3600)).toBe("1h");
    expect(formatDuration(7200)).toBe("2h");
  });

  it("formats minutes correctly", () => {
    expect(formatDuration(60)).toBe("1m");
    expect(formatDuration(120)).toBe("2m");
  });

  it("formats combined durations", () => {
    expect(formatDuration(90061)).toBe("1d 1h 1m");
  });

  it("handles zero duration", () => {
    expect(formatDuration(0)).toBe("0m");
  });
});

describe("shortAddress", () => {
  it("shortens PublicKey correctly", () => {
    const pubkey = new PublicKey("9zQDZPNyDqVxevUAwaWTGGvCGwLSpfvkMn6aDKx7x6hz");
    expect(shortAddress(pubkey)).toBe("9zQD...x6hz");
  });

  it("shortens string address correctly", () => {
    const address = "9zQDZPNyDqVxevUAwaWTGGvCGwLSpfvkMn6aDKx7x6hz";
    expect(shortAddress(address)).toBe("9zQD...x6hz");
  });

  it("allows custom char count", () => {
    const address = "9zQDZPNyDqVxevUAwaWTGGvCGwLSpfvkMn6aDKx7x6hz";
    expect(shortAddress(address, 6)).toBe("9zQDZP...x7x6hz");
  });
});

describe("formatPercentage", () => {
  it("formats whole percentages", () => {
    expect(formatPercentage(50)).toBe("50.00%");
  });

  it("formats decimal percentages", () => {
    expect(formatPercentage(33.3333)).toBe("33.33%");
  });

  it("handles zero", () => {
    expect(formatPercentage(0)).toBe("0.00%");
  });
});

describe("calculateVotePercentages", () => {
  it("calculates equal votes correctly", () => {
    const forVotes = new BN(100);
    const againstVotes = new BN(100);
    const abstainVotes = new BN(100);

    const result = calculateVotePercentages(forVotes, againstVotes, abstainVotes);

    expect(result.for).toBeCloseTo(33.33, 1);
    expect(result.against).toBeCloseTo(33.33, 1);
    expect(result.abstain).toBeCloseTo(33.33, 1);
  });

  it("handles zero total votes", () => {
    const forVotes = new BN(0);
    const againstVotes = new BN(0);
    const abstainVotes = new BN(0);

    const result = calculateVotePercentages(forVotes, againstVotes, abstainVotes);

    expect(result.for).toBe(0);
    expect(result.against).toBe(0);
    expect(result.abstain).toBe(0);
  });

  it("calculates majority correctly", () => {
    const forVotes = new BN(70);
    const againstVotes = new BN(20);
    const abstainVotes = new BN(10);

    const result = calculateVotePercentages(forVotes, againstVotes, abstainVotes);

    expect(result.for).toBe(70);
    expect(result.against).toBe(20);
    expect(result.abstain).toBe(10);
  });
});

describe("getTimeRemaining", () => {
  it("returns 'Ended' for past timestamps", () => {
    const pastTimestamp = new BN(Math.floor(Date.now() / 1000) - 3600);
    expect(getTimeRemaining(pastTimestamp)).toBe("Ended");
  });

  it("returns formatted duration for future timestamps", () => {
    const futureTimestamp = new BN(Math.floor(Date.now() / 1000) + 86400);
    const result = getTimeRemaining(futureTimestamp);
    // Should contain days or hours
    expect(result).toMatch(/\d+[dhm]/);
  });

  it("handles number input", () => {
    const pastTimestamp = Math.floor(Date.now() / 1000) - 3600;
    expect(getTimeRemaining(pastTimestamp)).toBe("Ended");
  });
});
