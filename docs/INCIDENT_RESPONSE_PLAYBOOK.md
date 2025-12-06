# Incident Response Playbook

**Version:** 1.0
**Last Updated:** 2025-12-06
**Sprint:** Y10.10

## Overview

This playbook defines procedures for responding to security incidents in the AEGIS decentralized edge network.

## Incident Severity Levels

| Level | Description | Response Time | Examples |
|-------|-------------|---------------|----------|
| **P0** | Critical - Active exploitation | Immediate (15 min) | RCE, active DDoS, smart contract drain |
| **P1** | High - Imminent threat | 1 hour | Vulnerability with exploit code, credential leak |
| **P2** | Medium - Security issue | 24 hours | Vulnerability without exploit, suspicious activity |
| **P3** | Low - Minor issue | 7 days | Configuration weakness, minor info disclosure |

## Incident Response Team

### Roles
- **Incident Commander (IC)**: Coordinates response, makes decisions
- **Technical Lead**: Investigates technical details, implements fixes
- **Communications Lead**: Handles internal/external communications
- **On-Call Engineer**: First responder, initial triage

### Escalation Path
```
On-Call → Technical Lead → Incident Commander → Executive Team
```

## Response Procedures

### Phase 1: Detection & Triage (0-15 minutes)

#### Detection Sources
- [ ] Automated monitoring alerts
- [ ] Security researcher report
- [ ] Bug bounty submission
- [ ] Community report
- [ ] Internal discovery

#### Initial Assessment
1. **Verify the incident**
   - Is this a real security issue or false positive?
   - What systems are affected?

2. **Determine severity**
   - Is there active exploitation?
   - What's the potential impact?
   - How many users/nodes affected?

3. **Assign severity level** (P0-P3)

4. **Notify appropriate team members**
   - P0/P1: Immediate escalation to Incident Commander
   - P2/P3: Log and schedule response

### Phase 2: Containment (15 min - 2 hours)

#### For Smart Contract Vulnerabilities
```bash
# 1. Pause affected functionality (if pausable)
# Example: Emergency pause DAO

# 2. Revoke compromised keys
anchor run revoke-authority --program <PROGRAM_ID>

# 3. Snapshot current state for analysis
solana account <ADDRESS> --output json > snapshot.json
```

#### For Node/Network Vulnerabilities
```bash
# 1. Isolate affected nodes
# Update blocklist via P2P threat intel
curl -X POST http://localhost:8081/threat-intel/block \
  -H "Content-Type: application/json" \
  -d '{"ip": "ATTACKER_IP", "reason": "Active exploitation"}'

# 2. Deploy emergency WAF rule
curl -X POST http://localhost:8080/waf/rule \
  -H "Content-Type: application/json" \
  -d '{"pattern": "ATTACK_PATTERN", "action": "block"}'

# 3. Enable enhanced logging
export RUST_LOG=aegis_node=debug
```

#### For Credential Leaks
1. Rotate affected credentials immediately
2. Revoke compromised tokens
3. Force re-authentication

### Phase 3: Investigation (2-24 hours)

#### Collect Evidence
```bash
# Node logs
journalctl -u aegis-node --since "1 hour ago" > incident_logs.txt

# Network traffic (if available)
tcpdump -i any -w incident_capture.pcap

# Blockchain transactions
solana transaction-history <ADDRESS> --limit 1000 > tx_history.json
```

#### Analysis Checklist
- [ ] Timeline of events
- [ ] Attack vector identification
- [ ] Scope of compromise
- [ ] Data accessed/modified
- [ ] Attacker attribution (if possible)

#### Root Cause Analysis
Document:
1. What vulnerability was exploited?
2. Why wasn't it detected earlier?
3. What controls failed?

### Phase 4: Remediation (24 hours - 7 days)

#### Code Fix Process
1. **Develop patch**
   - Create fix on private branch
   - Peer review by 2+ engineers
   - Security-focused code review

2. **Test patch**
   - Unit tests
   - Integration tests
   - Regression tests
   - Fuzzing (if applicable)

3. **Deploy patch**
   - Canary deployment (1% of nodes)
   - Monitor for issues (30 minutes)
   - Gradual rollout (10% → 50% → 100%)

#### Smart Contract Fix Process
1. **For upgradeable contracts**
   - Deploy new implementation
   - DAO vote (if required)
   - Execute upgrade through timelock

2. **For non-upgradeable contracts**
   - Deploy new contract
   - Migrate state
   - Update references

### Phase 5: Recovery (1-7 days)

#### Service Restoration
- [ ] Verify fix effectiveness
- [ ] Restore disabled functionality
- [ ] Clear emergency blocks
- [ ] Return to normal operations

#### Communication
- [ ] Notify affected users
- [ ] Update status page
- [ ] Post incident report (if public)

### Phase 6: Post-Incident Review (7-14 days)

#### Blameless Postmortem
Schedule within 1 week of resolution.

**Template:**
```markdown
# Incident Postmortem: [INCIDENT_ID]

## Summary
Brief description of what happened.

## Timeline
| Time (UTC) | Event |
|------------|-------|
| HH:MM | Detection |
| HH:MM | Containment started |
| HH:MM | Root cause identified |
| HH:MM | Fix deployed |
| HH:MM | All-clear |

## Impact
- Users affected: X
- Duration: X hours
- Data exposed: Y/N

## Root Cause
Detailed technical explanation.

## What Went Well
- Quick detection
- Effective containment

## What Could Be Improved
- Detection time
- Communication

## Action Items
| Action | Owner | Due Date |
|--------|-------|----------|
| Add detection rule | @engineer | 2025-XX-XX |
| Improve logging | @engineer | 2025-XX-XX |
```

## Specific Incident Types

### DDoS Attack

1. **Detection**: Traffic spike, latency increase
2. **Containment**:
   ```bash
   # Enable aggressive rate limiting
   curl -X POST http://localhost:8080/rate-limit/emergency \
     -d '{"max_rps": 100}'

   # Update eBPF blocklist (Linux)
   aegis-node blocklist add --ip ATTACKER_IP
   ```
3. **Recovery**: Gradually relax rate limits

### Smart Contract Exploit

1. **Detection**: Abnormal transactions, balance changes
2. **Containment**:
   ```bash
   # Pause contract (if pausable)
   anchor run pause --program dao

   # Notify exchanges (if token-related)
   ```
3. **Recovery**: Deploy fix, unpause, compensate affected users

### Credential Leak

1. **Detection**: Unauthorized access, exposed secrets
2. **Containment**:
   ```bash
   # Rotate keys
   solana-keygen new -o ~/.config/solana/new-id.json

   # Update all references
   # Revoke old keys
   ```
3. **Recovery**: Audit access logs, notify affected users

### Malicious Wasm Module

1. **Detection**: WAF alerts, suspicious behavior
2. **Containment**:
   ```bash
   # Disable module
   curl -X DELETE http://localhost:8080/modules/MALICIOUS_ID

   # Block IPFS CID
   curl -X POST http://localhost:8081/blocklist/cid \
     -d '{"cid": "MALICIOUS_CID"}'
   ```
3. **Recovery**: Remove from route configs, audit module signing

## Communication Templates

### Internal Alert (Slack/Discord)
```
:rotating_light: SECURITY INCIDENT [P0/P1/P2/P3]

Summary: Brief description
Affected: Systems/users affected
Status: Investigating/Contained/Resolved
IC: @incident_commander
Channel: #incident-YYYY-MM-DD

DO NOT discuss externally until cleared by Communications Lead.
```

### External Disclosure (After Resolution)
```
Security Advisory: [TITLE]

We identified and resolved a security issue affecting [COMPONENT].

Impact: [DESCRIPTION]
Timeline: Discovered [DATE], Fixed [DATE]
Action Required: [UPDATE/NO ACTION]

We recommend all users [ACTION].

Full details: [LINK]
```

## Tools & Resources

### Monitoring Dashboards
- Metrics: `/verifiable-metrics` endpoint
- Logs: Grafana/Loki
- Blockchain: Solscan, Solana Explorer

### Emergency Contacts
- On-Call: [ROTATION]
- Security Lead: [CONTACT]
- Legal: [CONTACT]
- PR: [CONTACT]

### Reference Documents
- SECURITY.md - Security policy
- CARGO_AUDIT_REPORT.md - Dependency vulnerabilities
- SOLANA-AUDIT-REQUEST.md - Smart contract audit scope

## Appendix: Command Reference

```bash
# Check node health
curl http://localhost:8080/health

# View active connections
curl http://localhost:8080/metrics | grep connections

# Force blocklist sync
curl -X POST http://localhost:8081/blocklist/sync

# Emergency shutdown
systemctl stop aegis-node

# View recent threats
curl http://localhost:8081/threat-intel/recent

# Check rate limit status
curl http://localhost:8080/rate-limit/status
```
