import { useAuth } from '../../contexts/AuthContext';

export default function Billing() {
  const { usage, refreshUsage } = useAuth();

  const formatBytes = (bytes: number): string => {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
  };

  const formatNumber = (num: number): string => {
    if (num >= 1000000) return (num / 1000000).toFixed(1) + 'M';
    if (num >= 1000) return (num / 1000).toFixed(1) + 'K';
    return num.toString();
  };

  const plans = [
    {
      name: 'Free',
      price: '$0',
      period: '/month',
      features: [
        '5 DNS zones',
        '1M queries/month',
        '100 GB bandwidth',
        'Basic DDoS protection',
        'Community support',
      ],
      current: usage?.currentPlan === 'free',
    },
    {
      name: 'Pro',
      price: '$29',
      period: '/month',
      features: [
        '50 DNS zones',
        '10M queries/month',
        '1 TB bandwidth',
        'Advanced DDoS protection',
        'Priority support',
        'Custom WAF rules',
        'API access',
      ],
      current: usage?.currentPlan === 'pro',
      popular: true,
    },
    {
      name: 'Enterprise',
      price: 'Custom',
      period: '',
      features: [
        'Unlimited DNS zones',
        'Unlimited queries',
        'Unlimited bandwidth',
        'Enterprise DDoS protection',
        'Dedicated support',
        'SLA guarantees',
        'Custom integrations',
      ],
      current: usage?.currentPlan === 'enterprise',
    },
  ];

  return (
    <div className="max-w-6xl space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-gray-900">Billing & Usage</h1>
          <p className="text-gray-500 mt-1">
            Monitor your usage and manage your subscription
          </p>
        </div>
        <button onClick={() => refreshUsage()} className="btn-secondary">
          Refresh Usage
        </button>
      </div>

      {/* Current Plan */}
      <div className="card p-6">
        <div className="flex items-center justify-between mb-6">
          <div>
            <h2 className="text-lg font-semibold text-gray-900">Current Plan</h2>
            <p className="text-gray-500">
              You are currently on the{' '}
              <span className="font-medium text-teal-600 capitalize">
                {usage?.currentPlan || 'Free'}
              </span>{' '}
              plan
            </p>
          </div>
          <span className="px-3 py-1 bg-teal-100 text-teal-700 rounded-full text-sm font-medium capitalize">
            {usage?.currentPlan || 'Free'}
          </span>
        </div>

        {/* Usage Stats */}
        <div className="grid gap-6 md:grid-cols-3">
          <UsageCard
            title="DNS Zones"
            used={usage?.dnsZones || 0}
            limit={usage?.limits.dnsZones || 5}
            unit="zones"
          />
          <UsageCard
            title="DNS Queries"
            used={usage?.dnsQueries || 0}
            limit={usage?.limits.dnsQueriesPerMonth || 1000000}
            unit="queries"
            format={formatNumber}
          />
          <UsageCard
            title="Bandwidth"
            used={usage?.bandwidthUsed || 0}
            limit={usage?.limits.bandwidthPerMonth || 100 * 1024 * 1024 * 1024}
            unit=""
            format={formatBytes}
          />
        </div>
      </div>

      {/* Quick Stats */}
      <div className="grid gap-4 md:grid-cols-4">
        <StatCard
          title="DDoS Attacks Blocked"
          value={formatNumber(usage?.ddosRequestsBlocked || 0)}
          icon={<ShieldIcon className="w-5 h-5" />}
          color="red"
        />
        <StatCard
          title="DNS Queries Today"
          value={formatNumber(Math.floor((usage?.dnsQueries || 0) / 30))}
          icon={<GlobeIcon className="w-5 h-5" />}
          color="blue"
        />
        <StatCard
          title="Active Zones"
          value={(usage?.dnsZones || 0).toString()}
          icon={<LayersIcon className="w-5 h-5" />}
          color="green"
        />
        <StatCard
          title="Bandwidth Used"
          value={formatBytes(usage?.bandwidthUsed || 0)}
          icon={<ChartIcon className="w-5 h-5" />}
          color="purple"
        />
      </div>

      {/* Plans */}
      <div>
        <h2 className="text-lg font-semibold text-gray-900 mb-4">Available Plans</h2>
        <div className="grid gap-6 md:grid-cols-3">
          {plans.map((plan) => (
            <div
              key={plan.name}
              className={`card p-6 relative ${
                plan.popular ? 'border-2 border-teal-500' : ''
              }`}
            >
              {plan.popular && (
                <span className="absolute -top-3 left-1/2 -translate-x-1/2 px-3 py-1 bg-teal-500 text-white text-xs font-medium rounded-full">
                  Most Popular
                </span>
              )}
              <div className="text-center mb-6">
                <h3 className="text-xl font-bold text-gray-900">{plan.name}</h3>
                <div className="mt-2">
                  <span className="text-3xl font-bold text-gray-900">
                    {plan.price}
                  </span>
                  <span className="text-gray-500">{plan.period}</span>
                </div>
              </div>
              <ul className="space-y-3 mb-6">
                {plan.features.map((feature, index) => (
                  <li key={index} className="flex items-center gap-2">
                    <CheckIcon className="w-4 h-4 text-teal-500 flex-shrink-0" />
                    <span className="text-sm text-gray-600">{feature}</span>
                  </li>
                ))}
              </ul>
              <button
                className={`w-full ${
                  plan.current
                    ? 'btn-secondary cursor-default'
                    : plan.popular
                      ? 'btn-primary'
                      : 'btn-secondary hover:bg-gray-100'
                }`}
                disabled={plan.current}
              >
                {plan.current
                  ? 'Current Plan'
                  : plan.name === 'Enterprise'
                    ? 'Contact Sales'
                    : 'Upgrade'}
              </button>
            </div>
          ))}
        </div>
      </div>

      {/* Payment Method */}
      <div className="card p-6">
        <h2 className="text-lg font-semibold text-gray-900 mb-4">Payment Method</h2>
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-4">
            <div className="w-12 h-8 bg-gray-100 rounded flex items-center justify-center">
              <WalletIcon className="w-6 h-6 text-gray-400" />
            </div>
            <div>
              <p className="font-medium text-gray-900">Pay with $AEGIS Tokens</p>
              <p className="text-sm text-gray-500">
                Get 20% discount when paying with $AEGIS
              </p>
            </div>
          </div>
          <button className="btn-secondary">Add Payment Method</button>
        </div>
      </div>

      {/* Billing History */}
      <div className="card p-6">
        <h2 className="text-lg font-semibold text-gray-900 mb-4">Billing History</h2>
        <div className="text-center py-8 bg-gray-50 rounded-lg">
          <ReceiptIcon className="w-12 h-12 text-gray-300 mx-auto mb-3" />
          <p className="text-gray-500">No billing history yet</p>
          <p className="text-sm text-gray-400 mt-1">
            Your invoices will appear here after your first payment
          </p>
        </div>
      </div>
    </div>
  );
}

interface UsageCardProps {
  title: string;
  used: number;
  limit: number;
  unit: string;
  format?: (value: number) => string;
}

function UsageCard({ title, used, limit, unit, format }: UsageCardProps) {
  const percentage = Math.min(Math.round((used / limit) * 100), 100);
  const formatFn = format || ((v: number) => v.toString());

  let barColor = 'bg-teal-500';
  if (percentage >= 90) barColor = 'bg-red-500';
  else if (percentage >= 75) barColor = 'bg-yellow-500';

  return (
    <div className="p-4 bg-gray-50 rounded-lg">
      <p className="text-sm font-medium text-gray-600">{title}</p>
      <div className="mt-2 flex items-baseline gap-1">
        <span className="text-2xl font-bold text-gray-900">{formatFn(used)}</span>
        <span className="text-sm text-gray-500">
          / {formatFn(limit)} {unit}
        </span>
      </div>
      <div className="mt-3 h-2 bg-gray-200 rounded-full overflow-hidden">
        <div
          className={`h-full ${barColor} rounded-full transition-all`}
          style={{ width: `${percentage}%` }}
        />
      </div>
      <p className="mt-1 text-xs text-gray-500">{percentage}% used</p>
    </div>
  );
}

interface StatCardProps {
  title: string;
  value: string;
  icon: React.ReactNode;
  color: 'red' | 'blue' | 'green' | 'purple';
}

function StatCard({ title, value, icon, color }: StatCardProps) {
  const colors = {
    red: 'bg-red-100 text-red-600',
    blue: 'bg-blue-100 text-blue-600',
    green: 'bg-green-100 text-green-600',
    purple: 'bg-purple-100 text-purple-600',
  };

  return (
    <div className="card p-4">
      <div className="flex items-center gap-3">
        <div className={`w-10 h-10 rounded-lg flex items-center justify-center ${colors[color]}`}>
          {icon}
        </div>
        <div>
          <p className="text-sm text-gray-500">{title}</p>
          <p className="text-xl font-bold text-gray-900">{value}</p>
        </div>
      </div>
    </div>
  );
}

function CheckIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
    </svg>
  );
}

function ShieldIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth={2}
        d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"
      />
    </svg>
  );
}

function GlobeIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth={2}
        d="M3.055 11H5a2 2 0 012 2v1a2 2 0 002 2 2 2 0 012 2v2.945M8 3.935V5.5A2.5 2.5 0 0010.5 8h.5a2 2 0 012 2 2 2 0 104 0 2 2 0 012-2h1.064M15 20.488V18a2 2 0 012-2h3.064M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
      />
    </svg>
  );
}

function LayersIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth={2}
        d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10"
      />
    </svg>
  );
}

function ChartIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth={2}
        d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z"
      />
    </svg>
  );
}

function WalletIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth={2}
        d="M3 10h18M7 15h1m4 0h1m-7 4h12a3 3 0 003-3V8a3 3 0 00-3-3H6a3 3 0 00-3 3v8a3 3 0 003 3z"
      />
    </svg>
  );
}

function ReceiptIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth={2}
        d="M9 14l6-6m-5.5.5h.01m4.99 5h.01M19 21V5a2 2 0 00-2-2H7a2 2 0 00-2 2v16l3.5-2 3.5 2 3.5-2 3.5 2z"
      />
    </svg>
  );
}
