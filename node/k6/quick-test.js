// AEGIS Node Quick Load Test (k6)
// Sprint 26: Performance Stress Testing
//
// Run with: k6 run k6/quick-test.js
//
// Short 1-minute test for quick baseline measurements

import http from 'k6/http';
import { check, sleep } from 'k6';
import { Rate, Trend, Counter } from 'k6/metrics';

const errorRate = new Rate('errors');
const latencyTrend = new Trend('response_time');
const throughput = new Counter('total_requests');

export const options = {
    stages: [
        { duration: '10s', target: 50 },   // Warm-up
        { duration: '30s', target: 100 },  // Sustain
        { duration: '10s', target: 200 },  // Spike
        { duration: '10s', target: 0 },    // Cool down
    ],
    thresholds: {
        http_req_duration: ['p(95)<500', 'p(99)<1000'],
        errors: ['rate<0.05'],
    },
};

const BASE_URL = __ENV.BASE_URL || 'http://localhost:8080';

export default function() {
    // Test various endpoints
    const endpoints = [
        '/api/users',
        '/api/products',
        '/api/orders',
        '/health',
    ];

    const endpoint = endpoints[Math.floor(Math.random() * endpoints.length)];
    const response = http.get(`${BASE_URL}${endpoint}`);

    throughput.add(1);
    latencyTrend.add(response.timings.duration);

    const success = check(response, {
        'status is 200': (r) => r.status === 200,
    });

    errorRate.add(!success);

    sleep(0.01); // Small delay between requests
}

export function handleSummary(data) {
    const reqsPerSec = data.metrics.http_reqs.values.rate || 0;
    const p50 = data.metrics.http_req_duration.values['p(50)'] || 0;
    const p95 = data.metrics.http_req_duration.values['p(95)'] || 0;
    const p99 = data.metrics.http_req_duration.values['p(99)'] || 0;
    const errors = data.metrics.errors?.values.rate || 0;

    console.log('='.repeat(60));
    console.log('AEGIS QUICK LOAD TEST - BASELINE RESULTS');
    console.log('='.repeat(60));
    console.log(`Throughput:      ${reqsPerSec.toFixed(2)} req/sec`);
    console.log(`P50 Latency:     ${p50.toFixed(2)}ms`);
    console.log(`P95 Latency:     ${p95.toFixed(2)}ms`);
    console.log(`P99 Latency:     ${p99.toFixed(2)}ms`);
    console.log(`Error Rate:      ${(errors * 100).toFixed(4)}%`);
    console.log('='.repeat(60));

    return {};
}
