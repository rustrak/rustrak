#!/bin/bash
# Setup script for benchmark environment
# Creates a test project and saves credentials for the benchmark tool

set -e

SERVER_URL="${SERVER_URL:-http://localhost:8080}"
SUPERUSER_EMAIL="bench@test.local"
SUPERUSER_PASSWORD="benchpass123"
PROJECT_NAME="benchmark-project"
CREDENTIALS_FILE="$(dirname "$0")/../.bench-credentials"

echo "🔧 Setting up benchmark environment..."

# Wait for server to be ready
echo "⏳ Waiting for server at $SERVER_URL..."
max_attempts=60
attempt=0
while [ $attempt -lt $max_attempts ]; do
    if curl -s "$SERVER_URL/health" > /dev/null 2>&1; then
        echo "✅ Server is ready!"
        break
    fi
    attempt=$((attempt + 1))
    sleep 1
done

if [ $attempt -eq $max_attempts ]; then
    echo "❌ Server not ready after ${max_attempts}s"
    exit 1
fi

# Login to get session cookie
echo "🔐 Logging in as superuser..."
if ! curl -sSf -o /dev/null -c /tmp/bench-cookies.txt -b /tmp/bench-cookies.txt \
    -X POST "$SERVER_URL/auth/login" \
    -H "Content-Type: application/json" \
    -d "{\"email\": \"$SUPERUSER_EMAIL\", \"password\": \"$SUPERUSER_PASSWORD\"}"; then
    echo "❌ Login failed"
    exit 1
fi
echo "✅ Logged in successfully"

# Check if project already exists
echo "📦 Checking for existing benchmark project..."
PROJECT=$(curl -sSf -b /tmp/bench-cookies.txt "$SERVER_URL/api/projects" |
    jq -c --arg name "$PROJECT_NAME" '.items[] | select(.name == $name)' | head -1)

if [ -n "$PROJECT" ]; then
    echo "✅ Benchmark project already exists"
    PROJECT_ID=$(echo "$PROJECT" | jq -r .id)
    SENTRY_KEY=$(echo "$PROJECT" | jq -r .sentry_key)
else
    # Create benchmark project
    echo "📦 Creating benchmark project..."
    CREATE_RESPONSE=$(curl -sSf -b /tmp/bench-cookies.txt \
        -X POST "$SERVER_URL/api/projects" \
        -H "Content-Type: application/json" \
        -d "{\"name\": \"$PROJECT_NAME\", \"slug\": \"benchmark\"}")

    PROJECT_ID=$(echo "$CREATE_RESPONSE" | jq -r .id)
    SENTRY_KEY=$(echo "$CREATE_RESPONSE" | jq -r .sentry_key)
    echo "✅ Created project with ID: $PROJECT_ID"
fi

# Create an API token for the read-path scenario.
# Ingestion authenticates with the project's sentry_key, but the dashboard
# endpoints the read scenario measures go through ApiActor, which needs either a
# session cookie or a bearer token. A token is the reusable option.
echo "🎫 Creating API token..."
TOKEN_RESPONSE=$(curl -s -b /tmp/bench-cookies.txt \
    -X POST "$SERVER_URL/api/tokens" \
    -H "Content-Type: application/json" \
    -d '{"description": "benchmark-read-scenario"}')

API_TOKEN=$(echo "$TOKEN_RESPONSE" | jq -r '.token // empty')

if [ -z "$API_TOKEN" ]; then
    echo "⚠️  Could not create API token: $TOKEN_RESPONSE"
    echo "   The read scenario will not work without one."
else
    echo "✅ API token created"
fi

# PostgreSQL connection for engine statistics (pg_stat_* collection).
# Host-side port, since the benchmark tool runs outside the compose network.
PG_PORT="${PG_PORT:-55432}"
POSTGRES_URL="postgres://bench:bench@localhost:${PG_PORT}/rustrak_bench"

# Save credentials to file
echo "💾 Saving credentials..."
cat > "$CREDENTIALS_FILE" << EOF
# Benchmark credentials (auto-generated)
# Do not commit this file
PROJECT_ID=$PROJECT_ID
SENTRY_KEY=$SENTRY_KEY
SERVER_URL=$SERVER_URL
BENCH_API_TOKEN=$API_TOKEN
BENCH_POSTGRES_URL=$POSTGRES_URL
EOF

echo ""
echo "════════════════════════════════════════════════════════════"
echo "✅ Benchmark environment ready!"
echo ""
echo "   Project ID:  $PROJECT_ID"
echo "   Sentry Key:  $SENTRY_KEY"
echo "   Server URL:  $SERVER_URL"
echo ""
echo "Run benchmark with:"
echo "   cargo run --release -- --project-id $PROJECT_ID --sentry-key $SENTRY_KEY"
echo ""
echo "Or simply:"
echo "   pnpm bench"
echo "════════════════════════════════════════════════════════════"

# Cleanup
rm -f /tmp/bench-cookies.txt
