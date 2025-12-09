import type { SseEvent } from '@aegis/ddos-sdk';

interface EventLogProps {
  events: SseEvent[];
}

export default function EventLog({ events }: EventLogProps) {
  if (events.length === 0) {
    return (
      <div className="h-64 flex items-center justify-center text-gray-500">
        <div className="text-center">
          <div className="animate-pulse mb-2">
            <svg
              className="w-8 h-8 mx-auto text-gray-600"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"
              />
            </svg>
          </div>
          <p>Waiting for events...</p>
        </div>
      </div>
    );
  }

  return (
    <div className="h-64 overflow-y-auto space-y-2">
      {events.slice(0, 20).map((event, index) => (
        <EventItem key={`${event.timestamp}-${index}`} event={event} />
      ))}
    </div>
  );
}

function EventItem({ event }: { event: SseEvent }) {
  const getEventInfo = (event: SseEvent) => {
    switch (event.type) {
      case 'attack_detected':
        return {
          icon: (
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
            </svg>
          ),
          color: 'text-red-400 bg-red-500/10',
          label: 'Attack Detected',
        };
      case 'attack_mitigated':
        return {
          icon: (
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" />
            </svg>
          ),
          color: 'text-green-400 bg-green-500/10',
          label: 'Attack Mitigated',
        };
      case 'ip_blocked':
        return {
          icon: (
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M18.364 18.364A9 9 0 005.636 5.636m12.728 12.728A9 9 0 015.636 5.636m12.728 12.728L5.636 5.636" />
            </svg>
          ),
          color: 'text-orange-400 bg-orange-500/10',
          label: 'IP Blocked',
        };
      case 'ip_unblocked':
        return {
          icon: (
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
          ),
          color: 'text-blue-400 bg-blue-500/10',
          label: 'IP Unblocked',
        };
      case 'rate_limited':
        return {
          icon: (
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
          ),
          color: 'text-yellow-400 bg-yellow-500/10',
          label: 'Rate Limited',
        };
      case 'policy_updated':
        return {
          icon: (
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
            </svg>
          ),
          color: 'text-aegis-400 bg-aegis-500/10',
          label: 'Policy Updated',
        };
      case 'stats_update':
        return {
          icon: (
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z" />
            </svg>
          ),
          color: 'text-gray-400 bg-gray-500/10',
          label: 'Stats Update',
        };
      default:
        return {
          icon: (
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
          ),
          color: 'text-gray-400 bg-gray-500/10',
          label: event.type,
        };
    }
  };

  const info = getEventInfo(event);

  return (
    <div className="flex items-start gap-3 p-2 rounded-lg hover:bg-gray-800/50 transition-colors">
      <div className={`p-1.5 rounded ${info.color}`}>{info.icon}</div>
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <span className="text-sm font-medium text-white">{info.label}</span>
          <span className="text-xs text-gray-500">
            {new Date(event.timestamp * 1000).toLocaleTimeString()}
          </span>
        </div>
        {event.data !== null && event.data !== undefined && (
          <p className="text-xs text-gray-400 truncate mt-0.5">
            {formatEventData(event.data)}
          </p>
        )}
      </div>
    </div>
  );
}

function formatEventData(data: unknown): string {
  if (data === null || data === undefined) {
    return '';
  }
  if (typeof data === 'object') {
    return JSON.stringify(data).slice(0, 100);
  }
  return String(data).slice(0, 100);
}
