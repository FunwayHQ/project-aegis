import { useMemo } from 'react';
import { useDdos } from '../../contexts/DdosContext';
import StatCard from '../../components/ddos/StatCard';
import AttackChart from '../../components/ddos/AttackChart';
import EventLog from '../../components/ddos/EventLog';

export default function Dashboard() {
  const { loading, stats, attacks, recentEvents } = useDdos();

  // Calculate severity badge for active attacks
  const attackSeverity = useMemo(() => {
    if (!stats || stats.active_attacks === 0) return 'low';
    if (stats.active_attacks >= 5) return 'critical';
    if (stats.active_attacks >= 3) return 'high';
    if (stats.active_attacks >= 1) return 'medium';
    return 'low';
  }, [stats]);

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin rounded-full h-12 w-12 border-t-2 border-b-2 border-teal-500"></div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Page title */}
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold text-gray-900">DDoS Protection Dashboard</h1>
        <div className="flex items-center gap-2">
          <span
            className={`attack-badge attack-badge-${attackSeverity}`}
          >
            {stats?.active_attacks || 0} Active Attacks
          </span>
        </div>
      </div>

      {/* Stats grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
        <StatCard
          title="Total Requests"
          value={stats?.total_requests || 0}
          subtitle="All time"
          icon={
            <svg className="w-6 h-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z" />
            </svg>
          }
        />
        <StatCard
          title="Blocked"
          value={stats?.total_blocked || 0}
          subtitle={`${stats?.drop_rate.toFixed(1) || 0}% drop rate`}
          icon={
            <svg className="w-6 h-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M18.364 18.364A9 9 0 005.636 5.636m12.728 12.728A9 9 0 015.636 5.636m12.728 12.728L5.636 5.636" />
            </svg>
          }
          color="red"
        />
        <StatCard
          title="Rate Limited"
          value={stats?.total_rate_limited || 0}
          subtitle="Throttled requests"
          icon={
            <svg className="w-6 h-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
          }
          color="yellow"
        />
        <StatCard
          title="Blocked IPs"
          value={stats?.blocked_ips || 0}
          subtitle={`${stats?.allowed_ips || 0} allowlisted`}
          icon={
            <svg className="w-6 h-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 12a9 9 0 01-9 9m9-9a9 9 0 00-9-9m9 9H3m9 9a9 9 0 01-9-9m9 9c1.657 0 3-4.03 3-9s-1.343-9-3-9m0 18c-1.657 0-3-4.03-3-9s1.343-9 3-9m-9 9a9 9 0 019-9" />
            </svg>
          }
          color="teal"
        />
      </div>

      {/* Attack chart and event log */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Attack chart */}
        <div className="card p-6">
          <h2 className="text-lg font-semibold text-gray-900 mb-4">Attack Activity</h2>
          <AttackChart attacks={attacks || []} />
        </div>

        {/* Event log */}
        <div className="card p-6">
          <h2 className="text-lg font-semibold text-gray-900 mb-4">Live Events</h2>
          <EventLog events={recentEvents || []} />
        </div>
      </div>

      {/* Recent attacks table */}
      <div className="card p-6">
        <h2 className="text-lg font-semibold text-gray-900 mb-4">Recent Attacks</h2>
        <div className="overflow-x-auto">
          <table className="w-full">
            <thead>
              <tr className="table-header">
                <th className="text-left p-3">Type</th>
                <th className="text-left p-3">Source IP</th>
                <th className="text-left p-3">Target</th>
                <th className="text-left p-3">PPS</th>
                <th className="text-left p-3">Severity</th>
                <th className="text-left p-3">Status</th>
                <th className="text-left p-3">Time</th>
              </tr>
            </thead>
            <tbody>
              {(attacks || []).slice(0, 10).map((attack) => (
                <tr key={attack.id} className="table-row">
                  <td className="p-3">
                    <span className="font-mono text-sm text-teal-600">
                      {attack.attack_type}
                    </span>
                  </td>
                  <td className="p-3 font-mono text-sm text-gray-900">{attack.source_ip}</td>
                  <td className="p-3 text-sm text-gray-900">{attack.target_domain}</td>
                  <td className="p-3 font-mono text-sm text-gray-900">
                    {attack.packets_per_second.toLocaleString()}
                  </td>
                  <td className="p-3">
                    <SeverityBadge severity={attack.severity} />
                  </td>
                  <td className="p-3">
                    <span
                      className={`inline-flex items-center px-2 py-0.5 rounded text-xs font-medium ${
                        attack.mitigated
                          ? 'bg-green-100 text-green-700'
                          : 'bg-red-100 text-red-700 animate-pulse'
                      }`}
                    >
                      {attack.mitigated ? 'Mitigated' : 'Active'}
                    </span>
                  </td>
                  <td className="p-3 text-sm text-gray-500">
                    {new Date(attack.timestamp * 1000).toLocaleTimeString()}
                  </td>
                </tr>
              ))}
              {(!attacks || attacks.length === 0) && (
                <tr>
                  <td colSpan={7} className="p-8 text-center text-gray-500">
                    No attacks detected
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}

function SeverityBadge({ severity }: { severity: number }) {
  let level: string;
  let badge: string;

  if (severity >= 80) {
    level = 'Critical';
    badge = 'attack-badge-critical';
  } else if (severity >= 60) {
    level = 'High';
    badge = 'attack-badge-high';
  } else if (severity >= 40) {
    level = 'Medium';
    badge = 'attack-badge-medium';
  } else {
    level = 'Low';
    badge = 'attack-badge-low';
  }

  return (
    <span className={`attack-badge ${badge}`}>
      {level} ({severity})
    </span>
  );
}
