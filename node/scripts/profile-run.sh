#!/bin/bash
# Sprint 26: Flamegraph profiling script for AEGIS Pingora proxy
#
# Usage: sudo ./scripts/profile-run.sh
#
# This script:
# 1. Starts the mock origin server
# 2. Runs the proxy under profiling for 30 seconds
# 3. Generates a flamegraph SVG

set -e

cd "$(dirname "$0")/.."

echo "=== AEGIS Flamegraph Profiling ==="
echo ""

# Kill any existing processes
pkill -f mock-origin 2>/dev/null || true
pkill -f aegis-pingora 2>/dev/null || true
sleep 1

# Start mock origin
echo "[1/4] Starting mock origin server..."
node k6/mock-origin.js &
ORIGIN_PID=$!
sleep 1

# Start proxy and get PID
echo "[2/4] Starting AEGIS proxy..."
./target/release/aegis-pingora test-config.toml &
PROXY_PID=$!
sleep 2

# Verify proxy is running
if ! curl -s -o /dev/null http://localhost:8080/test; then
    echo "ERROR: Proxy not responding"
    kill $ORIGIN_PID $PROXY_PID 2>/dev/null || true
    exit 1
fi

echo "[3/4] Running load test for 30 seconds..."
# Run a short load test in background
(
    for i in $(seq 1 1000); do
        curl -s http://localhost:8080/api/test > /dev/null &
    done
    wait
) &
LOAD_PID=$!

# Profile for 30 seconds using sample
echo "[4/4] Profiling PID $PROXY_PID for 30 seconds..."
echo "      (requires sudo for dtrace on macOS)"

# Use sample instead of dtrace for easier profiling
sample $PROXY_PID 30 -file profile-output.txt

# Stop processes
kill $LOAD_PID 2>/dev/null || true
kill $PROXY_PID 2>/dev/null || true
kill $ORIGIN_PID 2>/dev/null || true

echo ""
echo "=== Profiling Complete ==="
echo "Output: profile-output.txt"
echo ""
echo "Top CPU consumers:"
head -100 profile-output.txt | grep -E "^\s+\d+\s+" | head -20
