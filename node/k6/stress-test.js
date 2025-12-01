// AEGIS Node Stress Testing Script (k6)
// Sprint 25: Performance Benchmarking
//
// Run with: k6 run k6/stress-test.js
//
// This test pushes the node to its limits to find breaking points

import http from 'k6/http';
import { check, sleep } from 'k6';
import { Rate, Trend, Counter } from 'k6/metrics';

const errorRate = new Rate('errors');
const throughput = new Counter('total_requests');
const latencyTrend = new Trend('response_time');

export const options = {
    // Aggressive ramp-up to find breaking point
    stages: [
        { duration: '30s', target: 100 },
        { duration: '30s', target: 500 },
        { duration: '30s', target: 1000 },
        { duration: '1m', target: 2000 },   // Push to 2000 concurrent users
        { duration: '1m', target: 3000 },   // Push further
        { duration: '30s', target: 5000 },  // Extreme load
        { duration: '1m', target: 1000 },   // Recovery phase
        { duration: '30s', target: 0 },
    ],

    thresholds: {
        // Stress test has looser thresholds
        http_req_duration: ['p(95)<500', 'p(99)<1000'],
        errors: ['rate<0.05'], // Allow up to 5% errors under extreme load
    },
};

const BASE_URL = __ENV.BASE_URL || 'http://localhost:8080';

// Mix of request types
const REQUESTS = [
    { path: '/health', weight: 10 },
    { path: '/api/users', weight: 20 },
    { path: '/api/products', weight: 20 },
    { path: '/api/orders', weight: 15 },
    { path: '/static/app.js', weight: 15 },
    { path: '/static/style.css', weight: 10 },
    { path: '/api/users/123', weight: 5 },
    { path: '/api/products/456', weight: 5 },
];

function selectWeighted(items) {
    const totalWeight = items.reduce((sum, item) => sum + item.weight, 0);
    let random = Math.random() * totalWeight;
    for (const item of items) {
        random -= item.weight;
        if (random <= 0) return item;
    }
    return items[items.length - 1];
}

export default function() {
    const req = selectWeighted(REQUESTS);
    const url = `${BASE_URL}${req.path}`;

    const response = http.get(url, {
        headers: {
            'User-Agent': 'AEGIS-StressTest/1.0',
        },
        timeout: '30s',
    });

    throughput.add(1);
    latencyTrend.add(response.timings.duration);

    const success = check(response, {
        'status is 200': (r) => r.status === 200,
        'response time < 1s': (r) => r.timings.duration < 1000,
    });

    errorRate.add(!success);

    // Minimal sleep for maximum throughput
    sleep(Math.random() * 0.01);
}

export function handleSummary(data) {
    // Calculate key metrics
    const reqsPerSec = data.metrics.http_reqs.values.rate || 0;
    const p95 = data.metrics.http_req_duration.values['p(95)'] || 0;
    const p99 = data.metrics.http_req_duration.values['p(99)'] || 0;
    const errors = data.metrics.errors?.values.rate || 0;

    console.log('='.repeat(60));
    console.log('AEGIS STRESS TEST SUMMARY');
    console.log('='.repeat(60));
    console.log(`Throughput:      ${reqsPerSec.toFixed(2)} req/sec`);
    console.log(`P95 Latency:     ${p95.toFixed(2)}ms`);
    console.log(`P99 Latency:     ${p99.toFixed(2)}ms`);
    console.log(`Error Rate:      ${(errors * 100).toFixed(2)}%`);
    console.log('='.repeat(60));

    // Determine if targets met
    const throughputTarget = 10000;
    const p99Target = 200;

    if (reqsPerSec >= throughputTarget && p99 <= p99Target && errors < 0.01) {
        console.log('RESULT: PASS - All performance targets met');
    } else {
        console.log('RESULT: REVIEW NEEDED');
        if (reqsPerSec < throughputTarget) {
            console.log(`  - Throughput below target (${throughputTarget} req/sec)`);
        }
        if (p99 > p99Target) {
            console.log(`  - P99 latency above target (${p99Target}ms)`);
        }
        if (errors >= 0.01) {
            console.log(`  - Error rate above 1%`);
        }
    }

    return {};
}
