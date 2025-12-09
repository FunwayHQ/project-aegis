import { describe, it, expect, vi, beforeEach } from 'vitest';
import { DDoSClient, DDoSApiError } from '../src/client';
import type { DDoSPolicy, GlobalStats } from '../src/types';

describe('DDoSClient', () => {
  let client: DDoSClient;
  let mockFetch: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    mockFetch = vi.fn();
    client = new DDoSClient({
      baseUrl: 'http://localhost:8080',
      fetch: mockFetch as unknown as typeof fetch,
    });
  });

  describe('health', () => {
    it('should return health status', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({
          success: true,
          data: { status: 'healthy', version: '0.1.0' },
        }),
      });

      const result = await client.health();

      expect(result).toEqual({ status: 'healthy', version: '0.1.0' });
      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:8080/aegis/ddos/api/health',
        expect.objectContaining({
          method: 'GET',
        })
      );
    });
  });

  describe('policies', () => {
    it('should get policy for domain', async () => {
      const policy: DDoSPolicy = {
        domain: 'example.com',
        enabled: true,
        syn_threshold: 100,
        udp_threshold: 1000,
        block_duration_secs: 300,
      };

      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ success: true, data: policy }),
      });

      const result = await client.getPolicy('example.com');

      expect(result).toEqual(policy);
      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:8080/aegis/ddos/api/policy/example.com',
        expect.objectContaining({
          method: 'GET',
        })
      );
    });

    it('should create policy', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ success: true, message: 'Policy created' }),
      });

      await client.createPolicy('example.com', {
        enabled: true,
        syn_threshold: 100,
        udp_threshold: 1000,
      });

      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:8080/aegis/ddos/api/policy/example.com',
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({
            enabled: true,
            syn_threshold: 100,
            udp_threshold: 1000,
          }),
        })
      );
    });

    it('should update policy', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ success: true, message: 'Policy updated' }),
      });

      await client.updatePolicy('example.com', { enabled: false });

      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:8080/aegis/ddos/api/policy/example.com',
        expect.objectContaining({
          method: 'PATCH',
          body: JSON.stringify({ enabled: false }),
        })
      );
    });

    it('should delete policy', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ success: true, message: 'Policy deleted' }),
      });

      await client.deletePolicy('example.com');

      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:8080/aegis/ddos/api/policy/example.com',
        expect.objectContaining({
          method: 'DELETE',
        })
      );
    });

    it('should list policies', async () => {
      const policies: DDoSPolicy[] = [
        {
          domain: 'example.com',
          enabled: true,
          syn_threshold: 100,
          udp_threshold: 1000,
          block_duration_secs: 300,
        },
      ];

      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ success: true, data: policies }),
      });

      const result = await client.listPolicies();

      expect(result).toEqual(policies);
    });
  });

  describe('blocklist', () => {
    it('should get blocklist', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({
          success: true,
          data: {
            items: [],
            total: 0,
            page: 1,
            per_page: 50,
          },
        }),
      });

      const result = await client.getBlocklist();

      expect(result.items).toEqual([]);
      expect(result.page).toBe(1);
    });

    it('should add to blocklist', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ success: true, message: 'IP blocked' }),
      });

      await client.addToBlocklist({
        ip: '192.168.1.100',
        reason: 'Test block',
        duration_secs: 300,
      });

      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:8080/aegis/ddos/api/blocklist',
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({
            ip: '192.168.1.100',
            reason: 'Test block',
            duration_secs: 300,
          }),
        })
      );
    });

    it('should remove from blocklist', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ success: true, message: 'IP unblocked' }),
      });

      await client.removeFromBlocklist('192.168.1.100');

      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:8080/aegis/ddos/api/blocklist/192.168.1.100',
        expect.objectContaining({
          method: 'DELETE',
        })
      );
    });
  });

  describe('allowlist', () => {
    it('should get allowlist', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ success: true, data: [] }),
      });

      const result = await client.getAllowlist();

      expect(result).toEqual([]);
    });

    it('should add to allowlist', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ success: true, message: 'IP allowlisted' }),
      });

      await client.addToAllowlist({
        ip: '10.0.0.1',
        reason: 'Trusted server',
      });

      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:8080/aegis/ddos/api/allowlist',
        expect.objectContaining({
          method: 'POST',
        })
      );
    });
  });

  describe('statistics', () => {
    it('should get global stats', async () => {
      const stats: GlobalStats = {
        total_requests: 10000,
        total_blocked: 500,
        total_rate_limited: 200,
        total_attacks: 10,
        active_attacks: 1,
        blocked_ips: 50,
        allowed_ips: 5,
        drop_rate: 5.0,
        uptime_secs: 86400,
      };

      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ success: true, data: stats }),
      });

      const result = await client.getGlobalStats();

      expect(result).toEqual(stats);
    });

    it('should get attacks', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ success: true, data: [] }),
      });

      const result = await client.getAttacks(50);

      expect(result).toEqual([]);
      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:8080/aegis/ddos/api/attacks?limit=50',
        expect.anything()
      );
    });

    it('should get top attackers', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ success: true, data: [] }),
      });

      const result = await client.getTopAttackers(5);

      expect(result).toEqual([]);
      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:8080/aegis/ddos/api/stats/top-attackers?limit=5',
        expect.anything()
      );
    });
  });

  describe('error handling', () => {
    it('should throw DDoSApiError on HTTP error', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: false,
        status: 404,
        json: async () => ({ success: false, error: 'Policy not found' }),
      });

      await expect(client.getPolicy('notfound.com')).rejects.toThrow(DDoSApiError);
    });

    it('should throw DDoSApiError on API error response', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ success: false, error: 'Invalid domain' }),
      });

      await expect(client.getPolicy('')).rejects.toThrow(DDoSApiError);
    });

    it('should handle timeout', async () => {
      const slowClient = new DDoSClient({
        baseUrl: 'http://localhost:8080',
        timeout: 1,
        fetch: mockFetch as unknown as typeof fetch,
      });

      // Simulate abort error from AbortController
      const abortError = new Error('The operation was aborted');
      abortError.name = 'AbortError';

      mockFetch.mockImplementation(() => {
        return new Promise((_, reject) => {
          setTimeout(() => reject(abortError), 10);
        });
      });

      await expect(slowClient.health()).rejects.toThrow('Request timeout');
    });
  });

  describe('configuration', () => {
    it('should include API key in headers', async () => {
      const clientWithKey = new DDoSClient({
        baseUrl: 'http://localhost:8080',
        apiKey: 'test-api-key',
        fetch: mockFetch as unknown as typeof fetch,
      });

      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ success: true, data: { status: 'healthy', version: '0.1.0' } }),
      });

      await clientWithKey.health();

      expect(mockFetch).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          headers: expect.objectContaining({
            Authorization: 'Bearer test-api-key',
          }),
        })
      );
    });

    it('should remove trailing slash from base URL', async () => {
      const clientWithSlash = new DDoSClient({
        baseUrl: 'http://localhost:8080/',
        fetch: mockFetch as unknown as typeof fetch,
      });

      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ success: true, data: { status: 'healthy', version: '0.1.0' } }),
      });

      await clientWithSlash.health();

      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:8080/aegis/ddos/api/health',
        expect.anything()
      );
    });
  });
});
