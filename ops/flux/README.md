# FluxCD GitOps Configuration

FluxCD provides GitOps continuous deployment for AEGIS edge nodes, automatically syncing Kubernetes manifests from Git repository to the cluster.

## Overview

FluxCD monitors the Git repository and automatically applies changes to the Kubernetes cluster:

- **Pull-based deployment**: FluxCD pulls changes from Git (more secure than push-based CI/CD)
- **Automatic synchronization**: Changes detected within 1 minute
- **Declarative configuration**: All infrastructure defined in Git
- **Rollback capability**: Revert to any Git commit
- **Integration with Flagger**: Progressive canary deployments

## Architecture

```
┌─────────────┐         ┌─────────────┐         ┌─────────────┐
│   GitHub    │ ◄──────►│   FluxCD    │ ◄──────►│  K3s/K8s    │
│ Repository  │         │   Controller│         │   Cluster   │
└─────────────┘         └─────────────┘         └─────────────┘
       │                       │                       │
       │                       ▼                       │
       │                ┌─────────────┐               │
       │                │   Flagger   │               │
       │                │   Canary    │───────────────┘
       │                └─────────────┘
       │
       └──► Git commit triggers sync
```

## Components

### 1. Flux System (`flux-system/`)
- Core FluxCD controllers
- GitRepository source definition
- Kustomization sync configuration

### 2. Infrastructure (`infrastructure/`)
- Base infrastructure components
- DragonflyDB, BIRD, system services
- Deployed before applications

### 3. Applications (`apps/`)
- Application deployments
- River proxy, WAF, bot management
- Depends on infrastructure being ready

## Installation

### Prerequisites

```bash
# Install Flux CLI
curl -s https://fluxcd.io/install.sh | sudo bash

# Verify installation
flux --version
```

### Bootstrap FluxCD

```bash
# Export GitHub credentials
export GITHUB_TOKEN=<your-token>
export GITHUB_USER=<your-username>
export GITHUB_REPO=project-aegis

# Bootstrap Flux on the cluster
flux bootstrap github \
  --owner=$GITHUB_USER \
  --repository=$GITHUB_REPO \
  --branch=main \
  --path=ops/flux \
  --personal \
  --private=false
```

This will:
1. Install FluxCD controllers in the cluster
2. Create necessary CRDs (Custom Resource Definitions)
3. Configure Git repository as source
4. Start monitoring for changes

### Manual Installation (Alternative)

```bash
# Install Flux system
kubectl apply -f flux/flux-system/

# Apply Git repository source
kubectl apply -f flux/git-repository.yaml

# Apply Kustomizations
kubectl apply -f flux/infrastructure.yaml
kubectl apply -f flux/apps.yaml
```

## Configuration Files

### Git Repository Source

Defines where FluxCD pulls manifests from:

```yaml
# git-repository.yaml
apiVersion: source.toolkit.fluxcd.io/v1
kind: GitRepository
metadata:
  name: aegis-repo
spec:
  interval: 1m
  url: https://github.com/FunwayHQ/project-aegis
  ref:
    branch: main
```

### Kustomization

Defines what to deploy and when:

```yaml
# infrastructure.yaml
apiVersion: kustomize.toolkit.fluxcd.io/v1
kind: Kustomization
metadata:
  name: infrastructure
spec:
  interval: 10m
  sourceRef:
    kind: GitRepository
    name: aegis-repo
  path: ./ops/k3s
  prune: true
  wait: true
```

## Deployment Workflow

### Standard Deployment

1. **Developer commits change** to `ops/k3s/river-proxy.yaml`
2. **Push to GitHub** on main branch
3. **FluxCD detects change** within 1 minute
4. **Flagger initiates canary** deployment (if configured)
5. **Progressive rollout**: 1% → 10% → 50% → 100%
6. **Automatic rollback** if error rate increases

### Emergency Rollback

```bash
# Rollback to previous Git commit
git revert HEAD
git push origin main

# FluxCD automatically reverts to previous state
# Wait 1 minute for sync
```

### Manual Sync

```bash
# Force immediate sync (don't wait for interval)
flux reconcile kustomization infrastructure --with-source

# Check sync status
flux get kustomizations
```

## Monitoring

### Check FluxCD Status

```bash
# View all Flux resources
flux get all

# Check Git repository sync
flux get sources git

# Check Kustomization status
flux get kustomizations

# View recent reconciliation logs
flux logs --level=info --all-namespaces
```

### Troubleshooting

**FluxCD not syncing:**

```bash
# Check if controllers are running
kubectl get pods -n flux-system

# Check repository access
flux get sources git aegis-repo

# View detailed logs
kubectl logs -n flux-system deploy/source-controller
kubectl logs -n flux-system deploy/kustomize-controller
```

**Deployment stuck:**

```bash
# Check for errors in Kustomization
kubectl describe kustomization infrastructure -n flux-system

# View events
kubectl get events -n flux-system --sort-by='.lastTimestamp'
```

**Reconciliation failing:**

```bash
# Suspend auto-reconciliation
flux suspend kustomization infrastructure

# Fix the issue in Git

# Resume reconciliation
flux resume kustomization infrastructure
```

## Integration with Flagger

FluxCD works with Flagger for progressive canary deployments:

```yaml
# Example: Canary deployment for River proxy
apiVersion: flagger.app/v1beta1
kind: Canary
metadata:
  name: river-proxy
spec:
  targetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: river-proxy
  progressDeadlineSeconds: 60
  service:
    port: 80
  analysis:
    interval: 1m
    threshold: 5
    maxWeight: 50
    stepWeight: 10
    metrics:
    - name: request-success-rate
      thresholdRange:
        min: 99
      interval: 1m
```

Deployment flow:
1. FluxCD detects new image tag in Git
2. Flagger creates canary deployment
3. Traffic gradually shifted: 10% → 20% → 30% → ...
4. Metrics monitored at each step
5. Automatic rollback if success rate < 99%
6. Full promotion if all checks pass

## Security Best Practices

### Git Authentication

Use deploy keys instead of personal tokens:

```bash
# Generate deploy key
ssh-keygen -t ed25519 -C "flux-aegis-deploy"

# Add public key to GitHub repository
# Settings → Deploy keys → Add deploy key
# ✓ Allow write access (for status updates)

# Configure Flux to use SSH
flux create source git aegis-repo \
  --url=ssh://git@github.com/FunwayHQ/project-aegis \
  --branch=main \
  --interval=1m \
  --ssh-key-algorithm=ed25519
```

### Secret Management

Don't commit secrets to Git. Use Sealed Secrets or SOPS:

```bash
# Install Sealed Secrets controller
kubectl apply -f https://github.com/bitnami-labs/sealed-secrets/releases/download/v0.18.0/controller.yaml

# Seal a secret
kubeseal --format=yaml < secret.yaml > sealed-secret.yaml

# Commit sealed-secret.yaml to Git (safe)
# Controller decrypts it in the cluster
```

## Configuration as Code

### Directory Structure

```
ops/flux/
├── flux-system/          # FluxCD core components
│   ├── gotk-components.yaml
│   ├── gotk-sync.yaml
│   └── kustomization.yaml
├── infrastructure/       # Infrastructure layer
│   ├── kustomization.yaml
│   └── sources.yaml
└── apps/                # Application layer
    ├── kustomization.yaml
    └── river-proxy.yaml
```

### Dependency Management

Infrastructure deployed before apps:

```yaml
# apps/kustomization.yaml
apiVersion: kustomize.toolkit.fluxcd.io/v1
kind: Kustomization
metadata:
  name: apps
spec:
  dependsOn:
    - name: infrastructure  # Wait for infra first
  path: ./ops/k3s
  prune: true
```

## Production Checklist

Before enabling FluxCD in production:

- [ ] FluxCD controllers deployed and healthy
- [ ] Git repository accessible from cluster
- [ ] Deploy keys configured (not personal tokens)
- [ ] Kustomizations applied successfully
- [ ] Test manual sync: `flux reconcile kustomization infrastructure`
- [ ] Test automatic sync: commit change and wait 1 minute
- [ ] Flagger integrated for canary deployments
- [ ] Monitoring and alerting configured
- [ ] Tested rollback procedure
- [ ] Documented emergency procedures

## Common Operations

### Update Deployment

```bash
# 1. Update manifest in Git
vim ops/k3s/river-proxy.yaml

# 2. Commit and push
git add ops/k3s/river-proxy.yaml
git commit -m "Update River proxy to v1.2.0"
git push origin main

# 3. Watch deployment (automatic)
flux logs --follow --level=info

# 4. Verify deployment
kubectl get pods -n aegis -l app=river-proxy
```

### Add New Service

```bash
# 1. Create manifest
cat > ops/k3s/new-service.yaml << EOF
apiVersion: apps/v1
kind: Deployment
...
EOF

# 2. Commit and push
git add ops/k3s/new-service.yaml
git commit -m "Add new service"
git push origin main

# 3. FluxCD automatically deploys
```

### Pause Deployments

```bash
# Suspend all reconciliation
flux suspend kustomization infrastructure
flux suspend kustomization apps

# Make manual changes
kubectl apply -f manual-fix.yaml

# Resume automatic reconciliation
flux resume kustomization infrastructure
flux resume kustomization apps
```

## References

- [FluxCD Documentation](https://fluxcd.io/docs/)
- [GitOps Principles](https://opengitops.dev/)
- [Flagger Progressive Delivery](https://flagger.app/)
- [Sealed Secrets](https://github.com/bitnami-labs/sealed-secrets)
