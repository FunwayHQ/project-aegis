import { FC } from "react";
import { Link } from "react-router-dom";
import { Proposal, ProposalStatus } from "@aegis/dao-sdk";
import {
  shortAddress,
  calculateVotePercentages,
  getTimeRemaining,
} from "../utils/format";

interface ProposalCardProps {
  proposal: Proposal;
}

const statusColors: Record<ProposalStatus, string> = {
  [ProposalStatus.Active]: "bg-blue-500/20 text-blue-400 border-blue-500/30",
  [ProposalStatus.Passed]: "bg-green-500/20 text-green-400 border-green-500/30",
  [ProposalStatus.Defeated]: "bg-red-500/20 text-red-400 border-red-500/30",
  [ProposalStatus.Executed]: "bg-cyan-500/20 text-cyan-400 border-cyan-500/30",
  [ProposalStatus.Cancelled]: "bg-gray-500/20 text-gray-400 border-gray-500/30",
};

export const ProposalCard: FC<ProposalCardProps> = ({ proposal }) => {
  const percentages = calculateVotePercentages(
    proposal.forVotes,
    proposal.againstVotes,
    proposal.abstainVotes
  );

  const isActive = proposal.status === ProposalStatus.Active;
  const timeRemaining = isActive ? getTimeRemaining(proposal.voteEnd) : null;

  return (
    <Link
      to={`/proposals/${proposal.proposalId.toString()}`}
      className="card hover:border-aegis-500/50 transition-all"
    >
      <div className="flex items-start justify-between mb-4">
        <div>
          <div className="flex items-center space-x-2 mb-1">
            <span className="text-sm text-gray-500">
              #{proposal.proposalId.toString()}
            </span>
            <span
              className={`text-xs px-2 py-0.5 rounded-full border ${
                statusColors[proposal.status]
              }`}
            >
              {proposal.status.charAt(0).toUpperCase() + proposal.status.slice(1)}
            </span>
          </div>
          <h3 className="text-lg font-semibold text-white">{proposal.title}</h3>
        </div>
        {isActive && timeRemaining && (
          <div className="text-right">
            <div className="text-xs text-gray-500">Ends in</div>
            <div className="text-sm font-medium text-aegis-400">
              {timeRemaining}
            </div>
          </div>
        )}
      </div>

      {/* Vote Progress */}
      <div className="mb-4">
        <div className="flex h-2 rounded-full overflow-hidden bg-gray-700">
          <div
            className="bg-green-500 transition-all"
            style={{ width: `${percentages.for}%` }}
          />
          <div
            className="bg-red-500 transition-all"
            style={{ width: `${percentages.against}%` }}
          />
          <div
            className="bg-gray-500 transition-all"
            style={{ width: `${percentages.abstain}%` }}
          />
        </div>
        <div className="flex justify-between mt-2 text-xs text-gray-400">
          <span>For: {percentages.for.toFixed(1)}%</span>
          <span>Against: {percentages.against.toFixed(1)}%</span>
          <span>Abstain: {percentages.abstain.toFixed(1)}%</span>
        </div>
      </div>

      {/* Metadata */}
      <div className="flex items-center justify-between text-sm text-gray-500">
        <span>Proposer: {shortAddress(proposal.proposer)}</span>
        <span className="capitalize">{proposal.proposalType}</span>
      </div>
    </Link>
  );
};
