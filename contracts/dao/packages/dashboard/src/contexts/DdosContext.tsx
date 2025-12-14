import { createContext, useContext, useState, useEffect, ReactNode, useCallback } from 'react';
import { DDoSClient, GlobalStats, BlocklistEntry, AllowlistEntry, DDoSPolicy, AttackEvent, SseEvent } from '@aegis/ddos-sdk';

interface DdosContextType {
  client: DDoSClient;
  stats: GlobalStats | null;
  blocklist: BlocklistEntry[];
  allowlist: AllowlistEntry[];
  policies: DDoSPolicy[];
  attacks: AttackEvent[];
  recentEvents: SseEvent[];
  loading: boolean;
  error: string | null;
  refreshStats: () => Promise<void>;
  refreshBlocklist: () => Promise<void>;
  refreshAllowlist: () => Promise<void>;
  refreshPolicies: () => Promise<void>;
  refreshAttacks: () => Promise<void>;
}

const DdosContext = createContext<DdosContextType | undefined>(undefined);

const DDOS_API_URL = import.meta.env.VITE_DDOS_API_URL || 'http://localhost:8080';

export function DdosProvider({ children }: { children: ReactNode }) {
  const [client] = useState(() => new DDoSClient({ baseUrl: DDOS_API_URL }));
  const [stats, setStats] = useState<GlobalStats | null>(null);
  const [blocklist, setBlocklist] = useState<BlocklistEntry[]>([]);
  const [allowlist, setAllowlist] = useState<AllowlistEntry[]>([]);
  const [policies, setPolicies] = useState<DDoSPolicy[]>([]);
  const [attacks, setAttacks] = useState<AttackEvent[]>([]);
  const [recentEvents, setRecentEvents] = useState<SseEvent[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refreshStats = useCallback(async () => {
    try {
      const data = await client.getGlobalStats();
      setStats(data);
    } catch (err) {
      console.error('Failed to load DDoS stats:', err);
    }
  }, [client]);

  const refreshBlocklist = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const response = await client.getBlocklist();
      setBlocklist(response.items);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load blocklist');
    } finally {
      setLoading(false);
    }
  }, [client]);

  const refreshAllowlist = useCallback(async () => {
    try {
      const data = await client.getAllowlist();
      setAllowlist(data);
    } catch (err) {
      console.error('Failed to load allowlist:', err);
    }
  }, [client]);

  const refreshPolicies = useCallback(async () => {
    try {
      const data = await client.listPolicies();
      setPolicies(data);
    } catch (err) {
      console.error('Failed to load policies:', err);
    }
  }, [client]);

  const refreshAttacks = useCallback(async () => {
    try {
      const data = await client.getAttacks();
      setAttacks(data);
    } catch (err) {
      console.error('Failed to load attacks:', err);
    }
  }, [client]);

  useEffect(() => {
    refreshStats();
    refreshBlocklist();
    refreshAllowlist();
    refreshPolicies();
    refreshAttacks();

    const statsInterval = setInterval(refreshStats, 10000);
    const attacksInterval = setInterval(refreshAttacks, 5000);
    return () => {
      clearInterval(statsInterval);
      clearInterval(attacksInterval);
    };
  }, [refreshStats, refreshBlocklist, refreshAllowlist, refreshPolicies, refreshAttacks]);

  // Subscribe to SSE events for real-time updates
  useEffect(() => {
    const cleanup = client.subscribeToEvents({
      onEvent: (event: SseEvent) => {
        setRecentEvents((prev) => [event, ...prev].slice(0, 50));
        // Refresh relevant data based on event type
        if (event.type === 'attack_detected' || event.type === 'attack_mitigated') {
          refreshAttacks();
        }
        if (event.type === 'ip_blocked' || event.type === 'ip_unblocked') {
          refreshBlocklist();
        }
        if (event.type === 'stats_update') {
          refreshStats();
        }
      },
      onError: (error: Error) => {
        console.error('SSE error:', error);
      },
    });

    return cleanup;
  }, [client, refreshAttacks, refreshBlocklist, refreshStats]);

  return (
    <DdosContext.Provider
      value={{
        client,
        stats,
        blocklist,
        allowlist,
        policies,
        attacks,
        recentEvents,
        loading,
        error,
        refreshStats,
        refreshBlocklist,
        refreshAllowlist,
        refreshPolicies,
        refreshAttacks,
      }}
    >
      {children}
    </DdosContext.Provider>
  );
}

export function useDdos() {
  const context = useContext(DdosContext);
  if (context === undefined) {
    throw new Error('useDdos must be used within a DdosProvider');
  }
  return context;
}
