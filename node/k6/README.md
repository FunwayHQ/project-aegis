# AEGIS Node Load Testing (k6)

Sprint 25: Performance Benchmarking

## Prerequisites

Install k6:
```bash
# macOS
brew install k6

# Linux
sudo gpg -k
sudo gpg --no-default-keyring --keyring /usr/share/keyrings/k6-archive-keyring.gpg --keyserver hkp://keyserver.ubuntu.com:80 --recv-keys C5AD17C747E3415A3642D57D77C6C491D6AC1D69
echo "deb [signed-by=/usr/share/keyrings/k6-archive-keyring.gpg] https://dl.k6.io/deb stable main" | sudo tee /etc/apt/sources.list.d/k6.list
sudo apt-get update && sudo apt-get install k6
```

## Test Scripts

### 1. Load Test (`load-test.js`)
Standard load test simulating realistic traffic patterns.

```bash
# Basic run
k6 run k6/load-test.js

# With custom base URL
k6 run -e BASE_URL=http://aegis-node:8080 k6/load-test.js

# With JSON output for CI
k6 run --out json=results.json k6/load-test.js
```

**Performance Targets:**
- P95 Latency: < 60ms (cached assets)
- P99 Latency: < 200ms (proxied requests)
- Error Rate: < 0.1%
- Cache Hit Rate: > 80%

### 2. Stress Test (`stress-test.js`)
Aggressive test to find breaking points.

```bash
k6 run k6/stress-test.js
```

**Goals:**
- Find maximum throughput capacity
- Identify resource bottlenecks
- Test recovery behavior

**Targets:**
- Throughput: > 10,000 req/sec
- Error Rate under stress: < 5%

### 3. Soak Test (`soak-test.js`)
Extended duration test for stability.

```bash
k6 run k6/soak-test.js
```

**Duration:** ~40 minutes

**Goals:**
- Detect memory leaks
- Find resource exhaustion issues
- Measure latency degradation over time

## CI Integration

Add to GitHub Actions:

```yaml
- name: Run Load Tests
  run: |
    k6 run --out json=results.json k6/load-test.js

- name: Check Results
  run: |
    if grep -q '"passes":false' results.json; then
      echo "Performance regression detected!"
      exit 1
    fi
```

## Interpreting Results

### Success Criteria

| Metric | Load Test | Stress Test | Soak Test |
|--------|-----------|-------------|-----------|
| P95 Latency | < 60ms | < 500ms | < 100ms |
| P99 Latency | < 200ms | < 1000ms | < 300ms |
| Error Rate | < 0.1% | < 5% | < 0.1% |
| Throughput | - | > 10k/s | - |

### Common Issues

1. **High P99 but low P50**: Connection pooling or GC issues
2. **Error rate spike at load**: Resource limits hit
3. **Latency degradation in soak**: Memory leak or cache exhaustion
4. **Low cache hit rate**: Cache configuration or invalidation issues

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `BASE_URL` | `http://localhost:8080` | AEGIS node URL |
| `ORIGIN_URL` | `http://localhost:3000` | Origin server URL |
