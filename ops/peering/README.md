# BGP Peering Manager

Automated BGP peering configuration generation for AEGIS edge nodes using Jinja2 templates.

## Overview

The Peering Manager automates the generation of BIRD configuration files from structured data:

- **Template-based**: Use Jinja2 templates for BIRD configs
- **Data-driven**: Peering sessions defined in YAML
- **Validation**: Automated config validation before deployment
- **Multi-site**: Generate configs for multiple edge locations
- **Version control**: All configs tracked in Git

## Directory Structure

```
peering/
├── templates/
│   ├── bird.conf.j2        # Main BIRD config template
│   ├── peer.conf.j2        # BGP peer template
│   └── filters.conf.j2     # Filter template
├── data/
│   ├── global.yaml         # Global settings (AS number, anycast prefix)
│   ├── peers/              # Per-peer configuration files
│   │   ├── transit1.yaml
│   │   ├── ixp1.yaml
│   │   └── private1.yaml
│   └── nodes/              # Per-node configuration
│       ├── edge-us-east.yaml
│       ├── edge-eu-west.yaml
│       └── edge-ap-south.yaml
└── generate.py             # Configuration generator script
```

## Configuration Format

### Global Settings (`data/global.yaml`)

```yaml
# Global AEGIS BGP configuration
as_number: 64512
anycast_v4: 203.0.113.0/24
anycast_v6: 2001:db8::/32

rpki:
  enabled: true
  validator_host: 127.0.0.1
  validator_port: 3323

bfd:
  enabled: true
  min_rx_interval: 100  # milliseconds
  min_tx_interval: 100
  multiplier: 3

route_limits:
  max_import: 10000
  max_export: 10

communities:
  edge_node: "64512:1"
  do_not_advertise: "64512:666"
```

### Peer Configuration (`data/peers/transit1.yaml`)

```yaml
name: transit1
description: "Transit Provider 1"
neighbor_ip: 192.0.2.1
neighbor_as: 65001
peer_type: transit  # transit, ixp, or private

authentication:
  enabled: true
  password: "SecurePassword123"

multihop: 2

prepend_count: 0  # AS path prepending

import_limit: 100000
export_limit: 10

local_pref: 100  # BGP local preference

enabled: true
```

### Node Configuration (`data/nodes/edge-us-east.yaml`)

```yaml
node_name: edge-us-east
router_id: 10.0.1.1
location: "US East (Virginia)"

interfaces:
  primary: eth0
  loopback: lo

anycast_ips:
  v4: 203.0.113.1/32
  v6: 2001:db8::1/128

peers:
  - transit1
  - ixp1
  - private1

# Node-specific overrides
overrides:
  transit1:
    local_pref: 200  # Prefer this transit at this location
```

## Usage

### Generate Configuration

```bash
# Generate BIRD config for specific node
python3 generate.py --node edge-us-east --output /etc/bird/bird.conf

# Generate for all nodes
python3 generate.py --all --output-dir ./output/

# Validate generated config
bird -p -c ./output/edge-us-east/bird.conf
```

### Deploy to Node

```bash
# Copy generated config to node
scp output/edge-us-east/bird.conf root@edge-us-east:/etc/bird/

# Reload BIRD configuration
ssh root@edge-us-east 'systemctl reload bird'

# Verify BGP sessions
ssh root@edge-us-east 'birdc show protocols'
```

### Automate with Ansible

```bash
# Generate and deploy to all nodes
ansible-playbook -i inventory.yaml deploy-bird-config.yaml
```

## Configuration Generator Script

See `generate.py` for the implementation. It:

1. Loads global settings
2. Loads peer definitions
3. Loads node-specific configuration
4. Renders Jinja2 templates
5. Validates generated BIRD config
6. Outputs to specified location

## Template Examples

### Peer Template (`templates/peer.conf.j2`)

```jinja2
protocol bgp {{ peer.name }} from bgp_peer {
    description "{{ peer.description }}";
    neighbor {{ peer.neighbor_ip }} as {{ peer.neighbor_as }};
    {% if peer.authentication.enabled %}
    password "{{ peer.authentication.password }}";
    {% endif %}
    {% if peer.multihop %}
    multihop {{ peer.multihop }};
    {% endif %}
    {% if peer.local_pref %}
    default bgp_local_pref {{ peer.local_pref }};
    {% endif %}
}
```

## Adding New Peer

1. Create peer configuration file:
   ```bash
   cat > data/peers/newpeer.yaml <<EOF
   name: newpeer
   description: "New BGP Peer"
   neighbor_ip: 198.51.100.1
   neighbor_as: 65010
   peer_type: private
   authentication:
     enabled: true
     password: "GenerateSecurePassword"
   enabled: true
   EOF
   ```

2. Add peer to node configuration:
   ```bash
   # Edit data/nodes/edge-us-east.yaml
   peers:
     - transit1
     - ixp1
     - newpeer  # Add here
   ```

3. Regenerate and deploy:
   ```bash
   python3 generate.py --node edge-us-east
   # Review, test, deploy
   ```

## Best Practices

### Security

- Store passwords in encrypted files (Ansible Vault, SOPS)
- Use strong BGP authentication passwords (20+ characters)
- Limit AS path prepending (avoid routing loops)
- Set conservative route import/export limits

### Reliability

- Always validate configs before deployment: `bird -p -c config`
- Test on staging environment first
- Deploy changes during maintenance windows
- Monitor BGP sessions after configuration changes

### Maintenance

- Keep peer configurations in version control
- Document reason for each peer (transit SLA, IXP location, etc.)
- Regularly audit active peering sessions
- Remove inactive peers from configuration

## Integration with FluxCD

Future: Automate config generation and deployment via GitOps

```yaml
# Example: Flux Kustomization for peering
apiVersion: kustomize.toolkit.fluxcd.io/v1
kind: Kustomization
metadata:
  name: bgp-peering
spec:
  interval: 1h
  path: ./ops/peering/generated
  postBuild:
    substitute:
      NODE_NAME: ${NODE_NAME}
```

## Monitoring

Monitor these metrics:

- BGP session state (should be "Established")
- Route count (import/export)
- Prefix changes (detect route leaks)
- RPKI invalid routes (should be 0)

```bash
# Check all sessions
birdc show protocols

# Check imported routes
birdc show route protocol transit1

# Check RPKI validation
birdc show route all where roa_check(rpki4, net, bgp_path.last) = ROA_INVALID
```

## Production Checklist

- [ ] Global settings configured
- [ ] All BGP peers defined in YAML
- [ ] Node configurations created
- [ ] Generator script tested
- [ ] Generated configs validated with `bird -p`
- [ ] Deployed to staging environment
- [ ] BGP sessions established
- [ ] Route announcement verified
- [ ] Ansible/automation playbooks created
- [ ] Integration with FluxCD planned

## References

- [BIRD Configuration](https://bird.network.cz/?get_doc&v=20)
- [Jinja2 Templates](https://jinja.palletsprojects.com/)
- [BGP Best Practices](https://www.cisco.com/c/en/us/support/docs/ip/border-gateway-protocol-bgp/13753-25.html)
