import { NavLink, Outlet } from 'react-router-dom';
import { useDDoS } from '../contexts/DDoSContext';

export default function Layout() {
  const { isConnected, stats } = useDDoS();

  return (
    <div className="min-h-screen bg-gray-900">
      {/* Header */}
      <header className="bg-gray-800 border-b border-gray-700">
        <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
          <div className="flex items-center justify-between h-16">
            {/* Logo */}
            <div className="flex items-center gap-3">
              <div className="w-8 h-8 rounded-lg bg-aegis-600 flex items-center justify-center">
                <span className="text-white font-bold text-sm">A</span>
              </div>
              <span className="text-xl font-bold text-white">AEGIS DDoS</span>
            </div>

            {/* Nav */}
            <nav className="flex items-center gap-2">
              <NavLink
                to="/"
                end
                className={({ isActive }) =>
                  `nav-link ${isActive ? 'nav-link-active' : ''}`
                }
              >
                Dashboard
              </NavLink>
              <NavLink
                to="/policies"
                className={({ isActive }) =>
                  `nav-link ${isActive ? 'nav-link-active' : ''}`
                }
              >
                Policies
              </NavLink>
              <NavLink
                to="/blocklist"
                className={({ isActive }) =>
                  `nav-link ${isActive ? 'nav-link-active' : ''}`
                }
              >
                Blocklist
              </NavLink>
              <NavLink
                to="/statistics"
                className={({ isActive }) =>
                  `nav-link ${isActive ? 'nav-link-active' : ''}`
                }
              >
                Statistics
              </NavLink>
            </nav>

            {/* Status */}
            <div className="flex items-center gap-4">
              {stats && (
                <div className="text-sm text-gray-400">
                  <span className="text-white font-medium">
                    {stats.active_attacks}
                  </span>{' '}
                  active attacks
                </div>
              )}
              <div className="flex items-center gap-2">
                <div
                  className={`w-2 h-2 rounded-full ${
                    isConnected ? 'bg-green-500 animate-pulse' : 'bg-red-500'
                  }`}
                />
                <span className="text-sm text-gray-400">
                  {isConnected ? 'Connected' : 'Disconnected'}
                </span>
              </div>
            </div>
          </div>
        </div>
      </header>

      {/* Main content */}
      <main className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        <Outlet />
      </main>

      {/* Footer */}
      <footer className="bg-gray-800 border-t border-gray-700 mt-auto">
        <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-4">
          <div className="flex items-center justify-between text-sm text-gray-400">
            <span>AEGIS Decentralized Edge Network</span>
            <span>DDoS Protection Dashboard v0.1.0</span>
          </div>
        </div>
      </footer>
    </div>
  );
}
