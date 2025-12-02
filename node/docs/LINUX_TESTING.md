# AEGIS Linux Testing Guide

Sprint 27: Testing Features Requiring Linux

## Overview

Some AEGIS features require Linux-specific capabilities that cannot be tested on macOS:

1. **eBPF/XDP DDoS Protection** - Requires Linux kernel 5.x+
2. **BGP Anycast Testing** - Requires network infrastructure
3. **Production-Scale Distributed Tests** - Requires cloud/VMs

This guide covers how to test these features on Linux.

---

## 1. eBPF/XDP DDoS Testing

### Requirements

- Linux kernel 5.4+ (preferably 5.10+)
- Root access
- `clang` and `llvm` for eBPF compilation
- `libbpf-dev` library

### Setup on Ubuntu/Debian

```bash
# Install dependencies
sudo apt-get update
sudo apt-get install -y \
    clang \
    llvm \
    libbpf-dev \
    linux-headers-$(uname -r) \
    bpftool \
    iproute2

# Verify kernel supports XDP
cat /boot/config-$(uname -r) | grep XDP
# Should show: CONFIG_XDP_SOCKETS=y
```

### Setup on Fedora/RHEL

```bash
# Install dependencies
sudo dnf install -y \
    clang \
    llvm \
    libbpf-devel \
    kernel-devel \
    bpftool \
    iproute
```

### Building eBPF Programs

```bash
cd /path/to/project-aegis/node

# Build the eBPF XDP program
clang -O2 -g -target bpf \
    -D__TARGET_ARCH_x86 \
    -c ebpf/xdp_ddos.c \
    -o ebpf/xdp_ddos.o

# Verify the program
llvm-objdump -S ebpf/xdp_ddos.o
```

### Loading XDP Program

```bash
# Load XDP program on interface (e.g., eth0)
sudo ip link set dev eth0 xdpgeneric obj ebpf/xdp_ddos.o sec xdp

# Verify it's loaded
ip link show eth0
# Should show: xdpgeneric

# View eBPF maps
sudo bpftool map list

# Unload XDP program
sudo ip link set dev eth0 xdpgeneric off
```

### Testing DDoS Mitigation

```bash
# Terminal 1: Start AEGIS proxy
sudo ./target/release/aegis-pingora config.toml

# Terminal 2: Monitor XDP stats
sudo bpftool prog tracelog

# Terminal 3: Generate SYN flood (from another machine)
# WARNING: Only do this on isolated test networks!
hping3 -S --flood -p 8080 <target-ip>

# Terminal 4: Watch for dropped packets
watch -n 1 'sudo bpftool map dump name blocked_ips'
```

### XDP Performance Test

```bash
# Measure baseline throughput
iperf3 -s &  # Server
iperf3 -c localhost -t 30  # Client

# Load XDP program
sudo ip link set dev lo xdpgeneric obj ebpf/xdp_ddos.o sec xdp

# Measure throughput with XDP
iperf3 -c localhost -t 30

# Compare results - XDP should add <1% overhead
```

### Expected Results

| Metric | Without XDP | With XDP | Target |
|--------|-------------|----------|--------|
| Throughput | Baseline | -0.5% | <1% overhead |
| SYN Flood Drop Rate | 0% | >99% | >95% |
| XDP Latency | N/A | <1µs | <10µs |

---

## 2. BGP Anycast Testing

### Requirements

- 3+ Linux VMs with public IPs
- BGP-capable routers (physical or FRR/BIRD)
- Own AS number (or test with private ASN)
- IP prefix allocation

### Test Environment with BIRD

```bash
# Install BIRD routing daemon
sudo apt-get install bird2

# Configure BIRD (/etc/bird/bird.conf)
cat > /etc/bird/bird.conf << 'EOF'
router id 10.0.0.1;

protocol kernel {
    ipv4 { export all; };
}

protocol device {
}

protocol static {
    ipv4;
    route 192.0.2.0/24 via "lo";
}

protocol bgp upstream {
    local as 65001;
    neighbor 10.0.0.254 as 65000;
    ipv4 {
        import none;
        export where net = 192.0.2.0/24;
    };
}
EOF

# Start BIRD
sudo systemctl start bird

# Verify BGP session
sudo birdc show protocols
```

### Multi-Node Anycast Test

```
┌─────────────────────────────────────────────────────────┐
│                     BGP Router                          │
│                    AS 65000                             │
└─────────────┬─────────────┬─────────────┬──────────────┘
              │             │             │
              ▼             ▼             ▼
        ┌─────────┐   ┌─────────┐   ┌─────────┐
        │ Node 1  │   │ Node 2  │   │ Node 3  │
        │ AS 65001│   │ AS 65002│   │ AS 65003│
        │10.0.0.1 │   │10.0.0.2 │   │10.0.0.3 │
        └─────────┘   └─────────┘   └─────────┘
              │             │             │
              └──────┬──────┴──────┬──────┘
                     │             │
              Anycast IP: 192.0.2.1
```

### Testing Anycast Failover

```bash
# On each node, start AEGIS
./target/release/aegis-pingora config.toml &

# From client, make requests to anycast IP
for i in {1..100}; do
    curl -s http://192.0.2.1/health | jq .node_id
    sleep 0.1
done

# Should show nearest node

# Simulate node failure (on Node 1)
sudo birdc disable upstream
# or
sudo systemctl stop aegis

# Requests should automatically route to Node 2 or 3
# Measure failover time
time curl http://192.0.2.1/health
```

### Expected Results

| Metric | Result | Target |
|--------|--------|--------|
| BGP Convergence | <5s | <30s |
| Anycast Failover | <10s | <30s |
| Traffic Re-routing | Automatic | Yes |

---

## 3. Production-Scale Distributed Tests

### Cloud Setup (AWS/GCP/Azure)

```bash
# Terraform configuration for 10-node cluster
# See: ops/terraform/distributed-test/

cd ops/terraform/distributed-test
terraform init
terraform plan
terraform apply

# This creates:
# - 10 x c5.xlarge instances (4 vCPU, 8GB RAM)
# - VPC with private networking
# - NATS cluster with JetStream
# - Load balancer for traffic distribution
```

### Docker Compose Local Cluster

```yaml
# docker-compose.distributed.yml
version: '3.8'

services:
  nats:
    image: nats:2.12-alpine
    command: -js -cluster nats://0.0.0.0:6222
    ports:
      - "4222:4222"

  aegis-node-1:
    build: .
    environment:
      - NODE_ID=1
      - NATS_URL=nats://nats:4222
    depends_on:
      - nats
    ports:
      - "8081:8080"

  aegis-node-2:
    build: .
    environment:
      - NODE_ID=2
      - NATS_URL=nats://nats:4222
    depends_on:
      - nats
    ports:
      - "8082:8080"

  aegis-node-3:
    build: .
    environment:
      - NODE_ID=3
      - NATS_URL=nats://nats:4222
    depends_on:
      - nats
    ports:
      - "8083:8080"

  load-generator:
    image: grafana/k6
    volumes:
      - ./k6:/scripts
    command: run /scripts/distributed-load.js
    depends_on:
      - aegis-node-1
      - aegis-node-2
      - aegis-node-3
```

### Running Distributed Tests

```bash
# Start the cluster
docker-compose -f docker-compose.distributed.yml up -d

# Wait for startup
sleep 10

# Run distributed load test
docker-compose -f docker-compose.distributed.yml run \
    load-generator run /scripts/distributed-load.js

# Monitor CRDT sync
docker-compose exec nats nats stream ls
docker-compose exec nats nats stream info aegis-crdt

# Check convergence across nodes
for port in 8081 8082 8083; do
    echo "Node on port $port:"
    curl -s http://localhost:$port/metrics | grep crdt_value
done
```

### Distributed Load Test Script

```javascript
// k6/distributed-load.js
import http from 'k6/http';
import { check, sleep } from 'k6';
import { Counter } from 'k6/metrics';

const nodes = [
    'http://aegis-node-1:8080',
    'http://aegis-node-2:8080',
    'http://aegis-node-3:8080',
];

export const options = {
    stages: [
        { duration: '1m', target: 100 },   // Ramp up
        { duration: '5m', target: 500 },   // Sustained load
        { duration: '1m', target: 1000 },  // Spike
        { duration: '5m', target: 500 },   // Sustained
        { duration: '1m', target: 0 },     // Ramp down
    ],
    thresholds: {
        http_req_duration: ['p(95)<100'],
        http_req_failed: ['rate<0.01'],
    },
};

export default function() {
    // Distribute requests across nodes
    const node = nodes[Math.floor(Math.random() * nodes.length)];

    const response = http.get(`${node}/api/test`);

    check(response, {
        'status is 200': (r) => r.status === 200,
        'latency < 100ms': (r) => r.timings.duration < 100,
    });

    sleep(0.01);
}

export function handleSummary(data) {
    return {
        'stdout': JSON.stringify(data, null, 2),
        'distributed-results.json': JSON.stringify(data),
    };
}
```

### Network Partition Testing

```bash
# Using tc (traffic control) to simulate network partition
# Requires root on Linux

# Simulate partition between Node 1 and Node 2
docker exec aegis-node-1 tc qdisc add dev eth0 root netem loss 100%

# Wait for partition detection
sleep 30

# Verify CRDT handles partition
curl http://localhost:8081/metrics | grep crdt
curl http://localhost:8082/metrics | grep crdt

# Heal partition
docker exec aegis-node-1 tc qdisc del dev eth0 root

# Verify convergence after heal
sleep 10
for port in 8081 8082 8083; do
    curl -s http://localhost:$port/metrics | grep crdt_value
done
```

### Chaos Engineering with Pumba

```bash
# Install Pumba
curl -sL https://github.com/alexei-led/pumba/releases/download/0.9.0/pumba_linux_amd64 > pumba
chmod +x pumba

# Kill random container
./pumba --random kill

# Pause random container for 30s
./pumba --random pause --duration 30s

# Add network delay
./pumba netem --duration 1m delay --time 100 aegis-node-1

# Run chaos while load testing
./pumba --random --interval 1m kill &
k6 run k6/distributed-load.js
```

### Expected Production Results

| Metric | Target | Notes |
|--------|--------|-------|
| Throughput (10 nodes) | >50,000 req/sec | Linear scaling |
| CRDT Convergence | <5s | Cross-region |
| Network Partition Recovery | <30s | After heal |
| Node Failure Recovery | <10s | Anycast re-route |
| Error Rate @ Peak | <1% | Graceful degradation |

---

## 4. Quick Reference Commands

### eBPF/XDP

```bash
# Load XDP
sudo ip link set dev eth0 xdpgeneric obj ebpf/xdp_ddos.o sec xdp

# Unload XDP
sudo ip link set dev eth0 xdpgeneric off

# View eBPF maps
sudo bpftool map list
sudo bpftool map dump name blocked_ips

# Trace eBPF events
sudo bpftool prog tracelog
```

### BIRD/BGP

```bash
# Show protocols
sudo birdc show protocols

# Show routes
sudo birdc show route

# Disable BGP session
sudo birdc disable <protocol>

# Enable BGP session
sudo birdc enable <protocol>
```

### Docker Distributed

```bash
# Start cluster
docker-compose -f docker-compose.distributed.yml up -d

# Scale nodes
docker-compose -f docker-compose.distributed.yml up -d --scale aegis-node=10

# View logs
docker-compose logs -f aegis-node-1

# Stop cluster
docker-compose -f docker-compose.distributed.yml down
```

### Network Simulation

```bash
# Add latency
tc qdisc add dev eth0 root netem delay 50ms

# Add packet loss
tc qdisc add dev eth0 root netem loss 10%

# Add jitter
tc qdisc add dev eth0 root netem delay 50ms 20ms

# Remove rules
tc qdisc del dev eth0 root
```

---

## 5. CI/CD Integration

### GitHub Actions Workflow

```yaml
# .github/workflows/linux-tests.yml
name: Linux Integration Tests

on:
  push:
    branches: [main]
  pull_request:

jobs:
  ebpf-tests:
    runs-on: ubuntu-22.04
    steps:
      - uses: actions/checkout@v4

      - name: Install eBPF dependencies
        run: |
          sudo apt-get update
          sudo apt-get install -y clang llvm libbpf-dev

      - name: Build eBPF programs
        run: |
          clang -O2 -g -target bpf -c ebpf/xdp_ddos.c -o ebpf/xdp_ddos.o

      - name: Load XDP (dry run)
        run: |
          # Verify program is valid
          llvm-objdump -S ebpf/xdp_ddos.o

  distributed-tests:
    runs-on: ubuntu-22.04
    services:
      nats:
        image: nats:2.12-alpine
        ports:
          - 4222:4222
        options: --health-cmd "nats-server --help" --health-interval 10s

    steps:
      - uses: actions/checkout@v4

      - name: Build AEGIS
        run: cargo build --release

      - name: Run distributed tests
        run: cargo test --test game_day -- --nocapture
```

---

## Summary

| Feature | Platform | Status |
|---------|----------|--------|
| CRDT Sync | macOS/Linux | Tested |
| NATS JetStream | macOS/Linux | Tested |
| Node Failure Recovery | macOS/Linux | Tested |
| eBPF/XDP DDoS | Linux only | Guide provided |
| BGP Anycast | Linux + Network | Guide provided |
| Production Distributed | Linux VMs/Cloud | Guide provided |

For Sprint 27, the core distributed functionality (CRDT, NATS, recovery) has been tested on macOS. The Linux-specific features (eBPF, BGP) are documented here for production deployment.
