/**
 * AEGIS DNS SDK Types
 *
 * TypeScript types matching the Rust backend DNS API structures
 */

// =============================================================================
// ENUMS
// =============================================================================

/** DNS record types */
export type DnsRecordType = 'A' | 'AAAA' | 'CNAME' | 'MX' | 'TXT' | 'NS' | 'SOA' | 'CAA' | 'SRV' | 'PTR';

/** Account tier levels */
export type AccountTier = 'free' | 'pro' | 'business' | 'enterprise';

// =============================================================================
// ZONE TYPES
// =============================================================================

/** DNS Zone */
export interface Zone {
  /** Domain name (e.g., "example.com") */
  domain: string;
  /** Whether traffic is proxied through AEGIS */
  proxied: boolean;
  /** Whether DNSSEC is enabled */
  dnssec_enabled: boolean;
  /** Associated nameservers */
  nameservers: string[];
  /** Account ID that owns this zone */
  account_id?: string;
  /** Creation timestamp (Unix seconds) */
  created_at: number;
  /** Last update timestamp (Unix seconds) */
  updated_at: number;
}

/** Request to create a new zone */
export interface CreateZoneRequest {
  /** Domain name */
  domain: string;
  /** Whether to proxy traffic through AEGIS */
  proxied?: boolean;
}

/** Request to update a zone */
export interface UpdateZoneRequest {
  /** Whether to proxy traffic through AEGIS */
  proxied?: boolean;
}

// =============================================================================
// DNS RECORD TYPES
// =============================================================================

/** DNS Record */
export interface DnsRecord {
  /** Unique record ID */
  id: string;
  /** Record name (e.g., "www" or "@" for root) */
  name: string;
  /** Record type */
  type: DnsRecordType;
  /** Record value */
  value: string;
  /** Time to live in seconds */
  ttl: number;
  /** Priority (for MX/SRV records) */
  priority?: number;
  /** Whether record is proxied through AEGIS */
  proxied: boolean;
  /** Creation timestamp (Unix seconds) */
  created_at?: number;
  /** Last update timestamp (Unix seconds) */
  updated_at?: number;
}

/** Request to create a DNS record */
export interface CreateRecordRequest {
  /** Record name (e.g., "www" or "@" for root) */
  name: string;
  /** Record type */
  type: DnsRecordType;
  /** Record value (IP address, hostname, etc.) */
  value: string;
  /** Time to live in seconds (default: 300) */
  ttl?: number;
  /** Priority for MX/SRV records */
  priority?: number;
  /** Whether to proxy through AEGIS */
  proxied?: boolean;
}

/** Request to update a DNS record */
export interface UpdateRecordRequest {
  /** Record name */
  name?: string;
  /** Record type */
  type?: DnsRecordType;
  /** Record value */
  value?: string;
  /** Time to live in seconds */
  ttl?: number;
  /** Priority for MX/SRV records */
  priority?: number;
  /** Whether to proxy through AEGIS */
  proxied?: boolean;
}

// =============================================================================
// DNSSEC TYPES
// =============================================================================

/** DNSSEC status for a zone */
export interface DnssecStatus {
  /** Whether DNSSEC is enabled */
  enabled: boolean;
  /** DNSSEC algorithm (e.g., "ED25519", "ECDSAP256SHA256") */
  algorithm?: string;
  /** Key tag for the signing key */
  key_tag?: number;
  /** DS record for registrar configuration */
  ds_record?: string;
  /** DNSKEY record */
  dnskey_record?: string;
  /** Last signing timestamp */
  last_signed_at?: number;
  /** Next re-sign timestamp */
  next_resign_at?: number;
}

/** DS Record for registrar */
export interface DsRecord {
  /** Key tag */
  key_tag: number;
  /** Algorithm number */
  algorithm: number;
  /** Digest type */
  digest_type: number;
  /** Digest value (hex encoded) */
  digest: string;
}

// =============================================================================
// STATISTICS TYPES
// =============================================================================

/** Global DNS statistics */
export interface DnsStats {
  /** Total queries processed */
  total_queries: number;
  /** Queries in the last 24 hours */
  queries_today: number;
  /** Cache hit rate percentage (0-100) */
  cache_hit_rate: number;
  /** Top queried domains */
  top_queried_domains: Array<{ domain: string; count: number }>;
  /** Query type distribution */
  query_types: Record<DnsRecordType, number>;
  /** Queries blocked by rate limiting */
  rate_limited_queries: number;
  /** DNSSEC signed responses */
  dnssec_queries: number;
}

/** Per-zone DNS statistics */
export interface ZoneStats {
  /** Zone domain */
  domain: string;
  /** Total queries for this zone */
  total_queries: number;
  /** Queries in the last 24 hours */
  queries_today: number;
  /** Query type breakdown */
  query_types: Record<DnsRecordType, number>;
  /** Top queried record names */
  top_records: Array<{ name: string; type: DnsRecordType; count: number }>;
  /** Geographic distribution of queries */
  geo_distribution?: Record<string, number>;
}

// =============================================================================
// EDGE REGISTRY TYPES
// =============================================================================

/** Edge node for geo-aware DNS */
export interface EdgeNode {
  /** Node ID */
  id: string;
  /** IPv4 address */
  ipv4?: string;
  /** IPv6 address */
  ipv6?: string;
  /** Region identifier (e.g., "us-east") */
  region: string;
  /** Country code (ISO 3166-1 alpha-2) */
  country: string;
  /** City name */
  city?: string;
  /** Latitude */
  latitude: number;
  /** Longitude */
  longitude: number;
  /** Relative capacity weight */
  capacity: number;
  /** Whether node is healthy */
  healthy: boolean;
  /** Last health check timestamp */
  last_health_check: number;
}

// =============================================================================
// ACCOUNT TYPES
// =============================================================================

/** DNS Account */
export interface DnsAccount {
  /** Account ID */
  id: string;
  /** Account tier */
  tier: AccountTier;
  /** Maximum zones allowed */
  max_zones: number;
  /** Maximum records per zone */
  max_records_per_zone: number;
  /** Rate limit (queries per second) */
  rate_limit: number;
  /** Whether DNSSEC is available */
  dnssec_available: boolean;
  /** Whether advanced analytics are available */
  analytics_available: boolean;
  /** Creation timestamp */
  created_at: number;
}

/** Tier limits */
export interface TierLimits {
  /** Tier name */
  tier: AccountTier;
  /** Maximum zones */
  max_zones: number;
  /** Maximum records per zone */
  max_records_per_zone: number;
  /** Rate limit (queries per second) */
  rate_limit: number;
  /** DNSSEC enabled */
  dnssec: boolean;
  /** Analytics enabled */
  analytics: boolean;
  /** Geo-routing enabled */
  geo_routing: boolean;
}

// =============================================================================
// API RESPONSE TYPES
// =============================================================================

/** Standard API response wrapper */
export interface ApiResponse<T> {
  /** Whether the request was successful */
  success: boolean;
  /** Human-readable message */
  message?: string;
  /** Response data */
  data?: T;
  /** Error details if not successful */
  error?: string;
}

/** Paginated response */
export interface PaginatedResponse<T> {
  /** Items in this page */
  items: T[];
  /** Total count of all items */
  total: number;
  /** Page number (1-indexed) */
  page: number;
  /** Items per page */
  per_page: number;
}

/** Health check response */
export interface HealthResponse {
  /** Service status */
  status: 'healthy' | 'degraded' | 'unhealthy';
  /** Service version */
  version: string;
  /** Uptime in seconds */
  uptime: number;
  /** Component statuses */
  components?: Record<string, 'up' | 'down'>;
}

/** Nameserver information */
export interface NameserverInfo {
  /** Primary nameserver hostname */
  primary: string;
  /** Secondary nameserver hostnames */
  secondary: string[];
  /** Anycast IPs */
  anycast_ips: {
    ipv4: string[];
    ipv6: string[];
  };
}
