import { useState, useEffect } from 'react';
import { useDns } from '../../contexts/DnsContext';
import { ZoneStats } from '@aegis/dns-sdk';

export default function Analytics() {
  const { client, stats, zones } = useDns();
  const [selectedZone, setSelectedZone] = useState<string>('');
  const [zoneStats, setZoneStats] = useState<ZoneStats | null>(null);

  const loadZoneStats = async (domain: string) => {
    if (!domain) {
      setZoneStats(null);
      return;
    }

    try {
      const data = await client.getZoneStats(domain);
      setZoneStats(data);
    } catch (err) {
      console.error('Failed to load zone stats:', err);
      setZoneStats(null);
    }
  };

  useEffect(() => {
    if (selectedZone) {
      loadZoneStats(selectedZone);
    }
  }, [selectedZone]);

  // Determine what to display
  const showGlobalStats = !selectedZone;
  const showZoneStats = !!selectedZone && !!zoneStats;

  // Get total queries
  const totalQueries = showGlobalStats
    ? (stats?.total_queries ?? 0)
    : (zoneStats?.total_queries ?? 0);

  const queriesToday = showGlobalStats
    ? (stats?.queries_today ?? 0)
    : (zoneStats?.queries_today ?? 0);

  const queryTypes = showGlobalStats
    ? stats?.query_types
    : zoneStats?.query_types;

  return (
    <div>
      {/* Header */}
      <div className="flex justify-between items-center mb-6">
        <div>
          <h1 className="text-2xl font-bold text-gray-900">DNS Analytics</h1>
          <p className="text-gray-500 mt-1">Monitor DNS query traffic and performance</p>
        </div>
        <select
          value={selectedZone}
          onChange={(e) => setSelectedZone(e.target.value)}
          className="select max-w-xs"
        >
          <option value="">All Zones</option>
          {zones.map(zone => (
            <option key={zone.domain} value={zone.domain}>{zone.domain}</option>
          ))}
        </select>
      </div>

      {/* Stats Overview */}
      <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-4 mb-6">
        <StatCard
          title="Total Queries"
          value={totalQueries.toLocaleString()}
          icon={QueryIcon}
          color="teal"
        />
        <StatCard
          title="Queries Today"
          value={queriesToday.toLocaleString()}
          icon={TodayIcon}
          color="blue"
        />
        <StatCard
          title="Cache Hit Rate"
          value={showGlobalStats && stats ? `${(stats.cache_hit_rate * 100).toFixed(1)}%` : 'N/A'}
          icon={CacheIcon}
          color="green"
        />
        <StatCard
          title="DNSSEC Queries"
          value={showGlobalStats && stats ? stats.dnssec_queries.toLocaleString() : 'N/A'}
          icon={ShieldIcon}
          color="yellow"
        />
      </div>

      {/* Query Types Distribution */}
      <div className="grid gap-6 lg:grid-cols-2 mb-6">
        <div className="bg-white rounded-lg p-6 border border-gray-200 shadow-sm">
          <h2 className="text-lg font-semibold text-gray-900 mb-4">Query Types</h2>
          <div className="space-y-3">
            {queryTypes && Object.entries(queryTypes).map(([type, count]) => (
              <QueryTypeBar
                key={type}
                type={type}
                count={count}
                total={totalQueries || 1}
              />
            ))}
            {!queryTypes && (
              <p className="text-gray-500 text-center py-4">No query data available</p>
            )}
          </div>
        </div>

        {/* Top Queried Items */}
        <div className="bg-white rounded-lg p-6 border border-gray-200 shadow-sm">
          <h2 className="text-lg font-semibold text-gray-900 mb-4">
            {showZoneStats ? 'Top Queried Records' : 'Top Queried Domains'}
          </h2>
          <div className="space-y-2">
            {showGlobalStats && stats?.top_queried_domains?.map((item: { domain: string; count: number }, index: number) => (
              <div key={item.domain} className="flex items-center justify-between py-2">
                <div className="flex items-center gap-3">
                  <span className="w-6 h-6 rounded-full bg-teal-100 text-teal-600 flex items-center justify-center text-sm">
                    {index + 1}
                  </span>
                  <span className="text-gray-900 font-mono text-sm">{item.domain}</span>
                </div>
                <span className="text-gray-500">{item.count.toLocaleString()} queries</span>
              </div>
            ))}
            {showZoneStats && zoneStats?.top_records?.map((item, index: number) => (
              <div key={`${item.name}-${item.type}`} className="flex items-center justify-between py-2">
                <div className="flex items-center gap-3">
                  <span className="w-6 h-6 rounded-full bg-teal-100 text-teal-600 flex items-center justify-center text-sm">
                    {index + 1}
                  </span>
                  <span className="text-gray-900 font-mono text-sm">
                    {item.name} <span className="text-gray-500">({item.type})</span>
                  </span>
                </div>
                <span className="text-gray-500">{item.count.toLocaleString()} queries</span>
              </div>
            ))}
            {!stats?.top_queried_domains && !zoneStats?.top_records && (
              <p className="text-gray-500 text-center py-4">No data available</p>
            )}
          </div>
        </div>
      </div>

      {/* Geographic Distribution (Zone Stats Only) */}
      {showZoneStats && zoneStats?.geo_distribution && Object.keys(zoneStats.geo_distribution).length > 0 && (
        <div className="bg-white rounded-lg p-6 border border-gray-200 shadow-sm">
          <h2 className="text-lg font-semibold text-gray-900 mb-4">Query Sources by Region</h2>
          <div className="grid gap-4 md:grid-cols-3 lg:grid-cols-5">
            {Object.entries(zoneStats.geo_distribution).map(([region, count]) => (
              <div key={region} className="bg-gray-100 rounded-lg p-4 text-center">
                <p className="text-gray-500 text-sm">{region}</p>
                <p className="text-2xl font-bold text-gray-900 mt-1">{(count as number).toLocaleString()}</p>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Rate Limited & Security Stats (Global Only) */}
      {showGlobalStats && stats && (
        <div className="bg-white rounded-lg p-6 mt-6 border border-gray-200 shadow-sm">
          <h2 className="text-lg font-semibold text-gray-900 mb-4">Security Metrics</h2>
          <div className="grid gap-4 md:grid-cols-2">
            <div className="bg-gray-100 rounded-lg p-4">
              <p className="text-gray-500 text-sm">Rate Limited Queries</p>
              <p className="text-2xl font-bold text-red-600 mt-1">
                {stats.rate_limited_queries.toLocaleString()}
              </p>
            </div>
            <div className="bg-gray-100 rounded-lg p-4">
              <p className="text-gray-500 text-sm">DNSSEC Verified Responses</p>
              <p className="text-2xl font-bold text-green-600 mt-1">
                {stats.dnssec_queries.toLocaleString()}
              </p>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

interface StatCardProps {
  title: string;
  value: string;
  icon: React.FC<{ className?: string }>;
  color: 'teal' | 'blue' | 'green' | 'yellow';
}

function StatCard({ title, value, icon: Icon, color }: StatCardProps) {
  const colorClasses = {
    teal: 'bg-teal-100 text-teal-600',
    blue: 'bg-blue-100 text-blue-600',
    green: 'bg-green-100 text-green-600',
    yellow: 'bg-yellow-100 text-yellow-600',
  };

  return (
    <div className="bg-white rounded-lg p-6 border border-gray-200 shadow-sm">
      <div className="flex items-center gap-4">
        <div className={`w-12 h-12 rounded-lg flex items-center justify-center ${colorClasses[color]}`}>
          <Icon className="w-6 h-6" />
        </div>
        <div>
          <p className="text-gray-500 text-sm">{title}</p>
          <p className="text-2xl font-bold text-gray-900">{value}</p>
        </div>
      </div>
    </div>
  );
}

interface QueryTypeBarProps {
  type: string;
  count: number;
  total: number;
}

function QueryTypeBar({ type, count, total }: QueryTypeBarProps) {
  const percentage = total > 0 ? (count / total) * 100 : 0;

  return (
    <div>
      <div className="flex justify-between text-sm mb-1">
        <span className="text-gray-900 font-mono">{type}</span>
        <span className="text-gray-500">{count.toLocaleString()} ({percentage.toFixed(1)}%)</span>
      </div>
      <div className="h-2 bg-gray-200 rounded-full overflow-hidden">
        <div
          className="h-full bg-teal-500 rounded-full transition-all duration-300"
          style={{ width: `${percentage}%` }}
        />
      </div>
    </div>
  );
}

// Simple icons
function QueryIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
    </svg>
  );
}

function TodayIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
    </svg>
  );
}

function CacheIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7M4 7c0 2.21 3.582 4 8 4s8-1.79 8-4M4 7c0-2.21 3.582-4 8-4s8 1.79 8 4" />
    </svg>
  );
}

function ShieldIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" />
    </svg>
  );
}
