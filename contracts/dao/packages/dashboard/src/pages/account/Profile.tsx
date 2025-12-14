import { useState } from 'react';
import { useAuth } from '../../contexts/AuthContext';

export default function Profile() {
  const { user, walletAddress, updateProfile, apiKeys, createApiKey, revokeApiKey } = useAuth();
  const [displayName, setDisplayName] = useState(user?.displayName || '');
  const [email, setEmail] = useState(user?.email || '');
  const [isSaving, setIsSaving] = useState(false);
  const [showNewKeyModal, setShowNewKeyModal] = useState(false);
  const [newKeyName, setNewKeyName] = useState('');
  const [newKeyPermissions, setNewKeyPermissions] = useState<string[]>([]);
  const [generatedKey, setGeneratedKey] = useState<string | null>(null);

  const handleSaveProfile = async () => {
    setIsSaving(true);
    try {
      await updateProfile({ displayName, email: email || undefined });
      alert('Profile updated successfully!');
    } catch (error) {
      alert('Failed to update profile');
    } finally {
      setIsSaving(false);
    }
  };

  const handleCreateApiKey = async () => {
    if (!newKeyName || newKeyPermissions.length === 0) {
      alert('Please enter a name and select permissions');
      return;
    }
    try {
      const result = await createApiKey(newKeyName, newKeyPermissions as any);
      setGeneratedKey(result.key);
      setNewKeyName('');
      setNewKeyPermissions([]);
    } catch (error) {
      alert('Failed to create API key');
    }
  };

  const handleRevokeKey = async (keyId: string) => {
    if (!confirm('Are you sure you want to revoke this API key?')) return;
    try {
      await revokeApiKey(keyId);
    } catch (error) {
      alert('Failed to revoke API key');
    }
  };

  const copyToClipboard = (text: string) => {
    navigator.clipboard.writeText(text);
    alert('Copied to clipboard!');
  };

  return (
    <div className="max-w-4xl space-y-6">
      <div>
        <h1 className="text-2xl font-bold text-gray-900">Profile</h1>
        <p className="text-gray-500 mt-1">Manage your account settings and API keys</p>
      </div>

      {/* Profile Info */}
      <div className="card p-6">
        <h2 className="text-lg font-semibold text-gray-900 mb-4">Account Information</h2>
        <div className="space-y-4">
          <div>
            <label className="block text-sm font-medium text-gray-600 mb-1">
              Wallet Address
            </label>
            <div className="flex items-center gap-2">
              <input
                type="text"
                value={walletAddress || ''}
                readOnly
                className="input w-full bg-gray-50 font-mono text-sm"
              />
              <button
                onClick={() => copyToClipboard(walletAddress || '')}
                className="btn-secondary px-3"
              >
                Copy
              </button>
            </div>
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-600 mb-1">
              Display Name
            </label>
            <input
              type="text"
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              className="input w-full"
              placeholder="Enter your display name"
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-600 mb-1">
              Email (optional)
            </label>
            <input
              type="email"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              className="input w-full"
              placeholder="you@example.com"
            />
            <p className="text-xs text-gray-400 mt-1">
              Used for notifications and account recovery
            </p>
          </div>

          <div className="pt-4 border-t border-gray-200">
            <button
              onClick={handleSaveProfile}
              disabled={isSaving}
              className="btn-primary"
            >
              {isSaving ? 'Saving...' : 'Save Changes'}
            </button>
          </div>
        </div>
      </div>

      {/* API Keys */}
      <div className="card p-6">
        <div className="flex items-center justify-between mb-4">
          <div>
            <h2 className="text-lg font-semibold text-gray-900">API Keys</h2>
            <p className="text-sm text-gray-500">
              Manage API keys for programmatic access
            </p>
          </div>
          <button
            onClick={() => setShowNewKeyModal(true)}
            className="btn-primary flex items-center gap-2"
          >
            <PlusIcon className="w-4 h-4" />
            New API Key
          </button>
        </div>

        {apiKeys.length > 0 ? (
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead>
                <tr className="table-header">
                  <th className="text-left p-3">Name</th>
                  <th className="text-left p-3">Key Prefix</th>
                  <th className="text-left p-3">Permissions</th>
                  <th className="text-left p-3">Created</th>
                  <th className="text-right p-3">Actions</th>
                </tr>
              </thead>
              <tbody>
                {apiKeys.map((key) => (
                  <tr key={key.id} className="table-row">
                    <td className="p-3 font-medium text-gray-900">{key.name}</td>
                    <td className="p-3 font-mono text-sm text-gray-500">
                      {key.prefix}...
                    </td>
                    <td className="p-3">
                      <div className="flex flex-wrap gap-1">
                        {key.permissions.map((perm) => (
                          <span
                            key={perm}
                            className="px-2 py-0.5 bg-gray-100 text-gray-600 rounded text-xs"
                          >
                            {perm}
                          </span>
                        ))}
                      </div>
                    </td>
                    <td className="p-3 text-sm text-gray-500">
                      {new Date(key.createdAt).toLocaleDateString()}
                    </td>
                    <td className="p-3 text-right">
                      <button
                        onClick={() => handleRevokeKey(key.id)}
                        className="text-red-600 hover:text-red-700 text-sm font-medium"
                      >
                        Revoke
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : (
          <div className="text-center py-8 bg-gray-50 rounded-lg">
            <KeyIcon className="w-12 h-12 text-gray-300 mx-auto mb-3" />
            <p className="text-gray-500">No API keys created yet</p>
            <p className="text-sm text-gray-400 mt-1">
              Create an API key to access the AEGIS API programmatically
            </p>
          </div>
        )}
      </div>

      {/* New API Key Modal */}
      {showNewKeyModal && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-white rounded-lg p-6 w-full max-w-md border border-gray-200 shadow-lg">
            {generatedKey ? (
              <>
                <h2 className="text-xl font-bold text-gray-900 mb-4">
                  API Key Created
                </h2>
                <div className="bg-yellow-50 border border-yellow-200 rounded-lg p-4 mb-4">
                  <p className="text-sm text-yellow-800 mb-2">
                    Make sure to copy your API key now. You won't be able to see it
                    again!
                  </p>
                  <div className="bg-white border border-yellow-300 rounded p-2 font-mono text-sm break-all">
                    {generatedKey}
                  </div>
                </div>
                <div className="flex gap-3">
                  <button
                    onClick={() => copyToClipboard(generatedKey)}
                    className="btn-primary flex-1"
                  >
                    Copy Key
                  </button>
                  <button
                    onClick={() => {
                      setShowNewKeyModal(false);
                      setGeneratedKey(null);
                    }}
                    className="btn-secondary flex-1"
                  >
                    Done
                  </button>
                </div>
              </>
            ) : (
              <>
                <h2 className="text-xl font-bold text-gray-900 mb-4">
                  Create API Key
                </h2>
                <div className="space-y-4">
                  <div>
                    <label className="block text-sm font-medium text-gray-600 mb-1">
                      Key Name
                    </label>
                    <input
                      type="text"
                      value={newKeyName}
                      onChange={(e) => setNewKeyName(e.target.value)}
                      className="input w-full"
                      placeholder="My API Key"
                    />
                  </div>

                  <div>
                    <label className="block text-sm font-medium text-gray-600 mb-2">
                      Permissions
                    </label>
                    <div className="space-y-2">
                      {[
                        { value: 'dns:read', label: 'DNS Read' },
                        { value: 'dns:write', label: 'DNS Write' },
                        { value: 'ddos:read', label: 'DDoS Read' },
                        { value: 'ddos:write', label: 'DDoS Write' },
                      ].map((perm) => (
                        <label
                          key={perm.value}
                          className="flex items-center gap-2 cursor-pointer"
                        >
                          <input
                            type="checkbox"
                            checked={newKeyPermissions.includes(perm.value)}
                            onChange={(e) => {
                              if (e.target.checked) {
                                setNewKeyPermissions([
                                  ...newKeyPermissions,
                                  perm.value,
                                ]);
                              } else {
                                setNewKeyPermissions(
                                  newKeyPermissions.filter((p) => p !== perm.value)
                                );
                              }
                            }}
                            className="w-4 h-4 text-teal-500 rounded border-gray-300"
                          />
                          <span className="text-sm text-gray-700">{perm.label}</span>
                        </label>
                      ))}
                    </div>
                  </div>

                  <div className="flex gap-3 pt-4 border-t border-gray-200">
                    <button
                      onClick={() => setShowNewKeyModal(false)}
                      className="btn-secondary flex-1"
                    >
                      Cancel
                    </button>
                    <button
                      onClick={handleCreateApiKey}
                      className="btn-primary flex-1"
                    >
                      Create Key
                    </button>
                  </div>
                </div>
              </>
            )}
          </div>
        </div>
      )}

      {/* Account Info */}
      <div className="card p-6">
        <h2 className="text-lg font-semibold text-gray-900 mb-4">Account Details</h2>
        <div className="grid grid-cols-2 gap-4 text-sm">
          <div>
            <p className="text-gray-500">Member Since</p>
            <p className="text-gray-900 font-medium">
              {user ? new Date(user.createdAt).toLocaleDateString() : '-'}
            </p>
          </div>
          <div>
            <p className="text-gray-500">Last Updated</p>
            <p className="text-gray-900 font-medium">
              {user ? new Date(user.updatedAt).toLocaleDateString() : '-'}
            </p>
          </div>
        </div>
      </div>
    </div>
  );
}

function PlusIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
    </svg>
  );
}

function KeyIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth={2}
        d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z"
      />
    </svg>
  );
}
