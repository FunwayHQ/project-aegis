import { useState } from 'react';
import { useDDoS } from '../contexts/DDoSContext';
import type { DDoSPolicy, DDoSPolicyInput } from '@aegis/ddos-sdk';

export default function Policies() {
  const { client, policies, refreshPolicies, isLoading } = useDDoS();
  const [isCreating, setIsCreating] = useState(false);
  const [editingPolicy, setEditingPolicy] = useState<DDoSPolicy | null>(null);

  const handleDelete = async (domain: string) => {
    if (!client || !confirm(`Delete policy for ${domain}?`)) return;
    try {
      await client.deletePolicy(domain);
      await refreshPolicies();
    } catch (err) {
      console.error('Failed to delete policy:', err);
      alert('Failed to delete policy');
    }
  };

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
          <h1 className="text-2xl font-bold text-white">DDoS Policies</h1>
          <p className="text-gray-400 mt-1">
            Configure DDoS protection policies for your domains
          </p>
        </div>
        <button
          onClick={() => setIsCreating(true)}
          className="btn-primary flex items-center gap-2"
        >
          <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
          </svg>
          New Policy
        </button>
      </div>

      {/* Policies table */}
      <div className="stat-card overflow-hidden">
        <table className="w-full">
          <thead>
            <tr className="table-header">
              <th className="text-left p-4">Domain</th>
              <th className="text-left p-4">Status</th>
              <th className="text-left p-4">SYN Threshold</th>
              <th className="text-left p-4">UDP Threshold</th>
              <th className="text-left p-4">Rate Limit</th>
              <th className="text-left p-4">Challenge</th>
              <th className="text-right p-4">Actions</th>
            </tr>
          </thead>
          <tbody>
            {policies.map((policy) => (
              <tr key={policy.domain} className="table-row">
                <td className="p-4">
                  <span className="font-medium text-white">{policy.domain}</span>
                </td>
                <td className="p-4">
                  <span
                    className={`inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium ${
                      policy.enabled
                        ? 'bg-green-500/20 text-green-400'
                        : 'bg-gray-500/20 text-gray-400'
                    }`}
                  >
                    {policy.enabled ? 'Enabled' : 'Disabled'}
                  </span>
                </td>
                <td className="p-4 font-mono text-sm">
                  {policy.syn_threshold.toLocaleString()}/s
                </td>
                <td className="p-4 font-mono text-sm">
                  {policy.udp_threshold.toLocaleString()}/s
                </td>
                <td className="p-4">
                  {policy.rate_limit?.enabled ? (
                    <span className="text-sm">
                      {policy.rate_limit.max_requests_per_minute}/min
                    </span>
                  ) : (
                    <span className="text-gray-500 text-sm">Off</span>
                  )}
                </td>
                <td className="p-4">
                  {policy.challenge_mode?.enabled ? (
                    <span className="text-sm capitalize">
                      {policy.challenge_mode.challenge_type}
                    </span>
                  ) : (
                    <span className="text-gray-500 text-sm">Off</span>
                  )}
                </td>
                <td className="p-4 text-right">
                  <div className="flex items-center justify-end gap-2">
                    <button
                      onClick={() => setEditingPolicy(policy)}
                      className="p-2 text-gray-400 hover:text-white hover:bg-gray-700 rounded-lg transition-colors"
                      title="Edit"
                    >
                      <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
                      </svg>
                    </button>
                    <button
                      onClick={() => handleDelete(policy.domain)}
                      className="p-2 text-gray-400 hover:text-red-400 hover:bg-red-500/10 rounded-lg transition-colors"
                      title="Delete"
                    >
                      <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                      </svg>
                    </button>
                  </div>
                </td>
              </tr>
            ))}
            {policies.length === 0 && (
              <tr>
                <td colSpan={7} className="p-8 text-center text-gray-500">
                  No policies configured. Create one to get started.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>

      {/* Create/Edit Modal */}
      {(isCreating || editingPolicy) && (
        <PolicyModal
          policy={editingPolicy}
          onClose={() => {
            setIsCreating(false);
            setEditingPolicy(null);
          }}
          onSave={async (domain, input) => {
            if (!client) return;
            try {
              if (editingPolicy) {
                await client.updatePolicy(domain, input);
              } else {
                await client.createPolicy(domain, input);
              }
              await refreshPolicies();
              setIsCreating(false);
              setEditingPolicy(null);
            } catch (err) {
              console.error('Failed to save policy:', err);
              alert('Failed to save policy');
            }
          }}
        />
      )}
    </div>
  );
}

interface PolicyModalProps {
  policy: DDoSPolicy | null;
  onClose: () => void;
  onSave: (domain: string, input: DDoSPolicyInput) => Promise<void>;
}

function PolicyModal({ policy, onClose, onSave }: PolicyModalProps) {
  const [domain, setDomain] = useState(policy?.domain || '');
  const [enabled, setEnabled] = useState(policy?.enabled ?? true);
  const [synThreshold, setSynThreshold] = useState(policy?.syn_threshold ?? 100);
  const [udpThreshold, setUdpThreshold] = useState(policy?.udp_threshold ?? 1000);
  const [rateLimitEnabled, setRateLimitEnabled] = useState(
    policy?.rate_limit?.enabled ?? false
  );
  const [rateLimit, setRateLimit] = useState(
    policy?.rate_limit?.max_requests_per_minute ?? 100
  );
  const [isSaving, setIsSaving] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!domain) return;

    setIsSaving(true);
    try {
      await onSave(domain, {
        enabled,
        syn_threshold: synThreshold,
        udp_threshold: udpThreshold,
        rate_limit: rateLimitEnabled
          ? {
              enabled: true,
              max_requests_per_minute: rateLimit,
              window_duration_secs: 60,
              scope: 'per_ip',
            }
          : undefined,
      });
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-gray-800 rounded-lg p-6 w-full max-w-md border border-gray-700">
        <h2 className="text-xl font-bold text-white mb-4">
          {policy ? 'Edit Policy' : 'New Policy'}
        </h2>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="block text-sm font-medium text-gray-400 mb-1">
              Domain
            </label>
            <input
              type="text"
              value={domain}
              onChange={(e) => setDomain(e.target.value)}
              className="input w-full"
              placeholder="example.com"
              disabled={!!policy}
              required
            />
          </div>

          <div className="flex items-center justify-between">
            <label className="text-sm font-medium text-gray-400">
              Protection Enabled
            </label>
            <button
              type="button"
              onClick={() => setEnabled(!enabled)}
              className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors ${
                enabled ? 'bg-aegis-600' : 'bg-gray-600'
              }`}
            >
              <span
                className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
                  enabled ? 'translate-x-6' : 'translate-x-1'
                }`}
              />
            </button>
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-400 mb-1">
              SYN Threshold (packets/sec)
            </label>
            <input
              type="number"
              value={synThreshold}
              onChange={(e) => setSynThreshold(Number(e.target.value))}
              className="input w-full"
              min={10}
              max={100000}
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-400 mb-1">
              UDP Threshold (packets/sec)
            </label>
            <input
              type="number"
              value={udpThreshold}
              onChange={(e) => setUdpThreshold(Number(e.target.value))}
              className="input w-full"
              min={100}
              max={1000000}
            />
          </div>

          <div className="border-t border-gray-700 pt-4">
            <div className="flex items-center justify-between mb-3">
              <label className="text-sm font-medium text-gray-400">
                Rate Limiting
              </label>
              <button
                type="button"
                onClick={() => setRateLimitEnabled(!rateLimitEnabled)}
                className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors ${
                  rateLimitEnabled ? 'bg-aegis-600' : 'bg-gray-600'
                }`}
              >
                <span
                  className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
                    rateLimitEnabled ? 'translate-x-6' : 'translate-x-1'
                  }`}
                />
              </button>
            </div>
            {rateLimitEnabled && (
              <div>
                <label className="block text-sm font-medium text-gray-400 mb-1">
                  Requests per minute
                </label>
                <input
                  type="number"
                  value={rateLimit}
                  onChange={(e) => setRateLimit(Number(e.target.value))}
                  className="input w-full"
                  min={1}
                  max={100000}
                />
              </div>
            )}
          </div>

          <div className="flex justify-end gap-3 pt-4 border-t border-gray-700">
            <button
              type="button"
              onClick={onClose}
              className="btn-secondary"
              disabled={isSaving}
            >
              Cancel
            </button>
            <button type="submit" className="btn-primary" disabled={isSaving}>
              {isSaving ? 'Saving...' : policy ? 'Update' : 'Create'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
