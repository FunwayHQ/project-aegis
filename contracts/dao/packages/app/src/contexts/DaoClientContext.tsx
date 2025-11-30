import {
  FC,
  ReactNode,
  createContext,
  useContext,
  useMemo,
  useCallback,
} from "react";
import { useConnection, useAnchorWallet } from "@solana/wallet-adapter-react";
import { Wallet } from "@coral-xyz/anchor";
import { DaoClient, DaoConfig, Proposal, ProposalFilter } from "@aegis/dao-sdk";

interface DaoClientContextType {
  client: DaoClient | null;
  isReady: boolean;
  // Helper methods that handle client null checks
  getDaoConfig: () => Promise<DaoConfig | null>;
  getProposals: (filter?: ProposalFilter) => Promise<Proposal[]>;
  getActiveProposals: () => Promise<Proposal[]>;
}

const DaoClientContext = createContext<DaoClientContextType>({
  client: null,
  isReady: false,
  getDaoConfig: async () => null,
  getProposals: async () => [],
  getActiveProposals: async () => [],
});

export const useDaoClient = () => useContext(DaoClientContext);

interface DaoClientProviderProps {
  children: ReactNode;
}

export const DaoClientProvider: FC<DaoClientProviderProps> = ({ children }) => {
  const { connection } = useConnection();
  const wallet = useAnchorWallet();

  const client = useMemo(() => {
    if (!wallet) return null;
    // Cast AnchorWallet to Wallet - they have compatible interfaces for signing
    return new DaoClient(connection, wallet as unknown as Wallet);
  }, [connection, wallet]);

  const isReady = !!client;

  const getDaoConfig = useCallback(async () => {
    if (!client) return null;
    try {
      return await client.getDaoConfig();
    } catch (error) {
      console.error("Failed to fetch DAO config:", error);
      return null;
    }
  }, [client]);

  const getProposals = useCallback(
    async (filter?: ProposalFilter) => {
      if (!client) return [];
      try {
        return await client.getProposals(filter);
      } catch (error) {
        console.error("Failed to fetch proposals:", error);
        return [];
      }
    },
    [client]
  );

  const getActiveProposals = useCallback(async () => {
    if (!client) return [];
    try {
      return await client.getActiveProposals();
    } catch (error) {
      console.error("Failed to fetch active proposals:", error);
      return [];
    }
  }, [client]);

  const value = useMemo(
    () => ({
      client,
      isReady,
      getDaoConfig,
      getProposals,
      getActiveProposals,
    }),
    [client, isReady, getDaoConfig, getProposals, getActiveProposals]
  );

  return (
    <DaoClientContext.Provider value={value}>
      {children}
    </DaoClientContext.Provider>
  );
};
