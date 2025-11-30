import { Command } from "commander";
import ora from "ora";
import { PublicKey } from "@solana/web3.js";
import BN from "bn.js";
import { DaoClient, AEGIS_DECIMALS } from "@aegis/dao-sdk";
import { loadKeypair, createWallet, createConnection } from "../utils/wallet.js";
import { formatTokenAmount, success, error, info } from "../utils/display.js";

export const treasuryCommand = new Command("treasury")
  .description("Treasury management commands");

treasuryCommand
  .command("balance")
  .description("Show treasury balance")
  .option("-c, --cluster <cluster>", "Solana cluster", "devnet")
  .option("-k, --keypair <path>", "Path to keypair file")
  .action(async (options) => {
    const spinner = ora("Fetching treasury balance...").start();

    try {
      const connection = createConnection(options.cluster);
      const keypair = loadKeypair(options.keypair);
      const wallet = createWallet(keypair);
      const client = new DaoClient(connection, wallet);

      const balance = await client.getTreasuryBalance();
      spinner.stop();

      console.log(`\nTreasury Balance: ${formatTokenAmount(new BN(balance.toString()))} AEGIS\n`);
    } catch (err) {
      spinner.fail("Failed to fetch treasury balance");
      error("Could not retrieve treasury balance", err as Error);
      process.exit(1);
    }
  });

treasuryCommand
  .command("deposit")
  .description("Deposit tokens to the treasury")
  .requiredOption("-a, --amount <amount>", "Amount of AEGIS tokens to deposit")
  .requiredOption("--token-account <address>", "Your governance token account address")
  .option("-c, --cluster <cluster>", "Solana cluster", "devnet")
  .option("-k, --keypair <path>", "Path to keypair file")
  .action(async (options) => {
    const spinner = ora("Depositing to treasury...").start();

    try {
      const connection = createConnection(options.cluster);
      const keypair = loadKeypair(options.keypair);
      const wallet = createWallet(keypair);
      const client = new DaoClient(connection, wallet);

      const amountWithDecimals = new BN(parseFloat(options.amount) * Math.pow(10, AEGIS_DECIMALS));

      const sig = await client.depositToTreasury({
        amount: amountWithDecimals,
        depositorTokenAccount: new PublicKey(options.tokenAccount),
      });

      spinner.stop();
      success(`Deposited ${options.amount} AEGIS tokens to treasury!`, sig);
    } catch (err) {
      spinner.fail("Failed to deposit to treasury");
      error("Could not deposit tokens", err as Error);
      process.exit(1);
    }
  });

treasuryCommand
  .command("info")
  .description("Show detailed treasury information")
  .option("-c, --cluster <cluster>", "Solana cluster", "devnet")
  .option("-k, --keypair <path>", "Path to keypair file")
  .action(async (options) => {
    const spinner = ora("Fetching treasury info...").start();

    try {
      const connection = createConnection(options.cluster);
      const keypair = loadKeypair(options.keypair);
      const wallet = createWallet(keypair);
      const client = new DaoClient(connection, wallet);

      const config = await client.getDaoConfig();
      const balance = await client.getTreasuryBalance();

      spinner.stop();

      console.log("\nTreasury Information");
      console.log("─".repeat(50));
      console.log(`  Address:           ${config.treasury.toString()}`);
      console.log(`  Current Balance:   ${formatTokenAmount(new BN(balance.toString()))} AEGIS`);
      console.log(`  Total Deposits:    ${formatTokenAmount(config.totalTreasuryDeposits)} AEGIS`);
      console.log(`  Bond Escrow:       ${config.bondEscrow.toString()}`);
      console.log();
    } catch (err) {
      spinner.fail("Failed to fetch treasury info");
      error("Could not retrieve treasury information", err as Error);
      process.exit(1);
    }
  });
