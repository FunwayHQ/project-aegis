// AEGIS Node Load Testing Script (k6)
// Sprint 25: Performance Benchmarking
//
// Run with: k6 run k6/load-test.js
// Run with report: k6 run --out json=results.json k6/load-test.js
//
// Performance Targets:
// - Latency P99: < 200ms for proxied requests
// - Latency P95: < 60ms for cached assets
// - Throughput: > 10,000 req/sec per node
// - Error rate: < 0.1%

import http from 'k6/http';
import { check, sleep, group } from 'k6';
import { Rate, Trend, Counter } from 'k6/metrics';

// Custom metrics
const errorRate = new Rate('errors');
const cacheHitRate = new Rate('cache_hits');
const wafBlockRate = new Rate('waf_blocks');
const p99Latency = new Trend('p99_latency');
const throughput = new Counter('total_requests');

// Test configuration
export const options = {
    // Stages for gradual ramp-up
    stages: [
        { duration: '30s', target: 50 },   // Warm-up
        { duration: '1m', target: 200 },   // Ramp up to 200 VUs
        { duration: '2m', target: 200 },   // Sustain load
        { duration: '1m', target: 500 },   // Spike test
        { duration: '30s', target: 500 },  // Sustain spike
        { duration: '30s', target: 0 },    // Ramp down
    ],

    // Thresholds for pass/fail
    thresholds: {
        // Overall response time targets
        http_req_duration: ['p(95)<60', 'p(99)<200'],

        // Error rate must be below 0.1%
        errors: ['rate<0.001'],

        // Cache hit rate should be > 80%
        cache_hits: ['rate>0.8'],

        // WAF should not block legitimate traffic
        waf_blocks: ['rate<0.01'],

        // Custom P99 latency tracking
        p99_latency: ['p(99)<200'],
    },
};

// Configuration - override with environment variables
const BASE_URL = __ENV.BASE_URL || 'http://localhost:8080';
const ORIGIN_URL = __ENV.ORIGIN_URL || 'http://localhost:3000';

// Test payloads
const API_ENDPOINTS = [
    { path: '/api/users', method: 'GET', weight: 30 },
    { path: '/api/products', method: 'GET', weight: 25 },
    { path: '/api/orders', method: 'GET', weight: 15 },
    { path: '/api/users/123', method: 'GET', weight: 10 },
    { path: '/api/products/456', method: 'GET', weight: 10 },
    { path: '/health', method: 'GET', weight: 10 },
];

const STATIC_ASSETS = [
    { path: '/static/style.css', weight: 30 },
    { path: '/static/app.js', weight: 30 },
    { path: '/static/logo.png', weight: 20 },
    { path: '/favicon.ico', weight: 20 },
];

// WAF test payloads (should be blocked)
const WAF_TEST_PAYLOADS = [
    { path: "/login?user=admin' OR '1'='1", expected: 'blocked' },
    { path: '/search?q=<script>alert(1)</script>', expected: 'blocked' },
    { path: '/api/../../../etc/passwd', expected: 'blocked' },
    { path: '/api/users?id=1%27%20UNION%20SELECT%20*%20FROM%20passwords', expected: 'blocked' },
];

// Helper: weighted random selection
function selectWeighted(items) {
    const totalWeight = items.reduce((sum, item) => sum + item.weight, 0);
    let random = Math.random() * totalWeight;

    for (const item of items) {
        random -= item.weight;
        if (random <= 0) return item;
    }
    return items[items.length - 1];
}

// Helper: make request and record metrics
function makeRequest(url, method = 'GET', params = {}) {
    const defaultParams = {
        headers: {
            'User-Agent': 'AEGIS-LoadTest/1.0',
            'Accept': 'application/json',
        },
        timeout: '10s',
    };

    const mergedParams = { ...defaultParams, ...params };
    let response;

    switch (method) {
        case 'POST':
            response = http.post(url, params.body || '', mergedParams);
            break;
        case 'PUT':
            response = http.put(url, params.body || '', mergedParams);
            break;
        case 'DELETE':
            response = http.del(url, null, mergedParams);
            break;
        default:
            response = http.get(url, mergedParams);
    }

    // Record metrics
    throughput.add(1);
    p99Latency.add(response.timings.duration);

    // Check for cache hit (via X-Cache header)
    const isCacheHit = response.headers['X-Cache'] === 'HIT' ||
                       response.headers['x-cache'] === 'HIT';
    cacheHitRate.add(isCacheHit);

    // Check for WAF block
    const isWafBlock = response.status === 403 &&
                       (response.body.includes('WAF') || response.body.includes('blocked'));
    wafBlockRate.add(isWafBlock);

    return response;
}

// Main test scenario
export default function() {
    // Mix of different request types
    const scenario = Math.random();

    if (scenario < 0.4) {
        // 40% - API requests
        group('API Requests', () => {
            const endpoint = selectWeighted(API_ENDPOINTS);
            const url = `${BASE_URL}${endpoint.path}`;

            const response = makeRequest(url, endpoint.method);

            const success = check(response, {
                'API status is 200': (r) => r.status === 200,
                'API response time < 200ms': (r) => r.timings.duration < 200,
            });

            errorRate.add(!success);
        });
    } else if (scenario < 0.75) {
        // 35% - Static assets (should be cached)
        group('Static Assets', () => {
            const asset = selectWeighted(STATIC_ASSETS);
            const url = `${BASE_URL}${asset.path}`;

            const response = makeRequest(url);

            const success = check(response, {
                'Static status is 200 or 304': (r) => r.status === 200 || r.status === 304,
                'Static response time < 60ms': (r) => r.timings.duration < 60,
            });

            errorRate.add(!success);
        });
    } else if (scenario < 0.90) {
        // 15% - Burst requests (simulate real traffic patterns)
        group('Burst Traffic', () => {
            const responses = [];

            // Rapid fire 5 requests
            for (let i = 0; i < 5; i++) {
                const endpoint = selectWeighted(API_ENDPOINTS);
                responses.push(makeRequest(`${BASE_URL}${endpoint.path}`));
            }

            const allSuccess = responses.every(r => r.status === 200);
            errorRate.add(!allSuccess);
        });
    } else {
        // 10% - WAF testing (legitimate requests that look suspicious)
        group('WAF Validation', () => {
            // Test that WAF doesn't block legitimate requests
            const response = makeRequest(`${BASE_URL}/api/users?search=O'Brien`);

            const success = check(response, {
                'Legitimate request not blocked': (r) => r.status !== 403,
            });

            errorRate.add(!success);
        });
    }

    // Small sleep to simulate realistic user behavior
    sleep(Math.random() * 0.1);
}

// WAF validation scenario (separate)
export function wafTest() {
    group('WAF Attack Detection', () => {
        for (const payload of WAF_TEST_PAYLOADS) {
            const url = `${BASE_URL}${payload.path}`;
            const response = http.get(url);

            check(response, {
                'Attack is blocked': (r) => r.status === 403,
            });
        }
    });
}

// Health check scenario
export function healthCheck() {
    const response = http.get(`${BASE_URL}/health`);

    check(response, {
        'Health check returns 200': (r) => r.status === 200,
        'Health check < 10ms': (r) => r.timings.duration < 10,
    });
}

// Setup - runs once before test
export function setup() {
    console.log(`Testing AEGIS Node at: ${BASE_URL}`);
    console.log(`Origin server at: ${ORIGIN_URL}`);

    // Verify node is up
    const response = http.get(`${BASE_URL}/health`, { timeout: '5s' });
    if (response.status !== 200) {
        console.error('AEGIS Node is not responding. Aborting test.');
        return { error: true };
    }

    console.log('AEGIS Node is healthy. Starting load test...');
    return { startTime: Date.now() };
}

// Teardown - runs once after test
export function teardown(data) {
    if (data.error) return;

    const duration = (Date.now() - data.startTime) / 1000;
    console.log(`Test completed in ${duration.toFixed(2)} seconds`);
}
