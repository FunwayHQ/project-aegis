#!/usr/bin/env node

import { Command } from "commander";
import { configCommand } from "./commands/config.js";
import { proposalCommand } from "./commands/proposal.js";
import { voteCommand } from "./commands/vote.js";
import { treasuryCommand } from "./commands/treasury.js";
import { adminCommand } from "./commands/admin.js";

const program = new Command();

program
  .name("aegis-dao")
  .description("AEGIS DAO CLI - Manage DAO governance on Solana")
  .version("0.1.0");

// Register command groups
program.addCommand(configCommand);
program.addCommand(proposalCommand);
program.addCommand(voteCommand);
program.addCommand(treasuryCommand);
program.addCommand(adminCommand);

program.parse();
