import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import StatCard from '../../../components/ddos/StatCard';

describe('StatCard', () => {
  it('renders title and value correctly', () => {
    render(<StatCard title="Total Requests" value={12345} />);

    expect(screen.getByText('Total Requests')).toBeInTheDocument();
    expect(screen.getByText('12,345')).toBeInTheDocument();
  });

  it('renders subtitle when provided', () => {
    render(<StatCard title="Blocked" value={500} subtitle="5% drop rate" />);

    expect(screen.getByText('5% drop rate')).toBeInTheDocument();
  });

  it('renders icon when provided', () => {
    const icon = <svg data-testid="test-icon"></svg>;
    render(<StatCard title="Test" value={100} icon={icon} />);

    expect(screen.getByTestId('test-icon')).toBeInTheDocument();
  });

  it('applies teal color class by default', () => {
    render(<StatCard title="Test" value={100} icon={<span>icon</span>} />);

    const iconContainer = screen.getByText('icon').parentElement;
    expect(iconContainer?.className).toContain('text-teal-400');
    expect(iconContainer?.className).toContain('bg-teal-500/10');
  });

  it('applies red color class when specified', () => {
    render(<StatCard title="Test" value={100} color="red" icon={<span>icon</span>} />);

    const iconContainer = screen.getByText('icon').parentElement;
    expect(iconContainer?.className).toContain('text-red-400');
    expect(iconContainer?.className).toContain('bg-red-500/10');
  });

  it('applies yellow color class when specified', () => {
    render(<StatCard title="Test" value={100} color="yellow" icon={<span>icon</span>} />);

    const iconContainer = screen.getByText('icon').parentElement;
    expect(iconContainer?.className).toContain('text-yellow-400');
  });

  it('applies green color class when specified', () => {
    render(<StatCard title="Test" value={100} color="green" icon={<span>icon</span>} />);

    const iconContainer = screen.getByText('icon').parentElement;
    expect(iconContainer?.className).toContain('text-green-400');
  });

  it('formats number values with locale string', () => {
    render(<StatCard title="Large Number" value={1234567890} />);

    expect(screen.getByText('1,234,567,890')).toBeInTheDocument();
  });

  it('renders string values directly', () => {
    render(<StatCard title="Status" value="Active" />);

    expect(screen.getByText('Active')).toBeInTheDocument();
  });
});
