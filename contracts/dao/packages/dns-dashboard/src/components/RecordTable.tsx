import { DnsRecord } from '@aegis/dns-sdk';

interface RecordTableProps {
  records: DnsRecord[];
  onDelete: (recordId: string) => void;
}

export default function RecordTable({ records, onDelete }: RecordTableProps) {
  if (records.length === 0) {
    return (
      <div className="bg-gray-800 rounded-lg p-8 text-center">
        <p className="text-gray-400">No DNS records found</p>
      </div>
    );
  }

  return (
    <div className="bg-gray-800 rounded-lg overflow-hidden">
      <table className="w-full">
        <thead className="bg-gray-900">
          <tr>
            <th className="px-4 py-3 text-left text-sm font-semibold text-gray-400">Type</th>
            <th className="px-4 py-3 text-left text-sm font-semibold text-gray-400">Name</th>
            <th className="px-4 py-3 text-left text-sm font-semibold text-gray-400">Value</th>
            <th className="px-4 py-3 text-left text-sm font-semibold text-gray-400">TTL</th>
            <th className="px-4 py-3 text-left text-sm font-semibold text-gray-400">Proxied</th>
            <th className="px-4 py-3 text-right text-sm font-semibold text-gray-400">Actions</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-gray-700">
          {records.map((record) => (
            <RecordRow key={record.id} record={record} onDelete={onDelete} />
          ))}
        </tbody>
      </table>
    </div>
  );
}

interface RecordRowProps {
  record: DnsRecord;
  onDelete: (recordId: string) => void;
}

function RecordRow({ record, onDelete }: RecordRowProps) {
  const typeColors: Record<string, string> = {
    A: 'bg-blue-500/20 text-blue-400',
    AAAA: 'bg-purple-500/20 text-purple-400',
    CNAME: 'bg-yellow-500/20 text-yellow-400',
    MX: 'bg-green-500/20 text-green-400',
    TXT: 'bg-orange-500/20 text-orange-400',
    NS: 'bg-pink-500/20 text-pink-400',
    CAA: 'bg-red-500/20 text-red-400',
    SRV: 'bg-cyan-500/20 text-cyan-400',
  };

  return (
    <tr className="hover:bg-gray-700/50 transition-colors">
      <td className="px-4 py-3">
        <span className={`inline-flex items-center px-2.5 py-0.5 rounded text-xs font-bold ${typeColors[record.type] || 'bg-gray-500/20 text-gray-400'}`}>
          {record.type}
        </span>
      </td>
      <td className="px-4 py-3">
        <span className="text-white font-mono text-sm">
          {record.name === '@' ? '@' : record.name}
        </span>
      </td>
      <td className="px-4 py-3">
        <span className="text-gray-300 font-mono text-sm truncate max-w-xs block" title={record.value}>
          {record.priority !== undefined && record.priority !== null && (
            <span className="text-gray-500 mr-2">{record.priority}</span>
          )}
          {record.value}
        </span>
      </td>
      <td className="px-4 py-3">
        <span className="text-gray-400 text-sm">{formatTTL(record.ttl)}</span>
      </td>
      <td className="px-4 py-3">
        {record.proxied ? (
          <span className="badge badge-teal">Yes</span>
        ) : (
          <span className="badge badge-gray">No</span>
        )}
      </td>
      <td className="px-4 py-3 text-right">
        <button
          onClick={() => onDelete(record.id)}
          className="p-1.5 hover:bg-red-500/20 rounded text-gray-400 hover:text-red-400 transition-colors"
          title="Delete record"
        >
          <TrashIcon className="w-4 h-4" />
        </button>
      </td>
    </tr>
  );
}

function formatTTL(seconds: number): string {
  if (seconds >= 86400) {
    const days = seconds / 86400;
    return `${days}d`;
  }
  if (seconds >= 3600) {
    const hours = seconds / 3600;
    return `${hours}h`;
  }
  if (seconds >= 60) {
    const minutes = seconds / 60;
    return `${minutes}m`;
  }
  return `${seconds}s`;
}

function TrashIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
    </svg>
  );
}
