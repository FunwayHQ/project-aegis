import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, act } from '@testing-library/react';
import { AuthProvider, useAuth } from '../../contexts/AuthContext';

// Mock wallet adapter
const mockDisconnect = vi.fn();
const mockPublicKey = {
  toBase58: () => 'TestWallet123456789ABCDEF',
};

vi.mock('@solana/wallet-adapter-react', () => ({
  useWallet: () => ({
    connected: true,
    connecting: false,
    publicKey: mockPublicKey,
    disconnect: mockDisconnect,
  }),
}));

// Test component that uses the auth context
function TestConsumer() {
  const auth = useAuth();
  return (
    <div>
      <span data-testid="connected">{auth.isConnected.toString()}</span>
      <span data-testid="wallet">{auth.walletAddress || 'none'}</span>
      <span data-testid="user">{auth.user?.displayName || 'no-user'}</span>
      <span data-testid="team">{auth.currentTeam?.name || 'no-team'}</span>
      <span data-testid="teams-count">{auth.teams.length}</span>
      <span data-testid="api-keys-count">{auth.apiKeys.length}</span>
      <button onClick={() => auth.updateProfile({ displayName: 'Updated Name' })}>
        Update Profile
      </button>
      <button onClick={() => auth.createTeam('New Team')}>Create Team</button>
      <button onClick={() => auth.disconnect()}>Disconnect</button>
    </div>
  );
}

describe('AuthContext', () => {
  beforeEach(() => {
    localStorage.clear();
    vi.clearAllMocks();
  });

  it('throws error when useAuth is used outside AuthProvider', () => {
    // Suppress console.error for this test
    const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});

    expect(() => {
      render(<TestConsumer />);
    }).toThrow('useAuth must be used within an AuthProvider');

    consoleSpy.mockRestore();
  });

  it('provides connection status from wallet', async () => {
    render(
      <AuthProvider>
        <TestConsumer />
      </AuthProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('connected')).toHaveTextContent('true');
    });
  });

  it('provides wallet address when connected', async () => {
    render(
      <AuthProvider>
        <TestConsumer />
      </AuthProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('wallet')).toHaveTextContent('TestWallet123456789ABCDEF');
    });
  });

  it('creates user profile on first connection', async () => {
    render(
      <AuthProvider>
        <TestConsumer />
      </AuthProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('user')).not.toHaveTextContent('no-user');
    });
  });

  it('creates default personal team on first connection', async () => {
    render(
      <AuthProvider>
        <TestConsumer />
      </AuthProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('team')).toHaveTextContent('Personal');
      expect(screen.getByTestId('teams-count')).toHaveTextContent('1');
    });
  });

  it('starts with empty API keys', async () => {
    render(
      <AuthProvider>
        <TestConsumer />
      </AuthProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('api-keys-count')).toHaveTextContent('0');
    });
  });

  it('persists user data to localStorage', async () => {
    render(
      <AuthProvider>
        <TestConsumer />
      </AuthProvider>
    );

    await waitFor(() => {
      const stored = localStorage.getItem('aegis-user-TestWallet123456789ABCDEF');
      expect(stored).toBeTruthy();
      const data = JSON.parse(stored!);
      expect(data.profile).toBeDefined();
      expect(data.teams).toBeDefined();
    });
  });

  it('loads existing user data from localStorage', async () => {
    // Pre-populate localStorage
    const existingData = {
      profile: {
        walletAddress: 'TestWallet123456789ABCDEF',
        displayName: 'Existing User',
        createdAt: Date.now(),
        updatedAt: Date.now(),
      },
      teams: [
        {
          id: 'team-existing',
          name: 'Existing Team',
          ownerId: 'TestWallet123456789ABCDEF',
          members: [],
          createdAt: Date.now(),
        },
      ],
      apiKeys: [],
    };
    localStorage.setItem(
      'aegis-user-TestWallet123456789ABCDEF',
      JSON.stringify(existingData)
    );

    render(
      <AuthProvider>
        <TestConsumer />
      </AuthProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('user')).toHaveTextContent('Existing User');
      expect(screen.getByTestId('team')).toHaveTextContent('Existing Team');
    });
  });

  it('calls wallet disconnect when disconnect is called', async () => {
    render(
      <AuthProvider>
        <TestConsumer />
      </AuthProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('connected')).toHaveTextContent('true');
    });

    await act(async () => {
      screen.getByText('Disconnect').click();
    });

    expect(mockDisconnect).toHaveBeenCalled();
  });
});
