import { Command } from "commander";
import ora from "ora";
import { DaoClient, getDaoConfigPDA } from "@aegis/dao-sdk";
import { loadKeypair, createWallet, createConnection } from "../utils/wallet.js";
import { displayDaoConfig, success, error } from "../utils/display.js";

export const configCommand = new Command("config")
  .description("DAO configuration commands");

configCommand
  .command("show")
  .description("Display current DAO configuration")
  .option("-c, --cluster <cluster>", "Solana cluster (devnet, mainnet, localnet)", "devnet")
  .option("-k, --keypair <path>", "Path to keypair file")
  .action(async (options) => {
    const spinner = ora("Fetching DAO configuration...").start();

    try {
      const connection = createConnection(options.cluster);
      const keypair = loadKeypair(options.keypair);
      const wallet = createWallet(keypair);
      const client = new DaoClient(connection, wallet);

      const config = await client.getDaoConfig();
      spinner.stop();

      displayDaoConfig(config);
    } catch (err) {
      spinner.fail("Failed to fetch DAO configuration");
      error("Could not retrieve DAO config", err as Error);
      process.exit(1);
    }
  });

configCommand
  .command("pda")
  .description("Show DAO config PDA address")
  .action(() => {
    const [pda, bump] = getDaoConfigPDA();
    console.log(`\nDAO Config PDA: ${pda.toString()}`);
    console.log(`Bump: ${bump}\n`);
  });
