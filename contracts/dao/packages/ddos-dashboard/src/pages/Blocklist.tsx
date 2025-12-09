import { useState } from 'react';
import { useDDoS } from '../contexts/DDoSContext';
import type { BlocklistAddRequest, AllowlistAddRequest } from '@aegis/ddos-sdk';

export default function Blocklist() {
  const {
    client,
    blocklist,
    allowlist,
    refreshBlocklist,
    refreshAllowlist,
    isLoading,
  } = useDDoS();
  const [activeTab, setActiveTab] = useState<'blocklist' | 'allowlist'>('blocklist');
  const [isAdding, setIsAdding] = useState(false);

  const handleRemoveFromBlocklist = async (ip: string) => {
    if (!client || !confirm(`Remove ${ip} from blocklist?`)) return;
    try {
      await client.removeFromBlocklist(ip);
      await refreshBlocklist();
    } catch (err) {
      console.error('Failed to remove from blocklist:', err);
      alert('Failed to remove from blocklist');
    }
  };

  const handleRemoveFromAllowlist = async (ip: string) => {
    if (!client || !confirm(`Remove ${ip} from allowlist?`)) return;
    try {
      await client.removeFromAllowlist(ip);
      await refreshAllowlist();
    } catch (err) {
      console.error('Failed to remove from allowlist:', err);
      alert('Failed to remove from allowlist');
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
          <h1 className="text-2xl font-bold text-white">IP Management</h1>
          <p className="text-gray-400 mt-1">
            Manage blocked and allowed IP addresses
          </p>
        </div>
        <button
          onClick={() => setIsAdding(true)}
          className="btn-primary flex items-center gap-2"
        >
          <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
          </svg>
          Add IP
        </button>
      </div>

      {/* Tabs */}
      <div className="flex gap-2 border-b border-gray-700">
        <button
          onClick={() => setActiveTab('blocklist')}
          className={`px-4 py-2 font-medium transition-colors ${
            activeTab === 'blocklist'
              ? 'text-white border-b-2 border-aegis-500'
              : 'text-gray-400 hover:text-white'
          }`}
        >
          Blocklist ({blocklist.length})
        </button>
        <button
          onClick={() => setActiveTab('allowlist')}
          className={`px-4 py-2 font-medium transition-colors ${
            activeTab === 'allowlist'
              ? 'text-white border-b-2 border-aegis-500'
              : 'text-gray-400 hover:text-white'
          }`}
        >
          Allowlist ({allowlist.length})
        </button>
      </div>

      {/* Content */}
      {activeTab === 'blocklist' ? (
        <div className="stat-card overflow-hidden">
          <table className="w-full">
            <thead>
              <tr className="table-header">
                <th className="text-left p-4">IP Address</th>
                <th className="text-left p-4">Reason</th>
                <th className="text-left p-4">Source</th>
                <th className="text-left p-4">Blocked At</th>
                <th className="text-left p-4">Expires</th>
                <th className="text-right p-4">Actions</th>
              </tr>
            </thead>
            <tbody>
              {blocklist.map((entry) => (
                <tr key={entry.ip} className="table-row">
                  <td className="p-4 font-mono">{entry.ip}</td>
                  <td className="p-4 text-sm text-gray-400">{entry.reason}</td>
                  <td className="p-4">
                    <span className="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-gray-700 text-gray-300">
                      {entry.source}
                    </span>
                  </td>
                  <td className="p-4 text-sm text-gray-400">
                    {new Date(entry.blocked_at * 1000).toLocaleString()}
                  </td>
                  <td className="p-4 text-sm">
                    {entry.expires_at === 0 ? (
                      <span className="text-red-400">Permanent</span>
                    ) : (
                      <span className="text-gray-400">
                        {new Date(entry.expires_at * 1000).toLocaleString()}
                      </span>
                    )}
                  </td>
                  <td className="p-4 text-right">
                    <button
                      onClick={() => handleRemoveFromBlocklist(entry.ip)}
                      className="p-2 text-gray-400 hover:text-green-400 hover:bg-green-500/10 rounded-lg transition-colors"
                      title="Unblock"
                    >
                      <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
                      </svg>
                    </button>
                  </td>
                </tr>
              ))}
              {blocklist.length === 0 && (
                <tr>
                  <td colSpan={6} className="p-8 text-center text-gray-500">
                    No blocked IPs
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      ) : (
        <div className="stat-card overflow-hidden">
          <table className="w-full">
            <thead>
              <tr className="table-header">
                <th className="text-left p-4">IP Address</th>
                <th className="text-left p-4">Reason</th>
                <th className="text-left p-4">Added At</th>
                <th className="text-right p-4">Actions</th>
              </tr>
            </thead>
            <tbody>
              {allowlist.map((entry) => (
                <tr key={entry.ip} className="table-row">
                  <td className="p-4 font-mono">{entry.ip}</td>
                  <td className="p-4 text-sm text-gray-400">{entry.reason}</td>
                  <td className="p-4 text-sm text-gray-400">
                    {new Date(entry.added_at * 1000).toLocaleString()}
                  </td>
                  <td className="p-4 text-right">
                    <button
                      onClick={() => handleRemoveFromAllowlist(entry.ip)}
                      className="p-2 text-gray-400 hover:text-red-400 hover:bg-red-500/10 rounded-lg transition-colors"
                      title="Remove"
                    >
                      <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                      </svg>
                    </button>
                  </td>
                </tr>
              ))}
              {allowlist.length === 0 && (
                <tr>
                  <td colSpan={4} className="p-8 text-center text-gray-500">
                    No allowed IPs
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      )}

      {/* Add Modal */}
      {isAdding && (
        <AddIpModal
          type={activeTab}
          onClose={() => setIsAdding(false)}
          onAdd={async (type, data) => {
            if (!client) return;
            try {
              if (type === 'blocklist') {
                await client.addToBlocklist(data as BlocklistAddRequest);
                await refreshBlocklist();
              } else {
                await client.addToAllowlist(data as AllowlistAddRequest);
                await refreshAllowlist();
              }
              setIsAdding(false);
            } catch (err) {
              console.error('Failed to add IP:', err);
              alert('Failed to add IP');
            }
          }}
        />
      )}
    </div>
  );
}

interface AddIpModalProps {
  type: 'blocklist' | 'allowlist';
  onClose: () => void;
  onAdd: (
    type: 'blocklist' | 'allowlist',
    data: BlocklistAddRequest | AllowlistAddRequest
  ) => Promise<void>;
}

function AddIpModal({ type, onClose, onAdd }: AddIpModalProps) {
  const [listType, setListType] = useState<'blocklist' | 'allowlist'>(type);
  const [ip, setIp] = useState('');
  const [reason, setReason] = useState('');
  const [duration, setDuration] = useState(300);
  const [isPermanent, setIsPermanent] = useState(false);
  const [isSaving, setIsSaving] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!ip || !reason) return;

    setIsSaving(true);
    try {
      if (listType === 'blocklist') {
        await onAdd(listType, {
          ip,
          reason,
          duration_secs: isPermanent ? 0 : duration,
        });
      } else {
        await onAdd(listType, { ip, reason });
      }
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-gray-800 rounded-lg p-6 w-full max-w-md border border-gray-700">
        <h2 className="text-xl font-bold text-white mb-4">Add IP Address</h2>
        <form onSubmit={handleSubmit} className="space-y-4">
          {/* List type selector */}
          <div className="flex gap-2">
            <button
              type="button"
              onClick={() => setListType('blocklist')}
              className={`flex-1 py-2 px-4 rounded-lg font-medium transition-colors ${
                listType === 'blocklist'
                  ? 'bg-red-500/20 text-red-400 border border-red-500/30'
                  : 'bg-gray-700 text-gray-400 border border-gray-600'
              }`}
            >
              Block
            </button>
            <button
              type="button"
              onClick={() => setListType('allowlist')}
              className={`flex-1 py-2 px-4 rounded-lg font-medium transition-colors ${
                listType === 'allowlist'
                  ? 'bg-green-500/20 text-green-400 border border-green-500/30'
                  : 'bg-gray-700 text-gray-400 border border-gray-600'
              }`}
            >
              Allow
            </button>
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-400 mb-1">
              IP Address or CIDR
            </label>
            <input
              type="text"
              value={ip}
              onChange={(e) => setIp(e.target.value)}
              className="input w-full"
              placeholder="192.168.1.100 or 10.0.0.0/24"
              required
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-400 mb-1">
              Reason
            </label>
            <input
              type="text"
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              className="input w-full"
              placeholder="Suspicious activity"
              required
            />
          </div>

          {listType === 'blocklist' && (
            <>
              <div className="flex items-center justify-between">
                <label className="text-sm font-medium text-gray-400">
                  Permanent Block
                </label>
                <button
                  type="button"
                  onClick={() => setIsPermanent(!isPermanent)}
                  className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors ${
                    isPermanent ? 'bg-red-600' : 'bg-gray-600'
                  }`}
                >
                  <span
                    className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
                      isPermanent ? 'translate-x-6' : 'translate-x-1'
                    }`}
                  />
                </button>
              </div>

              {!isPermanent && (
                <div>
                  <label className="block text-sm font-medium text-gray-400 mb-1">
                    Duration (seconds)
                  </label>
                  <input
                    type="number"
                    value={duration}
                    onChange={(e) => setDuration(Number(e.target.value))}
                    className="input w-full"
                    min={10}
                    max={86400}
                  />
                  <p className="text-xs text-gray-500 mt-1">
                    {Math.floor(duration / 60)} minutes {duration % 60} seconds
                  </p>
                </div>
              )}
            </>
          )}

          <div className="flex justify-end gap-3 pt-4 border-t border-gray-700">
            <button
              type="button"
              onClick={onClose}
              className="btn-secondary"
              disabled={isSaving}
            >
              Cancel
            </button>
            <button
              type="submit"
              className={listType === 'blocklist' ? 'btn-danger' : 'btn-primary'}
              disabled={isSaving}
            >
              {isSaving
                ? 'Adding...'
                : listType === 'blocklist'
                  ? 'Block IP'
                  : 'Allow IP'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
