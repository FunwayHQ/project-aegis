import { useState } from 'react';
import { useDns } from '../../contexts/DnsContext';
import { DnsRecordType } from '@aegis/dns-sdk';

interface CreateRecordModalProps {
  domain: string;
  onClose: () => void;
  onCreate: () => Promise<void>;
}

const RECORD_TYPES: DnsRecordType[] = ['A', 'AAAA', 'CNAME', 'MX', 'TXT', 'NS', 'CAA', 'SRV'];

const TTL_OPTIONS = [
  { value: 60, label: '1 minute' },
  { value: 300, label: '5 minutes' },
  { value: 3600, label: '1 hour' },
  { value: 14400, label: '4 hours' },
  { value: 86400, label: '1 day' },
];

export default function CreateRecordModal({ domain, onClose, onCreate }: CreateRecordModalProps) {
  const { client } = useDns();
  const [type, setType] = useState<DnsRecordType>('A');
  const [name, setName] = useState('');
  const [value, setValue] = useState('');
  const [ttl, setTtl] = useState(300);
  const [priority, setPriority] = useState<number | undefined>(undefined);
  const [proxied, setProxied] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const needsPriority = type === 'MX' || type === 'SRV';
  const canBeProxied = type === 'A' || type === 'AAAA' || type === 'CNAME';

  const getPlaceholder = () => {
    switch (type) {
      case 'A':
        return '192.168.1.1';
      case 'AAAA':
        return '2001:db8::1';
      case 'CNAME':
        return 'target.example.com';
      case 'MX':
        return 'mail.example.com';
      case 'TXT':
        return 'v=spf1 include:_spf.google.com ~all';
      case 'NS':
        return 'ns1.example.com';
      case 'CAA':
        return '0 issue "letsencrypt.org"';
      case 'SRV':
        return '10 443 target.example.com';
      default:
        return '';
    }
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (!name.trim() && type !== 'MX') {
      setError('Name is required');
      return;
    }

    if (!value.trim()) {
      setError('Value is required');
      return;
    }

    if (needsPriority && (priority === undefined || priority < 0)) {
      setError('Priority is required for this record type');
      return;
    }

    try {
      setLoading(true);
      setError(null);

      await client.createRecord(domain, {
        name: name.trim() || '@',
        type,
        value: value.trim(),
        ttl,
        priority: needsPriority ? priority : undefined,
        proxied: canBeProxied ? proxied : false,
      });

      await onCreate();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create record');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50" onClick={onClose}>
      <div
        className="bg-gray-800 rounded-lg w-full max-w-lg mx-4 max-h-[90vh] overflow-y-auto"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex justify-between items-center px-6 py-4 border-b border-gray-700 sticky top-0 bg-gray-800">
          <h2 className="text-xl font-bold text-white">Add DNS Record</h2>
          <button
            onClick={onClose}
            className="p-1 hover:bg-gray-700 rounded text-gray-400 hover:text-white transition-colors"
          >
            <CloseIcon className="w-6 h-6" />
          </button>
        </div>

        {/* Form */}
        <form onSubmit={handleSubmit} className="p-6">
          {error && (
            <div className="mb-4 p-3 bg-red-500/10 border border-red-500 rounded-lg text-red-400 text-sm">
              {error}
            </div>
          )}

          <div className="space-y-4">
            {/* Record Type */}
            <div>
              <label className="block text-sm font-medium text-gray-300 mb-1">
                Type
              </label>
              <div className="grid grid-cols-4 gap-2">
                {RECORD_TYPES.map((t) => (
                  <button
                    key={t}
                    type="button"
                    onClick={() => setType(t)}
                    className={`px-3 py-2 rounded-lg text-sm font-bold transition-colors ${
                      type === t
                        ? 'bg-teal-500 text-white'
                        : 'bg-gray-700 text-gray-300 hover:bg-gray-600'
                    }`}
                  >
                    {t}
                  </button>
                ))}
              </div>
            </div>

            {/* Name */}
            <div>
              <label htmlFor="name" className="block text-sm font-medium text-gray-300 mb-1">
                Name
              </label>
              <div className="flex items-center">
                <input
                  id="name"
                  type="text"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  placeholder="www"
                  className="input rounded-r-none"
                  disabled={loading}
                />
                <span className="px-3 py-2 bg-gray-900 border border-l-0 border-gray-600 rounded-r-lg text-gray-400 text-sm">
                  .{domain}
                </span>
              </div>
              <p className="mt-1 text-xs text-gray-500">
                Use @ for root domain, or enter subdomain name
              </p>
            </div>

            {/* Value */}
            <div>
              <label htmlFor="value" className="block text-sm font-medium text-gray-300 mb-1">
                Value
              </label>
              <input
                id="value"
                type="text"
                value={value}
                onChange={(e) => setValue(e.target.value)}
                placeholder={getPlaceholder()}
                className="input"
                disabled={loading}
              />
            </div>

            {/* Priority (for MX/SRV) */}
            {needsPriority && (
              <div>
                <label htmlFor="priority" className="block text-sm font-medium text-gray-300 mb-1">
                  Priority
                </label>
                <input
                  id="priority"
                  type="number"
                  value={priority ?? ''}
                  onChange={(e) => setPriority(e.target.value ? parseInt(e.target.value, 10) : undefined)}
                  placeholder="10"
                  min="0"
                  max="65535"
                  className="input"
                  disabled={loading}
                />
                <p className="mt-1 text-xs text-gray-500">
                  Lower values have higher priority
                </p>
              </div>
            )}

            {/* TTL */}
            <div>
              <label htmlFor="ttl" className="block text-sm font-medium text-gray-300 mb-1">
                TTL (Time to Live)
              </label>
              <select
                id="ttl"
                value={ttl}
                onChange={(e) => setTtl(parseInt(e.target.value, 10))}
                className="select"
                disabled={loading}
              >
                {TTL_OPTIONS.map((opt) => (
                  <option key={opt.value} value={opt.value}>
                    {opt.label}
                  </option>
                ))}
              </select>
            </div>

            {/* Proxied Toggle (for A/AAAA/CNAME) */}
            {canBeProxied && (
              <div className="flex items-center justify-between py-3 px-4 bg-gray-900 rounded-lg">
                <div>
                  <p className="text-white font-medium">Proxy through AEGIS</p>
                  <p className="text-sm text-gray-400">
                    Enable DDoS protection for this record
                  </p>
                </div>
                <label className="relative inline-flex items-center cursor-pointer">
                  <input
                    type="checkbox"
                    checked={proxied}
                    onChange={(e) => setProxied(e.target.checked)}
                    className="sr-only peer"
                    disabled={loading}
                  />
                  <div className="w-11 h-6 bg-gray-600 peer-focus:outline-none peer-focus:ring-2 peer-focus:ring-teal-500 rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-teal-500"></div>
                </label>
              </div>
            )}
          </div>

          {/* Actions */}
          <div className="flex gap-3 mt-6">
            <button
              type="button"
              onClick={onClose}
              className="btn-secondary flex-1"
              disabled={loading}
            >
              Cancel
            </button>
            <button
              type="submit"
              className="btn-primary flex-1"
              disabled={loading}
            >
              {loading ? 'Creating...' : 'Add Record'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

function CloseIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
    </svg>
  );
}
