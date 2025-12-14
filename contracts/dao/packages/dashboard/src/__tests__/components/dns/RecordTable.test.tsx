import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import RecordTable from '../../../components/dns/RecordTable';
import type { DnsRecord } from '@aegis/dns-sdk';

const mockRecords: DnsRecord[] = [
  {
    id: 'rec1',
    name: 'www',
    type: 'A',
    value: '192.168.1.1',
    ttl: 300,
    proxied: true,
  },
  {
    id: 'rec2',
    name: '@',
    type: 'AAAA',
    value: '2001:db8::1',
    ttl: 3600,
    proxied: false,
  },
  {
    id: 'rec3',
    name: 'mail',
    type: 'MX',
    value: 'mail.example.com',
    ttl: 86400,
    priority: 10,
    proxied: false,
  },
];

describe('RecordTable', () => {
  it('renders empty state when no records', () => {
    render(<RecordTable records={[]} onDelete={vi.fn()} />);

    expect(screen.getByText('No DNS records found')).toBeInTheDocument();
  });

  it('renders all records', () => {
    render(<RecordTable records={mockRecords} onDelete={vi.fn()} />);

    expect(screen.getByText('www')).toBeInTheDocument();
    expect(screen.getByText('@')).toBeInTheDocument();
    expect(screen.getByText('mail')).toBeInTheDocument();
  });

  it('displays record types with badges', () => {
    render(<RecordTable records={mockRecords} onDelete={vi.fn()} />);

    expect(screen.getByText('A')).toBeInTheDocument();
    expect(screen.getByText('AAAA')).toBeInTheDocument();
    expect(screen.getByText('MX')).toBeInTheDocument();
  });

  it('displays record values', () => {
    render(<RecordTable records={mockRecords} onDelete={vi.fn()} />);

    expect(screen.getByText('192.168.1.1')).toBeInTheDocument();
    expect(screen.getByText('2001:db8::1')).toBeInTheDocument();
  });

  it('displays priority for MX records', () => {
    render(<RecordTable records={mockRecords} onDelete={vi.fn()} />);

    expect(screen.getByText('10')).toBeInTheDocument();
  });

  it('formats TTL correctly', () => {
    render(<RecordTable records={mockRecords} onDelete={vi.fn()} />);

    expect(screen.getByText('5m')).toBeInTheDocument(); // 300s
    expect(screen.getByText('1h')).toBeInTheDocument(); // 3600s
    expect(screen.getByText('1d')).toBeInTheDocument(); // 86400s
  });

  it('displays proxied status correctly', () => {
    render(<RecordTable records={mockRecords} onDelete={vi.fn()} />);

    const yesBadges = screen.getAllByText('Yes');
    const noBadges = screen.getAllByText('No');

    expect(yesBadges.length).toBe(1);
    expect(noBadges.length).toBe(2);
  });

  it('calls onDelete when delete button is clicked', () => {
    const onDelete = vi.fn();
    render(<RecordTable records={mockRecords} onDelete={onDelete} />);

    const deleteButtons = screen.getAllByTitle('Delete record');
    fireEvent.click(deleteButtons[0]);

    expect(onDelete).toHaveBeenCalledWith('rec1');
  });

  it('renders table headers', () => {
    render(<RecordTable records={mockRecords} onDelete={vi.fn()} />);

    expect(screen.getByText('Type')).toBeInTheDocument();
    expect(screen.getByText('Name')).toBeInTheDocument();
    expect(screen.getByText('Value')).toBeInTheDocument();
    expect(screen.getByText('TTL')).toBeInTheDocument();
    expect(screen.getByText('Proxied')).toBeInTheDocument();
    expect(screen.getByText('Actions')).toBeInTheDocument();
  });
});
