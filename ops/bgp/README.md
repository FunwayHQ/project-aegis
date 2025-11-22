# BGP/BIRD Configuration

BIRD v2 routing daemon configuration for AEGIS edge nodes to participate in the global anycast network.

## Overview

Each AEGIS edge node runs BIRD v2 to:
- Announce the anycast prefix (203.0.113.0/24) to upstream BGP peers
- Receive routes from transit providers and IXPs
- Validate route origins using RPKI via Routinator
- Provide fast failover using BFD
- Withdraw announcements when local service is unhealthy

## Files

- `bird.conf` - Main BIRD v2 configuration file
- `check-health.sh` - Health check script that withdraws routes on failure
- `birdc-commands.md` - Common BIRD control commands reference

## Configuration

### 1. Update Router ID

Edit `bird.conf` and set a unique router ID for each edge node:

```
router id 10.0.0.1;  # Replace with node's primary IP
```

### 2. Configure AS Number

Set your assigned AS number:

```
define AEGIS_AS = 64512;  # Replace with actual AS number
```

### 3. Configure Anycast Prefixes

Update the anycast IP ranges:

```
define AEGIS_ANYCAST_V4 = 203.0.113.0/24;  # Your anycast IPv4
define AEGIS_ANYCAST_V6 = 2001:db8::/32;   # Your anycast IPv6
```

### 4. Add BGP Peers

Add your actual BGP peers (transit providers, IXPs, private peers):

```bird
protocol bgp transit1 from bgp_peer {
    description "Your Transit Provider";
    neighbor 192.0.2.1 as 65001;
    password "YourSecurePassword";
}
```

## Deployment

### Installation

```bash
# Ubuntu/Debian
sudo apt-get install bird2

# RedHat/CentOS
sudo yum install bird

# Verify version (must be 2.x)
bird --version
```

### Copy Configuration

```bash
sudo cp bird.conf /etc/bird/bird.conf
sudo chown root:root /etc/bird/bird.conf
sudo chmod 644 /etc/bird/bird.conf
```

### Validate Configuration

```bash
# Parse configuration without starting daemon
sudo bird -p -c /etc/bird/bird.conf

# Expected output: "Configuration OK"
```

### Start BIRD

```bash
# Enable and start service
sudo systemctl enable bird
sudo systemctl start bird

# Check status
sudo systemctl status bird
```

## RPKI Validation

BIRD integrates with Routinator for RPKI (Resource Public Key Infrastructure) validation.

### Install Routinator

```bash
# Download latest release
curl -LO https://github.com/NLnetLabs/routinator/releases/latest/download/routinator-x86_64-unknown-linux-musl.tar.gz

# Extract
tar -xzf routinator-x86_64-unknown-linux-musl.tar.gz

# Move to PATH
sudo mv routinator /usr/local/bin/

# Initialize
routinator init
```

### Run Routinator

```bash
# Run as RTR server on port 3323
routinator server --rtr 127.0.0.1:3323

# Or run as systemd service
sudo systemctl enable routinator
sudo systemctl start routinator
```

### Verify RPKI

```bash
# Check RPKI connection in BIRD
sudo birdc show protocols rpki_validator

# Expected: state should be "Established"
```

## Health Check Integration

The health check script monitors the local AEGIS service and withdraws the anycast route if unhealthy.

### Setup Health Check

```bash
# Copy health check script
sudo cp check-health.sh /usr/local/bin/aegis-health-check
sudo chmod +x /usr/local/bin/aegis-health-check

# Add to cron (check every minute)
echo "* * * * * /usr/local/bin/aegis-health-check" | sudo crontab -
```

### How It Works

1. Script checks if River proxy is responding on port 80
2. If unhealthy for 3 consecutive checks: withdraw anycast route
3. If healthy again: re-announce anycast route
4. Uses `birdc` to dynamically control BIRD

## Common Operations

### View BGP Sessions

```bash
sudo birdc show protocols
```

Output:
```
Name       Proto      Table      State  Since         Info
transit1   BGP        ---        up     2025-11-22    Established
ixp1       BGP        ---        up     2025-11-22    Established
```

### View Routes

```bash
# Show all BGP routes
sudo birdc show route protocol transit1

# Show exported routes
sudo birdc show route export transit1

# Show RPKI validation status
sudo birdc show route all where roa_check(rpki4, net, bgp_path.last) != ROA_VALID
```

### Restart BIRD

```bash
# Graceful restart (maintains BGP sessions)
sudo systemctl reload bird

# Full restart
sudo systemctl restart bird
```

### Emergency Route Withdrawal

```bash
# Manually withdraw anycast route
sudo birdc configure

# Or disable specific peer
sudo birdc disable transit1
```

## Monitoring

### Key Metrics

Monitor these metrics for production:

- **BGP Session State**: All peers should be "Established"
- **Route Count**: Should announce exactly 1 route (anycast prefix)
- **RPKI State**: Should be "Established" with Routinator
- **BFD Sessions**: Fast failover detection active

### Logs

```bash
# View BIRD logs
sudo journalctl -u bird -f

# View last 100 lines
sudo journalctl -u bird -n 100
```

### Common Issues

**BGP session stuck in "Connect" state:**
- Check network connectivity: `ping <neighbor-ip>`
- Verify firewall allows TCP port 179
- Confirm neighbor IP and AS number are correct

**RPKI validation failing:**
- Check Routinator is running: `systemctl status routinator`
- Verify RTR connection: `birdc show protocols rpki_validator`
- Check Routinator logs: `journalctl -u routinator -f`

**Routes not being announced:**
- Verify export filter: `birdc show route export <peer>`
- Check anycast route exists: `birdc show route static4`
- Ensure service is healthy (health check not withdrawn route)

## Security

### BGP Authentication

Always use MD5 authentication for BGP sessions:

```bird
protocol bgp peer1 from bgp_peer {
    password "VerySecurePassword123!";
}
```

### RPKI Validation

RPKI prevents route hijacking by validating route origins:

- **ROA_VALID**: Route origin matches RPKI database
- **ROA_INVALID**: Route origin doesn't match (rejected)
- **ROA_UNKNOWN**: No RPKI data available (accepted with caution)

### Route Filtering

The configuration includes:
- Bogon prefix filtering (RFC 1918, etc.)
- Private AS number filtering
- Route limit protection (max 10,000 routes per peer)
- Automatic session reset on limit violation

## Troubleshooting

### Test Configuration Changes

```bash
# Parse config without applying
sudo bird -p -c /etc/bird/bird.conf

# Apply changes if valid
sudo systemctl reload bird
```

### Debug BGP Session

```bash
# Enable debug logging for specific protocol
sudo birdc debug transit1 all

# Disable debug
sudo birdc debug transit1 off
```

### Verify Anycast IP

```bash
# Check anycast IP is configured on loopback
ip addr show lo

# Should show: 203.0.113.1/32
```

### Check Route Propagation

```bash
# From another network, check if route is visible
# Use public looking glass or traceroute
traceroute 203.0.113.1
```

## Production Checklist

Before going to production:

- [ ] Router ID configured correctly
- [ ] AS number updated
- [ ] Anycast prefixes updated with actual allocations
- [ ] All BGP peers configured with correct IPs and AS numbers
- [ ] MD5 passwords configured for all peers
- [ ] Routinator installed and running
- [ ] RPKI validation working
- [ ] Health check script installed and running
- [ ] Monitoring and alerting configured
- [ ] Logs being collected centrally
- [ ] Tested route withdrawal and re-announcement
- [ ] Documented runbook for common operations

## References

- [BIRD Internet Routing Daemon](https://bird.network.cz/)
- [BIRD v2 Documentation](https://bird.network.cz/?get_doc&v=20)
- [Routinator RPKI Validator](https://github.com/NLnetLabs/routinator)
- [RFC 4271 - BGP-4](https://tools.ietf.org/html/rfc4271)
- [RFC 6811 - BGP Prefix Origin Validation](https://tools.ietf.org/html/rfc6811)
