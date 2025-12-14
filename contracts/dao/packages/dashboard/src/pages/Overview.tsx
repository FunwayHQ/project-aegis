import { useDns } from '../contexts/DnsContext';
import { useDdos } from '../contexts/DdosContext';
import { Link } from 'react-router-dom';

export default function Overview() {
  const { zones, stats: dnsStats } = useDns();
  const { stats: ddosStats, policies } = useDdos();

  return (
    <div>
      {/* Header */}
      <div className="mb-8">
        <h1 className="text-3xl font-bold text-gray-900">Dashboard Overview</h1>
        <p className="text-gray-500 mt-2">Monitor your AEGIS edge network at a glance</p>
      </div>

      {/* Quick Stats */}
      <div className="grid gap-6 md:grid-cols-2 lg:grid-cols-4 mb-8">
        <StatCard
          title="DNS Zones"
          value={zones.length.toString()}
          subtitle="Active zones"
          icon={GlobeIcon}
          color="teal"
          link="/dns/zones"
        />
        <StatCard
          title="DNS Queries Today"
          value={(dnsStats?.queries_today ?? 0).toLocaleString()}
          subtitle="Total queries"
          icon={QueryIcon}
          color="blue"
          link="/dns/analytics"
        />
        <StatCard
          title="Threats Blocked"
          value={(ddosStats?.total_blocked ?? 0).toLocaleString()}
          subtitle="All time"
          icon={ShieldIcon}
          color="red"
          link="/ddos/dashboard"
        />
        <StatCard
          title="Active Policies"
          value={policies.length.toString()}
          subtitle="Protection rules"
          icon={PolicyIcon}
          color="green"
          link="/ddos/policies"
        />
      </div>

      {/* Two Column Layout */}
      <div className="grid gap-6 lg:grid-cols-2">
        {/* DNS Summary */}
        <div className="card p-6">
          <div className="flex justify-between items-center mb-4">
            <h2 className="text-xl font-semibold text-gray-900">DNS Summary</h2>
            <Link to="/dns/zones" className="text-teal-600 hover:text-teal-700 text-sm">
              View All →
            </Link>
          </div>

          <div className="space-y-4">
            <div className="flex justify-between items-center py-3 border-b border-gray-200">
              <span className="text-gray-600">Cache Hit Rate</span>
              <span className="text-gray-900 font-semibold">
                {dnsStats ? `${(dnsStats.cache_hit_rate * 100).toFixed(1)}%` : 'N/A'}
              </span>
            </div>
            <div className="flex justify-between items-center py-3 border-b border-gray-200">
              <span className="text-gray-600">DNSSEC Enabled Zones</span>
              <span className="text-gray-900 font-semibold">
                {zones.filter(z => z.dnssec_enabled).length} / {zones.length}
              </span>
            </div>
            <div className="flex justify-between items-center py-3 border-b border-gray-200">
              <span className="text-gray-600">Proxied Zones</span>
              <span className="text-gray-900 font-semibold">
                {zones.filter(z => z.proxied).length} / {zones.length}
              </span>
            </div>
            <div className="flex justify-between items-center py-3">
              <span className="text-gray-600">Total Queries</span>
              <span className="text-gray-900 font-semibold">
                {(dnsStats?.total_queries ?? 0).toLocaleString()}
              </span>
            </div>
          </div>
        </div>

        {/* DDoS Summary */}
        <div className="card p-6">
          <div className="flex justify-between items-center mb-4">
            <h2 className="text-xl font-semibold text-gray-900">DDoS Protection</h2>
            <Link to="/ddos/dashboard" className="text-teal-600 hover:text-teal-700 text-sm">
              View Details →
            </Link>
          </div>

          <div className="space-y-4">
            <div className="flex justify-between items-center py-3 border-b border-gray-200">
              <span className="text-gray-600">Total Requests</span>
              <span className="text-gray-900 font-semibold">
                {(ddosStats?.total_requests ?? 0).toLocaleString()}
              </span>
            </div>
            <div className="flex justify-between items-center py-3 border-b border-gray-200">
              <span className="text-gray-600">Blocked Requests</span>
              <span className="text-red-600 font-semibold">
                {(ddosStats?.total_blocked ?? 0).toLocaleString()}
              </span>
            </div>
            <div className="flex justify-between items-center py-3 border-b border-gray-200">
              <span className="text-gray-600">Blocked IPs</span>
              <span className="text-gray-900 font-semibold">
                {(ddosStats?.blocked_ips ?? 0).toLocaleString()}
              </span>
            </div>
            <div className="flex justify-between items-center py-3">
              <span className="text-gray-600">Protection Status</span>
              <span className="badge badge-green">Active</span>
            </div>
          </div>
        </div>
      </div>

      {/* Recent Activity */}
      <div className="card p-6 mt-6">
        <h2 className="text-xl font-semibold text-gray-900 mb-4">System Status</h2>
        <div className="grid gap-4 md:grid-cols-3">
          <StatusItem name="DNS Server" status="healthy" />
          <StatusItem name="DDoS Protection" status="healthy" />
          <StatusItem name="Edge Network" status="healthy" />
        </div>
      </div>
    </div>
  );
}

interface StatCardProps {
  title: string;
  value: string;
  subtitle: string;
  icon: React.FC<{ className?: string }>;
  color: 'teal' | 'blue' | 'red' | 'green';
  link: string;
}

function StatCard({ title, value, subtitle, icon: Icon, color, link }: StatCardProps) {
  const colorClasses = {
    teal: 'bg-teal-100 text-teal-600',
    blue: 'bg-blue-100 text-blue-600',
    red: 'bg-red-100 text-red-600',
    green: 'bg-green-100 text-green-600',
  };

  return (
    <Link to={link} className="card card-hover p-6 stat-card">
      <div className="flex items-center gap-4">
        <div className={`w-12 h-12 rounded-lg flex items-center justify-center ${colorClasses[color]}`}>
          <Icon className="w-6 h-6" />
        </div>
        <div>
          <p className="text-gray-500 text-sm">{title}</p>
          <p className="text-2xl font-bold text-gray-900">{value}</p>
          <p className="text-xs text-gray-400">{subtitle}</p>
        </div>
      </div>
    </Link>
  );
}

interface StatusItemProps {
  name: string;
  status: 'healthy' | 'degraded' | 'down';
}

function StatusItem({ name, status }: StatusItemProps) {
  const statusColors = {
    healthy: 'bg-green-500',
    degraded: 'bg-yellow-500',
    down: 'bg-red-500',
  };

  return (
    <div className="flex items-center gap-3 p-4 bg-gray-100 rounded-lg">
      <div className={`w-3 h-3 rounded-full ${statusColors[status]}`} />
      <div>
        <p className="text-gray-900 font-medium">{name}</p>
        <p className="text-xs text-gray-500 capitalize">{status}</p>
      </div>
    </div>
  );
}

// Icons
function GlobeIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3.055 11H5a2 2 0 012 2v1a2 2 0 002 2 2 2 0 012 2v2.945M8 3.935V5.5A2.5 2.5 0 0010.5 8h.5a2 2 0 012 2 2 2 0 104 0 2 2 0 012-2h1.064M15 20.488V18a2 2 0 012-2h3.064M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
    </svg>
  );
}

function QueryIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
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

function PolicyIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
    </svg>
  );
}
