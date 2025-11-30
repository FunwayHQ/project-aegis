import { Command } from "commander";
import ora from "ora";
import { PublicKey } from "@solana/web3.js";
import BN from "bn.js";
import { DaoClient, ProposalType, ProposalStatus, AEGIS_DECIMALS } from "@aegis/dao-sdk";
import { loadKeypair, createWallet, createConnection } from "../utils/wallet.js";
import { displayProposal, success, error, info, warn } from "../utils/display.js";

export const proposalCommand = new Command("proposal")
  .description("Proposal management commands");

proposalCommand
  .command("list")
  .description("List all proposals")
  .option("-c, --cluster <cluster>", "Solana cluster", "devnet")
  .option("-k, --keypair <path>", "Path to keypair file")
  .option("-s, --status <status>", "Filter by status (active, passed, defeated, executed, cancelled)")
  .option("-v, --verbose", "Show detailed information", false)
  .action(async (options) => {
    const spinner = ora("Fetching proposals...").start();

    try {
      const connection = createConnection(options.cluster);
      const keypair = loadKeypair(options.keypair);
      const wallet = createWallet(keypair);
      const client = new DaoClient(connection, wallet);

      const filter: { status?: ProposalStatus } = {};
      if (options.status) {
        filter.status = options.status as ProposalStatus;
      }

      const proposals = await client.getProposals(filter);
      spinner.stop();

      if (proposals.length === 0) {
        info("No proposals found");
        return;
      }

      console.log(`\nFound ${proposals.length} proposal(s):`);
      for (const proposal of proposals) {
        displayProposal(proposal, options.verbose);
      }
    } catch (err) {
      spinner.fail("Failed to fetch proposals");
      error("Could not retrieve proposals", err as Error);
      process.exit(1);
    }
  });

proposalCommand
  .command("show <id>")
  .description("Show details of a specific proposal")
  .option("-c, --cluster <cluster>", "Solana cluster", "devnet")
  .option("-k, --keypair <path>", "Path to keypair file")
  .action(async (id, options) => {
    const spinner = ora(`Fetching proposal #${id}...`).start();

    try {
      const connection = createConnection(options.cluster);
      const keypair = loadKeypair(options.keypair);
      const wallet = createWallet(keypair);
      const client = new DaoClient(connection, wallet);

      const proposal = await client.getProposal(parseInt(id));
      spinner.stop();

      displayProposal(proposal, true);
    } catch (err) {
      spinner.fail(`Failed to fetch proposal #${id}`);
      error("Could not retrieve proposal", err as Error);
      process.exit(1);
    }
  });

proposalCommand
  .command("create")
  .description("Create a new proposal")
  .requiredOption("-t, --title <title>", "Proposal title")
  .requiredOption("-d, --description-cid <cid>", "IPFS CID for proposal description")
  .requiredOption("--token-account <address>", "Your governance token account address")
  .option("--type <type>", "Proposal type (general, treasuryWithdrawal, parameterChange)", "general")
  .option("--recipient <address>", "Recipient address (for treasury withdrawal)")
  .option("--amount <amount>", "Amount in AEGIS tokens (for treasury withdrawal)")
  .option("-c, --cluster <cluster>", "Solana cluster", "devnet")
  .option("-k, --keypair <path>", "Path to keypair file")
  .action(async (options) => {
    const spinner = ora("Creating proposal...").start();

    try {
      const connection = createConnection(options.cluster);
      const keypair = loadKeypair(options.keypair);
      const wallet = createWallet(keypair);
      const client = new DaoClient(connection, wallet);

      // Map string to ProposalType enum
      let proposalType: ProposalType;
      switch (options.type) {
        case "general":
          proposalType = ProposalType.General;
          break;
        case "treasuryWithdrawal":
          proposalType = ProposalType.TreasuryWithdrawal;
          break;
        case "parameterChange":
          proposalType = ProposalType.ParameterChange;
          break;
        default:
          throw new Error(`Invalid proposal type: ${options.type}`);
      }

      // Prepare execution data for treasury withdrawals
      let executionData: { recipient: PublicKey; amount: BN } | undefined;
      if (proposalType === ProposalType.TreasuryWithdrawal) {
        if (!options.recipient || !options.amount) {
          throw new Error("Treasury withdrawal requires --recipient and --amount");
        }
        const amountWithDecimals = new BN(parseFloat(options.amount) * Math.pow(10, AEGIS_DECIMALS));
        executionData = {
          recipient: new PublicKey(options.recipient),
          amount: amountWithDecimals,
        };
      }

      const sig = await client.createProposal({
        title: options.title,
        descriptionCid: options.descriptionCid,
        proposalType,
        executionData,
        proposerTokenAccount: new PublicKey(options.tokenAccount),
      });

      spinner.stop();
      success("Proposal created successfully!", sig);
    } catch (err) {
      spinner.fail("Failed to create proposal");
      error("Could not create proposal", err as Error);
      process.exit(1);
    }
  });

proposalCommand
  .command("cancel <id>")
  .description("Cancel your own proposal")
  .requiredOption("--token-account <address>", "Your governance token account (to receive bond back)")
  .option("-c, --cluster <cluster>", "Solana cluster", "devnet")
  .option("-k, --keypair <path>", "Path to keypair file")
  .action(async (id, options) => {
    const spinner = ora(`Cancelling proposal #${id}...`).start();

    try {
      const connection = createConnection(options.cluster);
      const keypair = loadKeypair(options.keypair);
      const wallet = createWallet(keypair);
      const client = new DaoClient(connection, wallet);

      const sig = await client.cancelProposal(
        parseInt(id),
        new PublicKey(options.tokenAccount)
      );

      spinner.stop();
      success(`Proposal #${id} cancelled successfully!`, sig);
    } catch (err) {
      spinner.fail(`Failed to cancel proposal #${id}`);
      error("Could not cancel proposal", err as Error);
      process.exit(1);
    }
  });

proposalCommand
  .command("finalize <id>")
  .description("Finalize a proposal after voting ends")
  .option("-c, --cluster <cluster>", "Solana cluster", "devnet")
  .option("-k, --keypair <path>", "Path to keypair file")
  .action(async (id, options) => {
    const spinner = ora(`Finalizing proposal #${id}...`).start();

    try {
      const connection = createConnection(options.cluster);
      const keypair = loadKeypair(options.keypair);
      const wallet = createWallet(keypair);
      const client = new DaoClient(connection, wallet);

      const sig = await client.finalizeProposal(parseInt(id));

      spinner.stop();
      success(`Proposal #${id} finalized successfully!`, sig);
    } catch (err) {
      spinner.fail(`Failed to finalize proposal #${id}`);
      error("Could not finalize proposal", err as Error);
      process.exit(1);
    }
  });

proposalCommand
  .command("execute <id>")
  .description("Execute a passed treasury withdrawal proposal")
  .requiredOption("--recipient-token-account <address>", "Recipient token account address")
  .option("-c, --cluster <cluster>", "Solana cluster", "devnet")
  .option("-k, --keypair <path>", "Path to keypair file")
  .action(async (id, options) => {
    const spinner = ora(`Executing proposal #${id}...`).start();

    try {
      const connection = createConnection(options.cluster);
      const keypair = loadKeypair(options.keypair);
      const wallet = createWallet(keypair);
      const client = new DaoClient(connection, wallet);

      const sig = await client.executeProposal(
        parseInt(id),
        new PublicKey(options.recipientTokenAccount)
      );

      spinner.stop();
      success(`Proposal #${id} executed successfully!`, sig);
    } catch (err) {
      spinner.fail(`Failed to execute proposal #${id}`);
      error("Could not execute proposal", err as Error);
      process.exit(1);
    }
  });

proposalCommand
  .command("return-bond <id>")
  .description("Return proposal bond for a passed proposal")
  .requiredOption("--token-account <address>", "Proposer token account address")
  .option("-c, --cluster <cluster>", "Solana cluster", "devnet")
  .option("-k, --keypair <path>", "Path to keypair file")
  .action(async (id, options) => {
    const spinner = ora(`Returning bond for proposal #${id}...`).start();

    try {
      const connection = createConnection(options.cluster);
      const keypair = loadKeypair(options.keypair);
      const wallet = createWallet(keypair);
      const client = new DaoClient(connection, wallet);

      const sig = await client.returnProposalBond(
        parseInt(id),
        new PublicKey(options.tokenAccount)
      );

      spinner.stop();
      success(`Bond returned successfully for proposal #${id}!`, sig);
    } catch (err) {
      spinner.fail(`Failed to return bond for proposal #${id}`);
      error("Could not return proposal bond", err as Error);
      process.exit(1);
    }
  });
