import React, { createContext, useContext, useEffect, useState, useCallback, useRef } from 'react';
import {
  DDoSClient,
  type GlobalStats,
  type DDoSPolicy,
  type BlocklistEntry,
  type AllowlistEntry,
  type AttackEvent,
  type SseEvent,
} from '@aegis/ddos-sdk';

// =============================================================================
// TYPES
// =============================================================================

interface DDoSContextType {
  // Client
  client: DDoSClient | null;

  // Connection state
  isConnected: boolean;
  isLoading: boolean;
  error: string | null;

  // Data
  stats: GlobalStats | null;
  policies: DDoSPolicy[];
  blocklist: BlocklistEntry[];
  allowlist: AllowlistEntry[];
  attacks: AttackEvent[];

  // Actions
  refreshStats: () => Promise<void>;
  refreshPolicies: () => Promise<void>;
  refreshBlocklist: () => Promise<void>;
  refreshAllowlist: () => Promise<void>;
  refreshAttacks: () => Promise<void>;

  // SSE events
  recentEvents: SseEvent[];
}

// =============================================================================
// CONTEXT
// =============================================================================

const DDoSContext = createContext<DDoSContextType | null>(null);

// =============================================================================
// HOOK
// =============================================================================

export function useDDoS() {
  const context = useContext(DDoSContext);
  if (!context) {
    throw new Error('useDDoS must be used within a DDoSProvider');
  }
  return context;
}

// =============================================================================
// PROVIDER
// =============================================================================

interface DDoSProviderProps {
  children: React.ReactNode;
  apiUrl?: string;
}

export function DDoSProvider({ children, apiUrl = '' }: DDoSProviderProps) {
  // Client state
  const [client] = useState(() => new DDoSClient({ baseUrl: apiUrl }));
  const [isConnected, setIsConnected] = useState(false);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Data state
  const [stats, setStats] = useState<GlobalStats | null>(null);
  const [policies, setPolicies] = useState<DDoSPolicy[]>([]);
  const [blocklist, setBlocklist] = useState<BlocklistEntry[]>([]);
  const [allowlist, setAllowlist] = useState<AllowlistEntry[]>([]);
  const [attacks, setAttacks] = useState<AttackEvent[]>([]);
  const [recentEvents, setRecentEvents] = useState<SseEvent[]>([]);

  // SSE cleanup ref
  const sseCleanupRef = useRef<(() => void) | null>(null);

  // Refresh functions
  const refreshStats = useCallback(async () => {
    try {
      const data = await client.getGlobalStats();
      setStats(data);
      setIsConnected(true);
      setError(null);
    } catch (err) {
      console.error('Failed to fetch stats:', err);
      setError(err instanceof Error ? err.message : 'Failed to fetch stats');
      setIsConnected(false);
    }
  }, [client]);

  const refreshPolicies = useCallback(async () => {
    try {
      const data = await client.listPolicies();
      setPolicies(data);
    } catch (err) {
      console.error('Failed to fetch policies:', err);
    }
  }, [client]);

  const refreshBlocklist = useCallback(async () => {
    try {
      const data = await client.getBlocklist();
      setBlocklist(data.items);
    } catch (err) {
      console.error('Failed to fetch blocklist:', err);
    }
  }, [client]);

  const refreshAllowlist = useCallback(async () => {
    try {
      const data = await client.getAllowlist();
      setAllowlist(data);
    } catch (err) {
      console.error('Failed to fetch allowlist:', err);
    }
  }, [client]);

  const refreshAttacks = useCallback(async () => {
    try {
      const data = await client.getAttacks(100);
      setAttacks(data);
    } catch (err) {
      console.error('Failed to fetch attacks:', err);
    }
  }, [client]);

  // Initial data load
  useEffect(() => {
    const loadInitialData = async () => {
      setIsLoading(true);
      await Promise.all([
        refreshStats(),
        refreshPolicies(),
        refreshBlocklist(),
        refreshAllowlist(),
        refreshAttacks(),
      ]);
      setIsLoading(false);
    };

    loadInitialData();
  }, [refreshStats, refreshPolicies, refreshBlocklist, refreshAllowlist, refreshAttacks]);

  // SSE subscription
  useEffect(() => {
    const cleanup = client.subscribeToEvents({
      onOpen: () => {
        setIsConnected(true);
        setError(null);
      },
      onError: (err) => {
        console.error('SSE error:', err);
        // Don't set disconnected - SSE will auto-reconnect
      },
      onEvent: (event) => {
        setRecentEvents((prev) => [event, ...prev.slice(0, 99)]);
      },
      onAttackDetected: () => {
        // Refresh attacks when new attack detected
        refreshAttacks();
      },
      onIpBlocked: () => {
        refreshBlocklist();
      },
      onIpUnblocked: () => {
        refreshBlocklist();
      },
      onStatsUpdate: (data) => {
        // Update stats from SSE for real-time updates
        setStats((prev) => (prev ? { ...prev, ...(data as Partial<GlobalStats>) } : prev));
      },
      onPolicyUpdated: () => {
        refreshPolicies();
      },
    });

    sseCleanupRef.current = cleanup;

    return () => {
      cleanup();
    };
  }, [client, refreshAttacks, refreshBlocklist, refreshPolicies]);

  // Periodic refresh (every 10 seconds)
  useEffect(() => {
    const interval = setInterval(() => {
      refreshStats();
    }, 10000);

    return () => clearInterval(interval);
  }, [refreshStats]);

  // Context value
  const value: DDoSContextType = {
    client,
    isConnected,
    isLoading,
    error,
    stats,
    policies,
    blocklist,
    allowlist,
    attacks,
    refreshStats,
    refreshPolicies,
    refreshBlocklist,
    refreshAllowlist,
    refreshAttacks,
    recentEvents,
  };

  return <DDoSContext.Provider value={value}>{children}</DDoSContext.Provider>;
}
