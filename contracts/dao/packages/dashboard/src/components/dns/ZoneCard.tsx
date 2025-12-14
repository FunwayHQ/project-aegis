import { Link } from 'react-router-dom';
import { Zone } from '@aegis/dns-sdk';

interface ZoneCardProps {
  zone: Zone;
  onDelete: (domain: string) => void;
}

export default function ZoneCard({ zone, onDelete }: ZoneCardProps) {
  const handleDelete = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    onDelete(zone.domain);
  };

  return (
    <Link to={`/dns/zones/${zone.domain}`} className="zone-card block bg-gray-800 rounded-lg p-6 border border-gray-700 hover:border-teal-500/50">
      <div className="flex justify-between items-start">
        <div>
          <h3 className="text-xl font-bold text-white">{zone.domain}</h3>
          <div className="flex gap-2 mt-2">
            {zone.proxied && (
              <span className="badge badge-teal">Proxied</span>
            )}
            {zone.dnssec_enabled && (
              <span className="badge badge-green">DNSSEC</span>
            )}
          </div>
        </div>
        <div className="flex gap-1">
          <button
            onClick={handleDelete}
            className="p-2 hover:bg-red-500/20 rounded text-gray-400 hover:text-red-400 transition-colors"
            title="Delete zone"
          >
            <TrashIcon className="w-5 h-5" />
          </button>
        </div>
      </div>

      <div className="mt-4 pt-4 border-t border-gray-700">
        <p className="text-sm text-gray-400 mb-2">Nameservers:</p>
        <div className="space-y-1">
          {zone.nameservers.map(ns => (
            <code key={ns} className="block text-sm text-teal-400 font-mono">
              {ns}
            </code>
          ))}
        </div>
      </div>

      <div className="mt-4 flex justify-between text-sm text-gray-500">
        <span>Created: {formatDate(zone.created_at)}</span>
        <span className="text-teal-400 hover:text-teal-300">Manage Records &rarr;</span>
      </div>
    </Link>
  );
}

function formatDate(timestamp: number): string {
  return new Date(timestamp * 1000).toLocaleDateString('en-US', {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  });
}

function TrashIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
    </svg>
  );
}
