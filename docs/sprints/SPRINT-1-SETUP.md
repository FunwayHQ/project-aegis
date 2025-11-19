# Sprint 1: Architecture & Solana Setup

## Objective
Define precise Solana architecture, set up development environments, and begin basic Solana program (smart contract) development.

## Deliverables
1. ✓ Detailed Solana program design for $AEGIS token
2. ⏳ Development environment setup for Rust (node) and Anchor (Solana)
3. ⏳ Initial $AEGIS token program deployed to Devnet
4. ⏳ Rust node basic HTTP server proof-of-concept

## Prerequisites

### System Requirements
- Windows 10/11 or Linux
- 8GB RAM minimum (16GB recommended)
- 20GB free disk space
- Stable internet connection

### Required Software

#### 1. Rust Toolchain
```bash
# Windows (via winget)
winget install Rustlang.Rustup

# Linux/macOS
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

After installation, verify:
```bash
rustc --version  # Should show 1.70+
cargo --version
```

**Important**: On Windows, close and reopen your terminal after Rust installation to refresh PATH.

#### 2. Solana CLI
```bash
# Windows (PowerShell as Administrator)
cmd /c "curl https://release.solana.com/v1.18.0/solana-install-init-x86_64-pc-windows-msvc.exe --output C:\solana-install-tmp\solana-install-init.exe --create-dirs"

# Linux/macOS
sh -c "$(curl -sSfL https://release.solana.com/v1.18.0/install)"
```

Verify installation:
```bash
solana --version  # Should show 1.18.0+
```

Configure for Devnet:
```bash
solana config set --url https://api.devnet.solana.com
solana config get  # Verify configuration
```

#### 3. Anchor Framework
Anchor requires Node.js 18+ first:

```bash
# Install Node.js (via winget on Windows)
winget install OpenJS.NodeJS.LTS

# Or download from: https://nodejs.org/
```

Then install Anchor CLI:
```bash
# Via Cargo
cargo install --git https://github.com/coral-xyz/anchor --tag v0.29.0 anchor-cli

# Verify
anchor --version  # Should show 0.29.0+
```

#### 4. Additional Tools
```bash
# Solana Program Library (SPL) CLI for token operations
cargo install spl-token-cli

# Verify
spl-token --version
```

## Solana Wallet Setup

### 1. Generate Keypair
```bash
# Create a new filesystem wallet for development
solana-keygen new --outfile ~/.config/solana/devnet-wallet.json

# Set as default
solana config set --keypair ~/.config/solana/devnet-wallet.json
```

**IMPORTANT**: This generates a new wallet. Save the seed phrase shown! For Devnet only, not for mainnet funds.

### 2. Fund Devnet Wallet
```bash
# Get your wallet address
solana address

# Request airdrop (2 SOL, can repeat)
solana airdrop 2

# Check balance
solana balance
```

If airdrop fails, use the web faucet: https://faucet.solana.com/

## $AEGIS Token Architecture Design

### Token Specifications

**Token Name**: Aegis
**Symbol**: $AEGIS
**Decimals**: 9 (standard for Solana)
**Total Supply**: 1,000,000,000 (1 billion tokens)
**Standard**: SPL Token (Solana Program Library)

### Supply Distribution (Initial Plan)
- **50%** - Node Operator Rewards Pool (500M $AEGIS)
- **20%** - Ecosystem Development Fund (200M $AEGIS)
- **15%** - Team & Advisors (150M $AEGIS, 4-year vest)
- **10%** - Initial Liquidity/Sale (100M $AEGIS)
- **5%** - Staking Rewards Reserve (50M $AEGIS)

### Token Program Structure

We'll use Anchor framework to create a custom token program with enhanced features:

```
contracts/token/
├── Anchor.toml              # Anchor project config
├── Cargo.toml               # Rust dependencies
├── programs/
│   └── aegis-token/
│       ├── Cargo.toml
│       └── src/
│           └── lib.rs       # Main token program
├── tests/
│   └── aegis-token.ts       # TypeScript tests
└── migrations/
    └── deploy.ts            # Deployment scripts
```

### Smart Contract Functions

#### Core Instructions

1. **`initialize_mint`**
   - Creates the $AEGIS token mint
   - Sets mint authority (initially controlled, later DAO)
   - Fixed supply: 1B tokens with 9 decimals
   - One-time initialization

2. **`mint_to`**
   - Mints new tokens to specified account
   - Only callable by mint authority
   - Used for initial distribution
   - Can be transferred to DAO governance later

3. **`transfer`**
   - Standard SPL token transfer
   - From one token account to another
   - Checks balance and authority

4. **`burn`**
   - Destroy tokens (decrease supply)
   - For deflationary tokenomics
   - Only token account owner can burn their tokens

5. **`freeze_account`** / **`thaw_account`**
   - Emergency freeze for security
   - Only freeze authority can execute
   - Initially controlled, later DAO-governed

#### Extended Features (Future Sprints)

- Time-locked transfers (vesting schedules)
- On-chain metadata (name, symbol, logo URI)
- Delegated transfer approval
- Multi-signature mint authority

### Anchor Program Structure

```rust
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount};

declare_id!("AEGIS111111111111111111111111111111111111111");

#[program]
pub mod aegis_token {
    use super::*;

    pub fn initialize_mint(
        ctx: Context<InitializeMint>,
        decimals: u8,
    ) -> Result<()> {
        // Initialize the $AEGIS token mint
        // Set mint authority, freeze authority
        // Emit initialization event
    }

    pub fn mint_to(
        ctx: Context<MintTo>,
        amount: u64,
    ) -> Result<()> {
        // Mint tokens to recipient
        // Check mint authority
        // Enforce supply cap
    }

    // Additional instructions...
}

#[derive(Accounts)]
pub struct InitializeMint<'info> {
    #[account(mut)]
    pub mint: Account<'info, Mint>,
    pub mint_authority: Signer<'info>,
    pub rent: Sysvar<'info, Rent>,
    pub token_program: Program<'info, Token>,
}

// Additional account structs...
```

### Program Deployment Flow

1. Build the Anchor program
2. Deploy to Devnet
3. Initialize mint with 1B supply cap
4. Create initial token accounts for distribution pools
5. Mint initial allocations
6. Generate IDL for client integration
7. Verify deployment with tests

## Rust HTTP Server Architecture

For the basic proof-of-concept in Sprint 1, we'll create a minimal HTTP server that will eventually become the River proxy.

### Technology Choice: Tokio + Hyper

While Pingora is our target, we'll start with Tokio/Hyper for Sprint 1 PoC because:
- Easier setup and learning curve
- Establishes Rust HTTP fundamentals
- Migration to Pingora in Sprint 3 will be straightforward

### Server Features (Sprint 1 PoC)

```
node/
├── Cargo.toml
└── src/
    ├── main.rs          # Entry point
    ├── server.rs        # HTTP server logic
    └── config.rs        # Configuration management
```

**Capabilities**:
- Listen on port 8080 (HTTP)
- Respond to GET /health with status
- Basic logging to stdout
- Graceful shutdown handling
- Configuration from TOML file

### Sample Implementation

```rust
// node/src/main.rs
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server};
use std::convert::Infallible;

async fn handle_request(_req: Request<Body>) -> Result<Response<Body>, Infallible> {
    Ok(Response::new(Body::from("AEGIS Node - Sprint 1 PoC")))
}

#[tokio::main]
async fn main() {
    let addr = ([127, 0, 0, 1], 8080).into();

    let make_svc = make_service_fn(|_conn| async {
        Ok::<_, Infallible>(service_fn(handle_request))
    });

    let server = Server::bind(&addr).serve(make_svc);

    println!("AEGIS Node listening on http://{}", addr);

    if let Err(e) = server.await {
        eprintln!("Server error: {}", e);
    }
}
```

## Directory Structure After Sprint 1

```
AEGIS/
├── contracts/
│   └── token/                    # Anchor project for $AEGIS token
│       ├── Anchor.toml
│       ├── Cargo.toml
│       ├── programs/
│       │   └── aegis-token/
│       │       └── src/lib.rs
│       ├── tests/
│       └── target/               # Build output
├── node/
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs               # HTTP server PoC
│       ├── server.rs
│       └── config.rs
├── cli/                          # Future: Node operator CLI
├── docs/
│   ├── sprints/
│   │   └── SPRINT-1-SETUP.md     # This file
│   └── architecture/
├── CLAUDE.md
├── README.md
└── .gitignore
```

## Sprint 1 Checklist

### Environment Setup
- [ ] Install Rust toolchain (rustc 1.70+)
- [ ] Install Solana CLI (v1.18.0+)
- [ ] Install Anchor framework (v0.29.0+)
- [ ] Configure Solana for Devnet
- [ ] Create and fund Devnet wallet

### Token Program Development
- [ ] Initialize Anchor project: `anchor init aegis-token`
- [ ] Design token account structures
- [ ] Implement `initialize_mint` instruction
- [ ] Implement `mint_to` instruction
- [ ] Implement `transfer` instruction
- [ ] Write unit tests (Rust)
- [ ] Write integration tests (TypeScript)
- [ ] Build program: `anchor build`
- [ ] Deploy to Devnet: `anchor deploy`
- [ ] Verify deployment with `solana program show`

### HTTP Server PoC
- [ ] Create Rust project: `cargo new node`
- [ ] Add dependencies (tokio, hyper)
- [ ] Implement basic HTTP listener
- [ ] Add health check endpoint
- [ ] Add logging
- [ ] Test server locally: `curl http://localhost:8080/health`

### Documentation
- [ ] Document environment setup steps
- [ ] Document token program design decisions
- [ ] Record deployed program ID
- [ ] Create deployment guide for future sprints

## Success Criteria

Sprint 1 is considered complete when:

1. ✅ All required development tools installed and verified
2. ✅ $AEGIS token program deployed to Solana Devnet
3. ✅ Token mint initialized with correct parameters (1B supply, 9 decimals)
4. ✅ Successful test minting of tokens to test accounts
5. ✅ Basic Rust HTTP server running and responding to requests
6. ✅ All code committed to Git with proper documentation

## Common Issues & Troubleshooting

### Rust Installation
**Problem**: `rustc: command not found` after installation
**Solution**: Restart terminal or run:
```bash
# Windows (PowerShell)
$env:PATH += ";$env:USERPROFILE\.cargo\bin"

# Linux/macOS
source $HOME/.cargo/env
```

### Solana Airdrop Fails
**Problem**: `Error: airdrop request failed`
**Solution**: Use web faucet at https://faucet.solana.com or try:
```bash
solana airdrop 1  # Request smaller amount
```

### Anchor Build Fails
**Problem**: `error: package `anchor-lang` cannot be built`
**Solution**: Update Rust and dependencies:
```bash
rustup update
cargo clean
anchor build
```

### Wrong Solana Network
**Problem**: Deploying to mainnet instead of devnet
**Solution**: Always verify cluster before deployment:
```bash
solana config get  # Check "RPC URL"
solana config set --url https://api.devnet.solana.com
```

## Next Steps (Sprint 2)

After completing Sprint 1, we'll move to:
- Node operator registration smart contract
- Staking mechanism implementation
- Node operator CLI tool development
- Integration tests between token and registry programs

## Resources

### Official Documentation
- Solana Docs: https://docs.solana.com/
- Anchor Book: https://book.anchor-lang.com/
- Rust Book: https://doc.rust-lang.org/book/
- Tokio Tutorial: https://tokio.rs/tokio/tutorial

### Example Repositories
- Solana Program Library: https://github.com/solana-labs/solana-program-library
- Anchor Examples: https://github.com/coral-xyz/anchor/tree/master/examples

### Community
- Solana Discord: https://discord.gg/solana
- Anchor Discord: https://discord.gg/PDeRXyVURd
- Rust Users Forum: https://users.rust-lang.org/

---

**Sprint 1 Status**: In Progress
**Last Updated**: 2025-11-19
**Next Review**: Upon completion of all deliverables
