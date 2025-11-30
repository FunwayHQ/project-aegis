import { FC } from "react";
import { Routes, Route } from "react-router-dom";
import { Layout } from "./components/Layout";
import { Dashboard } from "./pages/Dashboard";
import { Proposals } from "./pages/Proposals";
import { ProposalDetail } from "./pages/ProposalDetail";
import { Treasury } from "./pages/Treasury";

const App: FC = () => {
  return (
    <Layout>
      <Routes>
        <Route path="/" element={<Dashboard />} />
        <Route path="/proposals" element={<Proposals />} />
        <Route path="/proposals/:id" element={<ProposalDetail />} />
        <Route path="/treasury" element={<Treasury />} />
      </Routes>
    </Layout>
  );
};

export default App;
