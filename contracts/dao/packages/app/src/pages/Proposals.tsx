import { FC, useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { Proposal, ProposalStatus } from "@aegis/dao-sdk";
import { useDaoClient } from "../contexts/DaoClientContext";
import { ProposalCard } from "../components/ProposalCard";

const statusFilters: { value: ProposalStatus | "all"; label: string }[] = [
  { value: "all", label: "All" },
  { value: ProposalStatus.Active, label: "Active" },
  { value: ProposalStatus.Passed, label: "Passed" },
  { value: ProposalStatus.Defeated, label: "Defeated" },
  { value: ProposalStatus.Executed, label: "Executed" },
  { value: ProposalStatus.Cancelled, label: "Cancelled" },
];

export const Proposals: FC = () => {
  const { getProposals } = useDaoClient();
  const [proposals, setProposals] = useState<Proposal[]>([]);
  const [loading, setLoading] = useState(true);
  const [filter, setFilter] = useState<ProposalStatus | "all">("all");

  useEffect(() => {
    const fetchProposals = async () => {
      setLoading(true);
      const data = await getProposals();
      setProposals(data);
      setLoading(false);
    };

    fetchProposals();
  }, [getProposals]);

  const filteredProposals =
    filter === "all"
      ? proposals
      : proposals.filter((p) => p.status === filter);

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div>
          <h1 className="text-3xl font-bold text-white mb-2">Proposals</h1>
          <p className="text-gray-400">
            View and vote on governance proposals
          </p>
        </div>
        <Link to="/proposals/create" className="btn-primary">
          Create Proposal
        </Link>
      </div>

      {/* Filters */}
      <div className="flex flex-wrap gap-2">
        {statusFilters.map((status) => (
          <button
            key={status.value}
            onClick={() => setFilter(status.value)}
            className={`px-4 py-2 rounded-lg text-sm font-medium transition-colors ${
              filter === status.value
                ? "bg-aegis-600 text-white"
                : "bg-gray-800 text-gray-400 hover:bg-gray-700"
            }`}
          >
            {status.label}
          </button>
        ))}
      </div>

      {/* Proposals List */}
      {loading ? (
        <div className="flex items-center justify-center py-20">
          <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-aegis-500"></div>
        </div>
      ) : filteredProposals.length === 0 ? (
        <div className="card text-center py-12">
          <p className="text-gray-400 mb-4">
            {filter === "all"
              ? "No proposals found"
              : `No ${filter} proposals found`}
          </p>
          <Link to="/proposals/create" className="btn-primary">
            Create First Proposal
          </Link>
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {filteredProposals.map((proposal) => (
            <ProposalCard
              key={proposal.proposalId.toString()}
              proposal={proposal}
            />
          ))}
        </div>
      )}
    </div>
  );
};
