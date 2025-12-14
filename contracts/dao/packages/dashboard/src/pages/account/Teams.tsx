import { useState } from 'react';
import { useAuth, TeamMember } from '../../contexts/AuthContext';

export default function Teams() {
  const {
    teams,
    currentTeam,
    switchTeam,
    createTeam,
    inviteToTeam,
    removeFromTeam,
    walletAddress,
  } = useAuth();
  const [showCreateModal, setShowCreateModal] = useState(false);
  const [showInviteModal, setShowInviteModal] = useState(false);
  const [newTeamName, setNewTeamName] = useState('');
  const [inviteAddress, setInviteAddress] = useState('');
  const [inviteRole, setInviteRole] = useState<TeamMember['role']>('member');
  const [isCreating, setIsCreating] = useState(false);

  const handleCreateTeam = async () => {
    if (!newTeamName.trim()) {
      alert('Please enter a team name');
      return;
    }
    setIsCreating(true);
    try {
      const team = await createTeam(newTeamName);
      switchTeam(team.id);
      setShowCreateModal(false);
      setNewTeamName('');
    } catch (error) {
      alert('Failed to create team');
    } finally {
      setIsCreating(false);
    }
  };

  const handleInvite = async () => {
    if (!inviteAddress.trim() || !currentTeam) {
      alert('Please enter a wallet address');
      return;
    }
    try {
      await inviteToTeam(currentTeam.id, inviteAddress, inviteRole);
      setShowInviteModal(false);
      setInviteAddress('');
      setInviteRole('member');
    } catch (error: any) {
      alert(error.message || 'Failed to invite member');
    }
  };

  const handleRemoveMember = async (memberAddress: string) => {
    if (!currentTeam) return;
    if (!confirm('Are you sure you want to remove this member?')) return;
    try {
      await removeFromTeam(currentTeam.id, memberAddress);
    } catch (error) {
      alert('Failed to remove member');
    }
  };

  const isOwner = currentTeam?.ownerId === walletAddress;

  return (
    <div className="max-w-4xl space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-gray-900">Teams</h1>
          <p className="text-gray-500 mt-1">
            Manage your teams and team members
          </p>
        </div>
        <button
          onClick={() => setShowCreateModal(true)}
          className="btn-primary flex items-center gap-2"
        >
          <PlusIcon className="w-4 h-4" />
          Create Team
        </button>
      </div>

      {/* Team Selector */}
      <div className="card p-6">
        <h2 className="text-lg font-semibold text-gray-900 mb-4">Your Teams</h2>
        <div className="grid gap-3">
          {teams.map((team) => (
            <div
              key={team.id}
              onClick={() => switchTeam(team.id)}
              className={`p-4 rounded-lg border-2 cursor-pointer transition-colors ${
                currentTeam?.id === team.id
                  ? 'border-teal-500 bg-teal-50'
                  : 'border-gray-200 hover:border-gray-300 bg-white'
              }`}
            >
              <div className="flex items-center justify-between">
                <div>
                  <h3 className="font-medium text-gray-900">{team.name}</h3>
                  <p className="text-sm text-gray-500">
                    {team.members.length} member{team.members.length !== 1 ? 's' : ''}
                  </p>
                </div>
                {currentTeam?.id === team.id && (
                  <span className="px-2 py-1 bg-teal-100 text-teal-700 rounded text-xs font-medium">
                    Active
                  </span>
                )}
              </div>
            </div>
          ))}
        </div>
      </div>

      {/* Current Team Members */}
      {currentTeam && (
        <div className="card p-6">
          <div className="flex items-center justify-between mb-4">
            <div>
              <h2 className="text-lg font-semibold text-gray-900">
                {currentTeam.name} Members
              </h2>
              <p className="text-sm text-gray-500">
                Manage who has access to this team
              </p>
            </div>
            {isOwner && (
              <button
                onClick={() => setShowInviteModal(true)}
                className="btn-secondary flex items-center gap-2"
              >
                <UserPlusIcon className="w-4 h-4" />
                Invite Member
              </button>
            )}
          </div>

          <div className="overflow-x-auto">
            <table className="w-full">
              <thead>
                <tr className="table-header">
                  <th className="text-left p-3">Member</th>
                  <th className="text-left p-3">Role</th>
                  <th className="text-left p-3">Joined</th>
                  {isOwner && <th className="text-right p-3">Actions</th>}
                </tr>
              </thead>
              <tbody>
                {currentTeam.members.map((member) => (
                  <tr key={member.walletAddress} className="table-row">
                    <td className="p-3">
                      <div>
                        <p className="font-medium text-gray-900">
                          {member.displayName}
                        </p>
                        <p className="text-xs text-gray-500 font-mono">
                          {member.walletAddress.slice(0, 8)}...
                          {member.walletAddress.slice(-8)}
                        </p>
                      </div>
                    </td>
                    <td className="p-3">
                      <RoleBadge role={member.role} />
                    </td>
                    <td className="p-3 text-sm text-gray-500">
                      {new Date(member.joinedAt).toLocaleDateString()}
                    </td>
                    {isOwner && (
                      <td className="p-3 text-right">
                        {member.role !== 'owner' && (
                          <button
                            onClick={() => handleRemoveMember(member.walletAddress)}
                            className="text-red-600 hover:text-red-700 text-sm font-medium"
                          >
                            Remove
                          </button>
                        )}
                      </td>
                    )}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* Role Permissions Info */}
      <div className="card p-6">
        <h2 className="text-lg font-semibold text-gray-900 mb-4">Role Permissions</h2>
        <div className="grid gap-4 md:grid-cols-2">
          <RoleInfo
            role="owner"
            description="Full access. Can manage team members, billing, and all resources."
          />
          <RoleInfo
            role="admin"
            description="Can manage resources and invite members, but cannot manage billing."
          />
          <RoleInfo
            role="member"
            description="Can view and manage DNS zones and DDoS policies."
          />
          <RoleInfo
            role="viewer"
            description="Read-only access to all resources."
          />
        </div>
      </div>

      {/* Create Team Modal */}
      {showCreateModal && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-white rounded-lg p-6 w-full max-w-md border border-gray-200 shadow-lg">
            <h2 className="text-xl font-bold text-gray-900 mb-4">Create Team</h2>
            <div className="space-y-4">
              <div>
                <label className="block text-sm font-medium text-gray-600 mb-1">
                  Team Name
                </label>
                <input
                  type="text"
                  value={newTeamName}
                  onChange={(e) => setNewTeamName(e.target.value)}
                  className="input w-full"
                  placeholder="My Team"
                />
              </div>
              <div className="flex gap-3 pt-4 border-t border-gray-200">
                <button
                  onClick={() => setShowCreateModal(false)}
                  className="btn-secondary flex-1"
                  disabled={isCreating}
                >
                  Cancel
                </button>
                <button
                  onClick={handleCreateTeam}
                  className="btn-primary flex-1"
                  disabled={isCreating}
                >
                  {isCreating ? 'Creating...' : 'Create Team'}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Invite Member Modal */}
      {showInviteModal && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-white rounded-lg p-6 w-full max-w-md border border-gray-200 shadow-lg">
            <h2 className="text-xl font-bold text-gray-900 mb-4">Invite Member</h2>
            <div className="space-y-4">
              <div>
                <label className="block text-sm font-medium text-gray-600 mb-1">
                  Wallet Address
                </label>
                <input
                  type="text"
                  value={inviteAddress}
                  onChange={(e) => setInviteAddress(e.target.value)}
                  className="input w-full font-mono text-sm"
                  placeholder="Enter Solana wallet address"
                />
              </div>
              <div>
                <label className="block text-sm font-medium text-gray-600 mb-1">
                  Role
                </label>
                <select
                  value={inviteRole}
                  onChange={(e) => setInviteRole(e.target.value as TeamMember['role'])}
                  className="select w-full"
                >
                  <option value="viewer">Viewer</option>
                  <option value="member">Member</option>
                  <option value="admin">Admin</option>
                </select>
              </div>
              <div className="flex gap-3 pt-4 border-t border-gray-200">
                <button
                  onClick={() => setShowInviteModal(false)}
                  className="btn-secondary flex-1"
                >
                  Cancel
                </button>
                <button onClick={handleInvite} className="btn-primary flex-1">
                  Send Invite
                </button>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function RoleBadge({ role }: { role: TeamMember['role'] }) {
  const colors: Record<TeamMember['role'], string> = {
    owner: 'bg-purple-100 text-purple-700',
    admin: 'bg-blue-100 text-blue-700',
    member: 'bg-green-100 text-green-700',
    viewer: 'bg-gray-100 text-gray-700',
  };

  return (
    <span
      className={`inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium capitalize ${colors[role]}`}
    >
      {role}
    </span>
  );
}

function RoleInfo({ role, description }: { role: string; description: string }) {
  return (
    <div className="p-4 bg-gray-50 rounded-lg">
      <p className="font-medium text-gray-900 capitalize">{role}</p>
      <p className="text-sm text-gray-500 mt-1">{description}</p>
    </div>
  );
}

function PlusIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
    </svg>
  );
}

function UserPlusIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth={2}
        d="M18 9v3m0 0v3m0-3h3m-3 0h-3m-2-5a4 4 0 11-8 0 4 4 0 018 0zM3 20a6 6 0 0112 0v1H3v-1z"
      />
    </svg>
  );
}
