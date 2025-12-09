import { render, screen } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import StatCard from '../src/components/StatCard';

describe('StatCard', () => {
  it('renders title and value', () => {
    render(<StatCard title="Total Requests" value={1000} />);

    expect(screen.getByText('Total Requests')).toBeInTheDocument();
    expect(screen.getByText('1,000')).toBeInTheDocument();
  });

  it('renders subtitle when provided', () => {
    render(<StatCard title="Blocked" value={500} subtitle="Today" />);

    expect(screen.getByText('Today')).toBeInTheDocument();
  });

  it('renders icon when provided', () => {
    render(
      <StatCard
        title="Test"
        value={100}
        icon={<span data-testid="test-icon">Icon</span>}
      />
    );

    expect(screen.getByTestId('test-icon')).toBeInTheDocument();
  });

  it('formats large numbers with commas', () => {
    render(<StatCard title="Requests" value={1234567} />);

    expect(screen.getByText('1,234,567')).toBeInTheDocument();
  });

  it('handles string values', () => {
    render(<StatCard title="Status" value="Active" />);

    expect(screen.getByText('Active')).toBeInTheDocument();
  });
});
