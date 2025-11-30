import { PublicKey } from "@solana/web3.js";
import BN from "bn.js";
import { DAO_PROGRAM_ID, SEEDS } from "./constants";

/**
 * Derive the DAO Config PDA
 * Seeds: ["dao_config"]
 */
export function getDaoConfigPDA(
  programId: PublicKey = DAO_PROGRAM_ID
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync([SEEDS.DAO_CONFIG], programId);
}

/**
 * Derive a Proposal PDA
 * Seeds: ["proposal", proposal_id.to_le_bytes()]
 */
export function getProposalPDA(
  proposalId: BN | number | bigint,
  programId: PublicKey = DAO_PROGRAM_ID
): [PublicKey, number] {
  const id = toBN(proposalId);
  return PublicKey.findProgramAddressSync(
    [SEEDS.PROPOSAL, id.toArrayLike(Buffer, "le", 8)],
    programId
  );
}

/**
 * Derive a Vote Escrow PDA
 * Seeds: ["vote_escrow", proposal_id.to_le_bytes(), voter.as_ref()]
 */
export function getVoteEscrowPDA(
  proposalId: BN | number | bigint,
  voter: PublicKey,
  programId: PublicKey = DAO_PROGRAM_ID
): [PublicKey, number] {
  const id = toBN(proposalId);
  return PublicKey.findProgramAddressSync(
    [SEEDS.VOTE_ESCROW, id.toArrayLike(Buffer, "le", 8), voter.toBuffer()],
    programId
  );
}

/**
 * Derive a Vote Record PDA
 * Seeds: ["vote", proposal_id.to_le_bytes(), voter.as_ref()]
 */
export function getVoteRecordPDA(
  proposalId: BN | number | bigint,
  voter: PublicKey,
  programId: PublicKey = DAO_PROGRAM_ID
): [PublicKey, number] {
  const id = toBN(proposalId);
  return PublicKey.findProgramAddressSync(
    [SEEDS.VOTE_RECORD, id.toArrayLike(Buffer, "le", 8), voter.toBuffer()],
    programId
  );
}

/**
 * Helper to convert various number types to BN
 */
function toBN(value: BN | number | bigint): BN {
  if (BN.isBN(value)) {
    return value;
  }
  if (typeof value === "bigint") {
    return new BN(value.toString());
  }
  return new BN(value);
}
