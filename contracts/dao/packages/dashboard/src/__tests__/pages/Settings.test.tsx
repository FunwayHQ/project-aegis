import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import Settings from '../../pages/Settings';

describe('Settings Page', () => {
  beforeEach(() => {
    // Clear localStorage before each test
    localStorage.clear();
    // Mock alert
    vi.spyOn(window, 'alert').mockImplementation(() => {});
  });

  it('renders settings page title', () => {
    render(<Settings />);

    expect(screen.getByText('Settings')).toBeInTheDocument();
  });

  it('renders API Configuration section', () => {
    render(<Settings />);

    expect(screen.getByText('API Configuration')).toBeInTheDocument();
    expect(screen.getByText('DNS API URL')).toBeInTheDocument();
    expect(screen.getByText('DDoS API URL')).toBeInTheDocument();
  });

  it('renders Display section', () => {
    render(<Settings />);

    expect(screen.getByText('Display')).toBeInTheDocument();
    expect(screen.getByText('Theme')).toBeInTheDocument();
  });

  it('renders Notifications section', () => {
    render(<Settings />);

    expect(screen.getByText('Notifications')).toBeInTheDocument();
    expect(screen.getByText('Enable Notifications')).toBeInTheDocument();
  });

  it('renders Data Refresh section', () => {
    render(<Settings />);

    expect(screen.getByText('Data Refresh')).toBeInTheDocument();
    expect(screen.getByText('Auto Refresh')).toBeInTheDocument();
  });

  it('renders About section', () => {
    render(<Settings />);

    expect(screen.getByText('About')).toBeInTheDocument();
    expect(screen.getByText(/Version:/)).toBeInTheDocument();
    expect(screen.getByText(/1.0.0/)).toBeInTheDocument();
  });

  it('allows changing DNS API URL', () => {
    render(<Settings />);

    const dnsInput = screen.getByPlaceholderText('http://localhost:8054');
    fireEvent.change(dnsInput, { target: { value: 'http://newhost:8054' } });

    expect(dnsInput).toHaveValue('http://newhost:8054');
  });

  it('allows changing DDoS API URL', () => {
    render(<Settings />);

    const ddosInput = screen.getByPlaceholderText('http://localhost:8080');
    fireEvent.change(ddosInput, { target: { value: 'http://newhost:8080' } });

    expect(ddosInput).toHaveValue('http://newhost:8080');
  });

  it('allows toggling notifications', () => {
    render(<Settings />);

    // Find the notifications toggle - it should be the first toggle button after the text
    const toggles = screen.getAllByRole('button');
    const notificationToggle = toggles.find(t => t.className.includes('inline-flex'));

    expect(notificationToggle).toBeDefined();
    if (notificationToggle) {
      // Default is enabled (bg-teal-500 for light theme)
      expect(notificationToggle.className).toContain('bg-teal-500');

      fireEvent.click(notificationToggle);

      // After click, should be disabled (bg-gray-300 for light theme)
      expect(notificationToggle.className).toContain('bg-gray-300');
    }
  });

  it('saves settings to localStorage on save button click', () => {
    render(<Settings />);

    const saveButton = screen.getByText('Save Settings');
    fireEvent.click(saveButton);

    expect(window.alert).toHaveBeenCalledWith('Settings saved successfully!');
    expect(localStorage.getItem('aegis-settings')).toBeTruthy();
  });

  it('shows refresh interval input only when auto refresh is enabled', () => {
    render(<Settings />);

    // Auto refresh is enabled by default, so interval should be visible
    expect(screen.getByText('Refresh Interval (seconds)')).toBeInTheDocument();

    // Find and click auto refresh toggle
    const autoRefreshSection = screen.getByText('Auto Refresh').parentElement;
    const toggle = autoRefreshSection?.querySelector('button');

    if (toggle) {
      fireEvent.click(toggle);

      // After disabling, interval input should be hidden
      expect(screen.queryByText('Refresh Interval (seconds)')).not.toBeInTheDocument();
    }
  });

  it('allows changing refresh interval', () => {
    render(<Settings />);

    const intervalInput = screen.getByDisplayValue('10');
    fireEvent.change(intervalInput, { target: { value: '30' } });

    expect(intervalInput).toHaveValue(30);
  });
});
