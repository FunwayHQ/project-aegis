# AEGIS Bug Bounty Program

**Version:** 1.0
**Effective Date:** [TBD - Before Mainnet Launch]
**Last Updated:** December 2, 2025

## Program Overview

AEGIS is a decentralized CDN and edge security network built on Solana blockchain. We take security seriously and invite security researchers to help identify vulnerabilities in our systems.

This bug bounty program rewards responsible disclosure of security vulnerabilities.

## Rewards

| Severity | Reward Range | Response SLA |
|----------|--------------|--------------|
| **Critical** | $10,000 - $50,000 | 24 hours |
| **High** | $2,500 - $10,000 | 48 hours |
| **Medium** | $500 - $2,500 | 5 business days |
| **Low** | $100 - $500 | 10 business days |

### Payment Options
- USDC on Solana (preferred)
- SOL
- Wire transfer (for amounts > $10,000)

### Bonus Multipliers
- First reporter of a vulnerability class: +25%
- Exceptional quality report with PoC: +10%
- Fix suggestion included: +10%

---

## Scope

### In Scope

#### Solana Smart Contracts (Critical Priority)
| Contract | Program ID | Version |
|----------|------------|---------|
| DAO Governance | [Devnet ID] | 1.0.0 |
| Staking | [Devnet ID] | 1.0.0 |
| Registry | [Devnet ID] | 1.0.0 |
| Rewards | [Devnet ID] | 1.0.0 |
| Token | [Devnet ID] | 1.0.0 |

**Repository:** https://github.com/FunwayHQ/project-aegis/tree/main/contracts

#### Rust Edge Node (High Priority)
- Pingora proxy integration (`node/src/`)
- Wasm runtime and module loading
- eBPF/XDP DDoS protection
- P2P threat intelligence network
- Challenge system (PoW, fingerprinting)
- API security suite
- Distributed enforcement engine

**Repository:** https://github.com/FunwayHQ/project-aegis/tree/main/node

#### CLI Tools (Medium Priority)
- Node operator CLI (`cli/`)
- CDN publisher CLI (`cli/aegis-cdn/`)

#### DAO dApp (Medium Priority)
- React frontend (`dao/app/`)
- SDK (`dao/sdk/`)

### Out of Scope

- Third-party services (IPFS public gateways, Solana RPC providers)
- Social engineering attacks
- Physical security attacks
- Denial of service attacks against production infrastructure
- Attacks requiring physical access to hardware
- Vulnerabilities in dependencies (report upstream, then notify us)
- Issues already reported or known
- Theoretical vulnerabilities without proof of concept

---

## Severity Definitions

### Critical (CVSS 9.0-10.0)

**Impact:** Complete compromise of funds, infrastructure, or user data

**AEGIS-Specific Examples:**
- Unauthorized minting of $AEGIS tokens
- Theft of staked tokens from any user
- Bypassing DAO governance to execute arbitrary proposals
- Remote code execution on edge nodes
- Complete bypass of WAF protection
- P2P network takeover (Sybil attack enabling fake threat intel)
- Private key extraction from any component

### High (CVSS 7.0-8.9)

**Impact:** Significant financial loss, privilege escalation, or service disruption

**AEGIS-Specific Examples:**
- Claiming rewards without providing service
- Bypassing staking cooldown period
- DoS attack on DAO governance (blocking votes)
- WAF bypass for specific attack classes (SQLi, XSS)
- Spoofing threat intelligence to block legitimate IPs
- Challenge system bypass (automated solving)
- Unauthorized access to node operator accounts

### Medium (CVSS 4.0-6.9)

**Impact:** Limited information disclosure, minor service disruption

**AEGIS-Specific Examples:**
- Leaking node operator metadata
- Rate limiter bypass (limited scope)
- Cache poisoning (non-persistent)
- Information disclosure via error messages
- Timing attacks revealing internal state
- Minor DoS on non-critical components

### Low (CVSS 0.1-3.9)

**Impact:** Minimal security impact, best practice violations

**AEGIS-Specific Examples:**
- Missing security headers (non-critical)
- Verbose error messages in non-production
- Suboptimal cryptographic practices (still secure)
- Code quality issues that could lead to vulnerabilities
- Documentation inconsistencies

---

## Rules of Engagement

### Allowed Testing

- **Devnet/Testnet ONLY** for smart contracts
- Local testing environment for node software
- Provided test instances for integration testing
- Static analysis of source code
- Review of documentation and configurations

### Prohibited Actions

- Testing on mainnet or production systems
- Accessing or modifying other users' data
- Executing denial of service attacks
- Social engineering AEGIS team or users
- Public disclosure before coordinated fix
- Selling or transferring vulnerability information
- Automated scanning without rate limiting

### Safe Harbor

We will not pursue legal action against researchers who:
- Act in good faith following this policy
- Report vulnerabilities through official channels
- Do not access or modify data beyond what's necessary
- Do not cause service degradation
- Maintain confidentiality until public disclosure

---

## Submission Requirements

### Required Information

1. **Vulnerability Summary** (1-2 sentences)
2. **Affected Component** (contract, module, file)
3. **Severity Assessment** (with CVSS score if possible)
4. **Technical Details**
   - Step-by-step reproduction
   - Code snippets or transactions
   - Screenshots or logs
5. **Proof of Concept**
   - Working exploit code (testnet only)
   - Transaction signatures demonstrating issue
6. **Impact Analysis**
   - What could an attacker achieve?
   - How many users/funds at risk?
7. **Suggested Fix** (optional, earns bonus)

### Submission Channel

**Primary:** security@aegis.network (PGP key available)
**Backup:** [Immunefi platform - if selected]

### Response Timeline

| Stage | SLA |
|-------|-----|
| Initial acknowledgment | 24 hours |
| Severity assessment | 48 hours |
| Fix timeline estimate | 5 business days |
| Fix deployed | Varies by severity |
| Reward payment | 14 days after fix verified |
| Public disclosure | 90 days or mutual agreement |

---

## Responsible Disclosure Timeline

1. **Day 0:** Vulnerability reported
2. **Day 1:** AEGIS acknowledges receipt
3. **Day 2-5:** Severity assessed, fix timeline estimated
4. **Day X:** Fix developed and tested
5. **Day X+7:** Fix deployed to production
6. **Day X+14:** Reward paid
7. **Day 90:** Public disclosure (or earlier if mutually agreed)

### Extensions

We may request extensions for:
- Complex fixes requiring architectural changes
- Coordinated disclosure with other affected parties
- Fixes requiring smart contract migrations

Researchers will be kept informed and can negotiate timeline.

---

## Exclusions

The following are **not eligible** for rewards:

- Previously reported vulnerabilities
- Vulnerabilities in out-of-scope components
- Self-XSS or issues requiring unlikely user interaction
- Missing rate limiting (unless exploitable)
- Clickjacking on pages without sensitive actions
- CSRF on logout
- Missing security headers without demonstrated impact
- SSL/TLS best practices (unless actively exploitable)
- Content injection without impact (text only)
- Stack traces or path disclosure (unless containing secrets)
- Vulnerabilities requiring root/admin access
- Vulnerabilities in deprecated/unused code

---

## Recognition

### Hall of Fame

Top contributors will be recognized in our:
- Public Hall of Fame page
- Security acknowledgments in release notes
- Annual security report

### Swag (Optional)

Researchers can opt-in to receive:
- AEGIS branded merchandise
- NFT recognition badge
- Invitation to private security researcher program

---

## Contact

**Security Team Email:** security@aegis.network

**PGP Key:**
```
[PGP PUBLIC KEY TO BE ADDED]
```

**Response Hours:** Monday-Friday, 9am-6pm UTC

**Emergency Contact:** For critical vulnerabilities, add "CRITICAL" to subject line for 24/7 response.

---

## Program Changes

This program may be modified at any time. Changes will be announced via:
- This document (version controlled)
- Security mailing list
- Project Discord/Telegram

Submissions made before changes are grandfathered under the terms at time of submission.

---

## Legal

This bug bounty program is an invitation to participate in good faith security research. By participating, you agree to:
- Follow the rules of engagement
- Act within the scope defined above
- Coordinate disclosure with AEGIS

AEGIS reserves the right to:
- Determine reward amounts at its discretion
- Decline rewards for invalid submissions
- Modify or terminate the program at any time
