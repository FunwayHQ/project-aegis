/**
 * AEGIS DDoS Protection SDK Types
 *
 * TypeScript types matching the Rust backend structures
 */

// =============================================================================
// ENUMS
// =============================================================================

/** Scope for rate limiting */
export type RateLimitScope = 'per_ip' | 'per_route' | 'global';

/** Type of challenge to issue */
export type ChallengeType = 'invisible' | 'managed' | 'interactive';

/** Type of DDoS attack */
export type AttackType = 'syn_flood' | 'udp_flood' | 'http_flood' | 'slowloris' | 'unknown';

/** Source of the IP block */
export type BlockSource = 'manual' | 'ebpf' | 'waf' | 'rate_limiter' | 'p2p_threat' | 'auto';

// =============================================================================
// POLICY TYPES
// =============================================================================

/** Rate limiting configuration */
export interface RateLimitPolicy {
  /** Whether rate limiting is enabled */
  enabled: boolean;
  /** Maximum requests per minute */
  max_requests_per_minute: number;
  /** Window duration in seconds */
  window_duration_secs: number;
  /** Rate limit scope */
  scope: RateLimitScope;
  /** Burst allowance (extra requests above limit) */
  burst_allowance?: number;
}

/** Challenge mode configuration */
export interface ChallengePolicy {
  /** Whether challenge mode is enabled */
  enabled: boolean;
  /** Threshold before issuing challenge (requests per minute) */
  trigger_threshold: number;
  /** Type of challenge to issue */
  challenge_type: ChallengeType;
  /** Challenge validity duration in seconds */
  validity_secs: number;
}

/** Complete DDoS protection policy for a domain */
export interface DDoSPolicy {
  /** Domain name this policy applies to */
  domain: string;
  /** Whether DDoS protection is enabled */
  enabled: boolean;
  /** SYN flood threshold (packets per second per IP) */
  syn_threshold: number;
  /** UDP flood threshold (packets per second per IP) */
  udp_threshold: number;
  /** Duration to block offending IPs (seconds) */
  block_duration_secs: number;
  /** Rate limiting policy */
  rate_limit?: RateLimitPolicy;
  /** Challenge mode policy */
  challenge_mode?: ChallengePolicy;
  /** Custom allowlist IPs for this domain */
  allowlist?: string[];
  /** Custom blocklist IPs for this domain */
  blocklist?: string[];
  /** Creation timestamp (Unix seconds) */
  created_at?: number;
  /** Last update timestamp (Unix seconds) */
  updated_at?: number;
}

/** Partial update for DDoS policy */
export interface DDoSPolicyUpdate {
  enabled?: boolean;
  syn_threshold?: number;
  udp_threshold?: number;
  block_duration_secs?: number;
  rate_limit?: RateLimitPolicy;
  challenge_mode?: ChallengePolicy;
}

/** Input for creating/updating a policy */
export interface DDoSPolicyInput {
  enabled: boolean;
  syn_threshold?: number;
  udp_threshold?: number;
  block_duration_secs?: number;
  rate_limit?: RateLimitPolicy;
  challenge_mode?: ChallengePolicy;
  allowlist?: string[];
  blocklist?: string[];
}

// =============================================================================
// BLOCKLIST/ALLOWLIST TYPES
// =============================================================================

/** Entry in the blocklist */
export interface BlocklistEntry {
  /** IP address or CIDR notation */
  ip: string;
  /** Reason for blocking */
  reason: string;
  /** Source of the block */
  source: BlockSource;
  /** Unix timestamp when blocked */
  blocked_at: number;
  /** Unix timestamp when block expires (0 = permanent) */
  expires_at: number;
}

/** Entry in the allowlist */
export interface AllowlistEntry {
  /** IP address or CIDR notation */
  ip: string;
  /** Reason for allowlisting */
  reason: string;
  /** Unix timestamp when added */
  added_at: number;
}

/** Request to add an IP to the blocklist */
export interface BlocklistAddRequest {
  /** IP address or CIDR notation */
  ip: string;
  /** Reason for blocking */
  reason: string;
  /** Duration in seconds (0 = permanent) */
  duration_secs?: number;
}

/** Request to add an IP to the allowlist */
export interface AllowlistAddRequest {
  /** IP address or CIDR notation */
  ip: string;
  /** Reason for allowlisting */
  reason: string;
}

// =============================================================================
// STATISTICS TYPES
// =============================================================================

/** Attack event data */
export interface AttackEvent {
  /** Attack ID */
  id: string;
  /** Type of attack */
  attack_type: AttackType;
  /** Source IP address */
  source_ip: string;
  /** Target domain */
  target_domain: string;
  /** Packets per second */
  packets_per_second: number;
  /** Severity level (0-100) */
  severity: number;
  /** Whether the attack was mitigated */
  mitigated: boolean;
  /** Unix timestamp when detected */
  timestamp: number;
}

/** Global statistics */
export interface GlobalStats {
  /** Total requests processed */
  total_requests: number;
  /** Total requests blocked */
  total_blocked: number;
  /** Total requests rate limited */
  total_rate_limited: number;
  /** Total attacks detected */
  total_attacks: number;
  /** Currently active attacks */
  active_attacks: number;
  /** Blocked IPs count */
  blocked_ips: number;
  /** Allowed IPs count */
  allowed_ips: number;
  /** Drop rate percentage (0-100) */
  drop_rate: number;
  /** Uptime in seconds */
  uptime_secs: number;
}

/** Per-domain statistics */
export interface DomainStats {
  /** Domain name */
  domain: string;
  /** Total requests for this domain */
  requests: number;
  /** Blocked requests for this domain */
  blocked: number;
  /** Rate limited requests for this domain */
  rate_limited: number;
  /** Challenges issued */
  challenges_issued: number;
  /** Challenges passed */
  challenges_passed: number;
}

/** Top attacker entry */
export interface TopAttacker {
  /** Attacker IP */
  ip: string;
  /** Number of attacks from this IP */
  attack_count: number;
  /** Total packets sent */
  total_packets: number;
  /** Last attack timestamp */
  last_attack: number;
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

// =============================================================================
// SSE EVENT TYPES
// =============================================================================

/** Server-Sent Event types */
export type SseEventType =
  | 'attack_detected'
  | 'attack_mitigated'
  | 'ip_blocked'
  | 'ip_unblocked'
  | 'rate_limited'
  | 'policy_updated'
  | 'stats_update';

/** SSE event data */
export interface SseEvent {
  /** Event type */
  type: SseEventType;
  /** Event timestamp (Unix seconds) */
  timestamp: number;
  /** Event payload (varies by type) */
  data: unknown;
}

/** Attack detected event */
export interface AttackDetectedEvent {
  attack_type: AttackType;
  source_ip: string;
  target_domain: string;
  packets_per_second: number;
  severity: number;
}

/** IP blocked event */
export interface IpBlockedEvent {
  ip: string;
  reason: string;
  source: BlockSource;
  expires_at: number;
}

/** Stats update event */
export interface StatsUpdateEvent {
  total_requests: number;
  total_blocked: number;
  active_attacks: number;
  drop_rate: number;
}
