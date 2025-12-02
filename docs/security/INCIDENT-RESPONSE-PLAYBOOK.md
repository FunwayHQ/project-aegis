# AEGIS Incident Response Playbook

**Version:** 1.0
**Last Updated:** December 2, 2025
**Classification:** Internal (Shared with Node Operators)

---

## 1. Incident Severity Classification

### SEV-1: Critical

**Definition:** Complete service outage, active exploitation, or imminent threat to funds

**Examples:**
- Smart contract exploit in progress
- $AEGIS token theft
- Complete network outage (>50% nodes)
- RCE on multiple edge nodes
- P2P network compromise

**Response Time:** Immediate (24/7)
**Resolution Target:** 1 hour containment, 4 hours resolution

### SEV-2: High

**Definition:** Partial service degradation, confirmed vulnerability, or significant security event

**Examples:**
- Single region outage
- WAF bypass discovered
- Credential compromise (limited scope)
- DoS attack affecting performance
- Unauthorized admin access

**Response Time:** 1 hour (business hours), 4 hours (off-hours)
**Resolution Target:** 4 hours containment, 24 hours resolution

### SEV-3: Medium

**Definition:** Potential security issue, minor degradation, or investigation needed

**Examples:**
- Suspicious activity patterns
- Failed exploit attempts
- Minor configuration issues
- Non-critical vulnerability reported

**Response Time:** 4 hours (business hours)
**Resolution Target:** 24-48 hours

### SEV-4: Low

**Definition:** Informational security events, minor issues

**Examples:**
- Routine security scans detected
- Minor policy violations
- Documentation issues
- Best practice recommendations

**Response Time:** Next business day
**Resolution Target:** 1 week

---

## 2. Response Team

### Core Team

| Role | Primary | Backup | Contact |
|------|---------|--------|---------|
| Incident Commander | [Name] | [Name] | [Phone/Signal] |
| Security Lead | [Name] | [Name] | [Phone/Signal] |
| Infrastructure Lead | [Name] | [Name] | [Phone/Signal] |
| Smart Contract Lead | [Name] | [Name] | [Phone/Signal] |
| Communications | [Name] | [Name] | [Phone/Signal] |

### On-Call Schedule

- Rotation: Weekly
- Coverage: 24/7 for SEV-1, SEV-2
- Escalation: 15 min no response → backup

### Communication Channels

| Channel | Purpose |
|---------|---------|
| Signal Group (Encrypted) | SEV-1, SEV-2 coordination |
| Slack #incident-response | All incidents |
| Slack #security-alerts | Automated alerts |
| Email security@aegis.network | External reports |
| Phone tree | SEV-1 escalation |

---

## 3. Incident Response Phases

### Phase 1: Detection & Triage

**Duration:** 0-15 minutes

1. **Receive Alert**
   - Automated monitoring
   - User report
   - Bug bounty submission
   - Team member observation

2. **Initial Assessment**
   - What is happening?
   - What is affected?
   - When did it start?
   - Is it ongoing?

3. **Severity Assignment**
   - Use classification above
   - When in doubt, escalate

4. **Assemble Team**
   - Notify incident commander
   - Page required responders
   - Open incident channel

### Phase 2: Containment

**Duration:** 15 min - 2 hours (varies by severity)

**Objective:** Stop the bleeding, prevent further damage

#### Smart Contract Incidents

```
Containment Options:
1. Pause contract (if pause mechanism exists)
2. Emergency governance action
3. Frontend warning/block
4. RPC rate limiting
5. Coordinate with Solana Foundation (extreme)
```

#### Edge Node Incidents

```
Containment Options:
1. Remove node from rotation (BGP withdraw)
2. Enable emergency WAF rules
3. Increase eBPF thresholds
4. Block specific IPs/ranges
5. Disable affected Wasm modules
6. Restart affected services
```

#### P2P Network Incidents

```
Containment Options:
1. Ban malicious peer IDs
2. Pause threat intel processing
3. Clear blocklist (if poisoned)
4. Isolate affected nodes
5. Revert to known-good state
```

#### Credential Compromise

```
Containment Options:
1. Rotate compromised credentials immediately
2. Revoke access tokens
3. Audit access logs
4. Enable additional MFA
5. Notify affected parties
```

### Phase 3: Eradication

**Duration:** 2-24 hours

**Objective:** Remove the threat completely

1. **Root Cause Analysis**
   - How did attacker get in?
   - What vulnerability was exploited?
   - What persistence mechanisms exist?

2. **Remove Threat**
   - Patch vulnerability
   - Remove malware/backdoors
   - Reset compromised systems
   - Update credentials

3. **Verify Removal**
   - Scan for remaining threats
   - Monitor for recurrence
   - Validate fixes

### Phase 4: Recovery

**Duration:** 24-72 hours

**Objective:** Restore normal operations

1. **Restore Services**
   - Bring systems back online
   - Verify functionality
   - Enable monitoring

2. **Validate Integrity**
   - Check data integrity
   - Verify blockchain state
   - Confirm no unauthorized changes

3. **Communication**
   - Internal status updates
   - User communication (if needed)
   - Regulatory notification (if required)

### Phase 5: Post-Incident

**Duration:** 1-2 weeks after resolution

1. **Post-Incident Review (PIR)**
   - Timeline of events
   - What went well
   - What could improve
   - Action items

2. **Documentation**
   - Update runbooks
   - Improve monitoring
   - Document lessons learned

3. **Process Improvements**
   - Implement action items
   - Update this playbook
   - Training if needed

---

## 4. Specific Incident Runbooks

### Runbook: Smart Contract Exploit

```
┌─────────────────────────────────────────────────────────────────┐
│ SMART CONTRACT EXPLOIT DETECTED                                 │
├─────────────────────────────────────────────────────────────────┤
│ 1. IMMEDIATE (0-5 min)                                          │
│    □ Confirm exploit is real (check transactions)              │
│    □ Page smart contract lead + security lead                  │
│    □ Open Signal thread for coordination                       │
│                                                                 │
│ 2. CONTAINMENT (5-30 min)                                       │
│    □ Pause contract if possible                                │
│    □ Add warning banner to frontend                            │
│    □ Contact major exchanges to pause deposits                 │
│    □ Tweet thread acknowledging issue                          │
│                                                                 │
│ 3. ANALYSIS (30 min - 4 hours)                                  │
│    □ Identify vulnerability in code                            │
│    □ Determine scope of damage                                 │
│    □ Identify affected accounts                                │
│    □ Trace stolen funds (if any)                               │
│                                                                 │
│ 4. REMEDIATION (4-24 hours)                                     │
│    □ Develop fix                                               │
│    □ Test fix on devnet                                        │
│    □ Prepare migration if needed                               │
│    □ Coordinate upgrade                                        │
│                                                                 │
│ 5. RECOVERY (24-72 hours)                                       │
│    □ Deploy fix to mainnet                                     │
│    □ Verify fix works                                          │
│    □ Communicate resolution                                    │
│    □ Determine compensation (if applicable)                    │
└─────────────────────────────────────────────────────────────────┘
```

### Runbook: DDoS Attack

```
┌─────────────────────────────────────────────────────────────────┐
│ DDOS ATTACK DETECTED                                            │
├─────────────────────────────────────────────────────────────────┤
│ 1. IMMEDIATE (0-5 min)                                          │
│    □ Confirm via monitoring dashboards                         │
│    □ Identify attack vector (SYN, UDP, HTTP, etc.)             │
│    □ Page infrastructure lead                                  │
│                                                                 │
│ 2. MITIGATION (5-30 min)                                        │
│    □ eBPF thresholds auto-mitigating? Check stats              │
│    □ If L7: Enable emergency WAF rules                         │
│    □ If overwhelming: Enable upstream DDoS protection          │
│    □ Consider geo-blocking if attack is regional               │
│                                                                 │
│ 3. ANALYSIS (During attack)                                     │
│    □ Capture attack signature                                  │
│    □ Identify source IPs/ranges                                │
│    □ Determine if targeted or volumetric                       │
│                                                                 │
│ 4. RESPONSE (Ongoing)                                           │
│    □ Add attack signatures to blocklist                        │
│    □ Share threat intel via P2P network                        │
│    □ Monitor for attack evolution                              │
│                                                                 │
│ 5. POST-ATTACK                                                  │
│    □ Document attack patterns                                  │
│    □ Update eBPF rules if needed                               │
│    □ Review capacity for future attacks                        │
└─────────────────────────────────────────────────────────────────┘
```

### Runbook: Compromised Node

```
┌─────────────────────────────────────────────────────────────────┐
│ COMPROMISED NODE SUSPECTED                                      │
├─────────────────────────────────────────────────────────────────┤
│ 1. IMMEDIATE (0-5 min)                                          │
│    □ Remove from BGP rotation (stop serving traffic)           │
│    □ Page security lead                                        │
│    □ Preserve logs (don't reboot yet)                          │
│                                                                 │
│ 2. CONTAINMENT (5-30 min)                                       │
│    □ Block node's P2P peer ID                                  │
│    □ Revoke any API credentials                                │
│    □ Alert other nodes to block this peer                      │
│                                                                 │
│ 3. ANALYSIS (30 min - 4 hours)                                  │
│    □ Capture memory dump if possible                           │
│    □ Review access logs                                        │
│    □ Check for lateral movement                                │
│    □ Identify entry point                                      │
│                                                                 │
│ 4. REMEDIATION                                                  │
│    □ Wipe and rebuild node from known-good image               │
│    □ Generate new keys                                         │
│    □ Re-register with new identity                             │
│    □ Patch vulnerability that allowed compromise               │
│                                                                 │
│ 5. POST-INCIDENT                                                │
│    □ Update node hardening guide                               │
│    □ Alert other operators if vulnerability is common          │
│    □ Consider mandatory security update                        │
└─────────────────────────────────────────────────────────────────┘
```

---

## 5. Communication Templates

### Internal Notification (Slack)

```
🚨 INCIDENT OPENED: [Title]
Severity: SEV-[1/2/3/4]
Status: Investigating
Incident Commander: @[name]
War Room: [link]

Summary: [1-2 sentences describing the issue]

Next Update: [time]
```

### Status Update (Slack)

```
📢 INCIDENT UPDATE: [Title]
Status: [Investigating/Contained/Mitigating/Resolved]
Time Since Start: [X hours Y minutes]

Progress:
- [Bullet points of what's been done]

Next Steps:
- [What's happening next]

Next Update: [time]
```

### External Communication (Twitter/Blog)

**Initial (during incident):**
```
We're aware of [issue description] and are actively investigating.
Updates will be posted as we learn more.
All funds are SAFU. [If true]
```

**Resolution:**
```
The [issue] has been resolved.

What happened: [Brief explanation]
Impact: [Who was affected]
Resolution: [What we did]
Prevention: [What we're doing to prevent recurrence]

Full post-mortem: [link]
```

### Affected User Communication

```
Subject: Important Security Notice - Action Required

Dear [User/Node Operator],

We recently identified [brief issue description].

Impact to you:
- [Specific impact]

Actions you should take:
1. [Action 1]
2. [Action 2]

We have already:
- [Mitigation taken]

Questions? Contact security@aegis.network

Thank you for your patience and trust.

The AEGIS Team
```

---

## 6. External Contacts

| Entity | Contact | When to Use |
|--------|---------|-------------|
| Solana Foundation | [Contact] | Network-level issues |
| Major Exchanges | [Contact list] | Token-related incidents |
| Law Enforcement | [Local contacts] | Criminal activity |
| Legal Counsel | [Contact] | Any incident with liability |
| PR/Communications | [Contact] | Media inquiries |
| Insurance | [Contact] | Covered losses |

---

## 7. Incident Tracking

### Required Documentation

For every incident, document:

1. **Incident ID:** INC-YYYY-MM-DD-NNN
2. **Severity:** SEV-1/2/3/4
3. **Timeline:** All actions with timestamps
4. **Root Cause:** What actually happened
5. **Impact:** Users, funds, reputation
6. **Resolution:** What fixed it
7. **Action Items:** Follow-up work

### Post-Incident Review (PIR) Template

```markdown
# Post-Incident Review: [INC-YYYY-MM-DD-NNN]

## Summary
[2-3 sentences]

## Timeline
| Time (UTC) | Event |
|------------|-------|
| HH:MM | [Event] |

## Root Cause
[Detailed explanation]

## Impact
- Users affected: [N]
- Duration: [X hours]
- Financial impact: [$X]

## What Went Well
- [Bullet points]

## What Could Improve
- [Bullet points]

## Action Items
| Item | Owner | Due Date | Status |
|------|-------|----------|--------|
| [Action] | [Name] | [Date] | [Status] |

## Lessons Learned
[Key takeaways]
```

---

## 8. Regular Drills

### Quarterly Exercises

- **Tabletop Exercise:** Walk through scenario without taking action
- **Chaos Engineering:** Inject faults in staging environment
- **Red Team Exercise:** Simulated attack (annual)

### Drill Scenarios

1. Smart contract vulnerability discovered
2. DDoS attack at 10x normal traffic
3. Node operator reports compromise
4. Bug bounty critical submission
5. Regulatory inquiry

---

## Document Maintenance

This playbook is reviewed:
- After every SEV-1 or SEV-2 incident
- Quarterly at minimum
- When team structure changes

Owner: Security Lead
Last Review: December 2, 2025
Next Review: March 2026
