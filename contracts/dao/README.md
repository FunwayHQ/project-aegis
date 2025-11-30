# AEGIS DAO

Governance smart contracts and client tools for the AEGIS decentralized edge network.

## Overview

The AEGIS DAO enables community governance through:
- **Proposals**: Create and vote on network changes
- **Voting**: Deposit tokens to vote escrow, cast weighted votes
- **Treasury**: Manage community funds via governance proposals
- **Security**: Flash loan protection via snapshot-based voting and 48-hour timelocks

## Packages

| Package | Description |
|---------|-------------|
| `@aegis/dao-sdk` | TypeScript SDK with DaoClient, PDA helpers, and types |
| `@aegis/dao-cli` | Command-line interface for DAO operations |
| `@aegis/dao-app` | React dApp for web-based governance |

## Quick Start

```bash
# Install dependencies
pnpm install

# Build all packages
pnpm build

# Run tests (118 tests across all packages)
pnpm test
```

## CLI Usage

```bash
# Show DAO configuration
aegis-dao config show

# List all proposals
aegis-dao proposal list

# Get proposal details
aegis-dao proposal get 1

# Create a proposal (requires wallet)
aegis-dao proposal create \
  --type general \
  --title "My Proposal" \
  --description-cid "QmXxx..."

# Vote on a proposal
aegis-dao vote deposit 1 --amount 100
aegis-dao vote cast 1 --choice for
aegis-dao vote withdraw 1
```

## dApp

```bash
# Development server
cd packages/app
pnpm dev

# Production build
pnpm build
```

Open http://localhost:5173 and connect your Solana wallet.

## Smart Contract

- **Program ID**: `9zQDZPNyDqVxevUAwaWTGGvCGwLSpfvkMn6aDKx7x6hz`
- **Network**: Devnet

### Instructions

| Instruction | Description |
|-------------|-------------|
| `initialize` | Initialize DAO with governance token |
| `deposit_to_treasury` | Deposit tokens to treasury |
| `create_proposal` | Create a new governance proposal |
| `create_treasury_proposal` | Create treasury withdrawal proposal |
| `register_vote_snapshot` | Register voting power snapshot |
| `deposit_to_vote_escrow` | Deposit tokens for voting |
| `cast_vote` | Cast vote (for/against/abstain) |
| `retract_vote` | Retract vote before end |
| `withdraw_from_escrow` | Withdraw after voting ends |
| `finalize_proposal` | Finalize and determine outcome |
| `execute_treasury_proposal` | Execute approved treasury withdrawal |
| `cancel_proposal` | Cancel proposal (proposer only) |
| `refund_bond` | Refund proposer bond |
| `queue_config_update` | Queue parameter change (48h timelock) |
| `execute_config_update` | Execute queued config change |
| `set_paused` | Emergency pause/unpause |

## Security Features

- **Flash Loan Protection**: Snapshot-based voting power
- **Timelock**: 48-hour delay for configuration changes
- **Token Validation**: Account ownership and mint verification
- **Recipient Validation**: Treasury withdrawal checks
- **Bond System**: Stake required to create proposals

## Development

```bash
# Build smart contract
anchor build

# Run contract tests
anchor test

# Deploy to devnet
anchor deploy --provider.cluster devnet
```

## License

MIT
