// Simple mock origin server for load testing
// Run with: node k6/mock-origin.js

const http = require('http');

const PORT = 3000;

const server = http.createServer((req, res) => {
    // Simulate minimal work
    const response = JSON.stringify({
        path: req.url,
        method: req.method,
        timestamp: Date.now(),
    });

    res.writeHead(200, {
        'Content-Type': 'application/json',
        'Content-Length': response.length,
        'Cache-Control': 'public, max-age=60',
    });
    res.end(response);
});

server.listen(PORT, () => {
    console.log(`Mock origin server running on http://localhost:${PORT}`);
});
