import {
  createContext,
  useContext,
  useState,
  useEffect,
  useMemo,
  ReactNode,
} from 'react';
import { useWallet } from '@solana/wallet-adapter-react';
import { PublicKey } from '@solana/web3.js';

export interface UserProfile {
  walletAddress: string;
  displayName: string;
  email?: string;
  avatarUrl?: string;
  createdAt: number;
  updatedAt: number;
}

export interface Team {
  id: string;
  name: string;
  ownerId: string;
  members: TeamMember[];
  createdAt: number;
}

export interface TeamMember {
  walletAddress: string;
  displayName: string;
  role: 'owner' | 'admin' | 'member' | 'viewer';
  joinedAt: number;
}

export interface UsageStats {
  dnsQueries: number;
  dnsZones: number;
  ddosRequestsBlocked: number;
  bandwidthUsed: number; // in bytes
  currentPlan: 'free' | 'pro' | 'enterprise';
  limits: {
    dnsZones: number;
    dnsQueriesPerMonth: number;
    bandwidthPerMonth: number;
  };
}

export interface ApiKey {
  id: string;
  name: string;
  prefix: string; // First 8 chars for identification
  permissions: ('dns:read' | 'dns:write' | 'ddos:read' | 'ddos:write')[];
  createdAt: number;
  lastUsed?: number;
  expiresAt?: number;
}

interface AuthContextType {
  // Connection state
  isConnected: boolean;
  isConnecting: boolean;
  walletAddress: string | null;
  publicKey: PublicKey | null;

  // User state
  user: UserProfile | null;
  isLoading: boolean;

  // Team state
  currentTeam: Team | null;
  teams: Team[];

  // Usage
  usage: UsageStats | null;

  // API Keys
  apiKeys: ApiKey[];

  // Actions
  disconnect: () => Promise<void>;
  updateProfile: (updates: Partial<UserProfile>) => Promise<void>;
  createTeam: (name: string) => Promise<Team>;
  switchTeam: (teamId: string) => void;
  inviteToTeam: (teamId: string, walletAddress: string, role: TeamMember['role']) => Promise<void>;
  removeFromTeam: (teamId: string, walletAddress: string) => Promise<void>;
  createApiKey: (name: string, permissions: ApiKey['permissions']) => Promise<{ key: string; apiKey: ApiKey }>;
  revokeApiKey: (keyId: string) => Promise<void>;
  refreshUsage: () => Promise<void>;
}

const AuthContext = createContext<AuthContextType | null>(null);

export function useAuth() {
  const context = useContext(AuthContext);
  if (!context) {
    throw new Error('useAuth must be used within an AuthProvider');
  }
  return context;
}

interface AuthProviderProps {
  children: ReactNode;
}

export function AuthProvider({ children }: AuthProviderProps) {
  const { connected, connecting, publicKey, disconnect: walletDisconnect } = useWallet();

  const [user, setUser] = useState<UserProfile | null>(null);
  const [teams, setTeams] = useState<Team[]>([]);
  const [currentTeam, setCurrentTeam] = useState<Team | null>(null);
  const [usage, setUsage] = useState<UsageStats | null>(null);
  const [apiKeys, setApiKeys] = useState<ApiKey[]>([]);
  const [isLoading, setIsLoading] = useState(false);

  const walletAddress = publicKey?.toBase58() ?? null;

  // Load user data when wallet connects
  useEffect(() => {
    if (connected && walletAddress) {
      loadUserData(walletAddress);
    } else {
      // Clear state on disconnect
      setUser(null);
      setTeams([]);
      setCurrentTeam(null);
      setUsage(null);
      setApiKeys([]);
    }
  }, [connected, walletAddress]);

  const loadUserData = async (address: string) => {
    setIsLoading(true);
    try {
      // Try to load existing user from localStorage (mock backend)
      const storedUser = localStorage.getItem(`aegis-user-${address}`);
      if (storedUser) {
        const userData = JSON.parse(storedUser);
        setUser(userData.profile);
        setTeams(userData.teams || []);
        setCurrentTeam(userData.teams?.[0] || null);
        setApiKeys(userData.apiKeys || []);
      } else {
        // Create new user profile
        const newProfile: UserProfile = {
          walletAddress: address,
          displayName: `${address.slice(0, 4)}...${address.slice(-4)}`,
          createdAt: Date.now(),
          updatedAt: Date.now(),
        };

        // Create default personal team
        const personalTeam: Team = {
          id: `team-${address.slice(0, 8)}`,
          name: 'Personal',
          ownerId: address,
          members: [
            {
              walletAddress: address,
              displayName: newProfile.displayName,
              role: 'owner',
              joinedAt: Date.now(),
            },
          ],
          createdAt: Date.now(),
        };

        setUser(newProfile);
        setTeams([personalTeam]);
        setCurrentTeam(personalTeam);

        // Save to localStorage
        saveUserData(address, newProfile, [personalTeam], []);
      }

      // Load usage stats
      await refreshUsage();
    } catch (error) {
      console.error('Failed to load user data:', error);
    } finally {
      setIsLoading(false);
    }
  };

  const saveUserData = (
    address: string,
    profile: UserProfile,
    userTeams: Team[],
    keys: ApiKey[]
  ) => {
    localStorage.setItem(
      `aegis-user-${address}`,
      JSON.stringify({
        profile,
        teams: userTeams,
        apiKeys: keys,
      })
    );
  };

  const disconnect = async () => {
    await walletDisconnect();
  };

  const updateProfile = async (updates: Partial<UserProfile>) => {
    if (!user || !walletAddress) return;

    const updatedProfile = {
      ...user,
      ...updates,
      updatedAt: Date.now(),
    };
    setUser(updatedProfile);
    saveUserData(walletAddress, updatedProfile, teams, apiKeys);
  };

  const createTeam = async (name: string): Promise<Team> => {
    if (!walletAddress || !user) throw new Error('Not connected');

    const newTeam: Team = {
      id: `team-${Date.now()}`,
      name,
      ownerId: walletAddress,
      members: [
        {
          walletAddress,
          displayName: user.displayName,
          role: 'owner',
          joinedAt: Date.now(),
        },
      ],
      createdAt: Date.now(),
    };

    const updatedTeams = [...teams, newTeam];
    setTeams(updatedTeams);
    saveUserData(walletAddress, user, updatedTeams, apiKeys);
    return newTeam;
  };

  const switchTeam = (teamId: string) => {
    const team = teams.find((t) => t.id === teamId);
    if (team) {
      setCurrentTeam(team);
    }
  };

  const inviteToTeam = async (
    teamId: string,
    memberAddress: string,
    role: TeamMember['role']
  ) => {
    if (!walletAddress || !user) throw new Error('Not connected');

    const updatedTeams = teams.map((team) => {
      if (team.id === teamId) {
        // Check if already a member
        if (team.members.some((m) => m.walletAddress === memberAddress)) {
          throw new Error('Already a member');
        }
        return {
          ...team,
          members: [
            ...team.members,
            {
              walletAddress: memberAddress,
              displayName: `${memberAddress.slice(0, 4)}...${memberAddress.slice(-4)}`,
              role,
              joinedAt: Date.now(),
            },
          ],
        };
      }
      return team;
    });

    setTeams(updatedTeams);
    if (currentTeam?.id === teamId) {
      setCurrentTeam(updatedTeams.find((t) => t.id === teamId) || null);
    }
    saveUserData(walletAddress, user, updatedTeams, apiKeys);
  };

  const removeFromTeam = async (teamId: string, memberAddress: string) => {
    if (!walletAddress || !user) throw new Error('Not connected');

    const updatedTeams = teams.map((team) => {
      if (team.id === teamId) {
        return {
          ...team,
          members: team.members.filter((m) => m.walletAddress !== memberAddress),
        };
      }
      return team;
    });

    setTeams(updatedTeams);
    if (currentTeam?.id === teamId) {
      setCurrentTeam(updatedTeams.find((t) => t.id === teamId) || null);
    }
    saveUserData(walletAddress, user, updatedTeams, apiKeys);
  };

  const createApiKey = async (
    name: string,
    permissions: ApiKey['permissions']
  ): Promise<{ key: string; apiKey: ApiKey }> => {
    if (!walletAddress || !user) throw new Error('Not connected');

    // Generate a random API key
    const keyBytes = new Uint8Array(32);
    crypto.getRandomValues(keyBytes);
    const fullKey = `aegis_${Array.from(keyBytes)
      .map((b) => b.toString(16).padStart(2, '0'))
      .join('')}`;

    const apiKey: ApiKey = {
      id: `key-${Date.now()}`,
      name,
      prefix: fullKey.slice(0, 14), // "aegis_" + 8 chars
      permissions,
      createdAt: Date.now(),
    };

    const updatedKeys = [...apiKeys, apiKey];
    setApiKeys(updatedKeys);
    saveUserData(walletAddress, user, teams, updatedKeys);

    // Return full key only once (won't be stored)
    return { key: fullKey, apiKey };
  };

  const revokeApiKey = async (keyId: string) => {
    if (!walletAddress || !user) throw new Error('Not connected');

    const updatedKeys = apiKeys.filter((k) => k.id !== keyId);
    setApiKeys(updatedKeys);
    saveUserData(walletAddress, user, teams, updatedKeys);
  };

  const refreshUsage = async () => {
    // Mock usage data - in production, fetch from API
    setUsage({
      dnsQueries: Math.floor(Math.random() * 100000),
      dnsZones: Math.floor(Math.random() * 10),
      ddosRequestsBlocked: Math.floor(Math.random() * 50000),
      bandwidthUsed: Math.floor(Math.random() * 1024 * 1024 * 1024 * 10), // Up to 10 GB
      currentPlan: 'free',
      limits: {
        dnsZones: 5,
        dnsQueriesPerMonth: 1000000,
        bandwidthPerMonth: 1024 * 1024 * 1024 * 100, // 100 GB
      },
    });
  };

  const value = useMemo(
    () => ({
      isConnected: connected,
      isConnecting: connecting,
      walletAddress,
      publicKey,
      user,
      isLoading,
      currentTeam,
      teams,
      usage,
      apiKeys,
      disconnect,
      updateProfile,
      createTeam,
      switchTeam,
      inviteToTeam,
      removeFromTeam,
      createApiKey,
      revokeApiKey,
      refreshUsage,
    }),
    [
      connected,
      connecting,
      walletAddress,
      publicKey,
      user,
      isLoading,
      currentTeam,
      teams,
      usage,
      apiKeys,
    ]
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}
