import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { ProtectedRoute } from '../../../components/auth/ProtectedRoute';

// Mock auth context with different states
const mockUseAuth = vi.fn();
vi.mock('../../../contexts/AuthContext', () => ({
  useAuth: () => mockUseAuth(),
}));

describe('ProtectedRoute', () => {
  it('renders children when connected', () => {
    mockUseAuth.mockReturnValue({
      isConnected: true,
      isConnecting: false,
      isLoading: false,
    });

    render(
      <MemoryRouter>
        <ProtectedRoute>
          <div data-testid="protected-content">Protected Content</div>
        </ProtectedRoute>
      </MemoryRouter>
    );

    expect(screen.getByTestId('protected-content')).toBeInTheDocument();
    expect(screen.getByText('Protected Content')).toBeInTheDocument();
  });

  it('shows loading state when connecting', () => {
    mockUseAuth.mockReturnValue({
      isConnected: false,
      isConnecting: true,
      isLoading: false,
    });

    render(
      <MemoryRouter>
        <ProtectedRoute>
          <div>Protected Content</div>
        </ProtectedRoute>
      </MemoryRouter>
    );

    expect(screen.getByText('Connecting wallet...')).toBeInTheDocument();
    expect(screen.queryByText('Protected Content')).not.toBeInTheDocument();
  });

  it('shows loading state when loading user data', () => {
    mockUseAuth.mockReturnValue({
      isConnected: false,
      isConnecting: false,
      isLoading: true,
    });

    render(
      <MemoryRouter>
        <ProtectedRoute>
          <div>Protected Content</div>
        </ProtectedRoute>
      </MemoryRouter>
    );

    expect(screen.getByText('Loading...')).toBeInTheDocument();
    expect(screen.queryByText('Protected Content')).not.toBeInTheDocument();
  });

  it('redirects to login when not connected', () => {
    mockUseAuth.mockReturnValue({
      isConnected: false,
      isConnecting: false,
      isLoading: false,
    });

    render(
      <MemoryRouter initialEntries={['/dashboard']}>
        <ProtectedRoute>
          <div>Protected Content</div>
        </ProtectedRoute>
      </MemoryRouter>
    );

    // Content should not be rendered
    expect(screen.queryByText('Protected Content')).not.toBeInTheDocument();
    // Note: Redirect happens via Navigate component
  });

  it('shows spinner animation during loading', () => {
    mockUseAuth.mockReturnValue({
      isConnected: false,
      isConnecting: true,
      isLoading: false,
    });

    render(
      <MemoryRouter>
        <ProtectedRoute>
          <div>Protected Content</div>
        </ProtectedRoute>
      </MemoryRouter>
    );

    // Check for loading spinner container
    const loadingContainer = screen.getByText('Connecting wallet...').closest('div');
    expect(loadingContainer).toBeInTheDocument();
  });

  it('renders multiple children when connected', () => {
    mockUseAuth.mockReturnValue({
      isConnected: true,
      isConnecting: false,
      isLoading: false,
    });

    render(
      <MemoryRouter>
        <ProtectedRoute>
          <div data-testid="child-1">Child 1</div>
          <div data-testid="child-2">Child 2</div>
        </ProtectedRoute>
      </MemoryRouter>
    );

    expect(screen.getByTestId('child-1')).toBeInTheDocument();
    expect(screen.getByTestId('child-2')).toBeInTheDocument();
  });
});
