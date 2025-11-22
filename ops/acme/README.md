# ACME Certificate Management

Automated TLS certificate provisioning and renewal using cert-manager and Let's Encrypt.

## Overview

AEGIS uses cert-manager to automatically:
- Provision TLS certificates from Let's Encrypt
- Renew certificates 30 days before expiration
- Store certificates in Kubernetes secrets
- Distribute certificates to edge nodes via NATS JetStream (future)

## Installation

### Install cert-manager

```bash
# Install CRDs and controller
kubectl apply -f https://github.com/cert-manager/cert-manager/releases/download/v1.13.0/cert-manager.yaml

# Verify installation
kubectl get pods -n cert-manager
```

### Configure Issuers

```bash
# Apply ClusterIssuers
kubectl apply -f cert-manager.yaml

# Verify issuers are ready
kubectl get clusterissuers
```

Expected output:
```
NAME                   READY   AGE
letsencrypt-prod       True    1m
letsencrypt-staging    True    1m
```

## Certificate Types

### HTTP-01 Challenge

Best for: Single domain certificates

```yaml
apiVersion: cert-manager.io/v1
kind: Certificate
metadata:
  name: aegis-http-cert
spec:
  secretName: aegis-http-tls
  dnsNames:
  - edge.aegis-network.io
  issuerRef:
    name: letsencrypt-prod
    kind: ClusterIssuer
```

Requires: Port 80 accessible from internet

### DNS-01 Challenge

Best for: Wildcard certificates

```yaml
apiVersion: cert-manager.io/v1
kind: Certificate
metadata:
  name: aegis-wildcard-cert
spec:
  secretName: aegis-wildcard-tls
  dnsNames:
  - "*.aegis-network.io"
  issuerRef:
    name: letsencrypt-prod
    kind: ClusterIssuer
```

Requires: DNS provider API credentials (Cloudflare, Route53, etc.)

## Usage

### Request a Certificate

```bash
# Apply certificate resource
kubectl apply -f - <<EOF
apiVersion: cert-manager.io/v1
kind: Certificate
metadata:
  name: my-cert
  namespace: aegis
spec:
  secretName: my-tls-secret
  dnsNames:
  - myservice.aegis-network.io
  issuerRef:
    name: letsencrypt-prod
    kind: ClusterIssuer
EOF

# Watch certificate issuance
kubectl get certificate -n aegis -w
```

### Check Certificate Status

```bash
# View certificate details
kubectl describe certificate aegis-tls -n aegis

# Check if ready
kubectl get certificate aegis-tls -n aegis

# View the actual certificate
kubectl get secret aegis-tls-secret -n aegis -o jsonpath='{.data.tls\.crt}' | base64 -d | openssl x509 -text -noout
```

### Use Certificate in Ingress

```yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: river-proxy-ingress
  annotations:
    cert-manager.io/cluster-issuer: "letsencrypt-prod"
spec:
  tls:
  - hosts:
    - edge.aegis-network.io
    secretName: aegis-tls-secret
  rules:
  - host: edge.aegis-network.io
    http:
      paths:
      - path: /
        pathType: Prefix
        backend:
          service:
            name: river-proxy
            port:
              number: 80
```

## Renewal Process

Certificates are automatically renewed 30 days before expiration.

### Monitor Renewal

```bash
# Check certificate expiration
kubectl get certificates -A

# View cert-manager logs
kubectl logs -n cert-manager deploy/cert-manager -f

# Check renewal events
kubectl get events -n aegis --field-selector involvedObject.name=aegis-tls
```

### Manual Renewal (Testing)

```bash
# Delete certificate to trigger renewal
kubectl delete certificate aegis-tls -n aegis

# Re-apply certificate
kubectl apply -f cert-manager.yaml

# Watch renewal process
kubectl describe certificate aegis-tls -n aegis
```

## DNS Provider Configuration

### Cloudflare

```bash
# Create API token secret
kubectl create secret generic cloudflare-api-token \
  --from-literal=api-token=YOUR_API_TOKEN \
  -n cert-manager

# Update issuer to use Cloudflare DNS-01
kubectl apply -f cert-manager.yaml
```

### Route53 (AWS)

```yaml
solvers:
- dns01:
    route53:
      region: us-east-1
      accessKeyID: AKIAIOSFODNN7EXAMPLE
      secretAccessKeySecretRef:
        name: route53-credentials
        key: secret-access-key
```

## Troubleshooting

### Certificate stuck in "Pending"

```bash
# Check CertificateRequest
kubectl get certificaterequest -n aegis

# Describe for errors
kubectl describe certificaterequest <name> -n aegis

# Check challenge status
kubectl get challenges -n aegis
```

### HTTP-01 Challenge Failing

Common issues:
- Port 80 not accessible from internet
- Firewall blocking Let's Encrypt validation servers
- Ingress controller not configured correctly

```bash
# Test HTTP-01 endpoint manually
curl http://edge.aegis-network.io/.well-known/acme-challenge/test

# Should be accessible from internet
```

### DNS-01 Challenge Failing

Common issues:
- DNS API credentials incorrect
- DNS propagation delay
- Rate limiting from DNS provider

```bash
# Check DNS propagation
dig _acme-challenge.edge.aegis-network.io TXT

# Verify API token has correct permissions
```

### Rate Limiting

Let's Encrypt has rate limits:
- 50 certificates per domain per week
- 5 duplicate certificates per week

Solution:
- Use staging issuer for testing
- Avoid repeatedly requesting the same certificate

## Security

### Production Considerations

1. **Use staging issuer for testing**
   ```bash
   # Test with staging first
   issuerRef:
     name: letsencrypt-staging

   # Switch to prod after testing
   issuerRef:
     name: letsencrypt-prod
   ```

2. **Protect API credentials**
   ```bash
   # Store DNS API tokens in secrets
   kubectl create secret generic dns-credentials \
     --from-file=api-token=token.txt \
     -n cert-manager

   # Delete local copy
   rm token.txt
   ```

3. **Monitor expiration**
   ```bash
   # Set up alerts for certificates expiring in < 30 days
   # Use Prometheus + Alertmanager
   ```

4. **Backup certificates**
   ```bash
   # Export certificate secrets
   kubectl get secret -n aegis -o yaml > certs-backup.yaml

   # Store securely (encrypted)
   ```

## Integration with River Proxy

River proxy automatically uses certificates from Kubernetes secrets:

```rust
// In River proxy configuration
tls_cert_path = "/etc/tls/tls.crt"
tls_key_path = "/etc/tls/tls.key"
```

Volume mount in deployment:
```yaml
volumeMounts:
- name: tls
  mountPath: /etc/tls
  readOnly: true
volumes:
- name: tls
  secret:
    secretName: aegis-tls-secret
```

## Production Checklist

- [ ] cert-manager installed and running
- [ ] ClusterIssuers configured (staging + prod)
- [ ] DNS API credentials configured (for DNS-01)
- [ ] First certificate issued successfully
- [ ] Certificate renewal tested (delete + recreate)
- [ ] Ingress controller configured to use certificates
- [ ] Monitoring and alerting for expiring certificates
- [ ] Backup procedure for certificate secrets
- [ ] Runbook for certificate issues documented

## References

- [cert-manager Documentation](https://cert-manager.io/docs/)
- [Let's Encrypt](https://letsencrypt.org/)
- [ACME Protocol](https://tools.ietf.org/html/rfc8555)
- [DNS-01 Challenge](https://cert-manager.io/docs/configuration/acme/dns01/)
