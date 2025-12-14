# @aegis/dns-sdk

TypeScript SDK for interacting with the AEGIS DNS Management API. Provides a type-safe client for managing DNS zones, records, DNSSEC, and viewing analytics.

## Installation

```bash
pnpm add @aegis/dns-sdk
```

## Quick Start

```typescript
import { DnsClient } from '@aegis/dns-sdk';

const client = new DnsClient({
  baseUrl: 'http://localhost:8054',
  apiKey: 'your-api-key', // optional
});

// List all zones
const zones = await client.listZones();

// Create a new zone
const zone = await client.createZone({
  domain: 'example.com',
  proxied: true,
});

// Add a DNS record
await client.createRecord('example.com', {
  name: 'www',
  type: 'A',
  value: '192.168.1.1',
  ttl: 300,
});
```

## API Reference

### Client Configuration

```typescript
interface DnsClientConfig {
  baseUrl?: string;  // API base URL (default: http://localhost:8054)
  apiKey?: string;   // Optional API key
  timeout?: number;  // Request timeout in ms (default: 30000)
}
```

### Zone Management

```typescript
// List all zones
const zones = await client.listZones();

// Get a specific zone
const zone = await client.getZone('example.com');

// Create a new zone
const zone = await client.createZone({
  domain: 'example.com',
  proxied: true,
});

// Update a zone
const updated = await client.updateZone('example.com', {
  proxied: false,
});

// Delete a zone
await client.deleteZone('example.com');
```

### Record Management

```typescript
// List all records for a zone
const records = await client.listRecords('example.com');

// Get a specific record
const record = await client.getRecord('example.com', 'record-id');

// Create a DNS record
const record = await client.createRecord('example.com', {
  name: 'www',          // Use '@' for root domain
  type: 'A',            // A, AAAA, CNAME, MX, TXT, NS, etc.
  value: '192.168.1.1',
  ttl: 300,             // Time to live in seconds
  priority: 10,         // For MX/SRV records
  proxied: true,        // Proxy through AEGIS
});

// Update a record
const updated = await client.updateRecord('example.com', 'record-id', {
  value: '192.168.1.2',
  ttl: 600,
});

// Delete a record
await client.deleteRecord('example.com', 'record-id');
```

### DNSSEC Management

```typescript
// Get DNSSEC status
const status = await client.getDnssecStatus('example.com');
// Returns: { enabled, algorithm, key_tag, ds_record, ... }

// Enable DNSSEC (returns DS record for registrar)
const dsRecord = await client.enableDnssec('example.com');

// Disable DNSSEC
await client.disableDnssec('example.com');

// Get DS record for registrar configuration
const ds = await client.getDsRecord('example.com');

// Force re-sign a zone
await client.resignZone('example.com');
```

### Statistics

```typescript
// Get global DNS statistics
const stats = await client.getStats();
// Returns: total_queries, queries_today, cache_hit_rate,
//          top_queried_domains, query_types, etc.

// Get per-zone statistics
const zoneStats = await client.getZoneStats('example.com');
```

### Nameserver Information

```typescript
// Get AEGIS nameserver information
const ns = await client.getNameservers();
// Returns: { primary, secondary, anycast_ips }
```

### Edge Node Management (Admin)

```typescript
// List all edge nodes
const nodes = await client.listEdgeNodes();

// Register a new edge node
const node = await client.registerEdgeNode({
  id: 'node-us-east-1',
  ipv4: '203.0.113.1',
  ipv6: '2001:db8::1',
  region: 'us-east',
  country: 'US',
  city: 'New York',
  latitude: 40.7128,
  longitude: -74.0060,
  capacity: 100,
});

// Unregister an edge node
await client.unregisterEdgeNode('node-us-east-1');

// Get edge node health
const health = await client.getEdgeNodeHealth('node-us-east-1');
```

## Types

### Zone

```typescript
interface Zone {
  domain: string;
  proxied: boolean;
  dnssec_enabled: boolean;
  nameservers: string[];
  account_id?: string;
  created_at: number;
  updated_at: number;
}
```

### DnsRecord

```typescript
interface DnsRecord {
  id: string;
  name: string;
  type: DnsRecordType;
  value: string;
  ttl: number;
  priority?: number;
  proxied: boolean;
  created_at?: number;
  updated_at?: number;
}

type DnsRecordType = 'A' | 'AAAA' | 'CNAME' | 'MX' | 'TXT' | 'NS' | 'SOA' | 'CAA' | 'SRV' | 'PTR';
```

### DnsStats

```typescript
interface DnsStats {
  total_queries: number;
  queries_today: number;
  cache_hit_rate: number;
  top_queried_domains: Array<{ domain: string; count: number }>;
  query_types: Record<DnsRecordType, number>;
  rate_limited_queries: number;
  dnssec_queries: number;
}
```

### DnssecStatus

```typescript
interface DnssecStatus {
  enabled: boolean;
  algorithm?: string;
  key_tag?: number;
  ds_record?: string;
  dnskey_record?: string;
  last_signed_at?: number;
  next_resign_at?: number;
}
```

## Error Handling

```typescript
import { DnsError } from '@aegis/dns-sdk';

try {
  await client.getZone('nonexistent.com');
} catch (error) {
  if (error instanceof DnsError) {
    console.log(`Status: ${error.statusCode}`);
    console.log(`Message: ${error.message}`);
  }
}
```

## Development

```bash
# Build
pnpm build

# Development mode (watch)
pnpm dev

# Run tests
pnpm test

# Type check
pnpm typecheck
```

## License

MIT
