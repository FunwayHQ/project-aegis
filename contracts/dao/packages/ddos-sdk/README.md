# @aegis/ddos-sdk

TypeScript SDK for interacting with the AEGIS DDoS Protection API. Provides a type-safe client for managing DDoS protection policies, blocklists, allowlists, and monitoring attack statistics.

## Installation

```bash
pnpm add @aegis/ddos-sdk
```

## Quick Start

```typescript
import { DDoSClient } from '@aegis/ddos-sdk';

const client = new DDoSClient({
  baseUrl: 'http://localhost:8080',
  apiKey: 'your-api-key', // optional
});

// Get global statistics
const stats = await client.getGlobalStats();
console.log(`Total blocked: ${stats.total_blocked}`);

// Create a protection policy
await client.createPolicy('example.com', {
  enabled: true,
  syn_threshold: 100,
  udp_threshold: 1000,
  block_duration_secs: 3600,
});

// Add IP to blocklist
await client.addToBlocklist({
  ip: '192.168.1.100',
  reason: 'Suspicious activity',
  duration_secs: 86400, // 24 hours
});
```

## API Reference

### Client Configuration

```typescript
interface DDoSClientConfig {
  baseUrl: string;       // API base URL
  apiKey?: string;       // Optional API key
  timeout?: number;      // Request timeout (default: 30000ms)
  fetch?: typeof fetch;  // Custom fetch implementation
}
```

### Policy Management

```typescript
// Get policy for a domain
const policy = await client.getPolicy('example.com');

// Create new policy
await client.createPolicy('example.com', {
  enabled: true,
  syn_threshold: 100,
  udp_threshold: 1000,
  block_duration_secs: 3600,
  rate_limit: {
    enabled: true,
    max_requests_per_minute: 1000,
    window_duration_secs: 60,
    scope: 'per_ip',
  },
  challenge_mode: {
    enabled: true,
    trigger_threshold: 500,
    challenge_type: 'managed',
    validity_secs: 3600,
  },
});

// Update policy
await client.updatePolicy('example.com', {
  syn_threshold: 200,
});

// Delete policy
await client.deletePolicy('example.com');

// List all policies
const policies = await client.listPolicies();
```

### Blocklist Management

```typescript
// Get blocklist (paginated)
const blocklist = await client.getBlocklist(1, 50);

// Add to blocklist
await client.addToBlocklist({
  ip: '192.168.1.100',
  reason: 'DDoS attack',
  duration_secs: 86400, // 0 for permanent
});

// Remove from blocklist
await client.removeFromBlocklist('192.168.1.100');

// Check if IP is blocked
const isBlocked = await client.isBlocklisted('192.168.1.100');
```

### Allowlist Management

```typescript
// Get allowlist
const allowlist = await client.getAllowlist();

// Add to allowlist
await client.addToAllowlist({
  ip: '10.0.0.1',
  reason: 'Trusted server',
});

// Remove from allowlist
await client.removeFromAllowlist('10.0.0.1');
```

### Statistics

```typescript
// Global stats
const globalStats = await client.getGlobalStats();
// Returns: total_requests, total_blocked, total_rate_limited,
//          total_attacks, active_attacks, blocked_ips, etc.

// Per-domain stats
const domainStats = await client.getDomainStats('example.com');

// Recent attacks
const attacks = await client.getAttacks(100);

// Top attackers
const attackers = await client.getTopAttackers(10);
```

### Real-time Events (SSE)

```typescript
const cleanup = client.subscribeToEvents({
  onEvent: (event) => console.log('Event:', event),
  onAttackDetected: (data) => console.log('Attack!', data),
  onAttackMitigated: (data) => console.log('Mitigated:', data),
  onIpBlocked: (data) => console.log('Blocked:', data),
  onStatsUpdate: (data) => console.log('Stats:', data),
  onError: (error) => console.error('Error:', error),
});

// Later: close connection
cleanup();
```

## Types

### DDoSPolicy

```typescript
interface DDoSPolicy {
  domain: string;
  enabled: boolean;
  syn_threshold: number;
  udp_threshold: number;
  block_duration_secs: number;
  rate_limit?: RateLimitPolicy;
  challenge_mode?: ChallengePolicy;
  allowlist?: string[];
  blocklist?: string[];
}
```

### GlobalStats

```typescript
interface GlobalStats {
  total_requests: number;
  total_blocked: number;
  total_rate_limited: number;
  total_attacks: number;
  active_attacks: number;
  blocked_ips: number;
  allowed_ips: number;
  drop_rate: number;
  uptime_secs: number;
}
```

### AttackEvent

```typescript
interface AttackEvent {
  id: string;
  attack_type: 'syn_flood' | 'udp_flood' | 'http_flood' | 'slowloris' | 'unknown';
  source_ip: string;
  target_domain: string;
  packets_per_second: number;
  severity: number; // 0-100
  mitigated: boolean;
  timestamp: number;
}
```

## Error Handling

```typescript
import { DDoSApiError } from '@aegis/ddos-sdk';

try {
  await client.getPolicy('nonexistent.com');
} catch (error) {
  if (error instanceof DDoSApiError) {
    console.log(`Status: ${error.statusCode}`);
    console.log(`Message: ${error.message}`);
  }
}
```

## Development

```bash
# Build
pnpm build

# Development mode
pnpm dev

# Run tests
pnpm test

# Type check
pnpm typecheck
```

## License

MIT
