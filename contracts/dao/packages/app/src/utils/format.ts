import BN from "bn.js";
import { PublicKey } from "@solana/web3.js";
import { AEGIS_DECIMALS } from "@aegis/dao-sdk";

/**
 * Format token amount with decimals
 */
export function formatTokenAmount(
  amount: BN | bigint | number,
  decimals: number = AEGIS_DECIMALS
): string {
  const amountBn = BN.isBN(amount)
    ? amount
    : new BN(typeof amount === "bigint" ? amount.toString() : amount);
  const divisor = new BN(10).pow(new BN(decimals));
  const whole = amountBn.div(divisor);
  const remainder = amountBn.mod(divisor);

  if (remainder.isZero()) {
    return whole.toString();
  }

  const remainderStr = remainder.toString().padStart(decimals, "0");
  const trimmed = remainderStr.replace(/0+$/, "");
  return `${whole}.${trimmed}`;
}

/**
 * Parse token amount from string to BN with decimals
 */
export function parseTokenAmount(
  amount: string,
  decimals: number = AEGIS_DECIMALS
): BN {
  const [whole, fraction = ""] = amount.split(".");
  const paddedFraction = fraction.padEnd(decimals, "0").slice(0, decimals);
  return new BN(whole + paddedFraction);
}

/**
 * Format timestamp to human readable
 */
export function formatTimestamp(timestamp: BN | number): string {
  const ts = BN.isBN(timestamp) ? timestamp.toNumber() : timestamp;
  const date = new Date(ts * 1000);
  return date.toLocaleDateString("en-US", {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/**
 * Format duration in seconds to human readable
 */
export function formatDuration(seconds: number): string {
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);

  const parts: string[] = [];
  if (days > 0) parts.push(`${days}d`);
  if (hours > 0) parts.push(`${hours}h`);
  if (minutes > 0) parts.push(`${minutes}m`);

  return parts.length > 0 ? parts.join(" ") : "0m";
}

/**
 * Shorten a public key for display
 */
export function shortAddress(pubkey: PublicKey | string, chars: number = 4): string {
  const str = typeof pubkey === "string" ? pubkey : pubkey.toString();
  return `${str.slice(0, chars)}...${str.slice(-chars)}`;
}

/**
 * Format percentage
 */
export function formatPercentage(value: number): string {
  return `${value.toFixed(2)}%`;
}

/**
 * Calculate vote percentages
 */
export function calculateVotePercentages(
  forVotes: BN,
  againstVotes: BN,
  abstainVotes: BN
): { for: number; against: number; abstain: number } {
  const total = forVotes.add(againstVotes).add(abstainVotes);

  if (total.isZero()) {
    return { for: 0, against: 0, abstain: 0 };
  }

  return {
    for: forVotes.mul(new BN(10000)).div(total).toNumber() / 100,
    against: againstVotes.mul(new BN(10000)).div(total).toNumber() / 100,
    abstain: abstainVotes.mul(new BN(10000)).div(total).toNumber() / 100,
  };
}

/**
 * Get time remaining until timestamp
 */
export function getTimeRemaining(endTimestamp: BN | number): string {
  const end = BN.isBN(endTimestamp) ? endTimestamp.toNumber() : endTimestamp;
  const now = Math.floor(Date.now() / 1000);
  const remaining = end - now;

  if (remaining <= 0) {
    return "Ended";
  }

  return formatDuration(remaining);
}
