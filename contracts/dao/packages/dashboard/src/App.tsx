import { Routes, Route, Navigate } from 'react-router-dom';
import { DnsProvider } from './contexts/DnsContext';
import { DdosProvider } from './contexts/DdosContext';
import { AuthProvider } from './contexts/AuthContext';
import { WalletProvider } from './components/auth/WalletProvider';
import { ProtectedRoute } from './components/auth/ProtectedRoute';
import Layout from './components/common/Layout';

// Pages
import Overview from './pages/Overview';
import Login from './pages/Login';

// DNS Pages
import DnsZones from './pages/dns/Zones';
import DnsRecords from './pages/dns/Records';
import DnsAnalytics from './pages/dns/Analytics';

// DDoS Pages
import DdosDashboard from './pages/ddos/Dashboard';
import DdosBlocklist from './pages/ddos/Blocklist';
import DdosPolicies from './pages/ddos/Policies';
import DdosStatistics from './pages/ddos/Statistics';

// Account Pages
import Profile from './pages/account/Profile';
import Teams from './pages/account/Teams';
import Billing from './pages/account/Billing';

// Settings
import Settings from './pages/Settings';

export default function App() {
  return (
    <WalletProvider>
      <AuthProvider>
        <DnsProvider>
          <DdosProvider>
            <Routes>
              {/* Public Routes */}
              <Route path="/login" element={<Login />} />

              {/* Protected Routes */}
              <Route
                path="/*"
                element={
                  <ProtectedRoute>
                    <Layout>
                      <Routes>
                        {/* Overview */}
                        <Route path="/" element={<Overview />} />

                        {/* DNS Routes */}
                        <Route path="/dns" element={<Navigate to="/dns/zones" replace />} />
                        <Route path="/dns/zones" element={<DnsZones />} />
                        <Route path="/dns/zones/:domain" element={<DnsRecords />} />
                        <Route path="/dns/analytics" element={<DnsAnalytics />} />

                        {/* DDoS Routes */}
                        <Route path="/ddos" element={<Navigate to="/ddos/dashboard" replace />} />
                        <Route path="/ddos/dashboard" element={<DdosDashboard />} />
                        <Route path="/ddos/blocklist" element={<DdosBlocklist />} />
                        <Route path="/ddos/policies" element={<DdosPolicies />} />
                        <Route path="/ddos/statistics" element={<DdosStatistics />} />

                        {/* Account Routes */}
                        <Route path="/account" element={<Navigate to="/account/profile" replace />} />
                        <Route path="/account/profile" element={<Profile />} />
                        <Route path="/account/teams" element={<Teams />} />
                        <Route path="/account/billing" element={<Billing />} />

                        {/* Settings */}
                        <Route path="/settings" element={<Settings />} />

                        {/* Catch all */}
                        <Route path="*" element={<Navigate to="/" replace />} />
                      </Routes>
                    </Layout>
                  </ProtectedRoute>
                }
              />
            </Routes>
          </DdosProvider>
        </DnsProvider>
      </AuthProvider>
    </WalletProvider>
  );
}
