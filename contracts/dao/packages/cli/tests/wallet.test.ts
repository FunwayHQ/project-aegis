import { describe, it, expect } from "vitest";
import { getRpcUrl } from "../src/utils/wallet";

describe("getRpcUrl", () => {
  it("returns devnet URL for devnet cluster", () => {
    expect(getRpcUrl("devnet")).toBe("https://api.devnet.solana.com");
  });

  it("returns mainnet URL for mainnet cluster", () => {
    expect(getRpcUrl("mainnet")).toBe("https://api.mainnet-beta.solana.com");
  });

  it("returns mainnet URL for mainnet-beta cluster", () => {
    expect(getRpcUrl("mainnet-beta")).toBe("https://api.mainnet-beta.solana.com");
  });

  it("returns localhost URL for localnet cluster", () => {
    expect(getRpcUrl("localnet")).toBe("http://localhost:8899");
  });

  it("returns localhost URL for localhost cluster", () => {
    expect(getRpcUrl("localhost")).toBe("http://localhost:8899");
  });

  it("returns custom URL as-is", () => {
    const customUrl = "https://my-custom-rpc.com";
    expect(getRpcUrl(customUrl)).toBe(customUrl);
  });

  it("returns devnet as default when no cluster specified", () => {
    // Clear env var for this test
    const originalEnv = process.env.SOLANA_RPC_URL;
    delete process.env.SOLANA_RPC_URL;

    expect(getRpcUrl()).toBe("https://api.devnet.solana.com");

    // Restore env var
    if (originalEnv) {
      process.env.SOLANA_RPC_URL = originalEnv;
    }
  });
});
