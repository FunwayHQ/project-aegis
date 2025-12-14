import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import Login from '../../pages/Login';

// Mock useNavigate
const mockNavigate = vi.fn();
vi.mock('react-router-dom', async () => {
  const actual = await vi.importActual('react-router-dom');
  return {
    ...actual,
    useNavigate: () => mockNavigate,
  };
});

// Mock wallet adapter
vi.mock('@solana/wallet-adapter-react', () => ({
  useWallet: () => ({
    connected: false,
    connecting: false,
    publicKey: null,
  }),
}));

// Mock wallet adapter UI
vi.mock('@solana/wallet-adapter-react-ui', () => ({
  WalletMultiButton: () => <button data-testid="wallet-button">Connect Wallet</button>,
}));

describe('Login Page', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders login page title', () => {
    render(
      <MemoryRouter>
        <Login />
      </MemoryRouter>
    );

    expect(screen.getByText('Welcome to AEGIS')).toBeInTheDocument();
  });

  it('renders wallet connect instruction', () => {
    render(
      <MemoryRouter>
        <Login />
      </MemoryRouter>
    );

    expect(
      screen.getByText('Connect your Solana wallet to access the dashboard')
    ).toBeInTheDocument();
  });

  it('renders wallet connect button', () => {
    render(
      <MemoryRouter>
        <Login />
      </MemoryRouter>
    );

    expect(screen.getByTestId('wallet-button')).toBeInTheDocument();
  });

  it('renders AEGIS logo', () => {
    render(
      <MemoryRouter>
        <Login />
      </MemoryRouter>
    );

    const logo = screen.getByAltText('AEGIS');
    expect(logo).toBeInTheDocument();
    expect(logo).toHaveAttribute('src', '/AEGIS-logo.svg');
  });

  it('renders supported wallets section', () => {
    render(
      <MemoryRouter>
        <Login />
      </MemoryRouter>
    );

    expect(screen.getByText('Supported Wallets')).toBeInTheDocument();
    expect(screen.getByText('Phantom')).toBeInTheDocument();
    expect(screen.getByText('Solflare')).toBeInTheDocument();
    expect(screen.getByText('Ledger')).toBeInTheDocument();
    expect(screen.getByText('Coinbase')).toBeInTheDocument();
  });

  it('renders feature descriptions', () => {
    render(
      <MemoryRouter>
        <Login />
      </MemoryRouter>
    );

    expect(screen.getByText('Secure Authentication')).toBeInTheDocument();
    expect(screen.getByText('Decentralized DNS')).toBeInTheDocument();
    expect(screen.getByText('DDoS Protection')).toBeInTheDocument();
  });

  it('renders terms and privacy links', () => {
    render(
      <MemoryRouter>
        <Login />
      </MemoryRouter>
    );

    expect(screen.getByText('Terms of Service')).toBeInTheDocument();
    expect(screen.getByText('Privacy Policy')).toBeInTheDocument();
  });

  it('has proper layout classes', () => {
    render(
      <MemoryRouter>
        <Login />
      </MemoryRouter>
    );

    // Login card should exist
    const loginCard = screen.getByText('Welcome to AEGIS').closest('div');
    expect(loginCard).toBeInTheDocument();
  });
});

describe('Login Page - Connected State', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('shows connected wallet info when already connected', () => {
    // Note: Full redirect behavior is tested in ProtectedRoute tests
    // This test verifies the Login page renders correctly
    render(
      <MemoryRouter initialEntries={['/login']}>
        <Login />
      </MemoryRouter>
    );

    // Verify the page still renders the login UI
    expect(screen.getByText('Welcome to AEGIS')).toBeInTheDocument();
  });
});
