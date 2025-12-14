import { useState } from 'react';

export default function Settings() {
  const [dnsApiUrl, setDnsApiUrl] = useState(
    import.meta.env.VITE_DNS_API_URL || 'http://localhost:8054'
  );
  const [ddosApiUrl, setDdosApiUrl] = useState(
    import.meta.env.VITE_DDOS_API_URL || 'http://localhost:8080'
  );
  const [theme, setTheme] = useState<'dark' | 'light'>('light');
  const [notifications, setNotifications] = useState(true);
  const [autoRefresh, setAutoRefresh] = useState(true);
  const [refreshInterval, setRefreshInterval] = useState(10);

  const handleSave = () => {
    // In a real app, this would persist to localStorage or a backend
    localStorage.setItem('aegis-settings', JSON.stringify({
      dnsApiUrl,
      ddosApiUrl,
      theme,
      notifications,
      autoRefresh,
      refreshInterval,
    }));
    alert('Settings saved successfully!');
  };

  return (
    <div className="max-w-2xl">
      <h1 className="text-2xl font-bold text-gray-900 mb-6">Settings</h1>

      <div className="space-y-6">
        {/* API Configuration */}
        <div className="card p-6">
          <h2 className="text-lg font-semibold text-gray-900 mb-4">API Configuration</h2>
          <div className="space-y-4">
            <div>
              <label className="block text-sm font-medium text-gray-600 mb-1">
                DNS API URL
              </label>
              <input
                type="text"
                value={dnsApiUrl}
                onChange={(e) => setDnsApiUrl(e.target.value)}
                className="input w-full"
                placeholder="http://localhost:8054"
              />
              <p className="mt-1 text-xs text-gray-500">
                The URL of the DNS management API server
              </p>
            </div>
            <div>
              <label className="block text-sm font-medium text-gray-600 mb-1">
                DDoS API URL
              </label>
              <input
                type="text"
                value={ddosApiUrl}
                onChange={(e) => setDdosApiUrl(e.target.value)}
                className="input w-full"
                placeholder="http://localhost:8080"
              />
              <p className="mt-1 text-xs text-gray-500">
                The URL of the DDoS protection API server
              </p>
            </div>
          </div>
        </div>

        {/* Display Settings */}
        <div className="card p-6">
          <h2 className="text-lg font-semibold text-gray-900 mb-4">Display</h2>
          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-gray-900 font-medium">Theme</p>
                <p className="text-sm text-gray-500">Choose your preferred color scheme</p>
              </div>
              <select
                value={theme}
                onChange={(e) => setTheme(e.target.value as 'dark' | 'light')}
                className="select"
              >
                <option value="light">Light</option>
                <option value="dark">Dark (coming soon)</option>
              </select>
            </div>
          </div>
        </div>

        {/* Notifications */}
        <div className="card p-6">
          <h2 className="text-lg font-semibold text-gray-900 mb-4">Notifications</h2>
          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-gray-900 font-medium">Enable Notifications</p>
                <p className="text-sm text-gray-500">
                  Receive alerts for attacks and important events
                </p>
              </div>
              <button
                onClick={() => setNotifications(!notifications)}
                className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors ${
                  notifications ? 'bg-teal-500' : 'bg-gray-300'
                }`}
              >
                <span
                  className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
                    notifications ? 'translate-x-6' : 'translate-x-1'
                  }`}
                />
              </button>
            </div>
          </div>
        </div>

        {/* Auto Refresh */}
        <div className="card p-6">
          <h2 className="text-lg font-semibold text-gray-900 mb-4">Data Refresh</h2>
          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-gray-900 font-medium">Auto Refresh</p>
                <p className="text-sm text-gray-500">
                  Automatically refresh dashboard data
                </p>
              </div>
              <button
                onClick={() => setAutoRefresh(!autoRefresh)}
                className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors ${
                  autoRefresh ? 'bg-teal-500' : 'bg-gray-300'
                }`}
              >
                <span
                  className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
                    autoRefresh ? 'translate-x-6' : 'translate-x-1'
                  }`}
                />
              </button>
            </div>
            {autoRefresh && (
              <div>
                <label className="block text-sm font-medium text-gray-600 mb-1">
                  Refresh Interval (seconds)
                </label>
                <input
                  type="number"
                  value={refreshInterval}
                  onChange={(e) => setRefreshInterval(Number(e.target.value))}
                  className="input w-full"
                  min={5}
                  max={300}
                />
              </div>
            )}
          </div>
        </div>

        {/* About */}
        <div className="card p-6">
          <h2 className="text-lg font-semibold text-gray-900 mb-4">About</h2>
          <div className="space-y-2 text-sm text-gray-500">
            <p>
              <span className="text-gray-700">Version:</span> 1.0.0
            </p>
            <p>
              <span className="text-gray-700">Platform:</span> AEGIS Decentralized Edge Network
            </p>
            <p className="pt-2">
              AEGIS is a blockchain-powered global edge network designed as a decentralized
              alternative to centralized CDN and edge security providers.
            </p>
          </div>
        </div>

        {/* Save Button */}
        <div className="flex justify-end">
          <button onClick={handleSave} className="btn-primary">
            Save Settings
          </button>
        </div>
      </div>
    </div>
  );
}
