import { Routes, Route, Link, useLocation } from 'react-router-dom';
import { DnsProvider } from './contexts/DnsContext';
import Zones from './pages/Zones';
import Records from './pages/Records';
import Analytics from './pages/Analytics';

function NavLink({ to, children }: { to: string; children: React.ReactNode }) {
  const location = useLocation();
  const isActive = location.pathname === to || location.pathname.startsWith(to + '/');

  return (
    <Link
      to={to}
      className={`px-4 py-2 rounded-lg transition-colors ${
        isActive
          ? 'bg-teal-500 text-white'
          : 'text-gray-300 hover:text-white hover:bg-gray-700'
      }`}
    >
      {children}
    </Link>
  );
}

export default function App() {
  return (
    <DnsProvider>
      <div className="min-h-screen bg-gray-900">
        {/* Header */}
        <header className="bg-gray-800 border-b border-gray-700">
          <div className="max-w-7xl mx-auto px-4 py-4">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-4">
                <h1 className="text-xl font-bold text-white">AEGIS DNS</h1>
                <nav className="flex gap-2 ml-8">
                  <NavLink to="/zones">Zones</NavLink>
                  <NavLink to="/analytics">Analytics</NavLink>
                </nav>
              </div>
            </div>
          </div>
        </header>

        {/* Main Content */}
        <main className="max-w-7xl mx-auto px-4 py-6">
          <Routes>
            <Route path="/" element={<Zones />} />
            <Route path="/zones" element={<Zones />} />
            <Route path="/zones/:domain/records" element={<Records />} />
            <Route path="/analytics" element={<Analytics />} />
          </Routes>
        </main>
      </div>
    </DnsProvider>
  );
}
