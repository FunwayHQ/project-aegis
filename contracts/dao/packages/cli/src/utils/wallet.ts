import { Keypair, Connection } from "@solana/web3.js";
import { Wallet } from "@coral-xyz/anchor";
import fs from "fs";
import os from "os";
import path from "path";

/**
 * Load keypair from file path or default Solana location
 */
export function loadKeypair(keypairPath?: string): Keypair {
  const resolvedPath =
    keypairPath || path.join(os.homedir(), ".config", "solana", "id.json");

  if (!fs.existsSync(resolvedPath)) {
    throw new Error(
      `Keypair file not found at ${resolvedPath}. ` +
        "Run 'solana-keygen new' to create one or specify --keypair option."
    );
  }

  const keypairData = JSON.parse(fs.readFileSync(resolvedPath, "utf-8"));
  return Keypair.fromSecretKey(Uint8Array.from(keypairData));
}

/**
 * Create an Anchor Wallet from a Keypair
 */
export function createWallet(keypair: Keypair): Wallet {
  return new Wallet(keypair);
}

/**
 * Get RPC URL from environment or default to devnet
 */
export function getRpcUrl(cluster?: string): string {
  if (cluster) {
    switch (cluster) {
      case "devnet":
        return "https://api.devnet.solana.com";
      case "mainnet":
      case "mainnet-beta":
        return "https://api.mainnet-beta.solana.com";
      case "localnet":
      case "localhost":
        return "http://localhost:8899";
      default:
        // Assume it's a custom URL
        return cluster;
    }
  }

  return process.env.SOLANA_RPC_URL || "https://api.devnet.solana.com";
}

/**
 * Create a connection to Solana
 */
export function createConnection(cluster?: string): Connection {
  const rpcUrl = getRpcUrl(cluster);
  return new Connection(rpcUrl, "confirmed");
}
