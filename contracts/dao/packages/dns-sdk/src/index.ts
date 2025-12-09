/**
 * AEGIS DNS SDK
 *
 * TypeScript SDK for the AEGIS DNS Management API
 *
 * @packageDocumentation
 *
 * @example
 * ```typescript
 * import { DnsClient } from '@aegis/dns-sdk';
 *
 * const client = new DnsClient({ baseUrl: 'http://localhost:8054' });
 *
 * // List zones
 * const zones = await client.listZones();
 *
 * // Create a zone
 * const zone = await client.createZone({ domain: 'example.com', proxied: true });
 *
 * // Add records
 * await client.createRecord('example.com', {
 *   name: 'www',
 *   type: 'A',
 *   value: '192.168.1.1',
 *   ttl: 300,
 * });
 *
 * // Enable DNSSEC
 * const dsRecord = await client.enableDnssec('example.com');
 * console.log('Add this DS record to your registrar:', dsRecord);
 * ```
 */

// Export client
export { DnsClient, DnsError } from './client';
export type { DnsClientConfig } from './client';

// Export types
export type {
  // Enums
  DnsRecordType,
  AccountTier,
  // Zone types
  Zone,
  CreateZoneRequest,
  UpdateZoneRequest,
  // Record types
  DnsRecord,
  CreateRecordRequest,
  UpdateRecordRequest,
  // DNSSEC types
  DnssecStatus,
  DsRecord,
  // Statistics types
  DnsStats,
  ZoneStats,
  // Edge registry types
  EdgeNode,
  // Account types
  DnsAccount,
  TierLimits,
  // API types
  ApiResponse,
  PaginatedResponse,
  HealthResponse,
  NameserverInfo,
} from './types';
