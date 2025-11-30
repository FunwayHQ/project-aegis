# @aegis/dao-cli

Command-line interface for AEGIS DAO governance on Solana.

## Installation

```bash
# Install globally
npm install -g @aegis/dao-cli

# Or run directly with npx
npx @aegis/dao-cli
```

## Configuration

The CLI uses your Solana keypair for signing transactions. By default, it looks for your keypair at `~/.config/solana/id.json`. You can specify a different keypair with the `--keypair` option.

### Environment Variables

- `SOLANA_RPC_URL` - Custom RPC endpoint (defaults to devnet)

## Commands

### DAO Configuration

```bash
# Show current DAO configuration
aegis-dao config show

# Show DAO config PDA address
aegis-dao config pda
```

### Proposals

```bash
# List all proposals
aegis-dao proposal list
aegis-dao proposal list --status active
aegis-dao proposal list --verbose

# Show a specific proposal
aegis-dao proposal show <id>

# Create a new proposal
aegis-dao proposal create \
  --title "My Proposal" \
  --description-cid "QmXxx..." \
  --token-account <your-token-account>

# Create a treasury withdrawal proposal
aegis-dao proposal create \
  --title "Fund Development" \
  --description-cid "QmXxx..." \
  --type treasuryWithdrawal \
  --recipient <recipient-address> \
  --amount 1000 \
  --token-account <your-token-account>

# Cancel your proposal
aegis-dao proposal cancel <id> --token-account <your-token-account>

# Finalize a proposal after voting ends
aegis-dao proposal finalize <id>

# Execute a passed treasury withdrawal
aegis-dao proposal execute <id> --recipient-token-account <address>

# Return proposal bond for passed proposals
aegis-dao proposal return-bond <id> --token-account <proposer-token-account>
```

### Voting

The voting process uses a vote escrow system for flash loan protection:

1. **Deposit** tokens to the vote escrow
2. **Cast** your vote
3. **Retract** vote (optional, to change or withdraw)
4. **Withdraw** tokens after proposal ends

```bash
# Deposit tokens to vote on a proposal
aegis-dao vote deposit <proposalId> \
  --amount 100 \
  --token-account <your-token-account>

# Cast your vote
aegis-dao vote cast <proposalId> --choice for
aegis-dao vote cast <proposalId> --choice against
aegis-dao vote cast <proposalId> --choice abstain

# Retract your vote (allows changing or withdrawing)
aegis-dao vote retract <proposalId>

# Withdraw your tokens after voting ends
aegis-dao vote withdraw <proposalId> --token-account <your-token-account>

# Check your vote escrow status
aegis-dao vote escrow <proposalId>

# Check your vote record
aegis-dao vote record <proposalId>
```

### Treasury

```bash
# Show treasury balance
aegis-dao treasury balance

# Show detailed treasury info
aegis-dao treasury info

# Deposit tokens to treasury
aegis-dao treasury deposit \
  --amount 100 \
  --token-account <your-token-account>
```

### Admin Commands (Authority Only)

```bash
# Pause the DAO (emergency stop)
aegis-dao admin pause

# Unpause the DAO
aegis-dao admin unpause

# Queue a config update (48h timelock)
aegis-dao admin queue-config-update \
  --voting-period 259200 \
  --quorum 15

# Execute queued config update
aegis-dao admin execute-config-update

# Cancel queued config update
aegis-dao admin cancel-config-update

# Close DAO config (destructive - for migration)
aegis-dao admin close-dao --confirm
```

## Global Options

All commands support the following options:

- `-c, --cluster <cluster>` - Solana cluster: devnet, mainnet, localnet, or custom URL (default: devnet)
- `-k, --keypair <path>` - Path to keypair file (default: ~/.config/solana/id.json)

## Examples

### Complete Voting Workflow

```bash
# 1. Check proposals
aegis-dao proposal list --status active

# 2. View proposal details
aegis-dao proposal show 1

# 3. Deposit tokens for voting
aegis-dao vote deposit 1 --amount 500 --token-account 4WGq...

# 4. Cast your vote
aegis-dao vote cast 1 --choice for

# 5. Wait for voting period to end...

# 6. Withdraw tokens after proposal ends
aegis-dao vote withdraw 1 --token-account 4WGq...
```

### Creating a Treasury Withdrawal Proposal

```bash
# 1. Create the proposal
aegis-dao proposal create \
  --title "Fund Marketing Initiative" \
  --description-cid "QmMarketingProposal123" \
  --type treasuryWithdrawal \
  --recipient HXyx...recipient \
  --amount 5000 \
  --token-account 4WGq...proposer

# 2. After proposal passes and voting ends, finalize it
aegis-dao proposal finalize 2

# 3. Execute the treasury withdrawal
aegis-dao proposal execute 2 --recipient-token-account HXyx...tokenAccount

# 4. Return the proposal bond
aegis-dao proposal return-bond 2 --token-account 4WGq...proposer
```

## Error Handling

The CLI provides helpful error messages and exit codes:

- Exit code 0: Success
- Exit code 1: Error (with description)

Common errors:
- `Keypair file not found` - Run `solana-keygen new` or specify `--keypair`
- `Account does not exist` - The on-chain account hasn't been created yet
- `InsufficientFunds` - Not enough tokens for the operation

## Development

```bash
# Build the CLI
pnpm build

# Run locally
pnpm start --help

# Watch mode
pnpm dev
```
