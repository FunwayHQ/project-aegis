import { ReactNode, useState } from 'react';
import { Link, useLocation } from 'react-router-dom';
import { useAuth } from '../../contexts/AuthContext';

interface LayoutProps {
  children: ReactNode;
}

export default function Layout({ children }: LayoutProps) {
  const location = useLocation();
  const { user, walletAddress, currentTeam, teams, switchTeam, disconnect } = useAuth();
  const [showUserMenu, setShowUserMenu] = useState(false);
  const [showTeamMenu, setShowTeamMenu] = useState(false);

  const isActive = (path: string) => {
    if (path === '/') return location.pathname === '/';
    return location.pathname.startsWith(path);
  };

  const handleDisconnect = async () => {
    await disconnect();
    setShowUserMenu(false);
  };

  return (
    <div className="min-h-screen bg-gray-50">
      {/* Top Header Bar */}
      <header className="fixed top-0 left-64 right-0 h-16 bg-white border-b border-gray-200 z-40 flex items-center justify-between px-6">
        {/* Team Selector */}
        <div className="relative">
          <button
            onClick={() => setShowTeamMenu(!showTeamMenu)}
            className="flex items-center gap-2 px-3 py-2 rounded-lg hover:bg-gray-100 transition-colors"
          >
            <div className="w-6 h-6 bg-teal-100 text-teal-600 rounded flex items-center justify-center text-xs font-bold">
              {currentTeam?.name.charAt(0).toUpperCase() || 'T'}
            </div>
            <span className="font-medium text-gray-900">{currentTeam?.name || 'Select Team'}</span>
            <ChevronDownIcon className="w-4 h-4 text-gray-400" />
          </button>

          {showTeamMenu && (
            <>
              <div className="fixed inset-0 z-40" onClick={() => setShowTeamMenu(false)} />
              <div className="absolute top-full left-0 mt-1 w-56 bg-white rounded-lg shadow-lg border border-gray-200 py-1 z-50">
                {teams.map((team) => (
                  <button
                    key={team.id}
                    onClick={() => {
                      switchTeam(team.id);
                      setShowTeamMenu(false);
                    }}
                    className={`w-full px-4 py-2 text-left hover:bg-gray-50 flex items-center justify-between ${
                      currentTeam?.id === team.id ? 'bg-teal-50' : ''
                    }`}
                  >
                    <span className="text-gray-900">{team.name}</span>
                    {currentTeam?.id === team.id && (
                      <CheckIcon className="w-4 h-4 text-teal-500" />
                    )}
                  </button>
                ))}
                <div className="border-t border-gray-100 mt-1 pt-1">
                  <Link
                    to="/account/teams"
                    onClick={() => setShowTeamMenu(false)}
                    className="w-full px-4 py-2 text-left text-teal-600 hover:bg-gray-50 flex items-center gap-2 text-sm"
                  >
                    <PlusIcon className="w-4 h-4" />
                    Create Team
                  </Link>
                </div>
              </div>
            </>
          )}
        </div>

        {/* User Menu */}
        <div className="relative">
          <button
            onClick={() => setShowUserMenu(!showUserMenu)}
            className="flex items-center gap-3 px-3 py-2 rounded-lg hover:bg-gray-100 transition-colors"
          >
            <div className="w-8 h-8 bg-gradient-to-br from-teal-400 to-teal-600 rounded-full flex items-center justify-center">
              <span className="text-white text-sm font-medium">
                {user?.displayName?.charAt(0).toUpperCase() || 'U'}
              </span>
            </div>
            <div className="text-left hidden sm:block">
              <p className="text-sm font-medium text-gray-900">{user?.displayName}</p>
              <p className="text-xs text-gray-500 font-mono">
                {walletAddress?.slice(0, 4)}...{walletAddress?.slice(-4)}
              </p>
            </div>
            <ChevronDownIcon className="w-4 h-4 text-gray-400" />
          </button>

          {showUserMenu && (
            <>
              <div className="fixed inset-0 z-40" onClick={() => setShowUserMenu(false)} />
              <div className="absolute top-full right-0 mt-1 w-56 bg-white rounded-lg shadow-lg border border-gray-200 py-1 z-50">
                <div className="px-4 py-3 border-b border-gray-100">
                  <p className="text-sm font-medium text-gray-900">{user?.displayName}</p>
                  <p className="text-xs text-gray-500 font-mono truncate">{walletAddress}</p>
                </div>
                <Link
                  to="/account/profile"
                  onClick={() => setShowUserMenu(false)}
                  className="w-full px-4 py-2 text-left text-gray-700 hover:bg-gray-50 flex items-center gap-2"
                >
                  <UserIcon className="w-4 h-4" />
                  Profile
                </Link>
                <Link
                  to="/account/teams"
                  onClick={() => setShowUserMenu(false)}
                  className="w-full px-4 py-2 text-left text-gray-700 hover:bg-gray-50 flex items-center gap-2"
                >
                  <UsersIcon className="w-4 h-4" />
                  Teams
                </Link>
                <Link
                  to="/account/billing"
                  onClick={() => setShowUserMenu(false)}
                  className="w-full px-4 py-2 text-left text-gray-700 hover:bg-gray-50 flex items-center gap-2"
                >
                  <CreditCardIcon className="w-4 h-4" />
                  Billing & Usage
                </Link>
                <div className="border-t border-gray-100 mt-1 pt-1">
                  <button
                    onClick={handleDisconnect}
                    className="w-full px-4 py-2 text-left text-red-600 hover:bg-red-50 flex items-center gap-2"
                  >
                    <LogoutIcon className="w-4 h-4" />
                    Disconnect Wallet
                  </button>
                </div>
              </div>
            </>
          )}
        </div>
      </header>

      {/* Sidebar */}
      <aside className="sidebar">
        {/* Logo */}
        <div className="p-6 border-b border-gray-200">
          <Link to="/" className="flex items-center justify-center">
            <img src="/AEGIS-logo.svg" alt="AEGIS" className="h-20" />
          </Link>
        </div>

        {/* Navigation */}
        <nav className="flex-1 py-4 overflow-y-auto">
          {/* Overview */}
          <Link to="/" className={`sidebar-link ${isActive('/') && location.pathname === '/' ? 'active' : ''}`}>
            <HomeIcon className="w-5 h-5" />
            <span>Overview</span>
          </Link>

          {/* DNS Section */}
          <div className="mt-6">
            <div className="sidebar-section">DNS Management</div>
            <Link to="/dns/zones" className={`sidebar-link ${isActive('/dns/zones') ? 'active' : ''}`}>
              <GlobeIcon className="w-5 h-5" />
              <span>Zones</span>
            </Link>
            <Link to="/dns/analytics" className={`sidebar-link ${isActive('/dns/analytics') ? 'active' : ''}`}>
              <ChartIcon className="w-5 h-5" />
              <span>DNS Analytics</span>
            </Link>
          </div>

          {/* DDoS Section */}
          <div className="mt-6">
            <div className="sidebar-section">DDoS Protection</div>
            <Link to="/ddos/dashboard" className={`sidebar-link ${isActive('/ddos/dashboard') ? 'active' : ''}`}>
              <DashboardIcon className="w-5 h-5" />
              <span>Dashboard</span>
            </Link>
            <Link to="/ddos/blocklist" className={`sidebar-link ${isActive('/ddos/blocklist') ? 'active' : ''}`}>
              <BlockIcon className="w-5 h-5" />
              <span>Blocklist</span>
            </Link>
            <Link to="/ddos/policies" className={`sidebar-link ${isActive('/ddos/policies') ? 'active' : ''}`}>
              <PolicyIcon className="w-5 h-5" />
              <span>Policies</span>
            </Link>
            <Link to="/ddos/statistics" className={`sidebar-link ${isActive('/ddos/statistics') ? 'active' : ''}`}>
              <StatsIcon className="w-5 h-5" />
              <span>Statistics</span>
            </Link>
          </div>

          {/* Account Section */}
          <div className="mt-6">
            <div className="sidebar-section">Account</div>
            <Link to="/account/profile" className={`sidebar-link ${isActive('/account/profile') ? 'active' : ''}`}>
              <UserIcon className="w-5 h-5" />
              <span>Profile</span>
            </Link>
            <Link to="/account/teams" className={`sidebar-link ${isActive('/account/teams') ? 'active' : ''}`}>
              <UsersIcon className="w-5 h-5" />
              <span>Teams</span>
            </Link>
            <Link to="/account/billing" className={`sidebar-link ${isActive('/account/billing') ? 'active' : ''}`}>
              <CreditCardIcon className="w-5 h-5" />
              <span>Billing</span>
            </Link>
          </div>

          {/* Settings */}
          <div className="mt-6">
            <div className="sidebar-section">System</div>
            <Link to="/settings" className={`sidebar-link ${isActive('/settings') ? 'active' : ''}`}>
              <SettingsIcon className="w-5 h-5" />
              <span>Settings</span>
            </Link>
          </div>
        </nav>

        {/* Footer */}
        <div className="p-4 border-t border-gray-200">
          <div className="flex items-center gap-3">
            <div className="w-8 h-8 bg-green-500 rounded-full flex items-center justify-center">
              <span className="text-xs font-bold text-white">ON</span>
            </div>
            <div>
              <p className="text-sm text-gray-900 font-medium">System Online</p>
              <p className="text-xs text-gray-500">All services healthy</p>
            </div>
          </div>
        </div>
      </aside>

      {/* Main Content */}
      <main className="ml-64 min-h-screen pt-16">
        <div className="p-6">
          {children}
        </div>
      </main>
    </div>
  );
}

// Icons
function HomeIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-6 0a1 1 0 001-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 001 1m-6 0h6" />
    </svg>
  );
}

function GlobeIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3.055 11H5a2 2 0 012 2v1a2 2 0 002 2 2 2 0 012 2v2.945M8 3.935V5.5A2.5 2.5 0 0010.5 8h.5a2 2 0 012 2 2 2 0 104 0 2 2 0 012-2h1.064M15 20.488V18a2 2 0 012-2h3.064M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
    </svg>
  );
}

function ChartIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z" />
    </svg>
  );
}

function DashboardIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 5a1 1 0 011-1h14a1 1 0 011 1v2a1 1 0 01-1 1H5a1 1 0 01-1-1V5zM4 13a1 1 0 011-1h6a1 1 0 011 1v6a1 1 0 01-1 1H5a1 1 0 01-1-1v-6zM16 13a1 1 0 011-1h2a1 1 0 011 1v6a1 1 0 01-1 1h-2a1 1 0 01-1-1v-6z" />
    </svg>
  );
}

function BlockIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M18.364 18.364A9 9 0 005.636 5.636m12.728 12.728A9 9 0 015.636 5.636m12.728 12.728L5.636 5.636" />
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

function StatsIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 12l3-3 3 3 4-4M8 21l4-4 4 4M3 4h18M4 4h16v12a1 1 0 01-1 1H5a1 1 0 01-1-1V4z" />
    </svg>
  );
}

function SettingsIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
    </svg>
  );
}

function UserIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z" />
    </svg>
  );
}

function UsersIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4.354a4 4 0 110 5.292M15 21H3v-1a6 6 0 0112 0v1zm0 0h6v-1a6 6 0 00-9-5.197M13 7a4 4 0 11-8 0 4 4 0 018 0z" />
    </svg>
  );
}

function CreditCardIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 10h18M7 15h1m4 0h1m-7 4h12a3 3 0 003-3V8a3 3 0 00-3-3H6a3 3 0 00-3 3v8a3 3 0 003 3z" />
    </svg>
  );
}

function LogoutIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M17 16l4-4m0 0l-4-4m4 4H7m6 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h4a3 3 0 013 3v1" />
    </svg>
  );
}

function ChevronDownIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
    </svg>
  );
}

function CheckIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
    </svg>
  );
}

function PlusIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
    </svg>
  );
}
