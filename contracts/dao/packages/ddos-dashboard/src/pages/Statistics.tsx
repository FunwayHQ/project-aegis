import { useState, useEffect, useCallback } from 'react';
import {
  AreaChart,
  Area,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  PieChart,
  Pie,
  Cell,
} from 'recharts';
import { useDDoS } from '../contexts/DDoSContext';
import type { TopAttacker } from '@aegis/ddos-sdk';

export default function Statistics() {
  const { client, stats, attacks, isLoading } = useDDoS();
  const [topAttackers, setTopAttackers] = useState<TopAttacker[]>([]);
  const [timeRange, setTimeRange] = useState<'1h' | '24h' | '7d'>('24h');

  const fetchTopAttackers = useCallback(async () => {
    if (!client) return;
    try {
      const data = await client.getTopAttackers(10);
      setTopAttackers(data);
    } catch (err) {
      console.error('Failed to fetch top attackers:', err);
    }
  }, [client]);

  useEffect(() => {
    fetchTopAttackers();
  }, [fetchTopAttackers]);

  // Calculate attack type distribution
  const attackTypeData = attacks.reduce(
    (acc, attack) => {
      const type = attack.attack_type;
      const existing = acc.find((item) => item.name === type);
      if (existing) {
        existing.value++;
      } else {
        acc.push({ name: type, value: 1 });
      }
      return acc;
    },
    [] as { name: string; value: number }[]
  );

  const COLORS = ['#EF4444', '#F59E0B', '#3B82F6', '#8B5CF6', '#10B981'];

  // Generate mock historical data for the area chart
  const historicalData = Array.from({ length: 24 }, (_, i) => ({
    time: `${23 - i}:00`,
    requests: Math.floor(Math.random() * 10000) + 5000,
    blocked: Math.floor(Math.random() * 500) + 100,
    attacks: Math.floor(Math.random() * 10),
  })).reverse();

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin rounded-full h-12 w-12 border-t-2 border-b-2 border-aegis-500"></div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-white">Statistics</h1>
          <p className="text-gray-400 mt-1">
            Historical attack data and analytics
          </p>
        </div>
        <div className="flex gap-2">
          {(['1h', '24h', '7d'] as const).map((range) => (
            <button
              key={range}
              onClick={() => setTimeRange(range)}
              className={`px-4 py-2 rounded-lg font-medium transition-colors ${
                timeRange === range
                  ? 'bg-aegis-600 text-white'
                  : 'bg-gray-700 text-gray-400 hover:text-white'
              }`}
            >
              {range}
            </button>
          ))}
        </div>
      </div>

      {/* Summary cards */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
        <div className="stat-card">
          <p className="text-sm text-gray-400">Total Attacks</p>
          <p className="text-3xl font-bold text-white mt-1">
            {stats?.total_attacks.toLocaleString() || 0}
          </p>
          <p className="text-xs text-gray-500 mt-2">All time</p>
        </div>
        <div className="stat-card">
          <p className="text-sm text-gray-400">Total Blocked</p>
          <p className="text-3xl font-bold text-red-400 mt-1">
            {stats?.total_blocked.toLocaleString() || 0}
          </p>
          <p className="text-xs text-gray-500 mt-2">Packets/requests</p>
        </div>
        <div className="stat-card">
          <p className="text-sm text-gray-400">Drop Rate</p>
          <p className="text-3xl font-bold text-yellow-400 mt-1">
            {stats?.drop_rate.toFixed(1) || 0}%
          </p>
          <p className="text-xs text-gray-500 mt-2">Of total traffic</p>
        </div>
        <div className="stat-card">
          <p className="text-sm text-gray-400">Uptime</p>
          <p className="text-3xl font-bold text-green-400 mt-1">
            {stats ? formatUptime(stats.uptime_secs) : '0d 0h'}
          </p>
          <p className="text-xs text-gray-500 mt-2">Since last restart</p>
        </div>
      </div>

      {/* Traffic chart */}
      <div className="stat-card">
        <h2 className="text-lg font-semibold text-white mb-4">Traffic Overview</h2>
        <div className="h-80">
          <ResponsiveContainer width="100%" height="100%">
            <AreaChart data={historicalData}>
              <CartesianGrid strokeDasharray="3 3" stroke="#374151" />
              <XAxis dataKey="time" stroke="#9CA3AF" fontSize={12} />
              <YAxis stroke="#9CA3AF" fontSize={12} />
              <Tooltip
                contentStyle={{
                  backgroundColor: '#1F2937',
                  border: '1px solid #374151',
                  borderRadius: '8px',
                }}
              />
              <Area
                type="monotone"
                dataKey="requests"
                name="Requests"
                stroke="#0EA5E9"
                fill="#0EA5E9"
                fillOpacity={0.3}
              />
              <Area
                type="monotone"
                dataKey="blocked"
                name="Blocked"
                stroke="#EF4444"
                fill="#EF4444"
                fillOpacity={0.3}
              />
            </AreaChart>
          </ResponsiveContainer>
        </div>
      </div>

      {/* Attack breakdown and top attackers */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Attack type breakdown */}
        <div className="stat-card">
          <h2 className="text-lg font-semibold text-white mb-4">
            Attack Type Breakdown
          </h2>
          {attackTypeData.length > 0 ? (
            <div className="h-64 flex items-center justify-center">
              <ResponsiveContainer width="100%" height="100%">
                <PieChart>
                  <Pie
                    data={attackTypeData}
                    cx="50%"
                    cy="50%"
                    labelLine={false}
                    label={({ name, percent }) =>
                      `${name} (${(percent * 100).toFixed(0)}%)`
                    }
                    outerRadius={80}
                    fill="#8884d8"
                    dataKey="value"
                  >
                    {attackTypeData.map((_, index) => (
                      <Cell
                        key={`cell-${index}`}
                        fill={COLORS[index % COLORS.length]}
                      />
                    ))}
                  </Pie>
                  <Tooltip
                    contentStyle={{
                      backgroundColor: '#1F2937',
                      border: '1px solid #374151',
                      borderRadius: '8px',
                    }}
                  />
                </PieChart>
              </ResponsiveContainer>
            </div>
          ) : (
            <div className="h-64 flex items-center justify-center text-gray-500">
              No attack data available
            </div>
          )}
        </div>

        {/* Top attackers */}
        <div className="stat-card">
          <h2 className="text-lg font-semibold text-white mb-4">Top Attackers</h2>
          <div className="space-y-3">
            {topAttackers.length > 0 ? (
              topAttackers.map((attacker, index) => (
                <div
                  key={attacker.ip}
                  className="flex items-center justify-between p-3 bg-gray-700/50 rounded-lg"
                >
                  <div className="flex items-center gap-3">
                    <span className="w-6 h-6 flex items-center justify-center bg-gray-600 rounded-full text-sm font-medium">
                      {index + 1}
                    </span>
                    <div>
                      <p className="font-mono text-sm text-white">
                        {attacker.ip}
                      </p>
                      <p className="text-xs text-gray-400">
                        {attacker.attack_count} attacks
                      </p>
                    </div>
                  </div>
                  <div className="text-right">
                    <p className="text-sm font-medium text-red-400">
                      {attacker.total_packets.toLocaleString()} pkts
                    </p>
                    <p className="text-xs text-gray-400">
                      Last:{' '}
                      {new Date(attacker.last_attack * 1000).toLocaleDateString()}
                    </p>
                  </div>
                </div>
              ))
            ) : (
              <div className="h-64 flex items-center justify-center text-gray-500">
                No attacker data available
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Export options */}
      <div className="stat-card">
        <h2 className="text-lg font-semibold text-white mb-4">Export Data</h2>
        <div className="flex gap-4">
          <button
            onClick={() => exportData('csv')}
            className="btn-secondary flex items-center gap-2"
          >
            <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 10v6m0 0l-3-3m3 3l3-3m2 8H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
            </svg>
            Export CSV
          </button>
          <button
            onClick={() => exportData('json')}
            className="btn-secondary flex items-center gap-2"
          >
            <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 10v6m0 0l-3-3m3 3l3-3m2 8H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
            </svg>
            Export JSON
          </button>
        </div>
      </div>
    </div>
  );
}

function formatUptime(seconds: number): string {
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  return `${days}d ${hours}h`;
}

function exportData(format: 'csv' | 'json') {
  // Placeholder - would trigger actual export
  alert(`Exporting data as ${format.toUpperCase()}...`);
}
