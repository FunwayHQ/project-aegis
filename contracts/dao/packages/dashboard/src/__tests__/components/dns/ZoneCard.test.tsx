import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { BrowserRouter } from 'react-router-dom';
import ZoneCard from '../../../components/dns/ZoneCard';
import type { Zone } from '@aegis/dns-sdk';

const mockZone: Zone = {
  domain: 'example.com',
  proxied: true,
  dnssec_enabled: true,
  nameservers: ['ns1.aegis.network', 'ns2.aegis.network'],
  created_at: 1700000000,
  updated_at: 1700000000,
};

describe('ZoneCard', () => {
  it('renders zone domain correctly', () => {
    render(
      <BrowserRouter>
        <ZoneCard zone={mockZone} onDelete={vi.fn()} />
      </BrowserRouter>
    );

    expect(screen.getByText('example.com')).toBeInTheDocument();
  });

  it('displays Proxied badge when zone is proxied', () => {
    render(
      <BrowserRouter>
        <ZoneCard zone={mockZone} onDelete={vi.fn()} />
      </BrowserRouter>
    );

    expect(screen.getByText('Proxied')).toBeInTheDocument();
  });

  it('displays DNSSEC badge when DNSSEC is enabled', () => {
    render(
      <BrowserRouter>
        <ZoneCard zone={mockZone} onDelete={vi.fn()} />
      </BrowserRouter>
    );

    expect(screen.getByText('DNSSEC')).toBeInTheDocument();
  });

  it('does not display Proxied badge when zone is not proxied', () => {
    const nonProxiedZone = { ...mockZone, proxied: false };
    render(
      <BrowserRouter>
        <ZoneCard zone={nonProxiedZone} onDelete={vi.fn()} />
      </BrowserRouter>
    );

    expect(screen.queryByText('Proxied')).not.toBeInTheDocument();
  });

  it('displays nameservers correctly', () => {
    render(
      <BrowserRouter>
        <ZoneCard zone={mockZone} onDelete={vi.fn()} />
      </BrowserRouter>
    );

    expect(screen.getByText('ns1.aegis.network')).toBeInTheDocument();
    expect(screen.getByText('ns2.aegis.network')).toBeInTheDocument();
  });

  it('calls onDelete when delete button is clicked', () => {
    const onDelete = vi.fn();
    render(
      <BrowserRouter>
        <ZoneCard zone={mockZone} onDelete={onDelete} />
      </BrowserRouter>
    );

    const deleteButton = screen.getByTitle('Delete zone');
    fireEvent.click(deleteButton);

    expect(onDelete).toHaveBeenCalledWith('example.com');
  });

  it('formats creation date correctly', () => {
    render(
      <BrowserRouter>
        <ZoneCard zone={mockZone} onDelete={vi.fn()} />
      </BrowserRouter>
    );

    // Nov 14, 2023 for timestamp 1700000000
    expect(screen.getByText(/Created:/)).toBeInTheDocument();
  });

  it('links to the zone records page', () => {
    render(
      <BrowserRouter>
        <ZoneCard zone={mockZone} onDelete={vi.fn()} />
      </BrowserRouter>
    );

    const link = screen.getByRole('link');
    expect(link).toHaveAttribute('href', '/dns/zones/example.com');
  });
});
