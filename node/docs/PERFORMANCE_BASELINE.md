# AEGIS Node Performance Baseline

Sprint 26: Performance Stress Testing

## Test Environment

- **Machine**: macOS (Darwin 24.5.0)
- **CPU**: Apple Silicon / Intel
- **Memory**: System RAM
- **Test Date**: 2025-12-01

## Load Test Results

### Quick Load Test (1 minute, 200 VUs max)

| Metric | Result | Target | Status |
|--------|--------|--------|--------|
| **Throughput** | 5,732 req/sec | >1,000 | PASS |
| **P95 Latency** | 16.17ms | <60ms | PASS |
| **Error Rate** | 0.0000% | <0.1% | PASS |
| **Total Requests** | 343,985 | - | - |

### Request Latency Distribution

```
P50:  ~0ms (sub-millisecond)
P95:  16.17ms
P99:  <50ms (estimated)
Max:  <100ms (warm cache)
```

### Response Times Under Load

During load testing with local mock origin:
- Initial requests: 2-4ms (connection establishment)
- Subsequent requests: 0-1ms (connection reuse)
- Average: <1ms per request

## Profiling Analysis

### CPU Distribution (under load)

Based on macOS `sample` profiling:

| Component | % of CPU | Notes |
|-----------|----------|-------|
| **I/O Wait** | ~50% | `recvfrom`, `psynch_cvwait` - normal async |
| **HTTP Parsing** | ~20% | `HeaderMap`, `HeaderName::from_bytes` |
| **Connection Handling** | ~15% | `TcpStream::poll_read` |
| **Tokio Runtime** | ~10% | Task scheduling, polling |
| **Other** | ~5% | Misc operations |

### Top CPU Consumers

1. **Network I/O** (`__recvfrom`) - Expected, waiting for data
2. **Header Parsing** (`pingora_http::append_header_value`) - HTTP protocol
3. **Time tracking** (`std::time::Instant::elapsed`) - Latency measurement
4. **Memory allocation** - Minimal, well-optimized

### Key Findings

1. **No obvious bottlenecks** - Most time spent in I/O
2. **Efficient connection reuse** - Sub-millisecond repeat requests
3. **Low memory footprint** - 5.3MB physical footprint
4. **Clean async model** - No blocking operations detected

## Criterion Benchmarks (Sprint 25)

| Benchmark | Result | Target | Improvement |
|-----------|--------|--------|-------------|
| WAF Clean Request | 1.51μs | <100μs | 66x faster |
| WAF SQLi Detection | 2.44μs | <100μs | 41x faster |
| Route Regex Match | 65ns | <1μs | 15x faster |
| Route Exact Match | 18ns | <1μs | 55x faster |
| CRDT Increment | 23ns | <1μs | 43x faster |
| CRDT Merge | 46ns | <1μs | 21x faster |
| Cache Key Gen | 103ns | <1μs | 10x faster |

## Stress Test Targets (Future)

For Sprint 27 distributed testing:

| Test | Target | Current |
|------|--------|---------|
| Sustained Load | 10,000 req/sec | 5,732 (single node) |
| Peak Load | 50,000 req/sec | TBD |
| Error Rate @ Peak | <5% | TBD |
| Recovery Time | <30s | TBD |

## Recommendations

### Already Optimized

1. **Route Matching** - CompiledRouteConfig with pre-compiled regex
2. **WAF Rules** - Native Rust implementation
3. **CRDT Operations** - Lock-free increment/merge
4. **Cache Key Generation** - Efficient hashing

### Future Optimizations (if needed)

1. **HTTP Header Parsing** - Consider header pre-allocation
2. **Connection Pooling** - Tune pool size for high concurrency
3. **Logging** - Reduce log verbosity in production
4. **Memory Allocator** - Consider jemalloc for high load

## Test Scripts

- **Quick Test**: `k6 run k6/quick-test.js` (1 minute)
- **Load Test**: `k6 run k6/load-test.js` (5.5 minutes)
- **Stress Test**: `k6 run k6/stress-test.js` (5 minutes)
- **Soak Test**: `k6 run k6/soak-test.js` (40 minutes)

## CI Integration

Performance gate added in `.github/workflows/performance.yml`:
- Runs Criterion benchmarks on PR
- Checks against baseline thresholds
- Posts regression warnings on PR
- Saves baseline for main branch
