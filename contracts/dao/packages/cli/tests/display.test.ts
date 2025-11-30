import { describe, it, expect } from "vitest";
import BN from "bn.js";
import { PublicKey } from "@solana/web3.js";
import {
  formatTokenAmount,
  formatTimestamp,
  formatDuration,
  shortAddress,
} from "../src/utils/display";

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

  it("handles large amounts", () => {
    const amount = new BN("100000000000000000"); // 100,000,000 AEGIS
    expect(formatTokenAmount(amount)).toBe("100000000");
  });

  it("trims trailing zeros in decimal part", () => {
    const amount = new BN("1100000000"); // 1.1 AEGIS
    expect(formatTokenAmount(amount)).toBe("1.1");
  });

  it("handles bigint input", () => {
    const amount = BigInt("2500000000"); // 2.5 AEGIS
    expect(formatTokenAmount(amount)).toBe("2.5");
  });

  it("uses custom decimals", () => {
    const amount = new BN("1000000"); // 1 token with 6 decimals
    expect(formatTokenAmount(amount, 6)).toBe("1");
  });
});

describe("formatTimestamp", () => {
  it("formats timestamp to date string", () => {
    const timestamp = new BN(1700000000);
    const formatted = formatTimestamp(timestamp);
    // Just verify it returns a non-empty string (locale-dependent)
    expect(formatted).toBeTruthy();
    expect(typeof formatted).toBe("string");
  });

  it("handles zero timestamp", () => {
    const timestamp = new BN(0);
    const formatted = formatTimestamp(timestamp);
    expect(formatted).toBeTruthy();
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

  it("formats typical voting period", () => {
    expect(formatDuration(259200)).toBe("3d"); // 3 days
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

  it("handles different addresses consistently", () => {
    const address1 = "11111111111111111111111111111111";
    const address2 = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
    expect(shortAddress(address1)).toBe("1111...1111");
    expect(shortAddress(address2)).toBe("Toke...Q5DA");
  });
});
