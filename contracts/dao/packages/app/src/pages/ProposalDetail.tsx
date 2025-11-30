import { FC, useEffect, useState, useCallback } from "react";
import { useParams, Link } from "react-router-dom";
import { useWallet } from "@solana/wallet-adapter-react";
import {
  Proposal,
  ProposalStatus,
  VoteChoice,
  VoteEscrow,
} from "@aegis/dao-sdk";
import { useDaoClient } from "../contexts/DaoClientContext";
import {
  formatTokenAmount,
  formatTimestamp,
  shortAddress,
  calculateVotePercentages,
  getTimeRemaining,
} from "../utils/format";

export const ProposalDetail: FC = () => {
  const { id } = useParams<{ id: string }>();
  const { publicKey } = useWallet();
  const { client, isReady } = useDaoClient();

  const [proposal, setProposal] = useState<Proposal | null>(null);
  const [voteEscrow, setVoteEscrow] = useState<VoteEscrow | null>(null);
  const [loading, setLoading] = useState(true);
  const [actionLoading, setActionLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Form state
  const [depositAmount, setDepositAmount] = useState("");
  const [selectedVote, setSelectedVote] = useState<VoteChoice | null>(null);

  const fetchData = useCallback(async () => {
    if (!client || !id) return;

    try {
      const proposalData = await client.getProposal(parseInt(id));
      setProposal(proposalData);

      if (publicKey) {
        const escrow = await client.getVoteEscrow(parseInt(id), publicKey);
        setVoteEscrow(escrow);
      }
    } catch (err) {
      setError("Failed to load proposal");
      console.error(err);
    } finally {
      setLoading(false);
    }
  }, [client, id, publicKey]);

  useEffect(() => {
    if (isReady) {
      fetchData();
    }
  }, [isReady, fetchData]);

  const handleDeposit = async () => {
    if (!client || !publicKey || !depositAmount) return;

    setActionLoading(true);
    setError(null);

    try {
      // Note: In a real app, you'd need the user's token account
      // This is simplified - you'd use getAssociatedTokenAddressSync
      alert(
        "To deposit, you need to provide your token account address. " +
          "Use the CLI: aegis-dao vote deposit " +
          id +
          " --amount " +
          depositAmount +
          " --token-account <your-token-account>"
      );
    } catch (err) {
      setError("Failed to deposit tokens");
      console.error(err);
    } finally {
      setActionLoading(false);
    }
  };

  const handleVote = async () => {
    if (!client || !selectedVote || !id) return;

    setActionLoading(true);
    setError(null);

    try {
      await client.castVote({
        proposalId: parseInt(id),
        voteChoice: selectedVote,
      });
      await fetchData();
    } catch (err) {
      setError("Failed to cast vote");
      console.error(err);
    } finally {
      setActionLoading(false);
    }
  };

  const handleRetract = async () => {
    if (!client || !id) return;

    setActionLoading(true);
    setError(null);

    try {
      await client.retractVote(parseInt(id));
      await fetchData();
    } catch (err) {
      setError("Failed to retract vote");
      console.error(err);
    } finally {
      setActionLoading(false);
    }
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center py-20">
        <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-aegis-500"></div>
      </div>
    );
  }

  if (!proposal) {
    return (
      <div className="text-center py-20">
        <h2 className="text-2xl font-bold text-gray-400 mb-4">
          Proposal Not Found
        </h2>
        <Link to="/proposals" className="btn-primary">
          Back to Proposals
        </Link>
      </div>
    );
  }

  const percentages = calculateVotePercentages(
    proposal.forVotes,
    proposal.againstVotes,
    proposal.abstainVotes
  );

  const isActive = proposal.status === ProposalStatus.Active;
  const hasDeposited = voteEscrow && !voteEscrow.depositedAmount.isZero();
  const hasVoted = voteEscrow?.hasVoted || false;
  const canVote = isActive && hasDeposited && !hasVoted;
  const canRetract = isActive && hasVoted;

  return (
    <div className="space-y-6">
      {/* Back Link */}
      <Link
        to="/proposals"
        className="text-gray-400 hover:text-white transition-colors"
      >
        &larr; Back to Proposals
      </Link>

      {/* Header */}
      <div className="card">
        <div className="flex items-start justify-between mb-4">
          <div>
            <div className="flex items-center space-x-3 mb-2">
              <span className="text-sm text-gray-500">
                #{proposal.proposalId.toString()}
              </span>
              <span
                className={`text-sm px-3 py-1 rounded-full ${
                  proposal.status === ProposalStatus.Active
                    ? "bg-blue-500/20 text-blue-400"
                    : proposal.status === ProposalStatus.Passed
                    ? "bg-green-500/20 text-green-400"
                    : proposal.status === ProposalStatus.Defeated
                    ? "bg-red-500/20 text-red-400"
                    : "bg-gray-500/20 text-gray-400"
                }`}
              >
                {proposal.status.charAt(0).toUpperCase() +
                  proposal.status.slice(1)}
              </span>
            </div>
            <h1 className="text-2xl font-bold text-white">{proposal.title}</h1>
          </div>
          {isActive && (
            <div className="text-right">
              <div className="text-sm text-gray-500">Ends in</div>
              <div className="text-lg font-medium text-aegis-400">
                {getTimeRemaining(proposal.voteEnd)}
              </div>
            </div>
          )}
        </div>

        {/* Metadata */}
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4 pt-4 border-t border-gray-700">
          <div>
            <p className="text-sm text-gray-500">Type</p>
            <p className="text-white capitalize">{proposal.proposalType}</p>
          </div>
          <div>
            <p className="text-sm text-gray-500">Proposer</p>
            <p className="text-white">{shortAddress(proposal.proposer)}</p>
          </div>
          <div>
            <p className="text-sm text-gray-500">Created</p>
            <p className="text-white">{formatTimestamp(proposal.createdAt)}</p>
          </div>
          <div>
            <p className="text-sm text-gray-500">Vote End</p>
            <p className="text-white">{formatTimestamp(proposal.voteEnd)}</p>
          </div>
        </div>

        {/* Governance Timeline (Whitepaper) */}
        <div className="mt-4 p-4 bg-gray-700/30 rounded-lg">
          <h3 className="text-sm font-medium text-gray-400 mb-3">
            Proposal Timeline (Whitepaper Compliant)
          </h3>
          <div className="flex flex-wrap gap-2 text-sm">
            <div className="flex items-center space-x-2">
              <span className="w-3 h-3 rounded-full bg-aegis-500"></span>
              <span className="text-gray-400">Created:</span>
              <span className="text-white">{formatTimestamp(proposal.createdAt)}</span>
            </div>
            <span className="text-gray-600">→</span>
            <div className="flex items-center space-x-2">
              <span className="w-3 h-3 rounded-full bg-blue-500"></span>
              <span className="text-gray-400">Vote Start:</span>
              <span className="text-white">{formatTimestamp(proposal.voteStart)}</span>
            </div>
            <span className="text-gray-600">→</span>
            <div className="flex items-center space-x-2">
              <span className="w-3 h-3 rounded-full bg-purple-500"></span>
              <span className="text-gray-400">Vote End:</span>
              <span className="text-white">{formatTimestamp(proposal.voteEnd)}</span>
            </div>
            {proposal.executionEligibleAt && (
              <>
                <span className="text-gray-600">→</span>
                <div className="flex items-center space-x-2">
                  <span className="w-3 h-3 rounded-full bg-yellow-500"></span>
                  <span className="text-gray-400">Executable After:</span>
                  <span className="text-yellow-400">{formatTimestamp(proposal.executionEligibleAt)}</span>
                  <span className="text-gray-500 text-xs">(3-day timelock)</span>
                </div>
              </>
            )}
          </div>
        </div>

        {/* Execution Data */}
        {proposal.executionData && (
          <div className="mt-4 p-4 bg-gray-700/50 rounded-lg">
            <h3 className="text-sm font-medium text-gray-400 mb-2">
              Treasury Withdrawal
            </h3>
            <div className="grid grid-cols-2 gap-4">
              <div>
                <p className="text-sm text-gray-500">Recipient</p>
                <p className="text-white font-mono text-sm">
                  {proposal.executionData.recipient.toString()}
                </p>
              </div>
              <div>
                <p className="text-sm text-gray-500">Amount</p>
                <p className="text-white">
                  {formatTokenAmount(proposal.executionData.amount)} AEGIS
                </p>
              </div>
            </div>
          </div>
        )}
      </div>

      {/* Voting Results */}
      <div className="card">
        <h2 className="text-xl font-semibold text-white mb-4">
          Voting Results
        </h2>

        {/* Progress Bars */}
        <div className="space-y-4">
          {/* For */}
          <div>
            <div className="flex justify-between text-sm mb-1">
              <span className="text-green-400">For</span>
              <span className="text-gray-400">
                {formatTokenAmount(proposal.forVotes)} AEGIS ({percentages.for.toFixed(1)}%)
              </span>
            </div>
            <div className="h-3 bg-gray-700 rounded-full overflow-hidden">
              <div
                className="h-full bg-green-500 transition-all"
                style={{ width: `${percentages.for}%` }}
              />
            </div>
          </div>

          {/* Against */}
          <div>
            <div className="flex justify-between text-sm mb-1">
              <span className="text-red-400">Against</span>
              <span className="text-gray-400">
                {formatTokenAmount(proposal.againstVotes)} AEGIS ({percentages.against.toFixed(1)}%)
              </span>
            </div>
            <div className="h-3 bg-gray-700 rounded-full overflow-hidden">
              <div
                className="h-full bg-red-500 transition-all"
                style={{ width: `${percentages.against}%` }}
              />
            </div>
          </div>

          {/* Abstain */}
          <div>
            <div className="flex justify-between text-sm mb-1">
              <span className="text-gray-400">Abstain</span>
              <span className="text-gray-400">
                {formatTokenAmount(proposal.abstainVotes)} AEGIS ({percentages.abstain.toFixed(1)}%)
              </span>
            </div>
            <div className="h-3 bg-gray-700 rounded-full overflow-hidden">
              <div
                className="h-full bg-gray-500 transition-all"
                style={{ width: `${percentages.abstain}%` }}
              />
            </div>
          </div>
        </div>
      </div>

      {/* Voting Section */}
      {isActive && (
        <div className="card">
          <h2 className="text-xl font-semibold text-white mb-4">
            Cast Your Vote
          </h2>

          {error && (
            <div className="mb-4 p-4 bg-red-500/20 border border-red-500/30 rounded-lg text-red-400">
              {error}
            </div>
          )}

          {/* Vote Escrow Status */}
          {voteEscrow && (
            <div className="mb-4 p-4 bg-gray-700/50 rounded-lg">
              <h3 className="text-sm font-medium text-gray-400 mb-2">
                Your Vote Escrow
              </h3>
              <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                <div>
                  <p className="text-sm text-gray-500">Deposited</p>
                  <p className="text-white">
                    {formatTokenAmount(voteEscrow.depositedAmount)} AEGIS
                  </p>
                </div>
                <div>
                  <p className="text-sm text-gray-500">Has Voted</p>
                  <p className={voteEscrow.hasVoted ? "text-green-400" : "text-gray-400"}>
                    {voteEscrow.hasVoted ? "Yes" : "No"}
                  </p>
                </div>
                {voteEscrow.voteChoice && (
                  <div>
                    <p className="text-sm text-gray-500">Your Vote</p>
                    <p className="text-white capitalize">{voteEscrow.voteChoice}</p>
                  </div>
                )}
              </div>
            </div>
          )}

          {/* Deposit Section */}
          {!hasDeposited && (
            <div className="mb-6">
              <h3 className="text-sm font-medium text-gray-300 mb-2">
                Step 1: Deposit Tokens
              </h3>
              <p className="text-sm text-gray-500 mb-3">
                Deposit AEGIS tokens to the vote escrow before voting.
              </p>
              <div className="flex gap-3">
                <input
                  type="number"
                  value={depositAmount}
                  onChange={(e) => setDepositAmount(e.target.value)}
                  placeholder="Amount in AEGIS"
                  className="input flex-1"
                />
                <button
                  onClick={handleDeposit}
                  disabled={!depositAmount || actionLoading}
                  className="btn-primary"
                >
                  {actionLoading ? "..." : "Deposit"}
                </button>
              </div>
            </div>
          )}

          {/* Vote Section */}
          {canVote && (
            <div className="mb-6">
              <h3 className="text-sm font-medium text-gray-300 mb-2">
                Step 2: Cast Your Vote
              </h3>
              <div className="flex gap-3 mb-4">
                {[VoteChoice.For, VoteChoice.Against, VoteChoice.Abstain].map(
                  (choice) => (
                    <button
                      key={choice}
                      onClick={() => setSelectedVote(choice)}
                      className={`flex-1 py-3 rounded-lg font-medium transition-colors ${
                        selectedVote === choice
                          ? choice === VoteChoice.For
                            ? "bg-green-600 text-white"
                            : choice === VoteChoice.Against
                            ? "bg-red-600 text-white"
                            : "bg-gray-600 text-white"
                          : "bg-gray-700 text-gray-300 hover:bg-gray-600"
                      }`}
                    >
                      {choice.charAt(0).toUpperCase() + choice.slice(1)}
                    </button>
                  )
                )}
              </div>
              <button
                onClick={handleVote}
                disabled={!selectedVote || actionLoading}
                className="btn-primary w-full"
              >
                {actionLoading ? "Submitting..." : "Submit Vote"}
              </button>
            </div>
          )}

          {/* Retract Vote */}
          {canRetract && (
            <div>
              <button
                onClick={handleRetract}
                disabled={actionLoading}
                className="btn-danger w-full"
              >
                {actionLoading ? "..." : "Retract Vote"}
              </button>
              <p className="text-sm text-gray-500 mt-2">
                Retract your vote to change it or withdraw your tokens early.
              </p>
            </div>
          )}
        </div>
      )}
    </div>
  );
};
