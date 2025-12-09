/**
 * AEGIS DNS SDK Client
 *
 * TypeScript client for the AEGIS DNS Management API
 */

import type {
  Zone,
  DnsRecord,
  CreateZoneRequest,
  UpdateZoneRequest,
  CreateRecordRequest,
  UpdateRecordRequest,
  DnssecStatus,
  DsRecord,
  DnsStats,
  ZoneStats,
  EdgeNode,
  DnsAccount,
  TierLimits,
  ApiResponse,
  HealthResponse,
  NameserverInfo,
} from './types';

/** DNS SDK Error */
export class DnsError extends Error {
  constructor(
    message: string,
    public readonly statusCode?: number,
    public readonly response?: unknown
  ) {
    super(message);
    this.name = 'DnsError';
  }
}

/** DNS Client Configuration */
export interface DnsClientConfig {
  /** Base URL of the DNS API (default: http://localhost:8054) */
  baseUrl?: string;
  /** API key for authentication */
  apiKey?: string;
  /** Request timeout in milliseconds (default: 30000) */
  timeout?: number;
}

/**
 * AEGIS DNS Client
 *
 * Client for interacting with the AEGIS DNS Management API.
 *
 * @example
 * ```typescript
 * const client = new DnsClient({ baseUrl: 'http://localhost:8054' });
 *
 * // List all zones
 * const zones = await client.listZones();
 *
 * // Create a new zone
 * const zone = await client.createZone({ domain: 'example.com', proxied: true });
 *
 * // Add a DNS record
 * await client.createRecord('example.com', {
 *   name: 'www',
 *   type: 'A',
 *   value: '192.168.1.1',
 *   ttl: 300,
 * });
 * ```
 */
export class DnsClient {
  private readonly baseUrl: string;
  private readonly apiKey?: string;
  private readonly timeout: number;

  constructor(config: DnsClientConfig = {}) {
    this.baseUrl = config.baseUrl || 'http://localhost:8054';
    this.apiKey = config.apiKey;
    this.timeout = config.timeout || 30000;
  }

  // ==========================================================================
  // HEALTH
  // ==========================================================================

  /**
   * Check API health
   */
  async health(): Promise<HealthResponse> {
    return this.request<HealthResponse>('GET', '/aegis/dns/api/health');
  }

  // ==========================================================================
  // ZONES
  // ==========================================================================

  /**
   * List all zones
   */
  async listZones(): Promise<Zone[]> {
    const response = await this.request<ApiResponse<Zone[]>>('GET', '/aegis/dns/api/zones');
    return response.data || [];
  }

  /**
   * Get a zone by domain
   */
  async getZone(domain: string): Promise<Zone> {
    const response = await this.request<ApiResponse<Zone>>('GET', `/aegis/dns/api/zones/${encodeURIComponent(domain)}`);
    if (!response.data) {
      throw new DnsError(`Zone not found: ${domain}`, 404);
    }
    return response.data;
  }

  /**
   * Create a new zone
   */
  async createZone(req: CreateZoneRequest): Promise<Zone> {
    const response = await this.request<ApiResponse<Zone>>('POST', '/aegis/dns/api/zones', req);
    if (!response.data) {
      throw new DnsError('Failed to create zone');
    }
    return response.data;
  }

  /**
   * Update a zone
   */
  async updateZone(domain: string, updates: UpdateZoneRequest): Promise<Zone> {
    const response = await this.request<ApiResponse<Zone>>(
      'PUT',
      `/aegis/dns/api/zones/${encodeURIComponent(domain)}`,
      updates
    );
    if (!response.data) {
      throw new DnsError('Failed to update zone');
    }
    return response.data;
  }

  /**
   * Delete a zone
   */
  async deleteZone(domain: string): Promise<void> {
    await this.request<ApiResponse<void>>('DELETE', `/aegis/dns/api/zones/${encodeURIComponent(domain)}`);
  }

  // ==========================================================================
  // RECORDS
  // ==========================================================================

  /**
   * List all records for a zone
   */
  async listRecords(domain: string): Promise<DnsRecord[]> {
    const response = await this.request<ApiResponse<DnsRecord[]>>(
      'GET',
      `/aegis/dns/api/zones/${encodeURIComponent(domain)}/records`
    );
    return response.data || [];
  }

  /**
   * Get a specific record
   */
  async getRecord(domain: string, recordId: string): Promise<DnsRecord> {
    const response = await this.request<ApiResponse<DnsRecord>>(
      'GET',
      `/aegis/dns/api/zones/${encodeURIComponent(domain)}/records/${encodeURIComponent(recordId)}`
    );
    if (!response.data) {
      throw new DnsError(`Record not found: ${recordId}`, 404);
    }
    return response.data;
  }

  /**
   * Create a DNS record
   */
  async createRecord(domain: string, req: CreateRecordRequest): Promise<DnsRecord> {
    const response = await this.request<ApiResponse<DnsRecord>>(
      'POST',
      `/aegis/dns/api/zones/${encodeURIComponent(domain)}/records`,
      req
    );
    if (!response.data) {
      throw new DnsError('Failed to create record');
    }
    return response.data;
  }

  /**
   * Update a DNS record
   */
  async updateRecord(domain: string, recordId: string, updates: UpdateRecordRequest): Promise<DnsRecord> {
    const response = await this.request<ApiResponse<DnsRecord>>(
      'PUT',
      `/aegis/dns/api/zones/${encodeURIComponent(domain)}/records/${encodeURIComponent(recordId)}`,
      updates
    );
    if (!response.data) {
      throw new DnsError('Failed to update record');
    }
    return response.data;
  }

  /**
   * Delete a DNS record
   */
  async deleteRecord(domain: string, recordId: string): Promise<void> {
    await this.request<ApiResponse<void>>(
      'DELETE',
      `/aegis/dns/api/zones/${encodeURIComponent(domain)}/records/${encodeURIComponent(recordId)}`
    );
  }

  // ==========================================================================
  // DNSSEC
  // ==========================================================================

  /**
   * Get DNSSEC status for a zone
   */
  async getDnssecStatus(domain: string): Promise<DnssecStatus> {
    const response = await this.request<ApiResponse<DnssecStatus>>(
      'GET',
      `/aegis/dns/api/zones/${encodeURIComponent(domain)}/dnssec`
    );
    if (!response.data) {
      throw new DnsError('Failed to get DNSSEC status');
    }
    return response.data;
  }

  /**
   * Enable DNSSEC for a zone
   */
  async enableDnssec(domain: string): Promise<DsRecord> {
    const response = await this.request<ApiResponse<DsRecord>>(
      'POST',
      `/aegis/dns/api/zones/${encodeURIComponent(domain)}/dnssec/enable`
    );
    if (!response.data) {
      throw new DnsError('Failed to enable DNSSEC');
    }
    return response.data;
  }

  /**
   * Disable DNSSEC for a zone
   */
  async disableDnssec(domain: string): Promise<void> {
    await this.request<ApiResponse<void>>(
      'POST',
      `/aegis/dns/api/zones/${encodeURIComponent(domain)}/dnssec/disable`
    );
  }

  /**
   * Get DS record for registrar configuration
   */
  async getDsRecord(domain: string): Promise<DsRecord> {
    const response = await this.request<ApiResponse<DsRecord>>(
      'GET',
      `/aegis/dns/api/zones/${encodeURIComponent(domain)}/dnssec/ds`
    );
    if (!response.data) {
      throw new DnsError('DS record not available');
    }
    return response.data;
  }

  /**
   * Force re-sign a zone
   */
  async resignZone(domain: string): Promise<void> {
    await this.request<ApiResponse<void>>(
      'POST',
      `/aegis/dns/api/zones/${encodeURIComponent(domain)}/dnssec/resign`
    );
  }

  // ==========================================================================
  // STATISTICS
  // ==========================================================================

  /**
   * Get global DNS statistics
   */
  async getStats(): Promise<DnsStats> {
    const response = await this.request<ApiResponse<DnsStats>>('GET', '/aegis/dns/api/stats');
    if (!response.data) {
      throw new DnsError('Failed to get statistics');
    }
    return response.data;
  }

  /**
   * Get statistics for a specific zone
   */
  async getZoneStats(domain: string): Promise<ZoneStats> {
    const response = await this.request<ApiResponse<ZoneStats>>(
      'GET',
      `/aegis/dns/api/stats/${encodeURIComponent(domain)}`
    );
    if (!response.data) {
      throw new DnsError('Failed to get zone statistics');
    }
    return response.data;
  }

  // ==========================================================================
  // NAMESERVERS
  // ==========================================================================

  /**
   * Get AEGIS nameserver information
   */
  async getNameservers(): Promise<NameserverInfo> {
    const response = await this.request<ApiResponse<NameserverInfo>>('GET', '/aegis/dns/api/nameservers');
    if (!response.data) {
      throw new DnsError('Failed to get nameserver information');
    }
    return response.data;
  }

  // ==========================================================================
  // EDGE NODES (Admin)
  // ==========================================================================

  /**
   * List all edge nodes
   */
  async listEdgeNodes(): Promise<EdgeNode[]> {
    const response = await this.request<ApiResponse<EdgeNode[]>>('GET', '/aegis/dns/api/edges');
    return response.data || [];
  }

  /**
   * Register an edge node
   */
  async registerEdgeNode(node: Omit<EdgeNode, 'healthy' | 'last_health_check'>): Promise<EdgeNode> {
    const response = await this.request<ApiResponse<EdgeNode>>('POST', '/aegis/dns/api/edges', node);
    if (!response.data) {
      throw new DnsError('Failed to register edge node');
    }
    return response.data;
  }

  /**
   * Unregister an edge node
   */
  async unregisterEdgeNode(nodeId: string): Promise<void> {
    await this.request<ApiResponse<void>>('DELETE', `/aegis/dns/api/edges/${encodeURIComponent(nodeId)}`);
  }

  /**
   * Get edge node health
   */
  async getEdgeNodeHealth(nodeId: string): Promise<{ healthy: boolean; last_check: number }> {
    const response = await this.request<ApiResponse<{ healthy: boolean; last_check: number }>>(
      'GET',
      `/aegis/dns/api/edges/${encodeURIComponent(nodeId)}/health`
    );
    if (!response.data) {
      throw new DnsError('Failed to get edge node health');
    }
    return response.data;
  }

  // ==========================================================================
  // ACCOUNTS (Admin)
  // ==========================================================================

  /**
   * Get account information
   */
  async getAccount(accountId: string): Promise<DnsAccount> {
    const response = await this.request<ApiResponse<DnsAccount>>(
      'GET',
      `/aegis/dns/api/accounts/${encodeURIComponent(accountId)}`
    );
    if (!response.data) {
      throw new DnsError('Account not found', 404);
    }
    return response.data;
  }

  /**
   * Get tier limits
   */
  async getTierLimits(): Promise<TierLimits[]> {
    const response = await this.request<ApiResponse<TierLimits[]>>('GET', '/aegis/dns/api/tiers');
    return response.data || [];
  }

  // ==========================================================================
  // PRIVATE METHODS
  // ==========================================================================

  private async request<T>(method: string, path: string, body?: unknown): Promise<T> {
    const url = `${this.baseUrl}${path}`;
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      Accept: 'application/json',
    };

    if (this.apiKey) {
      headers['Authorization'] = `Bearer ${this.apiKey}`;
    }

    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), this.timeout);

    try {
      const response = await fetch(url, {
        method,
        headers,
        body: body ? JSON.stringify(body) : undefined,
        signal: controller.signal,
      });

      clearTimeout(timeoutId);

      if (!response.ok) {
        let errorData: unknown;
        try {
          errorData = await response.json();
        } catch {
          errorData = await response.text();
        }

        throw new DnsError(
          `HTTP ${response.status}: ${response.statusText}`,
          response.status,
          errorData
        );
      }

      // Handle empty responses
      const text = await response.text();
      if (!text) {
        return {} as T;
      }

      return JSON.parse(text) as T;
    } catch (error) {
      clearTimeout(timeoutId);

      if (error instanceof DnsError) {
        throw error;
      }

      if (error instanceof Error) {
        if (error.name === 'AbortError') {
          throw new DnsError('Request timeout');
        }
        throw new DnsError(error.message);
      }

      throw new DnsError('Unknown error');
    }
  }
}
