# AEGIS JavaScript Challenge Integration Guide

This guide explains how to integrate the AEGIS JavaScript challenge system into your website to protect against bots while providing a seamless experience for legitimate users.

## Overview

The AEGIS challenge system is similar to Cloudflare Turnstile - it verifies that visitors are human through:
1. **Proof-of-Work (PoW)**: Client solves a computational puzzle
2. **Browser Fingerprinting**: Collects browser characteristics to detect headless browsers
3. **Token Issuance**: Verified clients receive a signed JWT token for subsequent requests

## Challenge Types

| Type | Description | User Experience |
|------|-------------|-----------------|
| `invisible` | Runs automatically, no UI | Seamless, no user action required |
| `managed` | Shows brief loading indicator | Minimal disruption |
| `interactive` | Requires user interaction | Most secure, used for high-risk actions |

## Integration Options

### Option 1: Automatic Challenge (Recommended)

When AEGIS detects a suspicious request, it automatically serves a challenge page. No code changes required - just enable the challenge manager in your AEGIS proxy configuration.

**How it works:**
1. Bot detection flags request as suspicious
2. AEGIS returns 403 with challenge HTML page
3. Client-side JS solves PoW + collects fingerprints
4. Solution submitted to `/aegis/challenge/verify`
5. On success, token stored in `aegis_token` cookie
6. Page reloads, request proceeds with valid token

### Option 2: Embed Challenge Widget

For forms or sensitive actions, embed the challenge widget directly:

```html
<!-- Add to your HTML page -->
<div id="aegis-challenge"></div>

<script>
// Fetch challenge from AEGIS
async function loadAegisChallenge() {
    const response = await fetch('https://prjaegis.org/aegis/challenge/issue?type=managed');
    const challenge = await response.json();

    // The challenge object contains:
    // - id: Challenge ID
    // - pow_challenge: Random data for PoW
    // - pow_difficulty: Required difficulty (leading zero bits)
    // - expires_at: Unix timestamp when challenge expires

    return challenge;
}

// Solve the challenge
async function solveChallenge(challenge) {
    // Collect browser fingerprint
    const fingerprint = collectFingerprint();

    // Solve Proof-of-Work
    const nonce = await solvePoW(challenge.pow_challenge, challenge.pow_difficulty);

    // Submit solution
    const response = await fetch('https://prjaegis.org/aegis/challenge/verify', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
            challenge_id: challenge.id,
            pow_nonce: nonce,
            fingerprint: fingerprint
        })
    });

    const result = await response.json();

    if (result.success) {
        // Token is automatically set in cookie
        // Can also access via result.token
        return result.token;
    } else {
        throw new Error(result.error);
    }
}

// Fingerprint collection
function collectFingerprint() {
    return {
        canvas_hash: getCanvasFingerprint(),
        webgl_renderer: getWebGLRenderer(),
        webgl_vendor: getWebGLVendor(),
        audio_hash: getAudioFingerprint(),
        screen: {
            width: screen.width,
            height: screen.height,
            color_depth: screen.colorDepth,
            pixel_ratio: window.devicePixelRatio || 1
        },
        timezone_offset: new Date().getTimezoneOffset(),
        language: navigator.language,
        platform: navigator.platform,
        cpu_cores: navigator.hardwareConcurrency || null,
        device_memory: navigator.deviceMemory || null,
        touch_support: 'ontouchstart' in window,
        webdriver_detected: navigator.webdriver || false,
        plugins_count: navigator.plugins ? navigator.plugins.length : 0
    };
}

// Canvas fingerprinting
function getCanvasFingerprint() {
    try {
        const canvas = document.createElement('canvas');
        const ctx = canvas.getContext('2d');
        canvas.width = 200;
        canvas.height = 50;
        ctx.textBaseline = 'top';
        ctx.font = '14px Arial';
        ctx.fillStyle = '#f60';
        ctx.fillRect(125, 1, 62, 20);
        ctx.fillStyle = '#069';
        ctx.fillText('AEGIS', 2, 15);
        ctx.fillStyle = 'rgba(102, 204, 0, 0.7)';
        ctx.fillText('AEGIS', 4, 17);
        return btoa(canvas.toDataURL()).substring(0, 64);
    } catch (e) {
        return '';
    }
}

// WebGL fingerprinting
function getWebGLRenderer() {
    try {
        const canvas = document.createElement('canvas');
        const gl = canvas.getContext('webgl') || canvas.getContext('experimental-webgl');
        if (!gl) return null;
        const debugInfo = gl.getExtension('WEBGL_debug_renderer_info');
        return debugInfo ? gl.getParameter(debugInfo.UNMASKED_RENDERER_WEBGL) : null;
    } catch (e) {
        return null;
    }
}

function getWebGLVendor() {
    try {
        const canvas = document.createElement('canvas');
        const gl = canvas.getContext('webgl') || canvas.getContext('experimental-webgl');
        if (!gl) return null;
        const debugInfo = gl.getExtension('WEBGL_debug_renderer_info');
        return debugInfo ? gl.getParameter(debugInfo.UNMASKED_VENDOR_WEBGL) : null;
    } catch (e) {
        return null;
    }
}

// Audio fingerprinting
function getAudioFingerprint() {
    try {
        const audioContext = new (window.AudioContext || window.webkitAudioContext)();
        const oscillator = audioContext.createOscillator();
        const analyser = audioContext.createAnalyser();
        const gain = audioContext.createGain();
        const processor = audioContext.createScriptProcessor(4096, 1, 1);

        gain.gain.value = 0;
        oscillator.type = 'triangle';
        oscillator.connect(analyser);
        analyser.connect(processor);
        processor.connect(gain);
        gain.connect(audioContext.destination);
        oscillator.start(0);

        const data = new Float32Array(analyser.frequencyBinCount);
        analyser.getFloatFrequencyData(data);

        oscillator.stop();
        audioContext.close();

        let hash = 0;
        for (let i = 0; i < data.length; i++) {
            hash = ((hash << 5) - hash) + (data[i] | 0);
            hash = hash & hash;
        }
        return hash.toString(16);
    } catch (e) {
        return null;
    }
}

// Proof-of-Work solver
async function solvePoW(challenge, difficulty) {
    const target = BigInt(2) ** BigInt(256 - difficulty);
    let nonce = 0;

    while (true) {
        const input = challenge + nonce;
        const hashBuffer = await crypto.subtle.digest(
            'SHA-256',
            new TextEncoder().encode(input)
        );
        const hashArray = new Uint8Array(hashBuffer);
        const hashHex = Array.from(hashArray)
            .map(b => b.toString(16).padStart(2, '0'))
            .join('');
        const hashBigInt = BigInt('0x' + hashHex);

        if (hashBigInt < target) {
            return nonce;
        }

        nonce++;

        // Yield to prevent UI blocking
        if (nonce % 10000 === 0) {
            await new Promise(resolve => setTimeout(resolve, 0));
        }
    }
}
</script>
```

### Option 3: Pre-flight Challenge for Forms

Protect form submissions by requiring challenge verification before submit:

```html
<form id="protected-form" action="/api/submit" method="POST">
    <input type="text" name="data" required>
    <div id="aegis-widget"></div>
    <button type="submit" id="submit-btn" disabled>Submit</button>
</form>

<script>
document.addEventListener('DOMContentLoaded', async () => {
    const form = document.getElementById('protected-form');
    const submitBtn = document.getElementById('submit-btn');
    const widget = document.getElementById('aegis-widget');

    // Show loading state
    widget.innerHTML = '<p>Verifying...</p>';

    try {
        // Load and solve challenge
        const challenge = await loadAegisChallenge();
        const token = await solveChallenge(challenge);

        // Enable form submission
        widget.innerHTML = '<p style="color: green;">✓ Verified</p>';
        submitBtn.disabled = false;

        // Add token to form as hidden field (optional, also in cookie)
        const tokenInput = document.createElement('input');
        tokenInput.type = 'hidden';
        tokenInput.name = 'aegis_token';
        tokenInput.value = token;
        form.appendChild(tokenInput);

    } catch (error) {
        widget.innerHTML = '<p style="color: red;">Verification failed. Please refresh.</p>';
        console.error('Challenge failed:', error);
    }
});
</script>
```

## API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `https://prjaegis.org/aegis/challenge/issue` | GET | Issue new challenge |
| `https://prjaegis.org/aegis/challenge/issue?type=invisible` | GET | Issue invisible challenge |
| `https://prjaegis.org/aegis/challenge/page` | GET | Get full challenge HTML page |
| `https://prjaegis.org/aegis/challenge/verify` | POST | Submit challenge solution |
| `https://prjaegis.org/aegis/challenge/verify-token` | POST | Verify existing token |
| `https://prjaegis.org/aegis/challenge/public-key` | GET | Get Ed25519 public key |
| `https://prjaegis.org/aegis/challenge/health` | GET | Health check |

### Issue Challenge Response

```json
{
    "id": "abc123...",
    "challenge_type": "managed",
    "pow_challenge": "random64chars...",
    "pow_difficulty": 16,
    "issued_at": 1700000000,
    "expires_at": 1700000300,
    "client_ip": "192.168.1.1"
}
```

### Verify Solution Request

```json
{
    "challenge_id": "abc123...",
    "pow_nonce": 12345,
    "fingerprint": {
        "canvas_hash": "...",
        "webgl_renderer": "...",
        "webgl_vendor": "...",
        "audio_hash": "...",
        "screen": {
            "width": 1920,
            "height": 1080,
            "color_depth": 24,
            "pixel_ratio": 2.0
        },
        "timezone_offset": -480,
        "language": "en-US",
        "platform": "MacIntel",
        "cpu_cores": 8,
        "device_memory": 16.0,
        "touch_support": false,
        "webdriver_detected": false,
        "plugins_count": 5
    }
}
```

### Verify Solution Response

```json
{
    "success": true,
    "token": "eyJ...",
    "score": 85,
    "issues": []
}
```

Or on failure:

```json
{
    "success": false,
    "token": null,
    "error": "Invalid proof-of-work",
    "score": 0,
    "issues": ["invalid_pow"]
}
```

## Token Usage

After successful verification, the token is:
1. **Set as cookie**: `aegis_token=<token>; Path=/; Max-Age=900; SameSite=Strict; HttpOnly`
2. **Returned in response**: `result.token`

For subsequent requests, include the token via:
- **Cookie** (automatic): `aegis_token=<token>`
- **Header** (explicit): `X-Aegis-Token: <token>`

Token validity: **15 minutes** (configurable)

## Bot Detection Signals

The fingerprint analysis detects:

| Signal | Suspicion Level | Description |
|--------|-----------------|-------------|
| `webdriver_detected` | High | Browser automation detected |
| `plugins_count: 0` | Medium | No browser plugins (common in headless) |
| `Google SwiftShader` | High | Software renderer (headless Chrome) |
| `invalid_screen` | Medium | Screen dimensions 0x0 |
| `empty_canvas` | Medium | Canvas fingerprinting blocked |

## Security Notes

1. **IP Binding**: Tokens are bound to client IP (with constant-time comparison)
2. **Fingerprint Binding**: Token includes fingerprint hash
3. **Ed25519 Signatures**: Tokens are cryptographically signed
4. **PoW Difficulty**: Default 16 bits (~65K iterations, <1 second on modern browsers)
5. **Challenge Expiry**: Challenges expire after 5 minutes
6. **One-Time Use**: Each challenge can only be solved once

## Proxy Configuration

Enable challenge manager in your AEGIS proxy:

```rust
use aegis_node::challenge::ChallengeManager;
use aegis_node::pingora_proxy::AegisProxy;

let challenge_manager = Arc::new(ChallengeManager::new());

let proxy = AegisProxy::new(/* ... */)
    .with_challenge_manager(challenge_manager);
```

Or run the standalone Challenge API server:

```rust
use aegis_node::challenge_api::{ChallengeApi, run_challenge_api};

let api = Arc::new(ChallengeApi::new(challenge_manager));
run_challenge_api("0.0.0.0:8081".parse().unwrap(), api).await?;
```

## Testing

Verify the integration:

```bash
# Health check
curl https://prjaegis.org/aegis/challenge/health

# Get challenge
curl https://prjaegis.org/aegis/challenge/issue

# Get challenge page
curl https://prjaegis.org/aegis/challenge/page

# Get public key
curl https://prjaegis.org/aegis/challenge/public-key
```

## Customization

The challenge page HTML can be customized by modifying `generate_challenge_page()` in `challenge.rs`. The default page includes:
- AEGIS branding
- Loading spinner
- Dark theme with gradient background
- "Protected by AEGIS" footer
