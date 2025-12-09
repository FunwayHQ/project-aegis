/**
 * AEGIS DDoS Protection SDK
 *
 * TypeScript SDK for interacting with AEGIS DDoS Protection API
 *
 * @packageDocumentation
 */

// Client
export { DDoSClient, DDoSApiError } from './client';
export type { DDoSClientConfig } from './client';

// Types
export type {
  // Enums
  RateLimitScope,
  ChallengeType,
  AttackType,
  BlockSource,

  // Policy Types
  RateLimitPolicy,
  ChallengePolicy,
  DDoSPolicy,
  DDoSPolicyUpdate,
  DDoSPolicyInput,

  // Blocklist/Allowlist
  BlocklistEntry,
  AllowlistEntry,
  BlocklistAddRequest,
  AllowlistAddRequest,

  // Statistics
  AttackEvent,
  GlobalStats,
  DomainStats,
  TopAttacker,

  // API Response
  ApiResponse,
  PaginatedResponse,

  // SSE Events
  SseEventType,
  SseEvent,
  AttackDetectedEvent,
  IpBlockedEvent,
  StatsUpdateEvent,
} from './types';
