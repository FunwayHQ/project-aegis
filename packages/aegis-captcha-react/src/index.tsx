/**
 * @aegis/captcha-react
 *
 * React component for AEGIS CAPTCHA - a privacy-respecting, decentralized
 * alternative to reCAPTCHA using Proof-of-Work and browser fingerprinting.
 */

import React, {
  useCallback,
  useEffect,
  useRef,
  useState,
  forwardRef,
  useImperativeHandle,
} from 'react';

// Types
export interface AegisCaptchaProps {
  /** API endpoint for challenge verification */
  apiEndpoint?: string;
  /** Challenge type: invisible, managed, or interactive */
  challengeType?: 'invisible' | 'managed' | 'interactive';
  /** Theme: dark or light */
  theme?: 'dark' | 'light';
  /** Size: compact, normal, or large */
  size?: 'compact' | 'normal' | 'large';
  /** Callback when verification succeeds */
  onSuccess?: (token: string) => void;
  /** Callback when verification fails */
  onError?: (error: Error) => void;
  /** Callback when token expires */
  onExpired?: () => void;
  /** Callback when widget loads */
  onLoad?: () => void;
  /** Enable debug logging */
  debug?: boolean;
  /** Custom class name */
  className?: string;
  /** Custom styles */
  style?: React.CSSProperties;
  /** Accessible label */
  'aria-label'?: string;
  /** Test ID for testing */
  'data-testid'?: string;
}

export interface AegisCaptchaRef {
  /** Execute the challenge */
  execute: () => Promise<string | null>;
  /** Reset the widget */
  reset: () => void;
  /** Get the current token */
  getToken: () => string | null;
  /** Check if verified */
  isVerified: () => boolean;
}

interface Challenge {
  id: string;
  pow_challenge: string;
  pow_difficulty: number;
  expires_at: number;
}

interface BrowserFingerprint {
  canvas_hash: string;
  webgl_renderer: string;
  webgl_vendor: string;
  audio_hash: string;
  screen: {
    width: number;
    height: number;
    color_depth: number;
    pixel_ratio: number;
  };
  timezone_offset: number;
  languages: string[];
  platform: string;
  hardware_concurrency: number;
  device_memory: number;
  touch_support: boolean;
  cookie_enabled: boolean;
  do_not_track: string | null;
}

// Status types
type CaptchaStatus = 'idle' | 'loading' | 'solving' | 'verified' | 'error';

// Default configuration
const DEFAULT_API_ENDPOINT = 'https://api.aegis.network';

/**
 * AEGIS CAPTCHA React Component
 *
 * @example
 * ```tsx
 * <AegisCaptcha
 *   onSuccess={(token) => console.log('Token:', token)}
 *   onError={(error) => console.error('Error:', error)}
 * />
 * ```
 */
export const AegisCaptcha = forwardRef<AegisCaptchaRef, AegisCaptchaProps>(
  (
    {
      apiEndpoint = DEFAULT_API_ENDPOINT,
      challengeType = 'managed',
      theme = 'dark',
      size = 'normal',
      onSuccess,
      onError,
      onExpired,
      onLoad,
      debug = false,
      className = '',
      style,
      'aria-label': ariaLabel = 'AEGIS CAPTCHA verification',
      'data-testid': testId = 'aegis-captcha',
    },
    ref
  ) => {
    // State
    const [status, setStatus] = useState<CaptchaStatus>('idle');
    const [token, setToken] = useState<string | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [progress, setProgress] = useState(0);

    // Refs
    const workerRef = useRef<Worker | null>(null);
    const challengeRef = useRef<Challenge | null>(null);

    // Logging helper
    const log = useCallback(
      (...args: unknown[]) => {
        if (debug) {
          console.log('[AEGIS CAPTCHA]', ...args);
        }
      },
      [debug]
    );

    // SHA-256 hash function
    const sha256 = async (message: string): Promise<string> => {
      const encoder = new TextEncoder();
      const data = encoder.encode(message);
      const hashBuffer = await crypto.subtle.digest('SHA-256', data);
      const hashArray = Array.from(new Uint8Array(hashBuffer));
      return hashArray.map((b) => b.toString(16).padStart(2, '0')).join('');
    };

    // Canvas fingerprint
    const getCanvasFingerprint = async (): Promise<string> => {
      try {
        const canvas = document.createElement('canvas');
        canvas.width = 200;
        canvas.height = 50;
        const ctx = canvas.getContext('2d');
        if (!ctx) return 'canvas_not_supported';

        ctx.textBaseline = 'top';
        ctx.font = '14px Arial';
        ctx.fillStyle = '#1EB5B0';
        ctx.fillRect(0, 0, 200, 50);
        ctx.fillStyle = '#10F7CD';
        ctx.fillText('AEGIS Verification', 10, 20);

        return await sha256(canvas.toDataURL());
      } catch {
        return 'canvas_not_supported';
      }
    };

    // WebGL renderer
    const getWebGLRenderer = (): string => {
      try {
        const canvas = document.createElement('canvas');
        const gl =
          canvas.getContext('webgl') ||
          canvas.getContext('experimental-webgl');
        if (!gl) return 'webgl_not_supported';

        const debugInfo = (gl as WebGLRenderingContext).getExtension(
          'WEBGL_debug_renderer_info'
        );
        if (!debugInfo) return 'no_debug_info';

        return (
          (gl as WebGLRenderingContext).getParameter(
            debugInfo.UNMASKED_RENDERER_WEBGL
          ) || 'unknown'
        );
      } catch {
        return 'error';
      }
    };

    // WebGL vendor
    const getWebGLVendor = (): string => {
      try {
        const canvas = document.createElement('canvas');
        const gl =
          canvas.getContext('webgl') ||
          canvas.getContext('experimental-webgl');
        if (!gl) return 'webgl_not_supported';

        const debugInfo = (gl as WebGLRenderingContext).getExtension(
          'WEBGL_debug_renderer_info'
        );
        if (!debugInfo) return 'no_debug_info';

        return (
          (gl as WebGLRenderingContext).getParameter(
            debugInfo.UNMASKED_VENDOR_WEBGL
          ) || 'unknown'
        );
      } catch {
        return 'error';
      }
    };

    // Audio fingerprint
    const getAudioFingerprint = async (): Promise<string> => {
      try {
        const AudioContext =
          window.AudioContext ||
          (window as unknown as { webkitAudioContext: typeof window.AudioContext })
            .webkitAudioContext;
        if (!AudioContext) return 'audio_not_supported';

        const context = new AudioContext();
        const oscillator = context.createOscillator();
        const analyser = context.createAnalyser();
        const gain = context.createGain();

        gain.gain.value = 0;
        oscillator.type = 'triangle';
        oscillator.frequency.value = 1000;

        oscillator.connect(analyser);
        analyser.connect(gain);
        gain.connect(context.destination);
        oscillator.start(0);

        return new Promise((resolve) => {
          setTimeout(() => {
            const data = new Float32Array(analyser.frequencyBinCount);
            analyser.getFloatFrequencyData(data);

            let sum = 0;
            for (let i = 0; i < data.length; i++) {
              sum += Math.abs(data[i]);
            }

            oscillator.stop();
            context.close();
            resolve(sum.toString(36).substring(0, 16));
          }, 100);
        });
      } catch {
        return 'audio_not_supported';
      }
    };

    // Collect fingerprint
    const collectFingerprint = async (): Promise<BrowserFingerprint> => {
      return {
        canvas_hash: await getCanvasFingerprint(),
        webgl_renderer: getWebGLRenderer(),
        webgl_vendor: getWebGLVendor(),
        audio_hash: await getAudioFingerprint(),
        screen: {
          width: window.screen.width,
          height: window.screen.height,
          color_depth: window.screen.colorDepth,
          pixel_ratio: window.devicePixelRatio || 1,
        },
        timezone_offset: new Date().getTimezoneOffset(),
        languages: navigator.languages
          ? [...navigator.languages]
          : [navigator.language],
        platform: navigator.platform,
        hardware_concurrency: navigator.hardwareConcurrency || 0,
        device_memory: (navigator as { deviceMemory?: number }).deviceMemory || 0,
        touch_support: 'ontouchstart' in window,
        cookie_enabled: navigator.cookieEnabled,
        do_not_track: navigator.doNotTrack,
      };
    };

    // Request challenge
    const requestChallenge = async (): Promise<Challenge> => {
      const response = await fetch(
        `${apiEndpoint}/aegis/challenge/issue?type=${challengeType}`,
        {
          method: 'GET',
          headers: { Accept: 'application/json' },
        }
      );

      if (!response.ok) {
        throw new Error(`Failed to get challenge: ${response.status}`);
      }

      return response.json();
    };

    // Solve PoW using Web Worker
    const solvePoW = (
      challenge: string,
      difficulty: number
    ): Promise<number> => {
      return new Promise((resolve, reject) => {
        const workerCode = `
          async function sha256(message) {
            const encoder = new TextEncoder();
            const data = encoder.encode(message);
            const hashBuffer = await crypto.subtle.digest('SHA-256', data);
            const hashArray = Array.from(new Uint8Array(hashBuffer));
            return hashArray.map(b => b.toString(16).padStart(2, '0')).join('');
          }

          function checkLeadingZeros(hash, difficulty) {
            let zeroBits = 0;
            for (let i = 0; i < hash.length && zeroBits < difficulty; i++) {
              const nibble = parseInt(hash[i], 16);
              if (nibble === 0) {
                zeroBits += 4;
              } else {
                zeroBits += Math.clz32(nibble) - 28;
                break;
              }
            }
            return zeroBits >= difficulty;
          }

          onmessage = async function(e) {
            const { challenge, difficulty, startNonce, batchSize } = e.data;
            for (let i = 0; i < batchSize; i++) {
              const nonce = startNonce + i;
              const hash = await sha256(challenge + nonce.toString());
              if (checkLeadingZeros(hash, difficulty)) {
                postMessage({ type: 'solved', nonce });
                return;
              }
              if (i % 10000 === 0) {
                postMessage({ type: 'progress', iterations: i });
              }
            }
            postMessage({ type: 'batch_complete', lastNonce: startNonce + batchSize });
          };
        `;

        const blob = new Blob([workerCode], { type: 'application/javascript' });
        const workerUrl = URL.createObjectURL(blob);
        const worker = new Worker(workerUrl);
        workerRef.current = worker;

        let currentNonce = 0;
        const batchSize = 100000;
        const expectedIterations = Math.pow(2, difficulty);

        worker.onmessage = (e: MessageEvent) => {
          const { type, nonce, iterations, lastNonce } = e.data;

          if (type === 'solved') {
            worker.terminate();
            URL.revokeObjectURL(workerUrl);
            workerRef.current = null;
            resolve(nonce);
          } else if (type === 'progress') {
            const pct = Math.min(
              ((currentNonce + iterations) / expectedIterations) * 100,
              99
            );
            setProgress(pct);
          } else if (type === 'batch_complete') {
            currentNonce = lastNonce;
            worker.postMessage({
              challenge,
              difficulty,
              startNonce: currentNonce,
              batchSize,
            });
          }
        };

        worker.onerror = (e: ErrorEvent) => {
          worker.terminate();
          URL.revokeObjectURL(workerUrl);
          workerRef.current = null;
          reject(new Error('PoW computation failed: ' + e.message));
        };

        worker.postMessage({
          challenge,
          difficulty,
          startNonce: 0,
          batchSize,
        });

        // Timeout
        setTimeout(() => {
          if (workerRef.current) {
            worker.terminate();
            URL.revokeObjectURL(workerUrl);
            workerRef.current = null;
            reject(new Error('PoW computation timed out'));
          }
        }, 120000);
      });
    };

    // Verify solution
    const verifySolution = async (
      challengeId: string,
      nonce: number,
      fingerprint: BrowserFingerprint
    ): Promise<{ success: boolean; token?: string; error?: string }> => {
      const response = await fetch(`${apiEndpoint}/aegis/challenge/verify`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          challenge_id: challengeId,
          pow_nonce: nonce,
          fingerprint,
        }),
      });

      if (!response.ok) {
        throw new Error(`Verification failed: ${response.status}`);
      }

      return response.json();
    };

    // Execute challenge
    const execute = useCallback(async (): Promise<string | null> => {
      if (status === 'solving' || status === 'verified') {
        return token;
      }

      setStatus('loading');
      setError(null);
      setProgress(0);

      try {
        // Request challenge
        log('Requesting challenge...');
        const challenge = await requestChallenge();
        challengeRef.current = challenge;
        log('Challenge received:', challenge.id);

        // Collect fingerprint
        log('Collecting fingerprint...');
        const fingerprint = await collectFingerprint();
        log('Fingerprint collected');

        // Solve PoW
        setStatus('solving');
        log('Solving PoW (difficulty:', challenge.pow_difficulty, ')...');
        const nonce = await solvePoW(
          challenge.pow_challenge,
          challenge.pow_difficulty
        );
        log('PoW solved, nonce:', nonce);

        // Verify
        log('Verifying solution...');
        const result = await verifySolution(challenge.id, nonce, fingerprint);

        if (result.success && result.token) {
          setToken(result.token);
          setStatus('verified');
          setProgress(100);

          // Store in cookie
          document.cookie = `aegis_token=${result.token}; path=/; max-age=300; SameSite=Strict`;

          log('Verification successful');
          onSuccess?.(result.token);
          return result.token;
        } else {
          throw new Error(result.error || 'Verification failed');
        }
      } catch (err) {
        const error = err instanceof Error ? err : new Error(String(err));
        setError(error.message);
        setStatus('error');
        log('Error:', error.message);
        onError?.(error);
        return null;
      }
    }, [status, token, apiEndpoint, challengeType, onSuccess, onError, log]);

    // Reset widget
    const reset = useCallback(() => {
      if (workerRef.current) {
        workerRef.current.terminate();
        workerRef.current = null;
      }

      setStatus('idle');
      setToken(null);
      setError(null);
      setProgress(0);
      challengeRef.current = null;

      // Clear cookie
      document.cookie = 'aegis_token=; path=/; max-age=0';

      log('Widget reset');
    }, [log]);

    // Expose methods via ref
    useImperativeHandle(
      ref,
      () => ({
        execute,
        reset,
        getToken: () => token,
        isVerified: () => status === 'verified',
      }),
      [execute, reset, token, status]
    );

    // Auto-execute for invisible type
    useEffect(() => {
      if (challengeType === 'invisible') {
        execute();
      }
      onLoad?.();
    }, []);

    // Theme styles
    const styles = getThemeStyles(theme);
    const sizeStyles = getSizeStyles(size);

    return (
      <div
        className={`aegis-captcha-widget ${className}`}
        style={{ ...sizeStyles.container, ...style }}
        aria-label={ariaLabel}
        data-testid={testId}
        role="group"
      >
        <div style={styles.container}>
          {/* Header */}
          <div style={styles.header}>
            <ShieldIcon />
            <span style={styles.title}>AEGIS Verification</span>
          </div>

          {/* Content */}
          <div style={styles.content}>
            {status === 'error' ? (
              <div style={styles.errorContainer}>
                <span style={styles.errorText}>{error}</span>
                <button
                  onClick={() => {
                    reset();
                    execute();
                  }}
                  style={styles.retryButton}
                  type="button"
                >
                  Retry
                </button>
              </div>
            ) : (
              <>
                <div style={styles.statusRow}>
                  <button
                    onClick={execute}
                    disabled={status === 'solving' || status === 'verified'}
                    style={{
                      ...styles.checkbox,
                      ...(status === 'verified' ? styles.checkboxVerified : {}),
                    }}
                    type="button"
                    role="checkbox"
                    aria-checked={status === 'verified'}
                    data-testid="aegis-captcha-checkbox"
                  >
                    {status === 'verified' && <CheckIcon />}
                    {(status === 'loading' || status === 'solving') && (
                      <Spinner />
                    )}
                  </button>
                  <span style={styles.label}>
                    {status === 'idle' && "I'm not a robot"}
                    {status === 'loading' && 'Loading...'}
                    {status === 'solving' && 'Verifying...'}
                    {status === 'verified' && 'Verified'}
                  </span>
                </div>

                {(status === 'loading' || status === 'solving') && (
                  <div style={styles.progressContainer}>
                    <div style={styles.progressBar}>
                      <div
                        style={{
                          ...styles.progressFill,
                          width: `${progress}%`,
                        }}
                      />
                    </div>
                    <span style={styles.progressText}>
                      {status === 'loading'
                        ? 'Preparing...'
                        : `${Math.round(progress)}%`}
                    </span>
                  </div>
                )}
              </>
            )}
          </div>

          {/* Footer */}
          <div style={styles.footer}>
            <a
              href="https://aegis.funwayinteractive.com"
              target="_blank"
              rel="noopener noreferrer"
              style={styles.footerLink}
            >
              Protected by AEGIS
            </a>
          </div>
        </div>
      </div>
    );
  }
);

AegisCaptcha.displayName = 'AegisCaptcha';

// Helper Components
const ShieldIcon = () => (
  <svg
    width="20"
    height="20"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
  >
    <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
    <path d="M9 12l2 2 4-4" />
  </svg>
);

const CheckIcon = () => (
  <svg
    width="16"
    height="16"
    viewBox="0 0 20 20"
    fill="none"
    stroke="#10F7CD"
    strokeWidth="3"
    strokeLinecap="round"
    strokeLinejoin="round"
  >
    <path d="M4 10l4 4 8-8" />
  </svg>
);

const Spinner = () => (
  <div
    style={{
      width: 14,
      height: 14,
      border: '2px solid rgba(255, 255, 255, 0.3)',
      borderTopColor: '#1EB5B0',
      borderRadius: '50%',
      animation: 'aegis-spin 0.8s linear infinite',
    }}
  />
);

// Theme styles
function getThemeStyles(theme: 'dark' | 'light') {
  const isDark = theme === 'dark';

  return {
    container: {
      fontFamily: "'Inter', -apple-system, BlinkMacSystemFont, sans-serif",
      borderRadius: 12,
      overflow: 'hidden' as const,
      boxShadow: '0 4px 20px rgba(0, 0, 0, 0.3)',
      background: isDark
        ? 'linear-gradient(180deg, #1A1D2E 0%, #0A0E27 100%)'
        : '#fff',
      color: isDark ? '#fff' : '#1A1D2E',
      border: isDark
        ? '1px solid rgba(30, 181, 176, 0.3)'
        : '1px solid #e5e7eb',
    },
    header: {
      display: 'flex' as const,
      alignItems: 'center' as const,
      gap: 8,
      padding: '12px 16px',
      background: isDark ? 'rgba(30, 181, 176, 0.1)' : '#f9fafb',
      color: '#1EB5B0',
      fontSize: 14,
      fontWeight: 600,
      borderBottom: isDark ? 'none' : '1px solid #e5e7eb',
    },
    title: {},
    content: {
      padding: '20px 16px',
    },
    statusRow: {
      display: 'flex' as const,
      alignItems: 'center' as const,
      gap: 12,
    },
    checkbox: {
      width: 28,
      height: 28,
      borderRadius: 6,
      display: 'flex' as const,
      alignItems: 'center' as const,
      justifyContent: 'center' as const,
      cursor: 'pointer' as const,
      transition: 'all 0.2s ease',
      background: isDark ? '#2D3142' : '#f9fafb',
      border: isDark ? '2px solid #4F5D75' : '2px solid #d1d5db',
    },
    checkboxVerified: {
      background: 'linear-gradient(135deg, #1EB5B0 0%, #10F7CD 100%)',
      borderColor: 'transparent',
    },
    label: {
      fontSize: 14,
      fontWeight: 500,
    },
    progressContainer: {
      marginTop: 16,
    },
    progressBar: {
      height: 4,
      borderRadius: 2,
      overflow: 'hidden' as const,
      marginBottom: 8,
      background: isDark ? '#2D3142' : '#e5e7eb',
    },
    progressFill: {
      height: '100%',
      background: 'linear-gradient(90deg, #1EB5B0 0%, #10F7CD 100%)',
      transition: 'width 0.3s ease',
    },
    progressText: {
      fontSize: 12,
      opacity: 0.7,
    },
    errorContainer: {
      display: 'flex' as const,
      alignItems: 'center' as const,
      justifyContent: 'space-between' as const,
      gap: 8,
      padding: 12,
      borderRadius: 8,
      background: isDark ? 'rgba(239, 68, 68, 0.1)' : '#fef2f2',
      color: isDark ? '#ef4444' : '#dc2626',
      border: isDark
        ? '1px solid rgba(239, 68, 68, 0.3)'
        : '1px solid #fecaca',
    },
    errorText: {
      fontSize: 13,
    },
    retryButton: {
      background: '#ef4444',
      color: 'white',
      border: 'none',
      padding: '6px 12px',
      borderRadius: 6,
      cursor: 'pointer' as const,
      fontSize: 12,
      fontWeight: 500,
    },
    footer: {
      padding: '8px 16px',
      textAlign: 'center' as const,
      fontSize: 11,
      background: isDark ? 'rgba(0, 0, 0, 0.2)' : '#f9fafb',
      borderTop: isDark
        ? '1px solid rgba(255, 255, 255, 0.1)'
        : '1px solid #e5e7eb',
    },
    footerLink: {
      color: '#1EB5B0',
      textDecoration: 'none',
      opacity: 0.6,
    },
  };
}

function getSizeStyles(size: 'compact' | 'normal' | 'large') {
  const widths = {
    compact: 200,
    normal: 300,
    large: 400,
  };

  return {
    container: {
      maxWidth: widths[size],
    },
  };
}

// Export types
export type { Challenge, BrowserFingerprint, CaptchaStatus };

// Export default
export default AegisCaptcha;
