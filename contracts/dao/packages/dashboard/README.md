<p align="center">
  <img src="./AEGIS-logo.svg" alt="AEGIS" width="400">
</p>

<h1 align="center">@aegis/dashboard</h1>

<p align="center">
Unified management dashboard for the AEGIS decentralized edge network.<br>
Provides a single interface for managing DNS zones, DDoS protection policies, and monitoring network statistics.
</p>

## Features

- **DNS Management**: Create and manage DNS zones, records, and DNSSEC settings
- **DDoS Protection**: Configure protection policies, manage blocklists/allowlists, view attack statistics
- **Real-time Monitoring**: Live event feeds and statistics updates via SSE
- **Unified Interface**: Single dashboard combining DNS and DDoS management

## Installation

```bash
pnpm install
```

## Development

```bash
# Start development server
pnpm dev

# Run tests
pnpm test

# Run tests in watch mode
pnpm test:watch

# Build for production
pnpm build
```

## Project Structure

```
src/
├── components/
│   ├── common/          # Shared components (Layout)
│   ├── dns/             # DNS-specific components
│   │   ├── ZoneCard.tsx
│   │   ├── RecordTable.tsx
│   │   ├── CreateZoneModal.tsx
│   │   └── CreateRecordModal.tsx
│   └── ddos/            # DDoS-specific components
│       ├── StatCard.tsx
│       ├── EventLog.tsx
│       └── AttackChart.tsx
├── contexts/
│   ├── DnsContext.tsx   # DNS state management
│   └── DdosContext.tsx  # DDoS state management
├── pages/
│   ├── Overview.tsx     # Main dashboard
│   ├── Settings.tsx     # Configuration
│   ├── dns/
│   │   ├── Zones.tsx    # Zone management
│   │   ├── Records.tsx  # Record management
│   │   └── Analytics.tsx # DNS statistics
│   └── ddos/
│       ├── Dashboard.tsx   # DDoS overview
│       ├── Blocklist.tsx   # IP management
│       ├── Policies.tsx    # Policy configuration
│       └── Statistics.tsx  # Attack analytics
└── __tests__/           # Test files
```

## Dependencies

- `@aegis/dns-sdk` - DNS API client
- `@aegis/ddos-sdk` - DDoS Protection API client
- `react` - UI framework
- `react-router-dom` - Routing
- `recharts` - Charts and graphs
- `tailwindcss` - Styling

## Environment Variables

```bash
VITE_DNS_API_URL=http://localhost:8054   # DNS API endpoint
VITE_DDOS_API_URL=http://localhost:8080  # DDoS API endpoint
```

## Testing

The dashboard includes comprehensive tests covering:

- Component rendering and interactions
- Context providers and state management
- Navigation and routing
- Form validation

Run tests with:

```bash
pnpm test              # Run once
pnpm test:watch        # Watch mode
pnpm test:coverage     # With coverage report
```

## License

MIT
