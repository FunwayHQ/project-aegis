import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { BrowserRouter, MemoryRouter } from 'react-router-dom';
import Layout from '../../../components/common/Layout';

// Mock the wallet adapter hooks
vi.mock('@solana/wallet-adapter-react', () => ({
  useWallet: () => ({
    connected: true,
    connecting: false,
    publicKey: { toBase58: () => 'TestWalletAddress123456789' },
    disconnect: vi.fn(),
  }),
  useConnection: () => ({
    connection: {},
  }),
}));

// Mock AuthContext
vi.mock('../../../contexts/AuthContext', () => ({
  useAuth: () => ({
    isConnected: true,
    isConnecting: false,
    walletAddress: 'TestWalletAddress123456789',
    publicKey: { toBase58: () => 'TestWalletAddress123456789' },
    user: {
      walletAddress: 'TestWalletAddress123456789',
      displayName: 'Test User',
      createdAt: Date.now(),
      updatedAt: Date.now(),
    },
    isLoading: false,
    currentTeam: {
      id: 'team-1',
      name: 'Personal',
      ownerId: 'TestWalletAddress123456789',
      members: [],
      createdAt: Date.now(),
    },
    teams: [
      {
        id: 'team-1',
        name: 'Personal',
        ownerId: 'TestWalletAddress123456789',
        members: [],
        createdAt: Date.now(),
      },
    ],
    usage: null,
    apiKeys: [],
    disconnect: vi.fn(),
    updateProfile: vi.fn(),
    createTeam: vi.fn(),
    switchTeam: vi.fn(),
    inviteToTeam: vi.fn(),
    removeFromTeam: vi.fn(),
    createApiKey: vi.fn(),
    revokeApiKey: vi.fn(),
    refreshUsage: vi.fn(),
  }),
}));

describe('Layout', () => {
  it('renders logo', () => {
    render(
      <BrowserRouter>
        <Layout>
          <div>Content</div>
        </Layout>
      </BrowserRouter>
    );

    const logo = screen.getByAltText('AEGIS');
    expect(logo).toBeInTheDocument();
    expect(logo).toHaveAttribute('src', '/AEGIS-logo.svg');
  });

  it('renders Overview navigation link', () => {
    render(
      <BrowserRouter>
        <Layout>
          <div>Content</div>
        </Layout>
      </BrowserRouter>
    );

    expect(screen.getByText('Overview')).toBeInTheDocument();
  });

  it('renders DNS section with links', () => {
    render(
      <BrowserRouter>
        <Layout>
          <div>Content</div>
        </Layout>
      </BrowserRouter>
    );

    expect(screen.getByText('DNS Management')).toBeInTheDocument();
    expect(screen.getByText('Zones')).toBeInTheDocument();
    expect(screen.getByText('DNS Analytics')).toBeInTheDocument();
  });

  it('renders DDoS section with links', () => {
    render(
      <BrowserRouter>
        <Layout>
          <div>Content</div>
        </Layout>
      </BrowserRouter>
    );

    expect(screen.getByText('DDoS Protection')).toBeInTheDocument();
    // Using regex to avoid matching "Dashboard" in logo area
    expect(screen.getAllByText(/Dashboard/)[0]).toBeInTheDocument();
    expect(screen.getByText('Blocklist')).toBeInTheDocument();
    expect(screen.getByText('Policies')).toBeInTheDocument();
    expect(screen.getByText('Statistics')).toBeInTheDocument();
  });

  it('renders Account section with links', () => {
    render(
      <BrowserRouter>
        <Layout>
          <div>Content</div>
        </Layout>
      </BrowserRouter>
    );

    expect(screen.getByText('Account')).toBeInTheDocument();
    expect(screen.getByText('Profile')).toBeInTheDocument();
    expect(screen.getByText('Teams')).toBeInTheDocument();
    expect(screen.getByText('Billing')).toBeInTheDocument();
  });

  it('renders Settings link', () => {
    render(
      <BrowserRouter>
        <Layout>
          <div>Content</div>
        </Layout>
      </BrowserRouter>
    );

    expect(screen.getByText('Settings')).toBeInTheDocument();
  });

  it('renders children content', () => {
    render(
      <BrowserRouter>
        <Layout>
          <div data-testid="test-content">Test Content</div>
        </Layout>
      </BrowserRouter>
    );

    expect(screen.getByTestId('test-content')).toBeInTheDocument();
    expect(screen.getByText('Test Content')).toBeInTheDocument();
  });

  it('navigation links point to correct paths', () => {
    render(
      <BrowserRouter>
        <Layout>
          <div>Content</div>
        </Layout>
      </BrowserRouter>
    );

    const overviewLink = screen.getByText('Overview').closest('a');
    expect(overviewLink).toHaveAttribute('href', '/');

    const zonesLink = screen.getByText('Zones').closest('a');
    expect(zonesLink).toHaveAttribute('href', '/dns/zones');

    const analyticsLink = screen.getByText('DNS Analytics').closest('a');
    expect(analyticsLink).toHaveAttribute('href', '/dns/analytics');

    const blocklistLink = screen.getByText('Blocklist').closest('a');
    expect(blocklistLink).toHaveAttribute('href', '/ddos/blocklist');

    const policiesLink = screen.getByText('Policies').closest('a');
    expect(policiesLink).toHaveAttribute('href', '/ddos/policies');

    const statisticsLink = screen.getByText('Statistics').closest('a');
    expect(statisticsLink).toHaveAttribute('href', '/ddos/statistics');

    const settingsLink = screen.getByText('Settings').closest('a');
    expect(settingsLink).toHaveAttribute('href', '/settings');

    // Account links
    const profileLink = screen.getByText('Profile').closest('a');
    expect(profileLink).toHaveAttribute('href', '/account/profile');

    const teamsLink = screen.getByText('Teams').closest('a');
    expect(teamsLink).toHaveAttribute('href', '/account/teams');

    const billingLink = screen.getByText('Billing').closest('a');
    expect(billingLink).toHaveAttribute('href', '/account/billing');
  });

  it('highlights active navigation link', () => {
    render(
      <MemoryRouter initialEntries={['/dns/zones']}>
        <Layout>
          <div>Content</div>
        </Layout>
      </MemoryRouter>
    );

    const zonesLink = screen.getByText('Zones').closest('a');
    expect(zonesLink?.className).toContain('active');
  });

  it('displays user info in header', () => {
    render(
      <BrowserRouter>
        <Layout>
          <div>Content</div>
        </Layout>
      </BrowserRouter>
    );

    // Check that user display name initial is shown
    expect(screen.getByText('T')).toBeInTheDocument(); // First letter of "Test User"
    expect(screen.getByText('Test User')).toBeInTheDocument();
  });

  it('displays team selector', () => {
    render(
      <BrowserRouter>
        <Layout>
          <div>Content</div>
        </Layout>
      </BrowserRouter>
    );

    expect(screen.getByText('Personal')).toBeInTheDocument();
  });
});
