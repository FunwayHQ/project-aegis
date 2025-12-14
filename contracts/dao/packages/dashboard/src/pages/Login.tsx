import { useEffect } from 'react';
import { useNavigate, useLocation } from 'react-router-dom';
import { useWallet } from '@solana/wallet-adapter-react';
import { WalletMultiButton } from '@solana/wallet-adapter-react-ui';

export default function Login() {
  const { connected } = useWallet();
  const navigate = useNavigate();
  const location = useLocation();

  // Get the intended destination from state, or default to home
  const from = (location.state as { from?: { pathname: string } })?.from?.pathname || '/';

  // Redirect if already connected
  useEffect(() => {
    if (connected) {
      navigate(from, { replace: true });
    }
  }, [connected, navigate, from]);

  return (
    <div className="min-h-screen bg-gray-50 flex flex-col items-center justify-center p-4">
      {/* Logo */}
      <div className="mb-8">
        <img src="/AEGIS-logo.svg" alt="AEGIS" className="h-24" />
      </div>

      {/* Login Card */}
      <div className="w-full max-w-md bg-white rounded-xl shadow-lg border border-gray-200 p-8">
        <div className="text-center mb-8">
          <h1 className="text-2xl font-bold text-gray-900">Welcome to AEGIS</h1>
          <p className="text-gray-500 mt-2">
            Connect your Solana wallet to access the dashboard
          </p>
        </div>

        {/* Wallet Connect Button */}
        <div className="flex justify-center mb-6">
          <WalletMultiButton className="!bg-teal-500 hover:!bg-teal-600 !rounded-lg !h-12 !px-6 !font-medium" />
        </div>

        {/* Supported Wallets */}
        <div className="border-t border-gray-200 pt-6">
          <p className="text-xs text-gray-400 text-center mb-4">
            Supported Wallets
          </p>
          <div className="flex justify-center gap-4">
            <WalletIcon name="Phantom" />
            <WalletIcon name="Solflare" />
            <WalletIcon name="Ledger" />
            <WalletIcon name="Coinbase" />
          </div>
        </div>

        {/* Features */}
        <div className="mt-8 space-y-3">
          <Feature
            icon={<ShieldIcon />}
            title="Secure Authentication"
            description="No passwords - your wallet is your identity"
          />
          <Feature
            icon={<GlobeIcon />}
            title="Decentralized DNS"
            description="Manage your domains on the edge network"
          />
          <Feature
            icon={<LockIcon />}
            title="DDoS Protection"
            description="Enterprise-grade security for your sites"
          />
        </div>
      </div>

      {/* Footer */}
      <div className="mt-8 text-center text-sm text-gray-400">
        <p>
          By connecting, you agree to our{' '}
          <a href="#" className="text-teal-600 hover:underline">
            Terms of Service
          </a>{' '}
          and{' '}
          <a href="#" className="text-teal-600 hover:underline">
            Privacy Policy
          </a>
        </p>
      </div>
    </div>
  );
}

function WalletIcon({ name }: { name: string }) {
  return (
    <div className="flex flex-col items-center gap-1">
      <div className="w-10 h-10 bg-gray-100 rounded-lg flex items-center justify-center">
        <span className="text-xs font-medium text-gray-600">
          {name.slice(0, 2)}
        </span>
      </div>
      <span className="text-xs text-gray-400">{name}</span>
    </div>
  );
}

function Feature({
  icon,
  title,
  description,
}: {
  icon: React.ReactNode;
  title: string;
  description: string;
}) {
  return (
    <div className="flex items-start gap-3">
      <div className="w-8 h-8 bg-teal-100 rounded-lg flex items-center justify-center text-teal-600 flex-shrink-0">
        {icon}
      </div>
      <div>
        <p className="text-sm font-medium text-gray-900">{title}</p>
        <p className="text-xs text-gray-500">{description}</p>
      </div>
    </div>
  );
}

function ShieldIcon() {
  return (
    <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth={2}
        d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"
      />
    </svg>
  );
}

function GlobeIcon() {
  return (
    <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth={2}
        d="M3.055 11H5a2 2 0 012 2v1a2 2 0 002 2 2 2 0 012 2v2.945M8 3.935V5.5A2.5 2.5 0 0010.5 8h.5a2 2 0 012 2 2 2 0 104 0 2 2 0 012-2h1.064M15 20.488V18a2 2 0 012-2h3.064M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
      />
    </svg>
  );
}

function LockIcon() {
  return (
    <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth={2}
        d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z"
      />
    </svg>
  );
}
