import { useState, useEffect } from 'react';
import { useParams, Link } from 'react-router-dom';
import { useDns } from '../contexts/DnsContext';
import { DnsRecord, Zone, DnssecStatus } from '@aegis/dns-sdk';
import RecordTable from '../components/RecordTable';
import CreateRecordModal from '../components/CreateRecordModal';

export default function Records() {
  const { domain } = useParams<{ domain: string }>();
  const { client } = useDns();
  const [zone, setZone] = useState<Zone | null>(null);
  const [records, setRecords] = useState<DnsRecord[]>([]);
  const [dnssec, setDnssec] = useState<DnssecStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [filter, setFilter] = useState('');

  const loadData = async () => {
    if (!domain) return;

    try {
      setLoading(true);
      setError(null);

      const [zoneData, recordsData, dnssecData] = await Promise.all([
        client.getZone(domain),
        client.listRecords(domain),
        client.getDnssecStatus(domain).catch(() => null),
      ]);

      setZone(zoneData);
      setRecords(recordsData);
      setDnssec(dnssecData);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load records');
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadData();
  }, [domain]);

  const handleDeleteRecord = async (recordId: string) => {
    if (!domain || !confirm('Are you sure you want to delete this record?')) return;

    try {
      await client.deleteRecord(domain, recordId);
      await loadData();
    } catch (err) {
      alert(err instanceof Error ? err.message : 'Failed to delete record');
    }
  };

  const handleToggleDnssec = async () => {
    if (!domain) return;

    try {
      if (dnssec?.enabled) {
        await client.disableDnssec(domain);
      } else {
        await client.enableDnssec(domain);
      }
      await loadData();
    } catch (err) {
      alert(err instanceof Error ? err.message : 'Failed to toggle DNSSEC');
    }
  };

  const filteredRecords = records.filter(record =>
    record.name.toLowerCase().includes(filter.toLowerCase()) ||
    record.type.toLowerCase().includes(filter.toLowerCase()) ||
    record.value.toLowerCase().includes(filter.toLowerCase())
  );

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-teal-500"></div>
      </div>
    );
  }

  if (error || !zone) {
    return (
      <div className="bg-red-500/10 border border-red-500 rounded-lg p-4 text-red-400">
        <h3 className="font-bold">Error</h3>
        <p>{error || 'Zone not found'}</p>
        <Link to="/zones" className="btn-secondary mt-2 inline-block">
          Back to Zones
        </Link>
      </div>
    );
  }

  return (
    <div>
      {/* Breadcrumb */}
      <div className="mb-4">
        <Link to="/zones" className="text-teal-400 hover:text-teal-300">
          Zones
        </Link>
        <span className="text-gray-500 mx-2">/</span>
        <span className="text-white">{domain}</span>
      </div>

      {/* Zone Header */}
      <div className="bg-gray-800 rounded-lg p-6 mb-6">
        <div className="flex justify-between items-start">
          <div>
            <h1 className="text-2xl font-bold text-white">{zone.domain}</h1>
            <div className="flex gap-2 mt-2">
              {zone.proxied && <span className="badge badge-teal">Proxied</span>}
              {dnssec?.enabled && <span className="badge badge-green">DNSSEC</span>}
            </div>
          </div>
          <div className="flex gap-2">
            <button
              onClick={handleToggleDnssec}
              className={dnssec?.enabled ? 'btn-secondary' : 'btn-primary'}
            >
              {dnssec?.enabled ? 'Disable DNSSEC' : 'Enable DNSSEC'}
            </button>
            <button onClick={() => setShowCreate(true)} className="btn-primary">
              + Add Record
            </button>
          </div>
        </div>

        {/* Nameservers */}
        <div className="mt-4 pt-4 border-t border-gray-700">
          <p className="text-sm text-gray-400 mb-2">Nameservers (update at your registrar):</p>
          <div className="flex flex-wrap gap-2">
            {zone.nameservers.map(ns => (
              <code key={ns} className="bg-gray-900 px-2 py-1 rounded text-sm text-teal-400">
                {ns}
              </code>
            ))}
          </div>
        </div>

        {/* DS Record */}
        {dnssec?.enabled && dnssec.ds_record && (
          <div className="mt-4 pt-4 border-t border-gray-700">
            <p className="text-sm text-gray-400 mb-2">DS Record (add to registrar for DNSSEC):</p>
            <code className="block bg-gray-900 px-3 py-2 rounded text-sm text-green-400 break-all">
              {dnssec.ds_record}
            </code>
          </div>
        )}
      </div>

      {/* Records Section */}
      <div className="flex justify-between items-center mb-4">
        <h2 className="text-xl font-bold text-white">DNS Records</h2>
        <input
          type="text"
          placeholder="Filter records..."
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          className="input max-w-xs"
        />
      </div>

      <RecordTable records={filteredRecords} onDelete={handleDeleteRecord} />

      {/* Create Modal */}
      {showCreate && domain && (
        <CreateRecordModal
          domain={domain}
          onClose={() => setShowCreate(false)}
          onCreate={async () => {
            await loadData();
            setShowCreate(false);
          }}
        />
      )}
    </div>
  );
}
