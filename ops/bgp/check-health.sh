#!/bin/bash
#
# AEGIS Edge Node Health Check
# Monitors local service health and controls BGP anycast route announcement
#
# If service is unhealthy, withdraws the anycast route from BGP
# If service recovers, re-announces the anycast route
#
# Run via cron every minute:
# * * * * * /usr/local/bin/aegis-health-check

set -euo pipefail

# Configuration
HEALTH_CHECK_URL="http://localhost:80/health"
HEALTH_CHECK_TIMEOUT=5
STATE_FILE="/var/run/aegis-health-state"
FAILURE_THRESHOLD=3
ANYCAST_ROUTE="203.0.113.0/24"  # Update with actual anycast prefix

# Function to check if service is healthy
check_service_health() {
    # Try HTTP health check endpoint
    if curl --fail --silent --max-time "$HEALTH_CHECK_TIMEOUT" "$HEALTH_CHECK_URL" > /dev/null 2>&1; then
        return 0  # Healthy
    fi

    # Fallback: Check if River proxy is listening on port 80
    if ss -tln | grep -q ':80 '; then
        return 0  # Healthy
    fi

    return 1  # Unhealthy
}

# Function to announce anycast route via BIRD
announce_route() {
    logger -t aegis-health "Service healthy - announcing anycast route $ANYCAST_ROUTE"

    # Add static route to BIRD (will be announced via BGP)
    echo "configure" | birdc
    echo "route $ANYCAST_ROUTE blackhole" | birdc

    # Update state
    echo "announced" > "$STATE_FILE"
}

# Function to withdraw anycast route via BIRD
withdraw_route() {
    logger -t aegis-health "Service unhealthy - withdrawing anycast route $ANYCAST_ROUTE"

    # Remove static route from BIRD (stops BGP announcement)
    echo "configure" | birdc
    echo "route $ANYCAST_ROUTE withdraw" | birdc

    # Update state
    echo "withdrawn" > "$STATE_FILE"
}

# Get current state
if [ -f "$STATE_FILE" ]; then
    CURRENT_STATE=$(cat "$STATE_FILE")
    FAILURE_COUNT=$(cat "${STATE_FILE}.failures" 2>/dev/null || echo "0")
else
    CURRENT_STATE="announced"
    FAILURE_COUNT=0
fi

# Check health
if check_service_health; then
    # Service is healthy
    FAILURE_COUNT=0

    # If route was withdrawn, re-announce it
    if [ "$CURRENT_STATE" = "withdrawn" ]; then
        announce_route
    fi
else
    # Service is unhealthy
    FAILURE_COUNT=$((FAILURE_COUNT + 1))
    logger -t aegis-health "Health check failed (count: $FAILURE_COUNT)"

    # If failures exceed threshold, withdraw route
    if [ "$FAILURE_COUNT" -ge "$FAILURE_THRESHOLD" ]; then
        if [ "$CURRENT_STATE" = "announced" ]; then
            withdraw_route
        fi
    fi
fi

# Save failure count
echo "$FAILURE_COUNT" > "${STATE_FILE}.failures"

exit 0
