import { useState } from 'react';
import { useDns } from '../contexts/DnsContext';
import ZoneCard from '../components/ZoneCard';
import CreateZoneModal from '../components/CreateZoneModal';

export default function Zones() {
  const { zones, loading, error, refreshZones, client } = useDns();
  const [showCreate, setShowCreate] = useState(false);
  const [filter, setFilter] = useState('');

  const filteredZones = zones.filter(zone =>
    zone.domain.toLowerCase().includes(filter.toLowerCase())
  );

  const handleDelete = async (domain: string) => {
    if (!confirm(`Are you sure you want to delete ${domain}?`)) return;

    try {
      await client.deleteZone(domain);
      await refreshZones();
    } catch (err) {
      alert(err instanceof Error ? err.message : 'Failed to delete zone');
    }
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-teal-500"></div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="bg-red-500/10 border border-red-500 rounded-lg p-4 text-red-400">
        <h3 className="font-bold">Error</h3>
        <p>{error}</p>
        <button onClick={refreshZones} className="btn-secondary mt-2">
          Retry
        </button>
      </div>
    );
  }

  return (
    <div>
      {/* Header */}
      <div className="flex justify-between items-center mb-6">
        <div>
          <h1 className="text-2xl font-bold text-white">DNS Zones</h1>
          <p className="text-gray-400 mt-1">{zones.length} zone{zones.length !== 1 ? 's' : ''} configured</p>
        </div>
        <button onClick={() => setShowCreate(true)} className="btn-primary">
          + Add Zone
        </button>
      </div>

      {/* Search */}
      <div className="mb-6">
        <input
          type="text"
          placeholder="Search zones..."
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          className="input max-w-md"
        />
      </div>

      {/* Zone Grid */}
      {filteredZones.length === 0 ? (
        <div className="text-center py-12 bg-gray-800 rounded-lg">
          <p className="text-gray-400">
            {filter ? 'No zones match your search' : 'No zones configured yet'}
          </p>
          {!filter && (
            <button onClick={() => setShowCreate(true)} className="btn-primary mt-4">
              Add Your First Zone
            </button>
          )}
        </div>
      ) : (
        <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
          {filteredZones.map(zone => (
            <ZoneCard key={zone.domain} zone={zone} onDelete={handleDelete} />
          ))}
        </div>
      )}

      {/* Create Modal */}
      {showCreate && (
        <CreateZoneModal
          onClose={() => setShowCreate(false)}
          onCreate={async () => {
            await refreshZones();
            setShowCreate(false);
          }}
        />
      )}
    </div>
  );
}
