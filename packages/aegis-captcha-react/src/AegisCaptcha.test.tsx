/**
 * Tests for AEGIS CAPTCHA React Component
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { AegisCaptcha, AegisCaptchaRef } from './index';
import React, { createRef } from 'react';

// Mock crypto.subtle
const mockDigest = vi.fn().mockResolvedValue(new ArrayBuffer(32));
Object.defineProperty(global, 'crypto', {
  value: {
    subtle: {
      digest: mockDigest,
    },
  },
});

// Mock fetch
const mockFetch = vi.fn();
global.fetch = mockFetch;

// Mock Worker
class MockWorker {
  onmessage: ((e: MessageEvent) => void) | null = null;
  onerror: ((e: ErrorEvent) => void) | null = null;
  private timeoutId: NodeJS.Timeout | null = null;

  constructor() {
    // Simulate solving after a delay - enough time for the status to update
    this.timeoutId = setTimeout(() => {
      this.onmessage?.({ data: { type: 'solved', nonce: 12345 } } as MessageEvent);
    }, 200);
  }

  postMessage() {}
  terminate() {
    if (this.timeoutId) {
      clearTimeout(this.timeoutId);
    }
  }
}

global.Worker = MockWorker as unknown as typeof Worker;
global.URL.createObjectURL = vi.fn(() => 'blob:test');
global.URL.revokeObjectURL = vi.fn();

// Mock AudioContext
class MockAudioContext {
  createOscillator() {
    return {
      connect: vi.fn(),
      start: vi.fn(),
      stop: vi.fn(),
      frequency: { value: 0 },
      type: 'triangle',
    };
  }
  createAnalyser() {
    return {
      connect: vi.fn(),
      frequencyBinCount: 128,
      getFloatFrequencyData: vi.fn(),
    };
  }
  createGain() {
    return {
      connect: vi.fn(),
      gain: { value: 0 },
    };
  }
  get destination() {
    return {};
  }
  close() {}
}

global.AudioContext = MockAudioContext as unknown as typeof AudioContext;

describe('AegisCaptcha', () => {
  beforeEach(() => {
    vi.clearAllMocks();

    // Mock successful challenge issuance
    mockFetch.mockImplementation((url: string) => {
      if (url.includes('/issue')) {
        return Promise.resolve({
          ok: true,
          json: () =>
            Promise.resolve({
              id: 'test-challenge-id',
              pow_challenge: 'test-challenge-data',
              pow_difficulty: 8, // Low difficulty for tests
              expires_at: Date.now() / 1000 + 300,
            }),
        });
      }
      if (url.includes('/verify')) {
        return Promise.resolve({
          ok: true,
          json: () =>
            Promise.resolve({
              success: true,
              token: 'test-token-123',
            }),
        });
      }
      return Promise.reject(new Error('Unknown endpoint'));
    });
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  describe('Rendering', () => {
    it('renders with default props', () => {
      render(<AegisCaptcha />);
      expect(screen.getByTestId('aegis-captcha')).toBeInTheDocument();
      expect(screen.getByText("I'm not a robot")).toBeInTheDocument();
      expect(screen.getByText('Protected by AEGIS')).toBeInTheDocument();
    });

    it('renders with dark theme', () => {
      render(<AegisCaptcha theme="dark" />);
      expect(screen.getByTestId('aegis-captcha')).toBeInTheDocument();
    });

    it('renders with light theme', () => {
      render(<AegisCaptcha theme="light" />);
      expect(screen.getByTestId('aegis-captcha')).toBeInTheDocument();
    });

    it('renders with custom className', () => {
      render(<AegisCaptcha className="custom-class" />);
      expect(screen.getByTestId('aegis-captcha')).toHaveClass('custom-class');
    });

    it('renders with custom aria-label', () => {
      render(<AegisCaptcha aria-label="Custom verification" />);
      expect(screen.getByLabelText('Custom verification')).toBeInTheDocument();
    });

    it('renders checkbox with correct role', () => {
      render(<AegisCaptcha />);
      const checkbox = screen.getByRole('checkbox');
      expect(checkbox).toBeInTheDocument();
      expect(checkbox).toHaveAttribute('aria-checked', 'false');
    });
  });

  describe('Interaction', () => {
    it('starts verification when checkbox is clicked', async () => {
      render(<AegisCaptcha />);

      const checkbox = screen.getByTestId('aegis-captcha-checkbox');
      fireEvent.click(checkbox);

      await waitFor(() => {
        expect(mockFetch).toHaveBeenCalledWith(
          expect.stringContaining('/issue'),
          expect.any(Object)
        );
      });
    });

    it('shows loading state during verification', async () => {
      render(<AegisCaptcha />);

      const checkbox = screen.getByTestId('aegis-captcha-checkbox');
      fireEvent.click(checkbox);

      await waitFor(() => {
        expect(
          screen.getByText('Loading...') || screen.getByText('Verifying...')
        ).toBeInTheDocument();
      });
    });

    it('shows verified state on success', async () => {
      render(<AegisCaptcha />);

      const checkbox = screen.getByTestId('aegis-captcha-checkbox');
      fireEvent.click(checkbox);

      await waitFor(
        () => {
          expect(screen.getByText('Verified')).toBeInTheDocument();
        },
        { timeout: 3000 }
      );
    });

    it('prevents double execution after verified', async () => {
      const onSuccess = vi.fn();
      render(<AegisCaptcha onSuccess={onSuccess} />);

      const checkbox = screen.getByTestId('aegis-captcha-checkbox');

      // Click once to start
      fireEvent.click(checkbox);

      // Wait for verification to complete
      await waitFor(
        () => {
          expect(onSuccess).toHaveBeenCalledWith('test-token-123');
        },
        { timeout: 3000 }
      );

      // Clear the mock to count only new calls
      mockFetch.mockClear();

      // These clicks should be ignored since we're already verified
      fireEvent.click(checkbox);
      fireEvent.click(checkbox);
      fireEvent.click(checkbox);

      // Wait a bit for any async operations
      await new Promise(resolve => setTimeout(resolve, 100));

      // Should not have made any new issue calls since already verified
      const issueCalls = mockFetch.mock.calls.filter((call) =>
        call[0].includes('/issue')
      );
      expect(issueCalls.length).toBe(0);
    });
  });

  describe('Callbacks', () => {
    it('calls onSuccess with token when verified', async () => {
      const onSuccess = vi.fn();
      render(<AegisCaptcha onSuccess={onSuccess} />);

      const checkbox = screen.getByTestId('aegis-captcha-checkbox');
      fireEvent.click(checkbox);

      await waitFor(
        () => {
          expect(onSuccess).toHaveBeenCalledWith('test-token-123');
        },
        { timeout: 3000 }
      );
    });

    it('calls onError when verification fails', async () => {
      mockFetch.mockImplementation((url: string) => {
        if (url.includes('/issue')) {
          return Promise.reject(new Error('Network error'));
        }
        return Promise.reject(new Error('Unknown'));
      });

      const onError = vi.fn();
      render(<AegisCaptcha onError={onError} />);

      const checkbox = screen.getByTestId('aegis-captcha-checkbox');
      fireEvent.click(checkbox);

      await waitFor(() => {
        expect(onError).toHaveBeenCalled();
      });
    });

    it('calls onLoad when component mounts', () => {
      const onLoad = vi.fn();
      render(<AegisCaptcha onLoad={onLoad} />);

      expect(onLoad).toHaveBeenCalled();
    });
  });

  describe('Ref Methods', () => {
    it('exposes execute method via ref', async () => {
      const ref = createRef<AegisCaptchaRef>();
      render(<AegisCaptcha ref={ref} />);

      expect(ref.current?.execute).toBeDefined();

      const token = await ref.current?.execute();
      await waitFor(
        () => {
          expect(token).toBe('test-token-123');
        },
        { timeout: 3000 }
      );
    });

    it('exposes reset method via ref', async () => {
      const ref = createRef<AegisCaptchaRef>();
      render(<AegisCaptcha ref={ref} />);

      // First verify
      await ref.current?.execute();

      await waitFor(
        () => {
          expect(ref.current?.isVerified()).toBe(true);
        },
        { timeout: 3000 }
      );

      // Reset
      ref.current?.reset();

      // Wait for state to update
      await waitFor(() => {
        expect(ref.current?.isVerified()).toBe(false);
        expect(ref.current?.getToken()).toBeNull();
      });
    });

    it('exposes getToken method via ref', async () => {
      const ref = createRef<AegisCaptchaRef>();
      render(<AegisCaptcha ref={ref} />);

      expect(ref.current?.getToken()).toBeNull();

      await ref.current?.execute();

      await waitFor(
        () => {
          expect(ref.current?.getToken()).toBe('test-token-123');
        },
        { timeout: 3000 }
      );
    });

    it('exposes isVerified method via ref', async () => {
      const ref = createRef<AegisCaptchaRef>();
      render(<AegisCaptcha ref={ref} />);

      expect(ref.current?.isVerified()).toBe(false);

      await ref.current?.execute();

      await waitFor(
        () => {
          expect(ref.current?.isVerified()).toBe(true);
        },
        { timeout: 3000 }
      );
    });
  });

  describe('Error Handling', () => {
    it('shows error message when challenge fails', async () => {
      mockFetch.mockRejectedValueOnce(new Error('Network error'));

      render(<AegisCaptcha />);

      const checkbox = screen.getByTestId('aegis-captcha-checkbox');
      fireEvent.click(checkbox);

      await waitFor(() => {
        expect(screen.getByText(/Network error/i)).toBeInTheDocument();
      });
    });

    it('shows retry button on error', async () => {
      mockFetch.mockRejectedValueOnce(new Error('Network error'));

      render(<AegisCaptcha />);

      const checkbox = screen.getByTestId('aegis-captcha-checkbox');
      fireEvent.click(checkbox);

      await waitFor(() => {
        expect(screen.getByText('Retry')).toBeInTheDocument();
      });
    });

    it('allows retry after error', async () => {
      // First call fails
      mockFetch.mockRejectedValueOnce(new Error('Network error'));

      render(<AegisCaptcha />);

      const checkbox = screen.getByTestId('aegis-captcha-checkbox');
      fireEvent.click(checkbox);

      await waitFor(() => {
        expect(screen.getByText('Retry')).toBeInTheDocument();
      });

      // Reset mock for retry (success)
      mockFetch.mockImplementation((url: string) => {
        if (url.includes('/issue')) {
          return Promise.resolve({
            ok: true,
            json: () =>
              Promise.resolve({
                id: 'test-challenge-id',
                pow_challenge: 'test-challenge-data',
                pow_difficulty: 8,
                expires_at: Date.now() / 1000 + 300,
              }),
          });
        }
        if (url.includes('/verify')) {
          return Promise.resolve({
            ok: true,
            json: () =>
              Promise.resolve({
                success: true,
                token: 'test-token-retry',
              }),
          });
        }
        return Promise.reject(new Error('Unknown'));
      });

      const retryButton = screen.getByText('Retry');
      fireEvent.click(retryButton);

      await waitFor(
        () => {
          expect(screen.getByText('Verified')).toBeInTheDocument();
        },
        { timeout: 3000 }
      );
    });

    it('handles verification rejection', async () => {
      mockFetch.mockImplementation((url: string) => {
        if (url.includes('/issue')) {
          return Promise.resolve({
            ok: true,
            json: () =>
              Promise.resolve({
                id: 'test-challenge-id',
                pow_challenge: 'test-challenge-data',
                pow_difficulty: 8,
                expires_at: Date.now() / 1000 + 300,
              }),
          });
        }
        if (url.includes('/verify')) {
          return Promise.resolve({
            ok: true,
            json: () =>
              Promise.resolve({
                success: false,
                error: 'Invalid solution',
              }),
          });
        }
        return Promise.reject(new Error('Unknown'));
      });

      const onError = vi.fn();
      render(<AegisCaptcha onError={onError} />);

      const checkbox = screen.getByTestId('aegis-captcha-checkbox');
      fireEvent.click(checkbox);

      await waitFor(
        () => {
          expect(onError).toHaveBeenCalled();
        },
        { timeout: 3000 }
      );
    });
  });

  describe('Challenge Types', () => {
    it('uses correct challenge type in request', async () => {
      render(<AegisCaptcha challengeType="interactive" />);

      const checkbox = screen.getByTestId('aegis-captcha-checkbox');
      fireEvent.click(checkbox);

      await waitFor(() => {
        expect(mockFetch).toHaveBeenCalledWith(
          expect.stringContaining('type=interactive'),
          expect.any(Object)
        );
      });
    });

    it('auto-executes for invisible type', async () => {
      render(<AegisCaptcha challengeType="invisible" />);

      await waitFor(() => {
        expect(mockFetch).toHaveBeenCalledWith(
          expect.stringContaining('type=invisible'),
          expect.any(Object)
        );
      });
    });
  });

  describe('Sizes', () => {
    it('renders compact size', () => {
      render(<AegisCaptcha size="compact" />);
      const widget = screen.getByTestId('aegis-captcha');
      expect(widget).toHaveStyle({ maxWidth: '200px' });
    });

    it('renders normal size', () => {
      render(<AegisCaptcha size="normal" />);
      const widget = screen.getByTestId('aegis-captcha');
      expect(widget).toHaveStyle({ maxWidth: '300px' });
    });

    it('renders large size', () => {
      render(<AegisCaptcha size="large" />);
      const widget = screen.getByTestId('aegis-captcha');
      expect(widget).toHaveStyle({ maxWidth: '400px' });
    });
  });

  describe('Accessibility', () => {
    it('has proper ARIA attributes', () => {
      render(<AegisCaptcha />);

      const widget = screen.getByTestId('aegis-captcha');
      expect(widget).toHaveAttribute('role', 'group');

      const checkbox = screen.getByRole('checkbox');
      expect(checkbox).toHaveAttribute('aria-checked', 'false');
    });

    it('updates aria-checked when verified', async () => {
      render(<AegisCaptcha />);

      const checkbox = screen.getByRole('checkbox');
      fireEvent.click(checkbox);

      await waitFor(
        () => {
          expect(checkbox).toHaveAttribute('aria-checked', 'true');
        },
        { timeout: 3000 }
      );
    });

    it('footer link has proper security attributes', () => {
      render(<AegisCaptcha />);

      const link = screen.getByText('Protected by AEGIS');
      expect(link).toHaveAttribute('target', '_blank');
      expect(link).toHaveAttribute('rel', 'noopener noreferrer');
    });
  });

  describe('Cookie Management', () => {
    it('sets token cookie on success', async () => {
      const cookieSpy = vi.spyOn(document, 'cookie', 'set');

      render(<AegisCaptcha />);

      const checkbox = screen.getByTestId('aegis-captcha-checkbox');
      fireEvent.click(checkbox);

      await waitFor(
        () => {
          expect(cookieSpy).toHaveBeenCalledWith(
            expect.stringContaining('aegis_token=test-token-123')
          );
        },
        { timeout: 3000 }
      );
    });

    it('clears cookie on reset', async () => {
      const ref = createRef<AegisCaptchaRef>();
      const cookieSpy = vi.spyOn(document, 'cookie', 'set');

      render(<AegisCaptcha ref={ref} />);

      await ref.current?.execute();

      await waitFor(
        () => {
          expect(ref.current?.isVerified()).toBe(true);
        },
        { timeout: 3000 }
      );

      ref.current?.reset();

      expect(cookieSpy).toHaveBeenCalledWith(
        expect.stringContaining('aegis_token=; path=/; max-age=0')
      );
    });
  });

  describe('Debug Mode', () => {
    it('logs to console when debug is true', async () => {
      const consoleSpy = vi.spyOn(console, 'log');

      render(<AegisCaptcha debug={true} />);

      const checkbox = screen.getByTestId('aegis-captcha-checkbox');
      fireEvent.click(checkbox);

      await waitFor(() => {
        expect(consoleSpy).toHaveBeenCalledWith(
          '[AEGIS CAPTCHA]',
          expect.any(String)
        );
      });
    });

    it('does not log when debug is false', async () => {
      // Clear any previous console spies
      vi.restoreAllMocks();
      const consoleSpy = vi.spyOn(console, 'log').mockImplementation(() => {});

      render(<AegisCaptcha debug={false} />);

      // Just render and check - don't click to avoid triggering async operations
      // that might log from other sources

      // Filter only AEGIS logs
      const aegisLogs = consoleSpy.mock.calls.filter(
        (call) => call[0] === '[AEGIS CAPTCHA]'
      );
      expect(aegisLogs.length).toBe(0);

      consoleSpy.mockRestore();
    });
  });
});
