import { createContext, useContext, useState, useEffect, ReactNode } from 'react';
import { DnsClient, Zone, DnsStats } from '@aegis/dns-sdk';

interface DnsContextType {
  client: DnsClient;
  zones: Zone[];
  stats: DnsStats | null;
  loading: boolean;
  error: string | null;
  refreshZones: () => Promise<void>;
  refreshStats: () => Promise<void>;
}

const DnsContext = createContext<DnsContextType | undefined>(undefined);

export function DnsProvider({ children }: { children: ReactNode }) {
  const [client] = useState(() => new DnsClient({ baseUrl: 'http://localhost:8054' }));
  const [zones, setZones] = useState<Zone[]>([]);
  const [stats, setStats] = useState<DnsStats | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refreshZones = async () => {
    try {
      setLoading(true);
      setError(null);
      const data = await client.listZones();
      setZones(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load zones');
    } finally {
      setLoading(false);
    }
  };

  const refreshStats = async () => {
    try {
      const data = await client.getStats();
      setStats(data);
    } catch (err) {
      console.error('Failed to load stats:', err);
    }
  };

  useEffect(() => {
    refreshZones();
    refreshStats();

    // Refresh stats every 30 seconds
    const interval = setInterval(refreshStats, 30000);
    return () => clearInterval(interval);
  }, []);

  return (
    <DnsContext.Provider value={{ client, zones, stats, loading, error, refreshZones, refreshStats }}>
      {children}
    </DnsContext.Provider>
  );
}

export function useDns() {
  const context = useContext(DnsContext);
  if (context === undefined) {
    throw new Error('useDns must be used within a DnsProvider');
  }
  return context;
}
