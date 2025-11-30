import { Command } from "commander";
import ora from "ora";
import { PublicKey } from "@solana/web3.js";
import BN from "bn.js";
import { DaoClient, VoteChoice, AEGIS_DECIMALS } from "@aegis/dao-sdk";
import { loadKeypair, createWallet, createConnection } from "../utils/wallet.js";
import { displayVoteEscrow, displayVoteRecord, success, error, info } from "../utils/display.js";

export const voteCommand = new Command("vote")
  .description("Voting commands");

voteCommand
  .command("deposit <proposalId>")
  .description("Deposit tokens to vote escrow (required before voting)")
  .requiredOption("-a, --amount <amount>", "Amount of AEGIS tokens to deposit")
  .requiredOption("--token-account <address>", "Your governance token account address")
  .option("-c, --cluster <cluster>", "Solana cluster", "devnet")
  .option("-k, --keypair <path>", "Path to keypair file")
  .action(async (proposalId, options) => {
    const spinner = ora("Depositing vote tokens...").start();

    try {
      const connection = createConnection(options.cluster);
      const keypair = loadKeypair(options.keypair);
      const wallet = createWallet(keypair);
      const client = new DaoClient(connection, wallet);

      const amountWithDecimals = new BN(parseFloat(options.amount) * Math.pow(10, AEGIS_DECIMALS));

      const sig = await client.depositVoteTokens({
        proposalId: parseInt(proposalId),
        amount: amountWithDecimals,
        voterTokenAccount: new PublicKey(options.tokenAccount),
      });

      spinner.stop();
      success(`Deposited ${options.amount} AEGIS tokens to vote escrow!`, sig);
      info("You can now cast your vote using 'aegis-dao vote cast'");
    } catch (err) {
      spinner.fail("Failed to deposit vote tokens");
      error("Could not deposit tokens", err as Error);
      process.exit(1);
    }
  });

voteCommand
  .command("cast <proposalId>")
  .description("Cast your vote on a proposal")
  .requiredOption("--choice <choice>", "Vote choice (for, against, abstain)")
  .option("-c, --cluster <cluster>", "Solana cluster", "devnet")
  .option("-k, --keypair <path>", "Path to keypair file")
  .action(async (proposalId, options) => {
    const spinner = ora("Casting vote...").start();

    try {
      const connection = createConnection(options.cluster);
      const keypair = loadKeypair(options.keypair);
      const wallet = createWallet(keypair);
      const client = new DaoClient(connection, wallet);

      // Map string to VoteChoice enum
      let voteChoice: VoteChoice;
      switch (options.choice.toLowerCase()) {
        case "for":
        case "yes":
          voteChoice = VoteChoice.For;
          break;
        case "against":
        case "no":
          voteChoice = VoteChoice.Against;
          break;
        case "abstain":
          voteChoice = VoteChoice.Abstain;
          break;
        default:
          throw new Error(`Invalid vote choice: ${options.choice}. Use 'for', 'against', or 'abstain'`);
      }

      const sig = await client.castVote({
        proposalId: parseInt(proposalId),
        voteChoice,
      });

      spinner.stop();
      success(`Vote cast successfully! You voted: ${voteChoice.toUpperCase()}`, sig);
    } catch (err) {
      spinner.fail("Failed to cast vote");
      error("Could not cast vote", err as Error);
      process.exit(1);
    }
  });

voteCommand
  .command("retract <proposalId>")
  .description("Retract your vote (allows token withdrawal)")
  .option("-c, --cluster <cluster>", "Solana cluster", "devnet")
  .option("-k, --keypair <path>", "Path to keypair file")
  .action(async (proposalId, options) => {
    const spinner = ora("Retracting vote...").start();

    try {
      const connection = createConnection(options.cluster);
      const keypair = loadKeypair(options.keypair);
      const wallet = createWallet(keypair);
      const client = new DaoClient(connection, wallet);

      const sig = await client.retractVote(parseInt(proposalId));

      spinner.stop();
      success("Vote retracted successfully!", sig);
      info("You can now withdraw your tokens using 'aegis-dao vote withdraw'");
    } catch (err) {
      spinner.fail("Failed to retract vote");
      error("Could not retract vote", err as Error);
      process.exit(1);
    }
  });

voteCommand
  .command("withdraw <proposalId>")
  .description("Withdraw escrowed vote tokens")
  .requiredOption("--token-account <address>", "Your governance token account address")
  .option("-c, --cluster <cluster>", "Solana cluster", "devnet")
  .option("-k, --keypair <path>", "Path to keypair file")
  .action(async (proposalId, options) => {
    const spinner = ora("Withdrawing vote tokens...").start();

    try {
      const connection = createConnection(options.cluster);
      const keypair = loadKeypair(options.keypair);
      const wallet = createWallet(keypair);
      const client = new DaoClient(connection, wallet);

      const sig = await client.withdrawVoteTokens(
        parseInt(proposalId),
        new PublicKey(options.tokenAccount)
      );

      spinner.stop();
      success("Vote tokens withdrawn successfully!", sig);
    } catch (err) {
      spinner.fail("Failed to withdraw vote tokens");
      error("Could not withdraw tokens", err as Error);
      process.exit(1);
    }
  });

voteCommand
  .command("escrow <proposalId>")
  .description("Show your vote escrow status for a proposal")
  .option("-c, --cluster <cluster>", "Solana cluster", "devnet")
  .option("-k, --keypair <path>", "Path to keypair file")
  .action(async (proposalId, options) => {
    const spinner = ora("Fetching vote escrow...").start();

    try {
      const connection = createConnection(options.cluster);
      const keypair = loadKeypair(options.keypair);
      const wallet = createWallet(keypair);
      const client = new DaoClient(connection, wallet);

      const escrow = await client.getVoteEscrow(parseInt(proposalId), keypair.publicKey);
      spinner.stop();

      if (!escrow) {
        info(`No vote escrow found for proposal #${proposalId}`);
        return;
      }

      displayVoteEscrow(escrow);
    } catch (err) {
      spinner.fail("Failed to fetch vote escrow");
      error("Could not retrieve vote escrow", err as Error);
      process.exit(1);
    }
  });

voteCommand
  .command("record <proposalId>")
  .description("Show your vote record for a proposal")
  .option("-c, --cluster <cluster>", "Solana cluster", "devnet")
  .option("-k, --keypair <path>", "Path to keypair file")
  .action(async (proposalId, options) => {
    const spinner = ora("Fetching vote record...").start();

    try {
      const connection = createConnection(options.cluster);
      const keypair = loadKeypair(options.keypair);
      const wallet = createWallet(keypair);
      const client = new DaoClient(connection, wallet);

      const record = await client.getVoteRecord(parseInt(proposalId), keypair.publicKey);
      spinner.stop();

      if (!record) {
        info(`No vote record found for proposal #${proposalId}`);
        return;
      }

      displayVoteRecord(record);
    } catch (err) {
      spinner.fail("Failed to fetch vote record");
      error("Could not retrieve vote record", err as Error);
      process.exit(1);
    }
  });
