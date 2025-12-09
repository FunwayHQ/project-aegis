/**
 * AEGIS CAPTCHA Widget
 *
 * A privacy-respecting, decentralized CAPTCHA alternative using:
 * - Proof-of-Work (PoW) computation
 * - Browser fingerprinting for bot detection
 * - Ed25519 signed tokens
 *
 * Usage:
 *   <div id="aegis-captcha" data-callback="onCaptchaComplete"></div>
 *   <script src="js/aegis-captcha.js"></script>
 *   <script>
 *     AegisCaptcha.init({
 *       container: '#aegis-captcha',
 *       apiEndpoint: 'https://api.aegis.network',
 *       onSuccess: (token) => console.log('Token:', token),
 *       onError: (error) => console.error('Error:', error)
 *     });
 *   </script>
 */

(function(global) {
    'use strict';

    // Configuration
    const DEFAULT_CONFIG = {
        container: '#aegis-captcha',
        apiEndpoint: 'https://api.aegis.network', // Production endpoint
        challengeType: 'managed', // invisible, managed, interactive
        theme: 'dark',
        size: 'normal', // compact, normal, large
        onSuccess: null,
        onError: null,
        onExpired: null,
        onLoad: null,
        debug: false
    };

    // State
    let config = { ...DEFAULT_CONFIG };
    let currentChallenge = null;
    let token = null;
    let solving = false;
    let worker = null;

    /**
     * Initialize the AEGIS CAPTCHA widget
     */
    function init(options = {}) {
        config = { ...DEFAULT_CONFIG, ...options };

        // Find container
        const container = document.querySelector(config.container);
        if (!container) {
            console.error('[AEGIS CAPTCHA] Container not found:', config.container);
            return;
        }

        // Render widget
        renderWidget(container);

        // Auto-start for invisible type
        if (config.challengeType === 'invisible') {
            execute();
        }

        if (config.onLoad) {
            config.onLoad();
        }

        log('AEGIS CAPTCHA initialized');
    }

    /**
     * Render the CAPTCHA widget UI
     */
    function renderWidget(container) {
        const theme = config.theme === 'dark' ? getDarkStyles() : getLightStyles();
        const sizeClass = `aegis-captcha-${config.size}`;

        container.innerHTML = `
            <div class="aegis-captcha-widget ${sizeClass}" style="${theme.container}">
                <div class="aegis-captcha-header" style="${theme.header}">
                    <svg class="aegis-captcha-shield" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                        <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>
                        <path d="M9 12l2 2 4-4"/>
                    </svg>
                    <span class="aegis-captcha-title">AEGIS Verification</span>
                </div>

                <div class="aegis-captcha-content" style="${theme.content}">
                    <div class="aegis-captcha-status" id="aegis-status">
                        <div class="aegis-captcha-checkbox" id="aegis-checkbox" style="${theme.checkbox}" tabindex="0" role="checkbox" aria-checked="false">
                            <svg class="aegis-check-icon" width="20" height="20" viewBox="0 0 20 20" fill="none" style="display: none;">
                                <path d="M4 10l4 4 8-8" stroke="#10F7CD" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>
                            </svg>
                            <div class="aegis-spinner" style="display: none;"></div>
                        </div>
                        <span class="aegis-captcha-label" id="aegis-label">I'm not a robot</span>
                    </div>

                    <div class="aegis-captcha-progress" id="aegis-progress" style="display: none;">
                        <div class="aegis-progress-bar" style="${theme.progressBar}">
                            <div class="aegis-progress-fill" id="aegis-progress-fill" style="${theme.progressFill}"></div>
                        </div>
                        <span class="aegis-progress-text" id="aegis-progress-text">Verifying...</span>
                    </div>

                    <div class="aegis-captcha-error" id="aegis-error" style="display: none; ${theme.error}">
                        <span id="aegis-error-text"></span>
                        <button id="aegis-retry" style="${theme.retryButton}">Retry</button>
                    </div>
                </div>

                <div class="aegis-captcha-footer" style="${theme.footer}">
                    <a href="https://aegis.funwayinteractive.com" target="_blank" rel="noopener noreferrer" style="${theme.footerLink}">
                        Protected by AEGIS
                    </a>
                </div>
            </div>

            <style>
                .aegis-captcha-widget {
                    font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
                    border-radius: 12px;
                    overflow: hidden;
                    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.3);
                    max-width: 300px;
                    user-select: none;
                }

                .aegis-captcha-compact { max-width: 200px; }
                .aegis-captcha-large { max-width: 400px; }

                .aegis-captcha-header {
                    display: flex;
                    align-items: center;
                    gap: 8px;
                    padding: 12px 16px;
                    font-weight: 600;
                    font-size: 14px;
                }

                .aegis-captcha-shield {
                    flex-shrink: 0;
                }

                .aegis-captcha-content {
                    padding: 20px 16px;
                }

                .aegis-captcha-status {
                    display: flex;
                    align-items: center;
                    gap: 12px;
                }

                .aegis-captcha-checkbox {
                    width: 28px;
                    height: 28px;
                    border-radius: 6px;
                    display: flex;
                    align-items: center;
                    justify-content: center;
                    cursor: pointer;
                    transition: all 0.2s ease;
                }

                .aegis-captcha-checkbox:hover {
                    transform: scale(1.05);
                }

                .aegis-captcha-checkbox:focus {
                    outline: 2px solid #1EB5B0;
                    outline-offset: 2px;
                }

                .aegis-captcha-checkbox.verified {
                    background: linear-gradient(135deg, #1EB5B0 0%, #10F7CD 100%) !important;
                    border-color: transparent !important;
                }

                .aegis-captcha-checkbox.solving {
                    cursor: wait;
                }

                .aegis-spinner {
                    width: 16px;
                    height: 16px;
                    border: 2px solid rgba(255, 255, 255, 0.3);
                    border-top-color: #1EB5B0;
                    border-radius: 50%;
                    animation: aegis-spin 0.8s linear infinite;
                }

                @keyframes aegis-spin {
                    to { transform: rotate(360deg); }
                }

                .aegis-captcha-label {
                    font-size: 14px;
                    font-weight: 500;
                }

                .aegis-captcha-progress {
                    margin-top: 16px;
                }

                .aegis-progress-bar {
                    height: 4px;
                    border-radius: 2px;
                    overflow: hidden;
                    margin-bottom: 8px;
                }

                .aegis-progress-fill {
                    height: 100%;
                    width: 0%;
                    transition: width 0.3s ease;
                }

                .aegis-progress-text {
                    font-size: 12px;
                    opacity: 0.7;
                }

                .aegis-captcha-error {
                    margin-top: 12px;
                    padding: 12px;
                    border-radius: 8px;
                    font-size: 13px;
                    display: flex;
                    align-items: center;
                    justify-content: space-between;
                    gap: 8px;
                }

                .aegis-captcha-footer {
                    padding: 8px 16px;
                    text-align: center;
                    font-size: 11px;
                }

                .aegis-captcha-footer a {
                    text-decoration: none;
                    opacity: 0.6;
                    transition: opacity 0.2s;
                }

                .aegis-captcha-footer a:hover {
                    opacity: 1;
                }
            </style>
        `;

        // Bind events
        const checkbox = container.querySelector('#aegis-checkbox');
        const retryBtn = container.querySelector('#aegis-retry');

        checkbox.addEventListener('click', () => {
            if (!solving && !token) {
                execute();
            }
        });

        checkbox.addEventListener('keydown', (e) => {
            if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                if (!solving && !token) {
                    execute();
                }
            }
        });

        retryBtn.addEventListener('click', () => {
            reset();
            execute();
        });
    }

    /**
     * Get dark theme styles
     */
    function getDarkStyles() {
        return {
            container: 'background: linear-gradient(180deg, #1A1D2E 0%, #0A0E27 100%); color: #fff; border: 1px solid rgba(30, 181, 176, 0.3);',
            header: 'background: rgba(30, 181, 176, 0.1); color: #1EB5B0;',
            content: '',
            checkbox: 'background: #2D3142; border: 2px solid #4F5D75;',
            progressBar: 'background: #2D3142;',
            progressFill: 'background: linear-gradient(90deg, #1EB5B0 0%, #10F7CD 100%);',
            error: 'background: rgba(239, 68, 68, 0.1); color: #ef4444; border: 1px solid rgba(239, 68, 68, 0.3);',
            retryButton: 'background: #ef4444; color: white; border: none; padding: 6px 12px; border-radius: 6px; cursor: pointer; font-size: 12px;',
            footer: 'background: rgba(0, 0, 0, 0.2); border-top: 1px solid rgba(255, 255, 255, 0.1);',
            footerLink: 'color: #1EB5B0;'
        };
    }

    /**
     * Get light theme styles
     */
    function getLightStyles() {
        return {
            container: 'background: #fff; color: #1A1D2E; border: 1px solid #e5e7eb;',
            header: 'background: #f9fafb; color: #1EB5B0; border-bottom: 1px solid #e5e7eb;',
            content: '',
            checkbox: 'background: #f9fafb; border: 2px solid #d1d5db;',
            progressBar: 'background: #e5e7eb;',
            progressFill: 'background: linear-gradient(90deg, #1EB5B0 0%, #10F7CD 100%);',
            error: 'background: #fef2f2; color: #dc2626; border: 1px solid #fecaca;',
            retryButton: 'background: #dc2626; color: white; border: none; padding: 6px 12px; border-radius: 6px; cursor: pointer; font-size: 12px;',
            footer: 'background: #f9fafb; border-top: 1px solid #e5e7eb;',
            footerLink: 'color: #1EB5B0;'
        };
    }

    /**
     * Execute the CAPTCHA challenge
     */
    async function execute() {
        if (solving) return;
        solving = true;

        const container = document.querySelector(config.container);
        const checkbox = container.querySelector('#aegis-checkbox');
        const label = container.querySelector('#aegis-label');
        const progress = container.querySelector('#aegis-progress');
        const progressFill = container.querySelector('#aegis-progress-fill');
        const progressText = container.querySelector('#aegis-progress-text');
        const error = container.querySelector('#aegis-error');
        const spinner = container.querySelector('.aegis-spinner');

        // Update UI - solving state
        checkbox.classList.add('solving');
        spinner.style.display = 'block';
        label.textContent = 'Verifying...';
        error.style.display = 'none';
        progress.style.display = 'block';
        progressFill.style.width = '0%';

        try {
            // Step 1: Request challenge
            progressText.textContent = 'Requesting challenge...';
            progressFill.style.width = '10%';

            const challenge = await requestChallenge();
            currentChallenge = challenge;
            log('Challenge received:', challenge);

            // Step 2: Collect fingerprint
            progressText.textContent = 'Collecting fingerprint...';
            progressFill.style.width = '20%';

            const fingerprint = await collectFingerprint();
            log('Fingerprint collected');

            // Step 3: Solve PoW
            progressText.textContent = 'Solving proof-of-work...';

            const nonce = await solvePoW(challenge.pow_challenge, challenge.pow_difficulty, (progress) => {
                const pct = 20 + (progress * 60);
                progressFill.style.width = `${pct}%`;
            });
            log('PoW solved, nonce:', nonce);

            // Step 4: Submit solution
            progressText.textContent = 'Verifying solution...';
            progressFill.style.width = '90%';

            const result = await verifySolution(challenge.id, nonce, fingerprint);

            if (result.success) {
                token = result.token;

                // Store token in cookie
                document.cookie = `aegis_token=${token}; path=/; max-age=300; SameSite=Strict`;

                // Update UI - success state
                progressFill.style.width = '100%';
                progressText.textContent = 'Verified!';

                setTimeout(() => {
                    progress.style.display = 'none';
                    checkbox.classList.remove('solving');
                    checkbox.classList.add('verified');
                    spinner.style.display = 'none';
                    container.querySelector('.aegis-check-icon').style.display = 'block';
                    label.textContent = 'Verified';
                    checkbox.setAttribute('aria-checked', 'true');

                    if (config.onSuccess) {
                        config.onSuccess(token);
                    }
                }, 500);

                log('Verification successful, token:', token.substring(0, 20) + '...');
            } else {
                throw new Error(result.error || 'Verification failed');
            }
        } catch (err) {
            console.error('[AEGIS CAPTCHA] Error:', err);

            // Update UI - error state
            solving = false;
            checkbox.classList.remove('solving');
            spinner.style.display = 'none';
            label.textContent = 'Verification failed';
            progress.style.display = 'none';
            error.style.display = 'flex';
            container.querySelector('#aegis-error-text').textContent = err.message || 'Please try again';

            if (config.onError) {
                config.onError(err);
            }
        }

        solving = false;
    }

    /**
     * Request a new challenge from the API
     */
    async function requestChallenge() {
        const response = await fetch(`${config.apiEndpoint}/aegis/challenge/issue?type=${config.challengeType}`, {
            method: 'GET',
            headers: {
                'Accept': 'application/json'
            }
        });

        if (!response.ok) {
            throw new Error(`Failed to get challenge: ${response.status}`);
        }

        return response.json();
    }

    /**
     * Collect browser fingerprint for bot detection
     */
    async function collectFingerprint() {
        const fingerprint = {
            canvas_hash: await getCanvasFingerprint(),
            webgl_renderer: getWebGLRenderer(),
            webgl_vendor: getWebGLVendor(),
            audio_hash: await getAudioFingerprint(),
            screen: {
                width: screen.width,
                height: screen.height,
                color_depth: screen.colorDepth,
                pixel_ratio: window.devicePixelRatio || 1
            },
            timezone_offset: new Date().getTimezoneOffset(),
            languages: navigator.languages ? [...navigator.languages] : [navigator.language],
            platform: navigator.platform,
            hardware_concurrency: navigator.hardwareConcurrency || 0,
            device_memory: navigator.deviceMemory || 0,
            touch_support: 'ontouchstart' in window,
            cookie_enabled: navigator.cookieEnabled,
            do_not_track: navigator.doNotTrack
        };

        return fingerprint;
    }

    /**
     * Get canvas fingerprint
     */
    async function getCanvasFingerprint() {
        try {
            const canvas = document.createElement('canvas');
            canvas.width = 200;
            canvas.height = 50;
            const ctx = canvas.getContext('2d');

            ctx.textBaseline = 'top';
            ctx.font = '14px Arial';
            ctx.fillStyle = '#1EB5B0';
            ctx.fillRect(0, 0, 200, 50);
            ctx.fillStyle = '#10F7CD';
            ctx.fillText('AEGIS Verification', 10, 20);
            ctx.strokeStyle = '#fff';
            ctx.strokeRect(5, 5, 190, 40);

            const dataUrl = canvas.toDataURL();
            return await sha256(dataUrl);
        } catch (e) {
            return 'canvas_not_supported';
        }
    }

    /**
     * Get WebGL renderer
     */
    function getWebGLRenderer() {
        try {
            const canvas = document.createElement('canvas');
            const gl = canvas.getContext('webgl') || canvas.getContext('experimental-webgl');
            if (!gl) return 'webgl_not_supported';

            const debugInfo = gl.getExtension('WEBGL_debug_renderer_info');
            if (!debugInfo) return 'no_debug_info';

            return gl.getParameter(debugInfo.UNMASKED_RENDERER_WEBGL);
        } catch (e) {
            return 'error';
        }
    }

    /**
     * Get WebGL vendor
     */
    function getWebGLVendor() {
        try {
            const canvas = document.createElement('canvas');
            const gl = canvas.getContext('webgl') || canvas.getContext('experimental-webgl');
            if (!gl) return 'webgl_not_supported';

            const debugInfo = gl.getExtension('WEBGL_debug_renderer_info');
            if (!debugInfo) return 'no_debug_info';

            return gl.getParameter(debugInfo.UNMASKED_VENDOR_WEBGL);
        } catch (e) {
            return 'error';
        }
    }

    /**
     * Get audio fingerprint
     */
    async function getAudioFingerprint() {
        try {
            const audioContext = new (window.AudioContext || window.webkitAudioContext)();
            const oscillator = audioContext.createOscillator();
            const analyser = audioContext.createAnalyser();
            const gainNode = audioContext.createGain();
            const scriptProcessor = audioContext.createScriptProcessor(4096, 1, 1);

            gainNode.gain.value = 0; // Mute
            oscillator.type = 'triangle';
            oscillator.frequency.value = 1000;

            oscillator.connect(analyser);
            analyser.connect(scriptProcessor);
            scriptProcessor.connect(gainNode);
            gainNode.connect(audioContext.destination);

            oscillator.start(0);

            return new Promise((resolve) => {
                setTimeout(() => {
                    const frequencyData = new Float32Array(analyser.frequencyBinCount);
                    analyser.getFloatFrequencyData(frequencyData);

                    let sum = 0;
                    for (let i = 0; i < frequencyData.length; i++) {
                        sum += Math.abs(frequencyData[i]);
                    }

                    oscillator.stop();
                    audioContext.close();

                    resolve(sum.toString(36).substring(0, 16));
                }, 100);
            });
        } catch (e) {
            return 'audio_not_supported';
        }
    }

    /**
     * Solve Proof-of-Work challenge
     */
    async function solvePoW(challenge, difficulty, onProgress) {
        return new Promise((resolve, reject) => {
            // Use Web Worker for non-blocking computation
            const workerCode = `
                async function sha256(message) {
                    const encoder = new TextEncoder();
                    const data = encoder.encode(message);
                    const hashBuffer = await crypto.subtle.digest('SHA-256', data);
                    const hashArray = Array.from(new Uint8Array(hashBuffer));
                    return hashArray.map(b => b.toString(16).padStart(2, '0')).join('');
                }

                function checkLeadingZeros(hash, difficulty) {
                    const bitsNeeded = difficulty;
                    let zeroBits = 0;

                    for (let i = 0; i < hash.length && zeroBits < bitsNeeded; i++) {
                        const nibble = parseInt(hash[i], 16);
                        if (nibble === 0) {
                            zeroBits += 4;
                        } else {
                            zeroBits += Math.clz32(nibble) - 28;
                            break;
                        }
                    }

                    return zeroBits >= bitsNeeded;
                }

                onmessage = async function(e) {
                    const { challenge, difficulty, startNonce, batchSize } = e.data;

                    for (let i = 0; i < batchSize; i++) {
                        const nonce = startNonce + i;
                        const hash = await sha256(challenge + nonce.toString());

                        if (checkLeadingZeros(hash, difficulty)) {
                            postMessage({ type: 'solved', nonce: nonce });
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
            worker = new Worker(workerUrl);

            let currentNonce = 0;
            const batchSize = 100000;
            const expectedIterations = Math.pow(2, difficulty);

            worker.onmessage = function(e) {
                const { type, nonce, iterations, lastNonce } = e.data;

                if (type === 'solved') {
                    worker.terminate();
                    URL.revokeObjectURL(workerUrl);
                    resolve(nonce);
                } else if (type === 'progress') {
                    const progress = Math.min((currentNonce + iterations) / expectedIterations, 0.99);
                    if (onProgress) onProgress(progress);
                } else if (type === 'batch_complete') {
                    currentNonce = lastNonce;
                    worker.postMessage({ challenge, difficulty, startNonce: currentNonce, batchSize });
                }
            };

            worker.onerror = function(e) {
                worker.terminate();
                URL.revokeObjectURL(workerUrl);
                reject(new Error('PoW computation failed: ' + e.message));
            };

            // Start first batch
            worker.postMessage({ challenge, difficulty, startNonce: 0, batchSize });

            // Timeout after 2 minutes
            setTimeout(() => {
                if (worker) {
                    worker.terminate();
                    URL.revokeObjectURL(workerUrl);
                    reject(new Error('PoW computation timed out'));
                }
            }, 120000);
        });
    }

    /**
     * Submit solution for verification
     */
    async function verifySolution(challengeId, nonce, fingerprint) {
        const response = await fetch(`${config.apiEndpoint}/aegis/challenge/verify`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json'
            },
            body: JSON.stringify({
                challenge_id: challengeId,
                pow_nonce: nonce,
                fingerprint: fingerprint
            })
        });

        if (!response.ok) {
            throw new Error(`Verification failed: ${response.status}`);
        }

        return response.json();
    }

    /**
     * SHA-256 hash function
     */
    async function sha256(message) {
        const encoder = new TextEncoder();
        const data = encoder.encode(message);
        const hashBuffer = await crypto.subtle.digest('SHA-256', data);
        const hashArray = Array.from(new Uint8Array(hashBuffer));
        return hashArray.map(b => b.toString(16).padStart(2, '0')).join('');
    }

    /**
     * Get the current verification token
     */
    function getToken() {
        return token;
    }

    /**
     * Reset the CAPTCHA widget
     */
    function reset() {
        token = null;
        currentChallenge = null;
        solving = false;

        if (worker) {
            worker.terminate();
            worker = null;
        }

        const container = document.querySelector(config.container);
        if (container) {
            const checkbox = container.querySelector('#aegis-checkbox');
            const label = container.querySelector('#aegis-label');
            const progress = container.querySelector('#aegis-progress');
            const error = container.querySelector('#aegis-error');
            const spinner = container.querySelector('.aegis-spinner');
            const checkIcon = container.querySelector('.aegis-check-icon');

            checkbox.classList.remove('solving', 'verified');
            checkbox.setAttribute('aria-checked', 'false');
            spinner.style.display = 'none';
            checkIcon.style.display = 'none';
            label.textContent = "I'm not a robot";
            progress.style.display = 'none';
            error.style.display = 'none';
        }

        // Clear cookie
        document.cookie = 'aegis_token=; path=/; max-age=0';

        log('CAPTCHA reset');
    }

    /**
     * Check if the CAPTCHA has been verified
     */
    function isVerified() {
        return !!token;
    }

    /**
     * Debug logging
     */
    function log(...args) {
        if (config.debug) {
            console.log('[AEGIS CAPTCHA]', ...args);
        }
    }

    // Export public API
    global.AegisCaptcha = {
        init,
        execute,
        reset,
        getToken,
        isVerified,
        version: '1.0.0'
    };

})(typeof window !== 'undefined' ? window : this);
