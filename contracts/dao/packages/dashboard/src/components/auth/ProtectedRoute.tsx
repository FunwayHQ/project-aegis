import { ReactNode } from 'react';
import { Navigate, useLocation } from 'react-router-dom';
import { useAuth } from '../../contexts/AuthContext';

interface ProtectedRouteProps {
  children: ReactNode;
}

export function ProtectedRoute({ children }: ProtectedRouteProps) {
  const { isConnected, isConnecting, isLoading } = useAuth();
  const location = useLocation();

  // Show loading state while connecting or loading user data
  if (isConnecting || isLoading) {
    return (
      <div className="min-h-screen bg-gray-50 flex items-center justify-center">
        <div className="text-center">
          <div className="animate-spin rounded-full h-12 w-12 border-t-2 border-b-2 border-teal-500 mx-auto"></div>
          <p className="mt-4 text-gray-500">
            {isConnecting ? 'Connecting wallet...' : 'Loading...'}
          </p>
        </div>
      </div>
    );
  }

  // Redirect to login if not connected
  if (!isConnected) {
    return <Navigate to="/login" state={{ from: location }} replace />;
  }

  return <>{children}</>;
}
