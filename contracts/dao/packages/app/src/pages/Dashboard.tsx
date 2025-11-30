import { FC, useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { DaoConfig, Proposal, ProposalStatus } from "@aegis/dao-sdk";
import { useDaoClient } from "../contexts/DaoClientContext";
import { ProposalCard } from "../components/ProposalCard";
import { StatCard } from "../components/StatCard";
import { formatTokenAmount, formatDuration } from "../utils/format";

export const Dashboard: FC = () => {
  const { getDaoConfig, getProposals } = useDaoClient();
  const [config, setConfig] = useState<DaoConfig | null>(null);
  const [proposals, setProposals] = useState<Proposal[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const fetchData = async () => {
      setLoading(true);
      const [configData, proposalsData] = await Promise.all([
        getDaoConfig(),
        getProposals(),
      ]);
      setConfig(configData);
      setProposals(proposalsData);
      setLoading(false);
    };

    fetchData();
  }, [getDaoConfig, getProposals]);

  const activeProposals = proposals.filter(
    (p) => p.status === ProposalStatus.Active
  );

  if (loading) {
    return (
      <div className="flex items-center justify-center py-20">
        <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-aegis-500"></div>
      </div>
    );
  }

  if (!config) {
    return (
      <div className="text-center py-20">
        <h2 className="text-2xl font-bold text-gray-400 mb-4">
          DAO Not Initialized
        </h2>
        <p className="text-gray-500">
          The DAO configuration could not be loaded. Please try again later.
        </p>
      </div>
    );
  }

  return (
    <div className="space-y-8">
      {/* Header */}
      <div>
        <h1 className="text-3xl font-bold text-white mb-2">
          Welcome to AEGIS DAO
        </h1>
        <p className="text-gray-400">
          Participate in governance of the decentralized edge network
        </p>
      </div>

      {/* Stats Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
        <StatCard
          title="Total Proposals"
          value={config.proposalCount.toString()}
          subtitle="Governance decisions"
        />
        <StatCard
          title="Active Proposals"
          value={activeProposals.length}
          subtitle="Open for voting"
        />
        <StatCard
          title="Treasury Balance"
          value={`${formatTokenAmount(config.totalTreasuryDeposits)} AEGIS`}
          subtitle="Community funds"
        />
        <StatCard
          title="Voting Period"
          value={formatDuration(config.votingPeriod.toNumber())}
          subtitle="Per proposal"
        />
      </div>

      {/* DAO Configuration */}
      <div className="card">
        <h2 className="text-xl font-semibold text-white mb-4">
          DAO Configuration
        </h2>
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6">
          <div>
            <p className="text-sm text-gray-500 mb-1">Proposal Bond</p>
            <p className="text-lg font-medium text-white">
              {formatTokenAmount(config.proposalBond)} AEGIS
            </p>
          </div>
          <div>
            <p className="text-sm text-gray-500 mb-1">Quorum Required</p>
            <p className="text-lg font-medium text-white">
              {config.quorumPercentage}%
            </p>
          </div>
          <div>
            <p className="text-sm text-gray-500 mb-1">Approval Threshold</p>
            <p className="text-lg font-medium text-white">
              {config.approvalThreshold}%
            </p>
          </div>
          <div>
            <p className="text-sm text-gray-500 mb-1">Status</p>
            <p
              className={`text-lg font-medium ${
                config.paused ? "text-red-400" : "text-green-400"
              }`}
            >
              {config.paused ? "Paused" : "Active"}
            </p>
          </div>
        </div>
      </div>

      {/* Active Proposals */}
      <div>
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-xl font-semibold text-white">
            Active Proposals
          </h2>
          <Link to="/proposals" className="btn-secondary text-sm">
            View All
          </Link>
        </div>

        {activeProposals.length === 0 ? (
          <div className="card text-center py-12">
            <p className="text-gray-400 mb-4">
              No active proposals at the moment
            </p>
            <Link to="/proposals/create" className="btn-primary">
              Create Proposal
            </Link>
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {activeProposals.slice(0, 4).map((proposal) => (
              <ProposalCard
                key={proposal.proposalId.toString()}
                proposal={proposal}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
};
