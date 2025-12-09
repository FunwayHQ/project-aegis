import { Routes, Route } from 'react-router-dom';
import Layout from './components/Layout';
import Dashboard from './pages/Dashboard';
import Policies from './pages/Policies';
import Blocklist from './pages/Blocklist';
import Statistics from './pages/Statistics';

export default function App() {
  return (
    <Routes>
      <Route path="/" element={<Layout />}>
        <Route index element={<Dashboard />} />
        <Route path="policies" element={<Policies />} />
        <Route path="blocklist" element={<Blocklist />} />
        <Route path="statistics" element={<Statistics />} />
      </Route>
    </Routes>
  );
}
