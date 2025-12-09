# @aegis/captcha-react

React component for AEGIS CAPTCHA - a privacy-respecting, decentralized alternative to reCAPTCHA.

## Features

- **Privacy-First**: No tracking, no third-party dependencies
- **Proof-of-Work**: Uses SHA-256 computational puzzles instead of user data
- **Browser Fingerprinting**: Detects headless browsers and bots
- **Ed25519 Signed Tokens**: Cryptographically verified challenge tokens
- **TypeScript**: Full type support
- **Accessible**: ARIA compliant
- **Customizable**: Themes, sizes, and callbacks

## Installation

```bash
npm install @aegis/captcha-react
# or
yarn add @aegis/captcha-react
# or
pnpm add @aegis/captcha-react
```

## Quick Start

```tsx
import { AegisCaptcha } from '@aegis/captcha-react';

function ContactForm() {
  const [token, setToken] = useState<string | null>(null);

  return (
    <form>
      {/* Your form fields */}

      <AegisCaptcha
        onSuccess={(token) => setToken(token)}
        onError={(error) => console.error('CAPTCHA error:', error)}
      />

      <button type="submit" disabled={!token}>
        Submit
      </button>
    </form>
  );
}
```

## Props

| Prop | Type | Default | Description |
|------|------|---------|-------------|
| `apiEndpoint` | `string` | `https://api.aegis.network` | API endpoint for challenges |
| `challengeType` | `'invisible' \| 'managed' \| 'interactive'` | `'managed'` | Challenge type |
| `theme` | `'dark' \| 'light'` | `'dark'` | Widget theme |
| `size` | `'compact' \| 'normal' \| 'large'` | `'normal'` | Widget size |
| `onSuccess` | `(token: string) => void` | - | Called on successful verification |
| `onError` | `(error: Error) => void` | - | Called on error |
| `onExpired` | `() => void` | - | Called when token expires |
| `onLoad` | `() => void` | - | Called when widget loads |
| `debug` | `boolean` | `false` | Enable console logging |
| `className` | `string` | - | Custom CSS class |
| `style` | `React.CSSProperties` | - | Custom styles |

## Ref Methods

Access methods via ref:

```tsx
import { useRef } from 'react';
import { AegisCaptcha, AegisCaptchaRef } from '@aegis/captcha-react';

function Form() {
  const captchaRef = useRef<AegisCaptchaRef>(null);

  const handleSubmit = async () => {
    // Programmatically execute
    const token = await captchaRef.current?.execute();

    if (token) {
      // Submit form with token
    }
  };

  const handleReset = () => {
    captchaRef.current?.reset();
  };

  return (
    <>
      <AegisCaptcha ref={captchaRef} />
      <button onClick={handleSubmit}>Submit</button>
      <button onClick={handleReset}>Reset</button>
    </>
  );
}
```

### Available Methods

| Method | Returns | Description |
|--------|---------|-------------|
| `execute()` | `Promise<string \| null>` | Execute the challenge |
| `reset()` | `void` | Reset the widget |
| `getToken()` | `string \| null` | Get current token |
| `isVerified()` | `boolean` | Check verification status |

## Challenge Types

### Invisible
Runs automatically in the background. Best for low-friction experiences.

```tsx
<AegisCaptcha challengeType="invisible" onSuccess={setToken} />
```

### Managed (Default)
Shows a checkbox that users click. Balances security and UX.

```tsx
<AegisCaptcha challengeType="managed" onSuccess={setToken} />
```

### Interactive
Highest security. May show visual challenges for suspicious traffic.

```tsx
<AegisCaptcha challengeType="interactive" onSuccess={setToken} />
```

## Theming

### Dark Theme (Default)
```tsx
<AegisCaptcha theme="dark" />
```

### Light Theme
```tsx
<AegisCaptcha theme="light" />
```

### Custom Styling
```tsx
<AegisCaptcha
  className="my-captcha"
  style={{ borderRadius: 16 }}
/>
```

## How It Works

1. **User clicks checkbox** - Initiates the challenge
2. **Browser collects fingerprint** - Canvas, WebGL, audio, etc.
3. **Proof-of-Work computation** - Browser solves SHA-256 puzzle (Web Worker)
4. **Server verification** - AEGIS validates solution and fingerprint
5. **Token issued** - Ed25519 signed JWT stored in cookie

## Server-Side Verification

Always verify tokens server-side:

```typescript
// Node.js example
const response = await fetch('https://api.aegis.network/aegis/token/verify', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ token }),
});

const result = await response.json();
if (result.valid) {
  // Token is valid, process the request
}
```

## Browser Support

- Chrome 80+
- Firefox 75+
- Safari 14+
- Edge 80+

Requires:
- Web Workers
- Web Crypto API (crypto.subtle)
- ES2020

## License

MIT
