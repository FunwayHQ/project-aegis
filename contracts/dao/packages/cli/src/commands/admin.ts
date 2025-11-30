import { Command } from "commander";
import ora from "ora";
import BN from "bn.js";
import { DaoClient, AEGIS_DECIMALS } from "@aegis/dao-sdk";
import { loadKeypair, createWallet, createConnection } from "../utils/wallet.js";
import { success, error, warn, info } from "../utils/display.js";

export const adminCommand = new Command("admin")
  .description("Admin commands (authority only)");

adminCommand
  .command("pause")
  .description("Pause the DAO (emergency stop)")
  .option("-c, --cluster <cluster>", "Solana cluster", "devnet")
  .option("-k, --keypair <path>", "Path to keypair file")
  .action(async (options) => {
    const spinner = ora("Pausing DAO...").start();

    try {
      const connection = createConnection(options.cluster);
      const keypair = loadKeypair(options.keypair);
      const wallet = createWallet(keypair);
      const client = new DaoClient(connection, wallet);

      const sig = await client.setDaoPaused(true);

      spinner.stop();
      success("DAO paused successfully!", sig);
      warn("The DAO is now in emergency pause mode. Most operations are disabled.");
    } catch (err) {
      spinner.fail("Failed to pause DAO");
      error("Could not pause DAO", err as Error);
      process.exit(1);
    }
  });

adminCommand
  .command("unpause")
  .description("Unpause the DAO (resume operations)")
  .option("-c, --cluster <cluster>", "Solana cluster", "devnet")
  .option("-k, --keypair <path>", "Path to keypair file")
  .action(async (options) => {
    const spinner = ora("Unpausing DAO...").start();

    try {
      const connection = createConnection(options.cluster);
      const keypair = loadKeypair(options.keypair);
      const wallet = createWallet(keypair);
      const client = new DaoClient(connection, wallet);

      const sig = await client.setDaoPaused(false);

      spinner.stop();
      success("DAO unpaused successfully!", sig);
      info("Normal DAO operations have resumed.");
    } catch (err) {
      spinner.fail("Failed to unpause DAO");
      error("Could not unpause DAO", err as Error);
      process.exit(1);
    }
  });

adminCommand
  .command("queue-config-update")
  .description("Queue a config update (subject to 48h timelock)")
  .option("--voting-period <seconds>", "New voting period in seconds")
  .option("--proposal-bond <amount>", "New proposal bond in AEGIS tokens")
  .option("--quorum <percentage>", "New quorum percentage (1-100)")
  .option("--approval-threshold <percentage>", "New approval threshold percentage (1-100)")
  .option("-c, --cluster <cluster>", "Solana cluster", "devnet")
  .option("-k, --keypair <path>", "Path to keypair file")
  .action(async (options) => {
    // Check that at least one option is provided
    if (!options.votingPeriod && !options.proposalBond && !options.quorum && !options.approvalThreshold) {
      error("At least one config parameter must be specified");
      process.exit(1);
    }

    const spinner = ora("Queueing config update...").start();

    try {
      const connection = createConnection(options.cluster);
      const keypair = loadKeypair(options.keypair);
      const wallet = createWallet(keypair);
      const client = new DaoClient(connection, wallet);

      const params: {
        newVotingPeriod?: BN;
        newProposalBond?: BN;
        newQuorumPercentage?: number;
        newApprovalThreshold?: number;
      } = {};

      if (options.votingPeriod) {
        params.newVotingPeriod = new BN(parseInt(options.votingPeriod));
      }
      if (options.proposalBond) {
        params.newProposalBond = new BN(parseFloat(options.proposalBond) * Math.pow(10, AEGIS_DECIMALS));
      }
      if (options.quorum) {
        params.newQuorumPercentage = parseInt(options.quorum);
      }
      if (options.approvalThreshold) {
        params.newApprovalThreshold = parseInt(options.approvalThreshold);
      }

      const sig = await client.queueConfigUpdate(params);

      spinner.stop();
      success("Config update queued successfully!", sig);
      info("The update will be executable after 48 hours.");
    } catch (err) {
      spinner.fail("Failed to queue config update");
      error("Could not queue config update", err as Error);
      process.exit(1);
    }
  });

adminCommand
  .command("execute-config-update")
  .description("Execute a queued config update (after 48h timelock)")
  .option("-c, --cluster <cluster>", "Solana cluster", "devnet")
  .option("-k, --keypair <path>", "Path to keypair file")
  .action(async (options) => {
    const spinner = ora("Executing config update...").start();

    try {
      const connection = createConnection(options.cluster);
      const keypair = loadKeypair(options.keypair);
      const wallet = createWallet(keypair);
      const client = new DaoClient(connection, wallet);

      const sig = await client.executeConfigUpdate();

      spinner.stop();
      success("Config update executed successfully!", sig);
    } catch (err) {
      spinner.fail("Failed to execute config update");
      error("Could not execute config update", err as Error);
      process.exit(1);
    }
  });

adminCommand
  .command("cancel-config-update")
  .description("Cancel a queued config update")
  .option("-c, --cluster <cluster>", "Solana cluster", "devnet")
  .option("-k, --keypair <path>", "Path to keypair file")
  .action(async (options) => {
    const spinner = ora("Cancelling config update...").start();

    try {
      const connection = createConnection(options.cluster);
      const keypair = loadKeypair(options.keypair);
      const wallet = createWallet(keypair);
      const client = new DaoClient(connection, wallet);

      const sig = await client.cancelConfigUpdate();

      spinner.stop();
      success("Config update cancelled successfully!", sig);
    } catch (err) {
      spinner.fail("Failed to cancel config update");
      error("Could not cancel config update", err as Error);
      process.exit(1);
    }
  });

adminCommand
  .command("close-dao")
  .description("Close DAO config account (DESTRUCTIVE - for migration only)")
  .option("-c, --cluster <cluster>", "Solana cluster", "devnet")
  .option("-k, --keypair <path>", "Path to keypair file")
  .option("--confirm", "Confirm this destructive action")
  .action(async (options) => {
    if (!options.confirm) {
      warn("This is a destructive operation that will close the DAO config account.");
      info("Add --confirm flag to proceed.");
      process.exit(1);
    }

    const spinner = ora("Closing DAO config...").start();

    try {
      const connection = createConnection(options.cluster);
      const keypair = loadKeypair(options.keypair);
      const wallet = createWallet(keypair);
      const client = new DaoClient(connection, wallet);

      const sig = await client.closeDaoConfig();

      spinner.stop();
      success("DAO config closed successfully!", sig);
      warn("The DAO config account has been closed. Rent was returned to authority.");
    } catch (err) {
      spinner.fail("Failed to close DAO config");
      error("Could not close DAO config", err as Error);
      process.exit(1);
    }
  });
