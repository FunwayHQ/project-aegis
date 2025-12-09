import { useState } from 'react';
import { useDns } from '../contexts/DnsContext';

interface CreateZoneModalProps {
  onClose: () => void;
  onCreate: () => Promise<void>;
}

export default function CreateZoneModal({ onClose, onCreate }: CreateZoneModalProps) {
  const { client } = useDns();
  const [domain, setDomain] = useState('');
  const [proxied, setProxied] = useState(true);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (!domain.trim()) {
      setError('Domain is required');
      return;
    }

    // Basic domain validation
    const domainRegex = /^[a-zA-Z0-9][a-zA-Z0-9-]{0,61}[a-zA-Z0-9]?\.[a-zA-Z]{2,}$/;
    if (!domainRegex.test(domain.trim())) {
      setError('Please enter a valid domain name (e.g., example.com)');
      return;
    }

    try {
      setLoading(true);
      setError(null);

      await client.createZone({
        domain: domain.trim().toLowerCase(),
        proxied,
      });

      await onCreate();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create zone');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50" onClick={onClose}>
      <div
        className="bg-gray-800 rounded-lg w-full max-w-md mx-4"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex justify-between items-center px-6 py-4 border-b border-gray-700">
          <h2 className="text-xl font-bold text-white">Add DNS Zone</h2>
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
            {/* Domain Input */}
            <div>
              <label htmlFor="domain" className="block text-sm font-medium text-gray-300 mb-1">
                Domain Name
              </label>
              <input
                id="domain"
                type="text"
                value={domain}
                onChange={(e) => setDomain(e.target.value)}
                placeholder="example.com"
                className="input"
                disabled={loading}
                autoFocus
              />
              <p className="mt-1 text-xs text-gray-500">
                Enter your domain without www or http://
              </p>
            </div>

            {/* Proxied Toggle */}
            <div className="flex items-center justify-between py-3 px-4 bg-gray-900 rounded-lg">
              <div>
                <p className="text-white font-medium">Proxy through AEGIS</p>
                <p className="text-sm text-gray-400">
                  Enable DDoS protection and edge caching
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
          </div>

          {/* Info Box */}
          <div className="mt-4 p-4 bg-teal-500/10 border border-teal-500/30 rounded-lg">
            <p className="text-sm text-teal-400">
              After creating your zone, you'll need to update your domain's nameservers at your registrar to:
            </p>
            <ul className="mt-2 text-sm text-teal-300 font-mono">
              <li>ns1.aegis.network</li>
              <li>ns2.aegis.network</li>
            </ul>
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
              {loading ? 'Creating...' : 'Create Zone'}
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
