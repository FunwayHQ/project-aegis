import { FC, useEffect, useState } from "react";
import { DaoConfig } from "@aegis/dao-sdk";
import { useDaoClient } from "../contexts/DaoClientContext";
import { StatCard } from "../components/StatCard";
import { formatTokenAmount } from "../utils/format";

export const Treasury: FC = () => {
  const { getDaoConfig, client } = useDaoClient();
  const [config, setConfig] = useState<DaoConfig | null>(null);
  const [balance, setBalance] = useState<bigint | null>(null);
  const [loading, setLoading] = useState(true);
  const [depositAmount, setDepositAmount] = useState("");

  useEffect(() => {
    const fetchData = async () => {
      setLoading(true);
      const configData = await getDaoConfig();
      setConfig(configData);

      if (client && configData) {
        try {
          const treasuryBalance = await client.getTreasuryBalance();
          setBalance(treasuryBalance);
        } catch (err) {
          console.error("Failed to fetch treasury balance:", err);
        }
      }
      setLoading(false);
    };

    fetchData();
  }, [getDaoConfig, client]);

  const handleDeposit = async () => {
    if (!depositAmount) return;
    alert(
      "To deposit to treasury, use the CLI:\n\n" +
        `aegis-dao treasury deposit --amount ${depositAmount} --token-account <your-token-account>`
    );
  };

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
          Treasury Not Available
        </h2>
        <p className="text-gray-500">
          Could not load treasury information. Please try again later.
        </p>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div>
        <h1 className="text-3xl font-bold text-white mb-2">Treasury</h1>
        <p className="text-gray-400">
          Community funds managed by the DAO
        </p>
      </div>

      {/* Stats */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <StatCard
          title="Treasury Balance"
          value={balance !== null ? `${formatTokenAmount(balance)} AEGIS` : "Loading..."}
          subtitle="Current balance"
        />
        <StatCard
          title="Total Deposits"
          value={`${formatTokenAmount(config.totalTreasuryDeposits)} AEGIS`}
          subtitle="All-time deposits"
        />
        <StatCard
          title="Proposal Bond"
          value={`${formatTokenAmount(config.proposalBond)} AEGIS`}
          subtitle="Required to propose"
        />
      </div>

      {/* Treasury Details */}
      <div className="card">
        <h2 className="text-xl font-semibold text-white mb-4">
          Treasury Accounts
        </h2>
        <div className="space-y-4">
          <div className="flex items-center justify-between p-4 bg-gray-700/50 rounded-lg">
            <div>
              <p className="text-sm text-gray-500">Treasury Account</p>
              <p className="text-white font-mono text-sm">
                {config.treasury.toString()}
              </p>
            </div>
            <a
              href={`https://explorer.solana.com/address/${config.treasury.toString()}?cluster=devnet`}
              target="_blank"
              rel="noopener noreferrer"
              className="text-aegis-400 hover:text-aegis-300 text-sm"
            >
              View on Explorer &rarr;
            </a>
          </div>
          <div className="flex items-center justify-between p-4 bg-gray-700/50 rounded-lg">
            <div>
              <p className="text-sm text-gray-500">Bond Escrow</p>
              <p className="text-white font-mono text-sm">
                {config.bondEscrow.toString()}
              </p>
            </div>
            <a
              href={`https://explorer.solana.com/address/${config.bondEscrow.toString()}?cluster=devnet`}
              target="_blank"
              rel="noopener noreferrer"
              className="text-aegis-400 hover:text-aegis-300 text-sm"
            >
              View on Explorer &rarr;
            </a>
          </div>
          <div className="flex items-center justify-between p-4 bg-gray-700/50 rounded-lg">
            <div>
              <p className="text-sm text-gray-500">Vote Vault</p>
              <p className="text-white font-mono text-sm">
                {config.voteVault.toString()}
              </p>
            </div>
            <a
              href={`https://explorer.solana.com/address/${config.voteVault.toString()}?cluster=devnet`}
              target="_blank"
              rel="noopener noreferrer"
              className="text-aegis-400 hover:text-aegis-300 text-sm"
            >
              View on Explorer &rarr;
            </a>
          </div>
        </div>
      </div>

      {/* Deposit Section */}
      <div className="card">
        <h2 className="text-xl font-semibold text-white mb-4">
          Deposit to Treasury
        </h2>
        <p className="text-gray-400 mb-4">
          Contribute AEGIS tokens to the community treasury. Deposits can fund
          development, marketing, and other community initiatives through
          governance proposals.
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
            disabled={!depositAmount}
            className="btn-primary"
          >
            Deposit
          </button>
        </div>
      </div>

      {/* Governance Token */}
      <div className="card">
        <h2 className="text-xl font-semibold text-white mb-4">
          Governance Token
        </h2>
        <div className="flex items-center justify-between p-4 bg-gray-700/50 rounded-lg">
          <div>
            <p className="text-sm text-gray-500">Token Mint</p>
            <p className="text-white font-mono text-sm">
              {config.governanceTokenMint.toString()}
            </p>
          </div>
          <a
            href={`https://explorer.solana.com/address/${config.governanceTokenMint.toString()}?cluster=devnet`}
            target="_blank"
            rel="noopener noreferrer"
            className="text-aegis-400 hover:text-aegis-300 text-sm"
          >
            View on Explorer &rarr;
          </a>
        </div>
      </div>
    </div>
  );
};
