// AEGIS Node Soak Testing Script (k6)
// Sprint 25: Performance Benchmarking
//
// Run with: k6 run k6/soak-test.js
//
// Long-running test to detect memory leaks, resource exhaustion,
// and degradation over time

import http from 'k6/http';
import { check, sleep } from 'k6';
import { Rate, Trend, Counter, Gauge } from 'k6/metrics';

const errorRate = new Rate('errors');
const throughput = new Counter('total_requests');
const latencyTrend = new Trend('response_time');
const memoryGauge = new Gauge('server_memory');

export const options = {
    // Sustained load for extended period
    stages: [
        { duration: '5m', target: 100 },   // Ramp up
        { duration: '30m', target: 100 },  // Sustained load for 30 minutes
        { duration: '5m', target: 0 },     // Ramp down
    ],

    thresholds: {
        // Soak test: watch for degradation
        http_req_duration: ['p(95)<100', 'p(99)<300'],
        errors: ['rate<0.001'], // Very tight error threshold for soak
    },
};

const BASE_URL = __ENV.BASE_URL || 'http://localhost:8080';

// Track latency over time to detect degradation
const latencyHistory = [];
const HISTORY_SIZE = 100;

export default function() {
    // Mix of different operations
    const operations = [
        () => http.get(`${BASE_URL}/health`),
        () => http.get(`${BASE_URL}/api/users`),
        () => http.get(`${BASE_URL}/api/products`),
        () => http.get(`${BASE_URL}/static/app.js`),
    ];

    const op = operations[Math.floor(Math.random() * operations.length)];
    const response = op();

    throughput.add(1);
    latencyTrend.add(response.timings.duration);

    // Track latency history for degradation detection
    latencyHistory.push(response.timings.duration);
    if (latencyHistory.length > HISTORY_SIZE) {
        latencyHistory.shift();
    }

    const success = check(response, {
        'status is 200': (r) => r.status === 200,
        'response time < 300ms': (r) => r.timings.duration < 300,
    });

    errorRate.add(!success);

    // Try to fetch memory stats if available
    try {
        const statsResponse = http.get(`${BASE_URL}/metrics`);
        if (statsResponse.status === 200 && statsResponse.body) {
            const memMatch = statsResponse.body.match(/memory_used_bytes\s+(\d+)/);
            if (memMatch) {
                memoryGauge.add(parseInt(memMatch[1]));
            }
        }
    } catch (e) {
        // Metrics endpoint may not be available
    }

    sleep(0.1); // 10 req/sec per VU
}

export function handleSummary(data) {
    const reqsPerSec = data.metrics.http_reqs.values.rate || 0;
    const totalReqs = data.metrics.http_reqs.values.count || 0;
    const p50 = data.metrics.http_req_duration.values['p(50)'] || 0;
    const p95 = data.metrics.http_req_duration.values['p(95)'] || 0;
    const p99 = data.metrics.http_req_duration.values['p(99)'] || 0;
    const min = data.metrics.http_req_duration.values['min'] || 0;
    const max = data.metrics.http_req_duration.values['max'] || 0;
    const errors = data.metrics.errors?.values.rate || 0;

    console.log('='.repeat(60));
    console.log('AEGIS SOAK TEST SUMMARY');
    console.log('='.repeat(60));
    console.log(`Total Requests:  ${totalReqs.toLocaleString()}`);
    console.log(`Throughput:      ${reqsPerSec.toFixed(2)} req/sec`);
    console.log(`Min Latency:     ${min.toFixed(2)}ms`);
    console.log(`P50 Latency:     ${p50.toFixed(2)}ms`);
    console.log(`P95 Latency:     ${p95.toFixed(2)}ms`);
    console.log(`P99 Latency:     ${p99.toFixed(2)}ms`);
    console.log(`Max Latency:     ${max.toFixed(2)}ms`);
    console.log(`Error Rate:      ${(errors * 100).toFixed(4)}%`);
    console.log('='.repeat(60));

    // Check for degradation indicators
    const degradationThreshold = 2.0; // 2x increase
    if (max > p50 * degradationThreshold * 10) {
        console.log('WARNING: Significant latency spikes detected');
        console.log('  This may indicate resource exhaustion or GC pauses');
    }

    if (errors > 0.0001) {
        console.log('WARNING: Errors detected during soak test');
        console.log('  This may indicate memory leaks or connection issues');
    }

    // Overall result
    if (errors < 0.001 && p99 < 300 && max < p50 * 20) {
        console.log('RESULT: PASS - System stable under sustained load');
    } else {
        console.log('RESULT: REVIEW NEEDED - Potential stability issues detected');
    }

    return {};
}
