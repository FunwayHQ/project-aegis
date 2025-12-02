# AEGIS Solana Audit - Outreach Emails

**Date:** December 2, 2025

---

## Email 1: OtterSec

**To:** contact@osec.io
**Subject:** Audit Request - AEGIS Decentralized CDN (5 Solana/Anchor Contracts, ~4K LOC)

---

Hello OtterSec Team,

We are reaching out to request a security audit for AEGIS, a decentralized CDN and edge security network built on Solana.

**Project Overview:**
AEGIS is building a community-owned alternative to centralized CDN providers. Our smart contracts handle token economics, node operator staking, governance, and rewards distribution.

**Audit Scope:**
- **5 Anchor programs** (~4,100 lines of Rust)
- **Contracts:** DAO Governance, Staking, Registry, Rewards, Token
- **Key features:** Vote escrow pattern (flash loan protection), 48-hour timelock, CPI between staking/registry

**Security Features Already Implemented:**
- Vote escrow pattern with token locking (prevents flash loan attacks)
- 48-hour timelock on all config changes
- Account ownership and mint validation
- Cross-program invocation security (staking ↔ registry sync)

**What We're Looking For:**
- Full manual security audit
- Focus on economic attacks, access control, CPI security
- Re-audit after remediation
- Final public report

**Timeline:**
- Preferred start: Within 2-3 weeks
- Target completion: Before mainnet launch (Q1 2026)

**Budget:** $30,000 - $50,000 USD (flexible for the right partner)
**Payment:** USDC on Solana or wire transfer

**Resources Available:**
- Full source code on GitHub
- Comprehensive test suite (14+ DAO tests, unit/integration tests)
- Architecture documentation
- Devnet deployment for testing

We chose OtterSec because of your deep Solana expertise and track record with major protocols like Jupiter and Drift. We're looking for a thorough security partner, not just a checkbox audit.

Would you be available for a brief call to discuss scope and timeline?

Best regards,
[Name]
AEGIS Core Team
[Email]
[Discord/Telegram]

---

## Email 2: Neodyme

**To:** contact@neodyme.io
**Subject:** Security Audit Request - AEGIS (Solana/Anchor, 5 Programs)

---

Hello Neodyme Team,

We are seeking a security audit for AEGIS, a decentralized edge network with smart contracts on Solana.

**Project Summary:**
- **Framework:** Anchor
- **Total LOC:** ~4,100 lines of Rust
- **Programs:** 5 (DAO, Staking, Registry, Rewards, Token)

**Technical Highlights:**
- Vote escrow pattern for flash loan protection (tokens locked in PDA vault during voting)
- 48-hour timelock mechanism for governance changes
- Cross-program invocations between Staking and Registry contracts
- Snapshot-based voting with escrow verification

**Audit Focus Areas:**
1. Access control and authority validation
2. Economic attack vectors (flash loans, front-running)
3. CPI security and reentrancy
4. State machine integrity (proposal lifecycle, cooldowns)
5. Token security (mint/burn controls)

**Our Preparation:**
- Clean, documented codebase following Anchor best practices
- Existing test coverage with integration tests
- Security hardening already applied (Sprint 18.5)
- Devnet deployment available

**Desired Deliverables:**
- Detailed vulnerability report with severity ratings
- Remediation guidance
- Re-audit of fixes
- Public final report

**Timeline & Budget:**
- Start: Flexible, ideally within 3 weeks
- Budget: $25,000 - $45,000 USD
- Payment: USDC/SOL or wire transfer

We've followed Neodyme's work on Orca and other Solana protocols. Your expertise with Anchor and understanding of Solana-specific vulnerabilities is exactly what we need.

Are you currently accepting new audit engagements? Happy to schedule a call to discuss further.

Best regards,
[Name]
AEGIS Core Team
[Email]

---

## Email 3: Sec3

**To:** contact@sec3.dev
**Subject:** Audit Inquiry - AEGIS Decentralized CDN (Solana/Anchor)

---

Hello Sec3 Team,

We're interested in your security audit services for AEGIS, a decentralized CDN platform with smart contracts on Solana.

**Project Details:**
- **Programs:** 5 Anchor contracts
- **Lines of Code:** ~4,100
- **Complexity:** Medium-High (CPI flows, vote escrow, timelocks)

**Contracts:**
| Contract | LOC | Risk Level |
|----------|-----|------------|
| DAO Governance | 1,600+ | Critical |
| Staking | 800+ | High |
| Rewards | 700+ | High |
| Registry | 600+ | Medium |
| Token | 400+ | Critical |

**Security Measures Implemented:**
- Vote escrow pattern (flash loan mitigation)
- 48-hour timelock on config changes
- PDA-based authority for CPI calls
- Account ownership validation

**Why Sec3:**
We're particularly interested in your combination of automated analysis (Soteria) and manual review. The automated pre-scan could help identify issues early, making the manual audit more efficient.

**Questions:**
1. What is your current availability/lead time?
2. Do you offer a combined auto-audit + manual review package?
3. What's the estimated timeline for ~4K LOC of Anchor code?

**Budget:** $20,000 - $35,000 USD
**Payment:** USDC on Solana preferred

We have full documentation, test suites, and Devnet deployments ready for review.

Looking forward to hearing from you.

Best regards,
[Name]
AEGIS Core Team
[Email]

---

## Email 4: Halborn (Backup Option)

**To:** sales@halborn.com
**Subject:** Solana Smart Contract Audit - AEGIS (5 Anchor Programs)

---

Hello Halborn Team,

We are requesting a quote for a security audit of our Solana smart contracts.

**Project:** AEGIS - Decentralized CDN and Edge Security Network

**Scope:**
- 5 Anchor programs (~4,100 LOC)
- DAO governance with vote escrow
- Staking with CPI to Registry
- Rewards distribution
- Token program

**Key Security Features:**
- Flash loan protection via vote escrow
- 48-hour timelock mechanism
- Cross-program invocation security
- Comprehensive account validation

**Requirements:**
- Full security audit
- Vulnerability report with CVSS ratings
- Remediation support
- Re-audit and final report

**Timeline:** 2-4 weeks preferred
**Budget:** $25,000 - $45,000 USD

Please let us know your availability and provide a detailed quote.

Thank you,
[Name]
AEGIS Core Team

---

## Comparison Checklist

When evaluating responses, consider:

| Criteria | OtterSec | Neodyme | Sec3 | Halborn |
|----------|----------|---------|------|---------|
| Solana expertise | ★★★★★ | ★★★★★ | ★★★★★ | ★★★★☆ |
| Anchor experience | ★★★★★ | ★★★★★ | ★★★★★ | ★★★★☆ |
| Price quote | | | | |
| Timeline | | | | |
| Re-audit included | | | | |
| Public report | | | | |
| References | | | | |
| Communication | | | | |

---

## Next Steps After Sending

1. **Week 1:** Send emails, await responses
2. **Week 2:** Schedule calls with interested firms
3. **Week 3:** Compare quotes, check references
4. **Week 4:** Select auditor, sign engagement
5. **Week 5+:** Begin audit process

---

## Attachments to Prepare

Before sending, prepare:

- [ ] GitHub repo access (or zip of contracts/)
- [ ] Architecture diagram (from SOLANA-AUDIT-REQUEST.md)
- [ ] Test coverage report
- [ ] Devnet program IDs
- [ ] Team contact information
