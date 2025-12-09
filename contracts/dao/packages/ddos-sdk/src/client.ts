/**
 * AEGIS DDoS Protection SDK Client
 *
 * HTTP client for interacting with the AEGIS DDoS Protection API
 */

import type {
  DDoSPolicy,
  DDoSPolicyInput,
  DDoSPolicyUpdate,
  BlocklistEntry,
  AllowlistEntry,
  BlocklistAddRequest,
  AllowlistAddRequest,
  GlobalStats,
  DomainStats,
  AttackEvent,
  TopAttacker,
  ApiResponse,
  PaginatedResponse,
  SseEvent,
} from './types';

// =============================================================================
// CONFIGURATION
// =============================================================================

/** DDoS SDK configuration options */
export interface DDoSClientConfig {
  /** Base URL of the AEGIS DDoS API (e.g., 'http://localhost:8080') */
  baseUrl: string;
  /** Optional API key for authentication */
  apiKey?: string;
  /** Request timeout in milliseconds (default: 30000) */
  timeout?: number;
  /** Custom fetch implementation (for testing/SSR) */
  fetch?: typeof fetch;
}

/** Default configuration values */
const DEFAULT_CONFIG: Partial<DDoSClientConfig> = {
  timeout: 30000,
};

// =============================================================================
// ERROR TYPES
// =============================================================================

/** DDoS API Error */
export class DDoSApiError extends Error {
  constructor(
    message: string,
    public readonly statusCode: number,
    public readonly response?: unknown
  ) {
    super(message);
    this.name = 'DDoSApiError';
  }
}

// =============================================================================
// DDOS CLIENT
// =============================================================================

/**
 * DDoS Protection API Client
 *
 * Provides methods for managing DDoS protection policies, blocklists,
 * allowlists, and retrieving statistics.
 *
 * @example
 * ```typescript
 * const client = new DDoSClient({ baseUrl: 'http://localhost:8080' });
 *
 * // Create a policy
 * await client.createPolicy('example.com', {
 *   enabled: true,
 *   syn_threshold: 100,
 *   udp_threshold: 1000,
 * });
 *
 * // Get statistics
 * const stats = await client.getGlobalStats();
 * ```
 */
export class DDoSClient {
  private readonly baseUrl: string;
  private readonly apiKey?: string;
  private readonly timeout: number;
  private readonly fetchFn: typeof fetch;

  constructor(config: DDoSClientConfig) {
    this.baseUrl = config.baseUrl.replace(/\/$/, '');
    this.apiKey = config.apiKey;
    this.timeout = config.timeout ?? DEFAULT_CONFIG.timeout!;
    this.fetchFn = config.fetch ?? globalThis.fetch;
  }

  // ===========================================================================
  // PRIVATE HELPERS
  // ===========================================================================

  private get apiBase(): string {
    return `${this.baseUrl}/aegis/ddos/api`;
  }

  private async request<T>(
    method: string,
    path: string,
    body?: unknown
  ): Promise<T> {
    const url = `${this.apiBase}${path}`;
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
    };

    if (this.apiKey) {
      headers['Authorization'] = `Bearer ${this.apiKey}`;
    }

    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), this.timeout);

    try {
      const response = await this.fetchFn(url, {
        method,
        headers,
        body: body ? JSON.stringify(body) : undefined,
        signal: controller.signal,
      });

      clearTimeout(timeoutId);

      const data = await response.json() as ApiResponse<T>;

      if (!response.ok || !data.success) {
        throw new DDoSApiError(
          data.error || data.message || `Request failed with status ${response.status}`,
          response.status,
          data
        );
      }

      return data.data as T;
    } catch (error) {
      clearTimeout(timeoutId);

      if (error instanceof DDoSApiError) {
        throw error;
      }

      if (error instanceof Error && error.name === 'AbortError') {
        throw new DDoSApiError('Request timeout', 408);
      }

      throw new DDoSApiError(
        error instanceof Error ? error.message : 'Unknown error',
        0
      );
    }
  }

  // ===========================================================================
  // HEALTH
  // ===========================================================================

  /** Check API health status */
  async health(): Promise<{ status: string; version: string }> {
    return this.request('GET', '/health');
  }

  // ===========================================================================
  // POLICIES
  // ===========================================================================

  /**
   * Get DDoS policy for a domain
   * @param domain - The domain name
   */
  async getPolicy(domain: string): Promise<DDoSPolicy> {
    return this.request('GET', `/policy/${encodeURIComponent(domain)}`);
  }

  /**
   * Create a new DDoS policy for a domain
   * @param domain - The domain name
   * @param policy - Policy configuration
   */
  async createPolicy(domain: string, policy: DDoSPolicyInput): Promise<void> {
    await this.request('POST', `/policy/${encodeURIComponent(domain)}`, policy);
  }

  /**
   * Update an existing DDoS policy
   * @param domain - The domain name
   * @param update - Partial policy update
   */
  async updatePolicy(domain: string, update: DDoSPolicyUpdate): Promise<void> {
    await this.request('PATCH', `/policy/${encodeURIComponent(domain)}`, update);
  }

  /**
   * Delete a DDoS policy
   * @param domain - The domain name
   */
  async deletePolicy(domain: string): Promise<void> {
    await this.request('DELETE', `/policy/${encodeURIComponent(domain)}`);
  }

  /**
   * List all configured policies
   */
  async listPolicies(): Promise<DDoSPolicy[]> {
    return this.request('GET', '/policies');
  }

  // ===========================================================================
  // BLOCKLIST
  // ===========================================================================

  /**
   * Get blocklist entries with pagination
   * @param page - Page number (1-indexed)
   * @param perPage - Items per page
   */
  async getBlocklist(
    page = 1,
    perPage = 50
  ): Promise<PaginatedResponse<BlocklistEntry>> {
    return this.request('GET', `/blocklist?page=${page}&per_page=${perPage}`);
  }

  /**
   * Add an IP to the blocklist
   * @param request - Block request with IP, reason, and optional duration
   */
  async addToBlocklist(request: BlocklistAddRequest): Promise<void> {
    await this.request('POST', '/blocklist', request);
  }

  /**
   * Remove an IP from the blocklist
   * @param ip - IP address to unblock
   */
  async removeFromBlocklist(ip: string): Promise<void> {
    await this.request('DELETE', `/blocklist/${encodeURIComponent(ip)}`);
  }

  /**
   * Check if an IP is blocklisted
   * @param ip - IP address to check
   */
  async isBlocklisted(ip: string): Promise<boolean> {
    try {
      const result = await this.request<{ blocked: boolean }>(
        'GET',
        `/blocklist/check/${encodeURIComponent(ip)}`
      );
      return result.blocked;
    } catch (error) {
      if (error instanceof DDoSApiError && error.statusCode === 404) {
        return false;
      }
      throw error;
    }
  }

  // ===========================================================================
  // ALLOWLIST
  // ===========================================================================

  /**
   * Get allowlist entries
   */
  async getAllowlist(): Promise<AllowlistEntry[]> {
    return this.request('GET', '/allowlist');
  }

  /**
   * Add an IP to the allowlist
   * @param request - Allowlist request with IP and reason
   */
  async addToAllowlist(request: AllowlistAddRequest): Promise<void> {
    await this.request('POST', '/allowlist', request);
  }

  /**
   * Remove an IP from the allowlist
   * @param ip - IP address to remove
   */
  async removeFromAllowlist(ip: string): Promise<void> {
    await this.request('DELETE', `/allowlist/${encodeURIComponent(ip)}`);
  }

  // ===========================================================================
  // STATISTICS
  // ===========================================================================

  /**
   * Get global DDoS protection statistics
   */
  async getGlobalStats(): Promise<GlobalStats> {
    return this.request('GET', '/stats');
  }

  /**
   * Get statistics for a specific domain
   * @param domain - The domain name
   */
  async getDomainStats(domain: string): Promise<DomainStats> {
    return this.request('GET', `/stats/${encodeURIComponent(domain)}`);
  }

  /**
   * Get recent attack events
   * @param limit - Maximum number of events to return
   */
  async getAttacks(limit = 100): Promise<AttackEvent[]> {
    return this.request('GET', `/attacks?limit=${limit}`);
  }

  /**
   * Get top attackers
   * @param limit - Maximum number of attackers to return
   */
  async getTopAttackers(limit = 10): Promise<TopAttacker[]> {
    return this.request('GET', `/stats/top-attackers?limit=${limit}`);
  }

  // ===========================================================================
  // REAL-TIME EVENTS (SSE)
  // ===========================================================================

  /**
   * Subscribe to real-time DDoS events via Server-Sent Events
   *
   * @param handlers - Event handlers for different event types
   * @returns Cleanup function to close the connection
   *
   * @example
   * ```typescript
   * const cleanup = client.subscribeToEvents({
   *   onAttackDetected: (event) => console.log('Attack!', event),
   *   onStatsUpdate: (stats) => updateDashboard(stats),
   *   onError: (error) => console.error('SSE error:', error),
   * });
   *
   * // Later: cleanup to close connection
   * cleanup();
   * ```
   */
  subscribeToEvents(handlers: {
    onEvent?: (event: SseEvent) => void;
    onAttackDetected?: (data: unknown) => void;
    onAttackMitigated?: (data: unknown) => void;
    onIpBlocked?: (data: unknown) => void;
    onIpUnblocked?: (data: unknown) => void;
    onRateLimited?: (data: unknown) => void;
    onPolicyUpdated?: (data: unknown) => void;
    onStatsUpdate?: (data: unknown) => void;
    onError?: (error: Error) => void;
    onOpen?: () => void;
  }): () => void {
    const url = `${this.apiBase}/stats/live`;
    const eventSource = new EventSource(url);

    eventSource.onopen = () => {
      handlers.onOpen?.();
    };

    eventSource.onerror = (event) => {
      handlers.onError?.(new Error('EventSource connection error'));
    };

    eventSource.onmessage = (event) => {
      try {
        const sseEvent = JSON.parse(event.data) as SseEvent;

        // Call generic handler
        handlers.onEvent?.(sseEvent);

        // Call type-specific handlers
        switch (sseEvent.type) {
          case 'attack_detected':
            handlers.onAttackDetected?.(sseEvent.data);
            break;
          case 'attack_mitigated':
            handlers.onAttackMitigated?.(sseEvent.data);
            break;
          case 'ip_blocked':
            handlers.onIpBlocked?.(sseEvent.data);
            break;
          case 'ip_unblocked':
            handlers.onIpUnblocked?.(sseEvent.data);
            break;
          case 'rate_limited':
            handlers.onRateLimited?.(sseEvent.data);
            break;
          case 'policy_updated':
            handlers.onPolicyUpdated?.(sseEvent.data);
            break;
          case 'stats_update':
            handlers.onStatsUpdate?.(sseEvent.data);
            break;
        }
      } catch (error) {
        handlers.onError?.(error instanceof Error ? error : new Error('Parse error'));
      }
    };

    // Return cleanup function
    return () => {
      eventSource.close();
    };
  }
}
