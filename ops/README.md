# AEGIS Operations Infrastructure

This directory contains all production infrastructure configurations for deploying and managing AEGIS edge nodes.

## Directory Structure

```
ops/
├── bgp/           # BGP/BIRD routing configurations
├── k3s/           # Kubernetes (K3s) manifests for services
├── flux/          # FluxCD GitOps configurations
├── cilium/        # Cilium eBPF orchestration
├── acme/          # ACME/Let's Encrypt certificate management
└── peering/       # BGP peering manager configurations
```

## Overview

### BGP/BIRD (`bgp/`)
- BIRD v2 routing daemon configurations
- Anycast IP addressing for global edge network
- Route announcement and BGP session management
- RPKI validation via Routinator integration

### K3s (`k3s/`)
- Lightweight Kubernetes manifests
- Service deployments (River proxy, DragonflyDB, BIRD)
- Resource limits and health checks
- ConfigMaps and Secrets management

### FluxCD (`flux/`)
- GitOps continuous deployment
- Automatic synchronization from Git repository
- Deployment manifests and Kustomizations
- Progressive delivery with Flagger integration

### Cilium (`cilium/`)
- eBPF program orchestration
- Network policies for DDoS protection
- XDP program attachment and management
- Integration with blocklist persistence

### ACME (`acme/`)
- Let's Encrypt certificate automation
- cert-manager Kubernetes integration
- Certificate renewal workflows
- DNS-01 and HTTP-01 challenge support

### Peering (`peering/`)
- BGP peering automation
- Jinja2 templates for BIRD configuration generation
- IXP (Internet Exchange Point) configurations
- Automated peering session management

## Deployment Workflow

1. **Initial Setup**
   ```bash
   # Install K3s on edge node
   curl -sfL https://get.k3s.io | sh -

   # Apply K3s manifests
   kubectl apply -f k3s/

   # Install FluxCD
   kubectl apply -f flux/bootstrap/
   ```

2. **BGP Configuration**
   ```bash
   # Generate BIRD config from template
   cd peering && python3 generate_bird_config.py

   # Deploy BIRD via K3s
   kubectl apply -f k3s/bird.yaml
   ```

3. **Cilium Deployment**
   ```bash
   # Install Cilium with eBPF support
   kubectl apply -f cilium/install.yaml

   # Deploy eBPF programs
   kubectl apply -f cilium/ebpf-programs.yaml
   ```

4. **Certificate Management**
   ```bash
   # Install cert-manager
   kubectl apply -f acme/cert-manager.yaml

   # Configure Let's Encrypt issuer
   kubectl apply -f acme/letsencrypt-issuer.yaml
   ```

## Configuration Management

### GitOps with FluxCD

All configuration changes flow through Git:

1. Update manifest in `ops/k3s/` or `ops/flux/`
2. Commit and push to repository
3. FluxCD automatically detects changes
4. Flagger performs canary deployment (1% → 10% → 50% → 100%)
5. Automatic rollback on error rate increase

### Canary Deployment

Flagger monitors metrics and progressively rolls out changes:

```yaml
# Example canary progression
- 1% of nodes receive new config
- Wait 5 minutes, check error rate
- If errors < threshold: promote to 10%
- Continue until 100% or rollback on failure
```

## Monitoring & Observability

### Metrics Collection
- Prometheus scrapes metrics from edge nodes
- DragonflyDB metrics for cache performance
- BIRD metrics for BGP session health
- eBPF/XDP statistics for DDoS mitigation

### Alerting
- Alert on BGP session down
- Alert on high error rates during canary deployment
- Alert on certificate expiration (30 days before)
- Alert on eBPF program failures

## Security Considerations

### RPKI Validation
- Routinator validates route origins via RTR protocol
- Invalid routes automatically rejected by BIRD
- Protects against BGP hijacking

### Network Policies
- Cilium enforces pod-to-pod communication rules
- XDP programs drop malicious traffic at NIC level
- Blocklist integration with threat intelligence

### Certificate Security
- Automatic renewal 30 days before expiration
- Certificates stored in Kubernetes secrets
- Replicated via NATS JetStream for high availability

## High Availability

### Static Stability
- Edge nodes boot independently without control plane
- Last Known Good configuration persisted locally
- Fail open on configuration errors (preserve availability)

### Fault Isolation
- Control plane (dashboard, API) separate from data plane
- Each edge node operates independently
- P2P threat intelligence for decentralized security

## Development

### Testing BGP Configurations
```bash
cd bgp/
bird -p -c bird.conf  # Parse config without starting daemon
```

### Testing K3s Manifests
```bash
cd k3s/
kubectl apply --dry-run=client -f .
kubectl diff -f .
```

### Testing Cilium eBPF Programs
```bash
cd cilium/
# Validate eBPF program before deployment
cilium bpf check ebpf-programs/syn-flood-filter.c
```

## Production Checklist

Before deploying to production:

- [ ] BGP sessions tested with peering routers
- [ ] RPKI validation enabled and working
- [ ] K3s manifests applied successfully
- [ ] FluxCD synchronizing from Git repository
- [ ] Flagger canary deployments configured
- [ ] Cilium eBPF programs attached to interfaces
- [ ] ACME certificates provisioned and auto-renewing
- [ ] Metrics collection and alerting configured
- [ ] Backup and disaster recovery procedures documented

## References

- [BIRD Internet Routing Daemon](https://bird.network.cz/)
- [K3s Lightweight Kubernetes](https://k3s.io/)
- [FluxCD GitOps](https://fluxcd.io/)
- [Cilium eBPF Networking](https://cilium.io/)
- [cert-manager](https://cert-manager.io/)
- [Routinator RPKI](https://github.com/NLnetLabs/routinator)

## Support

For issues or questions:
- GitHub Issues: https://github.com/FunwayHQ/project-aegis/issues
- Documentation: /docs/
- Architecture: /CLAUDE.md
