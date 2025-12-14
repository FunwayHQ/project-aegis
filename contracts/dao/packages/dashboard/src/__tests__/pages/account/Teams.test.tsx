import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import Teams from '../../../pages/account/Teams';

// Mock window.confirm
const mockConfirm = vi.fn();
window.confirm = mockConfirm;

// Mock window.alert
const mockAlert = vi.fn();
window.alert = mockAlert;

// Mock auth context
const mockSwitchTeam = vi.fn();
const mockCreateTeam = vi.fn().mockResolvedValue({
  id: 'new-team',
  name: 'New Team',
  ownerId: 'TestWallet123456789ABCDEF',
  members: [],
  createdAt: Date.now(),
});
const mockInviteToTeam = vi.fn();
const mockRemoveFromTeam = vi.fn();

vi.mock('../../../contexts/AuthContext', () => ({
  useAuth: () => ({
    teams: [
      {
        id: 'team-personal',
        name: 'Personal',
        ownerId: 'TestWallet123456789ABCDEF',
        members: [
          {
            walletAddress: 'TestWallet123456789ABCDEF',
            displayName: 'Test User',
            role: 'owner',
            joinedAt: Date.now() - 86400000,
          },
        ],
        createdAt: Date.now() - 86400000,
      },
      {
        id: 'team-work',
        name: 'Work Team',
        ownerId: 'OtherWallet',
        members: [
          {
            walletAddress: 'OtherWallet',
            displayName: 'Other User',
            role: 'owner',
            joinedAt: Date.now() - 86400000,
          },
          {
            walletAddress: 'TestWallet123456789ABCDEF',
            displayName: 'Test User',
            role: 'member',
            joinedAt: Date.now() - 3600000,
          },
        ],
        createdAt: Date.now() - 86400000,
      },
    ],
    currentTeam: {
      id: 'team-personal',
      name: 'Personal',
      ownerId: 'TestWallet123456789ABCDEF',
      members: [
        {
          walletAddress: 'TestWallet123456789ABCDEF',
          displayName: 'Test User',
          role: 'owner',
          joinedAt: Date.now() - 86400000,
        },
      ],
      createdAt: Date.now() - 86400000,
    },
    walletAddress: 'TestWallet123456789ABCDEF',
    switchTeam: mockSwitchTeam,
    createTeam: mockCreateTeam,
    inviteToTeam: mockInviteToTeam,
    removeFromTeam: mockRemoveFromTeam,
  }),
}));

describe('Teams Page', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders page title', () => {
    render(<Teams />);
    expect(screen.getByText('Teams')).toBeInTheDocument();
  });

  it('renders page description', () => {
    render(<Teams />);
    expect(
      screen.getByText('Manage your teams and team members')
    ).toBeInTheDocument();
  });

  it('displays teams list', () => {
    render(<Teams />);
    expect(screen.getByText('Personal')).toBeInTheDocument();
    expect(screen.getByText('Work Team')).toBeInTheDocument();
  });

  it('shows active badge for current team', () => {
    render(<Teams />);
    expect(screen.getByText('Active')).toBeInTheDocument();
  });

  it('displays member count for teams', () => {
    render(<Teams />);
    expect(screen.getByText('1 member')).toBeInTheDocument();
    expect(screen.getByText('2 members')).toBeInTheDocument();
  });

  it('has Create Team button', () => {
    render(<Teams />);
    expect(screen.getByText('Create Team')).toBeInTheDocument();
  });

  it('opens create team modal when button clicked', () => {
    render(<Teams />);
    fireEvent.click(screen.getByText('Create Team'));
    expect(screen.getByText('Team Name')).toBeInTheDocument();
    expect(screen.getByPlaceholderText('My Team')).toBeInTheDocument();
  });

  it('creates team with name', async () => {
    render(<Teams />);

    // Click the header "Create Team" button to open modal
    const headerButton = screen.getAllByRole('button', { name: /Create Team/i })[0];
    fireEvent.click(headerButton);

    const nameInput = screen.getByPlaceholderText('My Team');
    fireEvent.change(nameInput, { target: { value: 'New Project Team' } });

    // Get the modal's Create Team button (the second one)
    const modalButton = screen.getAllByRole('button', { name: /Create Team/i })[1];
    fireEvent.click(modalButton);

    await waitFor(() => {
      expect(mockCreateTeam).toHaveBeenCalledWith('New Project Team');
    });
  });

  it('shows error alert when creating team without name', () => {
    render(<Teams />);

    // Click the header "Create Team" button to open modal
    const headerButton = screen.getAllByRole('button', { name: /Create Team/i })[0];
    fireEvent.click(headerButton);

    // Clear the input (just in case)
    const nameInput = screen.getByPlaceholderText('My Team');
    fireEvent.change(nameInput, { target: { value: '' } });

    // Get the modal's Create Team button (the second one)
    const modalButton = screen.getAllByRole('button', { name: /Create Team/i })[1];
    fireEvent.click(modalButton);

    expect(mockAlert).toHaveBeenCalledWith('Please enter a team name');
  });

  it('closes create modal with Cancel button', () => {
    render(<Teams />);
    fireEvent.click(screen.getByText('Create Team'));

    expect(screen.getByPlaceholderText('My Team')).toBeInTheDocument();

    fireEvent.click(screen.getByText('Cancel'));

    expect(screen.queryByPlaceholderText('My Team')).not.toBeInTheDocument();
  });

  it('displays current team members table', () => {
    render(<Teams />);
    expect(screen.getByText('Personal Members')).toBeInTheDocument();
    expect(screen.getByText('Test User')).toBeInTheDocument();
  });

  it('shows owner role badge', () => {
    render(<Teams />);
    // Multiple "owner" elements exist (badge and role permissions section)
    const ownerElements = screen.getAllByText('owner');
    expect(ownerElements.length).toBeGreaterThanOrEqual(2); // badge and permissions
  });

  it('shows Invite Member button for team owner', () => {
    render(<Teams />);
    expect(screen.getByText('Invite Member')).toBeInTheDocument();
  });

  it('opens invite modal when button clicked', () => {
    render(<Teams />);
    fireEvent.click(screen.getByText('Invite Member'));

    expect(screen.getByText('Wallet Address')).toBeInTheDocument();
    expect(screen.getByPlaceholderText('Enter Solana wallet address')).toBeInTheDocument();
    // Multiple "Role" texts exist (table header and modal label)
    const roleLabels = screen.getAllByText('Role');
    expect(roleLabels.length).toBeGreaterThanOrEqual(2);
  });

  it('invites member with wallet address and role', async () => {
    render(<Teams />);
    fireEvent.click(screen.getByText('Invite Member'));

    const addressInput = screen.getByPlaceholderText('Enter Solana wallet address');
    fireEvent.change(addressInput, { target: { value: 'NewMemberWallet123' } });

    fireEvent.click(screen.getByText('Send Invite'));

    await waitFor(() => {
      expect(mockInviteToTeam).toHaveBeenCalledWith(
        'team-personal',
        'NewMemberWallet123',
        'member'
      );
    });
  });

  it('shows error when inviting without wallet address', () => {
    render(<Teams />);
    fireEvent.click(screen.getByText('Invite Member'));

    fireEvent.click(screen.getByText('Send Invite'));

    expect(mockAlert).toHaveBeenCalledWith('Please enter a wallet address');
  });

  it('allows selecting different roles when inviting', () => {
    render(<Teams />);
    fireEvent.click(screen.getByText('Invite Member'));

    const roleSelect = screen.getByRole('combobox');
    fireEvent.change(roleSelect, { target: { value: 'admin' } });

    expect(roleSelect).toHaveValue('admin');
  });

  it('switches team when team card clicked', () => {
    render(<Teams />);

    const workTeamCard = screen.getByText('Work Team').closest('div[class*="cursor-pointer"]');
    fireEvent.click(workTeamCard!);

    expect(mockSwitchTeam).toHaveBeenCalledWith('team-work');
  });

  it('renders role permissions section', () => {
    render(<Teams />);
    expect(screen.getByText('Role Permissions')).toBeInTheDocument();
    expect(screen.getByText('Full access. Can manage team members, billing, and all resources.')).toBeInTheDocument();
    expect(screen.getByText('Can manage resources and invite members, but cannot manage billing.')).toBeInTheDocument();
    expect(screen.getByText('Can view and manage DNS zones and DDoS policies.')).toBeInTheDocument();
    expect(screen.getByText('Read-only access to all resources.')).toBeInTheDocument();
  });
});
