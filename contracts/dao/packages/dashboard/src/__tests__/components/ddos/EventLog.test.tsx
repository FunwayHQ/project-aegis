import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import EventLog from '../../../components/ddos/EventLog';
import type { SseEvent } from '@aegis/ddos-sdk';

const mockEvents: SseEvent[] = [
  {
    type: 'attack_detected',
    timestamp: 1700000000,
    data: { source_ip: '192.168.1.100' },
  },
  {
    type: 'attack_mitigated',
    timestamp: 1700000100,
    data: null,
  },
  {
    type: 'ip_blocked',
    timestamp: 1700000200,
    data: { ip: '10.0.0.1' },
  },
  {
    type: 'rate_limited',
    timestamp: 1700000300,
    data: null,
  },
  {
    type: 'policy_updated',
    timestamp: 1700000400,
    data: null,
  },
];

describe('EventLog', () => {
  it('renders empty state when no events', () => {
    render(<EventLog events={[]} />);

    expect(screen.getByText('Waiting for events...')).toBeInTheDocument();
  });

  it('renders all event types', () => {
    render(<EventLog events={mockEvents} />);

    expect(screen.getByText('Attack Detected')).toBeInTheDocument();
    expect(screen.getByText('Attack Mitigated')).toBeInTheDocument();
    expect(screen.getByText('IP Blocked')).toBeInTheDocument();
    expect(screen.getByText('Rate Limited')).toBeInTheDocument();
    expect(screen.getByText('Policy Updated')).toBeInTheDocument();
  });

  it('displays event data when available', () => {
    render(<EventLog events={mockEvents} />);

    // Event data should be displayed
    expect(screen.getByText(/192\.168\.1\.100/)).toBeInTheDocument();
  });

  it('limits displayed events to 20', () => {
    const manyEvents: SseEvent[] = Array.from({ length: 30 }, (_, i) => ({
      type: 'stats_update',
      timestamp: 1700000000 + i,
      data: null,
    }));

    render(<EventLog events={manyEvents} />);

    const eventLabels = screen.getAllByText('Stats Update');
    expect(eventLabels.length).toBeLessThanOrEqual(20);
  });

  it('displays timestamps for events', () => {
    render(<EventLog events={[mockEvents[0]]} />);

    // Should show time, the exact format depends on locale
    const timeElements = document.querySelectorAll('.text-xs.text-gray-500');
    expect(timeElements.length).toBeGreaterThan(0);
  });

  it('handles ip_unblocked event type', () => {
    const unblockEvent: SseEvent = {
      type: 'ip_unblocked',
      timestamp: 1700000000,
      data: null,
    };

    render(<EventLog events={[unblockEvent]} />);

    expect(screen.getByText('IP Unblocked')).toBeInTheDocument();
  });

  it('handles unknown event types gracefully', () => {
    const unknownEvent: SseEvent = {
      type: 'custom_event' as never,
      timestamp: 1700000000,
      data: null,
    };

    render(<EventLog events={[unknownEvent]} />);

    expect(screen.getByText('custom_event')).toBeInTheDocument();
  });

  it('truncates long event data', () => {
    const longDataEvent: SseEvent = {
      type: 'attack_detected',
      timestamp: 1700000000,
      data: { longField: 'a'.repeat(200) },
    };

    render(<EventLog events={[longDataEvent]} />);

    // The data display should be truncated (max 100 chars based on formatEventData)
    const dataElement = screen.getByText(/aaaa/);
    expect(dataElement.textContent?.length).toBeLessThanOrEqual(100);
  });
});
