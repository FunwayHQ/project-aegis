# @aegis/dao-app

Web application for AEGIS DAO governance on Solana.

## Features

- **Dashboard**: Overview of DAO stats and active proposals
- **Proposals**: Browse, filter, and view all governance proposals
- **Voting**: Deposit tokens and cast votes on active proposals
- **Treasury**: View treasury balance and accounts

## Supported Wallets

The dApp supports all major Solana wallets:

- Phantom
- Solflare
- Torus
- Ledger
- Coinbase Wallet

## Development

```bash
# Install dependencies
pnpm install

# Start development server
pnpm dev

# Build for production
pnpm build

# Preview production build
pnpm preview
```

## Configuration

### Environment Variables

Create a `.env` file in the app directory:

```env
# Custom RPC endpoint (optional, defaults to devnet)
VITE_SOLANA_RPC_URL=https://api.devnet.solana.com
```

## Architecture

```
src/
├── components/       # Reusable UI components
│   ├── Layout.tsx   # Main layout with header/footer
│   ├── ProposalCard.tsx
│   └── StatCard.tsx
├── contexts/        # React contexts
│   ├── WalletContext.tsx  # Solana wallet adapter
│   └── DaoClientContext.tsx  # DAO SDK client
├── pages/           # Page components
│   ├── Dashboard.tsx
│   ├── Proposals.tsx
│   ├── ProposalDetail.tsx
│   └── Treasury.tsx
├── utils/           # Utility functions
│   └── format.ts    # Formatting helpers
├── App.tsx          # Main app with routing
├── main.tsx         # Entry point
└── index.css        # Global styles (Tailwind)
```

## Tech Stack

- **React 18** - UI framework
- **Vite** - Build tool
- **TypeScript** - Type safety
- **Tailwind CSS** - Styling
- **React Router** - Client-side routing
- **@solana/wallet-adapter** - Wallet integration
- **@aegis/dao-sdk** - DAO client library

## Usage

### Connecting Wallet

1. Click "Select Wallet" in the header
2. Choose your preferred wallet
3. Approve the connection

### Viewing Proposals

1. Navigate to "Proposals" from the header
2. Use the filter buttons to filter by status
3. Click on a proposal to view details

### Voting on a Proposal

1. Connect your wallet
2. Navigate to an active proposal
3. Deposit AEGIS tokens to the vote escrow
4. Select your vote choice (For, Against, Abstain)
5. Submit your vote

### Vote Escrow System

The DAO uses a vote escrow system for flash loan protection:

1. **Deposit**: Lock tokens before voting
2. **Vote**: Cast your vote using deposited tokens
3. **Retract** (optional): Change your vote
4. **Withdraw**: Retrieve tokens after voting ends

## Screenshots

### Dashboard
The dashboard shows an overview of DAO statistics including:
- Total proposals
- Active proposals
- Treasury balance
- Voting period

### Proposal Detail
Each proposal shows:
- Current vote tallies with progress bars
- Proposal metadata (type, proposer, dates)
- Execution data for treasury withdrawals
- Interactive voting interface

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Submit a pull request

## License

MIT
