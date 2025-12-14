import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import Profile from '../../../pages/account/Profile';

// Mock clipboard API
Object.assign(navigator, {
  clipboard: {
    writeText: vi.fn().mockResolvedValue(undefined),
  },
});

// Mock window.alert
const mockAlert = vi.fn();
window.alert = mockAlert;

// Mock auth context
const mockUpdateProfile = vi.fn();
const mockCreateApiKey = vi.fn().mockResolvedValue({
  key: 'aegis_testkey12345678901234567890123456789012345678901234567890',
  apiKey: {
    id: 'key-1',
    name: 'Test Key',
    prefix: 'aegis_testkey1',
    permissions: ['dns:read'],
    createdAt: Date.now(),
  },
});
const mockRevokeApiKey = vi.fn();

vi.mock('../../../contexts/AuthContext', () => ({
  useAuth: () => ({
    user: {
      walletAddress: 'TestWallet123456789ABCDEF',
      displayName: 'Test User',
      email: 'test@example.com',
      createdAt: Date.now() - 86400000, // 1 day ago
      updatedAt: Date.now(),
    },
    walletAddress: 'TestWallet123456789ABCDEF',
    apiKeys: [
      {
        id: 'existing-key',
        name: 'Existing Key',
        prefix: 'aegis_exist123',
        permissions: ['dns:read', 'dns:write'],
        createdAt: Date.now() - 3600000,
      },
    ],
    updateProfile: mockUpdateProfile,
    createApiKey: mockCreateApiKey,
    revokeApiKey: mockRevokeApiKey,
  }),
}));

describe('Profile Page', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders page title', () => {
    render(<Profile />);
    expect(screen.getByText('Profile')).toBeInTheDocument();
  });

  it('renders page description', () => {
    render(<Profile />);
    expect(
      screen.getByText('Manage your account settings and API keys')
    ).toBeInTheDocument();
  });

  it('displays wallet address', () => {
    render(<Profile />);
    expect(screen.getByDisplayValue('TestWallet123456789ABCDEF')).toBeInTheDocument();
  });

  it('displays display name input with current value', () => {
    render(<Profile />);
    expect(screen.getByDisplayValue('Test User')).toBeInTheDocument();
  });

  it('displays email input with current value', () => {
    render(<Profile />);
    expect(screen.getByDisplayValue('test@example.com')).toBeInTheDocument();
  });

  it('allows editing display name', () => {
    render(<Profile />);
    const nameInput = screen.getByDisplayValue('Test User');
    fireEvent.change(nameInput, { target: { value: 'New Name' } });
    expect(nameInput).toHaveValue('New Name');
  });

  it('allows editing email', () => {
    render(<Profile />);
    const emailInput = screen.getByDisplayValue('test@example.com');
    fireEvent.change(emailInput, { target: { value: 'new@example.com' } });
    expect(emailInput).toHaveValue('new@example.com');
  });

  it('calls updateProfile when Save Changes is clicked', async () => {
    render(<Profile />);

    const nameInput = screen.getByDisplayValue('Test User');
    fireEvent.change(nameInput, { target: { value: 'Updated Name' } });

    const saveButton = screen.getByText('Save Changes');
    fireEvent.click(saveButton);

    await waitFor(() => {
      expect(mockUpdateProfile).toHaveBeenCalledWith({
        displayName: 'Updated Name',
        email: 'test@example.com',
      });
    });
  });

  it('shows success alert after saving', async () => {
    render(<Profile />);

    const saveButton = screen.getByText('Save Changes');
    fireEvent.click(saveButton);

    await waitFor(() => {
      expect(mockAlert).toHaveBeenCalledWith('Profile updated successfully!');
    });
  });

  it('copies wallet address to clipboard', async () => {
    render(<Profile />);

    const copyButtons = screen.getAllByText('Copy');
    fireEvent.click(copyButtons[0]);

    await waitFor(() => {
      expect(navigator.clipboard.writeText).toHaveBeenCalledWith(
        'TestWallet123456789ABCDEF'
      );
    });
  });

  it('renders API Keys section', () => {
    render(<Profile />);
    expect(screen.getByText('API Keys')).toBeInTheDocument();
    expect(
      screen.getByText('Manage API keys for programmatic access')
    ).toBeInTheDocument();
  });

  it('displays existing API keys in table', () => {
    render(<Profile />);
    expect(screen.getByText('Existing Key')).toBeInTheDocument();
    expect(screen.getByText('aegis_exist123...')).toBeInTheDocument();
  });

  it('displays API key permissions', () => {
    render(<Profile />);
    expect(screen.getByText('dns:read')).toBeInTheDocument();
    expect(screen.getByText('dns:write')).toBeInTheDocument();
  });

  it('has New API Key button', () => {
    render(<Profile />);
    expect(screen.getByText('New API Key')).toBeInTheDocument();
  });

  it('opens create API key modal when button clicked', () => {
    render(<Profile />);
    fireEvent.click(screen.getByText('New API Key'));
    expect(screen.getByText('Create API Key')).toBeInTheDocument();
    expect(screen.getByPlaceholderText('My API Key')).toBeInTheDocument();
  });

  it('shows permission checkboxes in modal', () => {
    render(<Profile />);
    fireEvent.click(screen.getByText('New API Key'));

    expect(screen.getByText('DNS Read')).toBeInTheDocument();
    expect(screen.getByText('DNS Write')).toBeInTheDocument();
    expect(screen.getByText('DDoS Read')).toBeInTheDocument();
    expect(screen.getByText('DDoS Write')).toBeInTheDocument();
  });

  it('creates API key with selected permissions', async () => {
    render(<Profile />);
    fireEvent.click(screen.getByText('New API Key'));

    // Fill in name
    const nameInput = screen.getByPlaceholderText('My API Key');
    fireEvent.change(nameInput, { target: { value: 'Test Key' } });

    // Select permissions
    const dnsReadCheckbox = screen.getByText('DNS Read').previousElementSibling;
    fireEvent.click(dnsReadCheckbox!);

    // Create key
    const createButton = screen.getByRole('button', { name: 'Create Key' });
    fireEvent.click(createButton);

    await waitFor(() => {
      expect(mockCreateApiKey).toHaveBeenCalledWith('Test Key', ['dns:read']);
    });
  });

  it('shows generated key after creation', async () => {
    render(<Profile />);
    fireEvent.click(screen.getByText('New API Key'));

    const nameInput = screen.getByPlaceholderText('My API Key');
    fireEvent.change(nameInput, { target: { value: 'Test Key' } });

    const dnsReadCheckbox = screen.getByText('DNS Read').previousElementSibling;
    fireEvent.click(dnsReadCheckbox!);

    const createButton = screen.getByRole('button', { name: 'Create Key' });
    fireEvent.click(createButton);

    await waitFor(() => {
      expect(screen.getByText('API Key Created')).toBeInTheDocument();
      expect(
        screen.getByText(/Make sure to copy your API key now/)
      ).toBeInTheDocument();
    });
  });

  it('calls revokeApiKey when Revoke clicked', async () => {
    render(<Profile />);

    const revokeButton = screen.getByText('Revoke');
    window.confirm = vi.fn().mockReturnValue(true);
    fireEvent.click(revokeButton);

    await waitFor(() => {
      expect(mockRevokeApiKey).toHaveBeenCalledWith('existing-key');
    });
  });

  it('does not revoke if confirm is cancelled', async () => {
    render(<Profile />);

    const revokeButton = screen.getByText('Revoke');
    window.confirm = vi.fn().mockReturnValue(false);
    fireEvent.click(revokeButton);

    expect(mockRevokeApiKey).not.toHaveBeenCalled();
  });

  it('renders Account Details section', () => {
    render(<Profile />);
    expect(screen.getByText('Account Details')).toBeInTheDocument();
    expect(screen.getByText('Member Since')).toBeInTheDocument();
    expect(screen.getByText('Last Updated')).toBeInTheDocument();
  });
});
