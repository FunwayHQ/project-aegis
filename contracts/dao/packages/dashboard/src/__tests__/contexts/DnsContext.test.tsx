import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { DnsProvider, useDns } from '../../contexts/DnsContext';

// Mock the DNS SDK
vi.mock('@aegis/dns-sdk', () => ({
  DnsClient: vi.fn().mockImplementation(() => ({
    listZones: vi.fn().mockResolvedValue([
      { domain: 'example.com', proxied: true, dnssec_enabled: false, nameservers: ['ns1.aegis.network'], created_at: 1700000000, updated_at: 1700000000 },
    ]),
    getStats: vi.fn().mockResolvedValue({
      total_queries: 10000,
      queries_today: 500,
      cache_hit_rate: 0.85,
      dnssec_queries: 200,
      rate_limited_queries: 10,
      top_queried_domains: [],
      query_types: { A: 300, AAAA: 200 },
    }),
    getZone: vi.fn().mockResolvedValue({
      domain: 'example.com',
      proxied: true,
      dnssec_enabled: false,
      nameservers: ['ns1.aegis.network'],
      created_at: 1700000000,
      updated_at: 1700000000,
    }),
  })),
}));

function TestConsumer() {
  const { zones, stats, loading, error, client } = useDns();

  return (
    <div>
      <div data-testid="loading">{loading ? 'loading' : 'not-loading'}</div>
      <div data-testid="error">{error || 'no-error'}</div>
      <div data-testid="zones-count">{zones.length}</div>
      <div data-testid="stats">{stats ? 'has-stats' : 'no-stats'}</div>
      <div data-testid="client">{client ? 'has-client' : 'no-client'}</div>
    </div>
  );
}

describe('DnsContext', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('provides client to consumers', async () => {
    render(
      <DnsProvider>
        <TestConsumer />
      </DnsProvider>
    );

    expect(screen.getByTestId('client')).toHaveTextContent('has-client');
  });

  it('loads zones on mount', async () => {
    render(
      <DnsProvider>
        <TestConsumer />
      </DnsProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('zones-count')).toHaveTextContent('1');
    });
  });

  it('loads stats on mount', async () => {
    render(
      <DnsProvider>
        <TestConsumer />
      </DnsProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('stats')).toHaveTextContent('has-stats');
    });
  });

  it('starts with loading state true', () => {
    render(
      <DnsProvider>
        <TestConsumer />
      </DnsProvider>
    );

    expect(screen.getByTestId('loading')).toHaveTextContent('loading');
  });

  it('sets loading to false after zones load', async () => {
    render(
      <DnsProvider>
        <TestConsumer />
      </DnsProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('loading')).toHaveTextContent('not-loading');
    });
  });

  it('throws error when used outside provider', () => {
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});

    expect(() => render(<TestConsumer />)).toThrow(
      'useDns must be used within a DnsProvider'
    );

    consoleError.mockRestore();
  });
});
