import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { DdosProvider, useDdos } from '../../contexts/DdosContext';

// Mock the DDoS SDK
vi.mock('@aegis/ddos-sdk', () => ({
  DDoSClient: vi.fn().mockImplementation(() => ({
    getGlobalStats: vi.fn().mockResolvedValue({
      total_requests: 50000,
      total_blocked: 1000,
      total_rate_limited: 500,
      blocked_ips: 25,
      allowed_ips: 5,
      active_attacks: 2,
      drop_rate: 2.0,
      total_attacks: 100,
      uptime_secs: 86400,
    }),
    getBlocklist: vi.fn().mockResolvedValue({
      items: [
        { ip: '192.168.1.100', reason: 'DDoS attack', source: 'auto', blocked_at: 1700000000, expires_at: 0 },
      ],
      total: 1,
      page: 1,
      per_page: 50,
    }),
    getAllowlist: vi.fn().mockResolvedValue([
      { ip: '10.0.0.1', reason: 'Trusted', added_at: 1700000000 },
    ]),
    listPolicies: vi.fn().mockResolvedValue([
      { domain: 'example.com', enabled: true, syn_threshold: 100, udp_threshold: 1000, block_duration_secs: 3600 },
    ]),
    getAttacks: vi.fn().mockResolvedValue([
      { id: 'atk1', attack_type: 'syn_flood', source_ip: '1.2.3.4', target_domain: 'example.com', packets_per_second: 10000, severity: 80, mitigated: true, timestamp: 1700000000 },
    ]),
    subscribeToEvents: vi.fn().mockReturnValue(() => {}),
  })),
}));

function TestConsumer() {
  const { stats, blocklist, allowlist, policies, attacks, loading, error, client } = useDdos();

  return (
    <div>
      <div data-testid="loading">{loading ? 'loading' : 'not-loading'}</div>
      <div data-testid="error">{error || 'no-error'}</div>
      <div data-testid="stats">{stats ? `${stats.total_requests}` : 'no-stats'}</div>
      <div data-testid="blocklist-count">{blocklist.length}</div>
      <div data-testid="allowlist-count">{allowlist.length}</div>
      <div data-testid="policies-count">{policies.length}</div>
      <div data-testid="attacks-count">{attacks.length}</div>
      <div data-testid="client">{client ? 'has-client' : 'no-client'}</div>
    </div>
  );
}

describe('DdosContext', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('provides client to consumers', async () => {
    render(
      <DdosProvider>
        <TestConsumer />
      </DdosProvider>
    );

    expect(screen.getByTestId('client')).toHaveTextContent('has-client');
  });

  it('loads stats on mount', async () => {
    render(
      <DdosProvider>
        <TestConsumer />
      </DdosProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('stats')).toHaveTextContent('50000');
    });
  });

  it('loads blocklist on mount', async () => {
    render(
      <DdosProvider>
        <TestConsumer />
      </DdosProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('blocklist-count')).toHaveTextContent('1');
    });
  });

  it('loads allowlist on mount', async () => {
    render(
      <DdosProvider>
        <TestConsumer />
      </DdosProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('allowlist-count')).toHaveTextContent('1');
    });
  });

  it('loads policies on mount', async () => {
    render(
      <DdosProvider>
        <TestConsumer />
      </DdosProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('policies-count')).toHaveTextContent('1');
    });
  });

  it('loads attacks on mount', async () => {
    render(
      <DdosProvider>
        <TestConsumer />
      </DdosProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('attacks-count')).toHaveTextContent('1');
    });
  });

  it('starts with loading state true', () => {
    render(
      <DdosProvider>
        <TestConsumer />
      </DdosProvider>
    );

    expect(screen.getByTestId('loading')).toHaveTextContent('loading');
  });

  it('throws error when used outside provider', () => {
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});

    expect(() => render(<TestConsumer />)).toThrow(
      'useDdos must be used within a DdosProvider'
    );

    consoleError.mockRestore();
  });
});
