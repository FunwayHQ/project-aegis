import { FC, ReactNode } from "react";
import { Link, useLocation } from "react-router-dom";
import { WalletMultiButton } from "@solana/wallet-adapter-react-ui";
import { useWallet } from "@solana/wallet-adapter-react";

interface LayoutProps {
  children: ReactNode;
}

const navItems = [
  { path: "/", label: "Dashboard" },
  { path: "/proposals", label: "Proposals" },
  { path: "/treasury", label: "Treasury" },
];

export const Layout: FC<LayoutProps> = ({ children }) => {
  const location = useLocation();
  const { connected } = useWallet();

  return (
    <div className="min-h-screen bg-gray-900">
      {/* Header */}
      <header className="border-b border-gray-800">
        <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
          <div className="flex items-center justify-between h-16">
            {/* Logo */}
            <Link to="/" className="flex items-center">
              <img
                src="/aegis-logo.svg"
                alt="AEGIS"
                className="h-12 w-auto"
              />
            </Link>

            {/* Navigation */}
            <nav className="hidden md:flex items-center space-x-8">
              {navItems.map((item) => (
                <Link
                  key={item.path}
                  to={item.path}
                  className={`text-sm font-medium transition-colors ${
                    location.pathname === item.path
                      ? "text-aegis-400"
                      : "text-gray-400 hover:text-white"
                  }`}
                >
                  {item.label}
                </Link>
              ))}
            </nav>

            {/* Wallet Button */}
            <WalletMultiButton className="!bg-aegis-600 hover:!bg-aegis-700 !rounded-lg !h-10" />
          </div>
        </div>
      </header>

      {/* Main Content */}
      <main className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        {!connected ? (
          <div className="text-center py-20">
            <img
              src="/aegis-logo.svg"
              alt="AEGIS"
              className="h-24 w-24 mx-auto mb-6 opacity-50"
            />
            <h2 className="text-2xl font-bold text-gray-400 mb-4">
              Connect Your Wallet
            </h2>
            <p className="text-gray-500 mb-8 max-w-md mx-auto">
              Connect your Solana wallet to participate in AEGIS DAO governance.
              Vote on proposals, create new proposals, and help shape the future
              of the decentralized edge network.
            </p>
            <WalletMultiButton className="!bg-aegis-600 hover:!bg-aegis-700 !rounded-lg" />
          </div>
        ) : (
          children
        )}
      </main>

      {/* Footer */}
      <footer className="border-t border-gray-800 mt-auto">
        <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-6">
          <div className="flex items-center justify-between text-sm text-gray-500">
            <span>AEGIS DAO - Decentralized Edge Network Governance</span>
            <div className="flex items-center space-x-4">
              <a
                href="https://github.com/FunwayHQ/project-aegis"
                target="_blank"
                rel="noopener noreferrer"
                className="hover:text-gray-400 transition-colors"
              >
                GitHub
              </a>
              <span>Devnet</span>
            </div>
          </div>
        </div>
      </footer>
    </div>
  );
};
