#!/usr/bin/env bash
set -euo pipefail
URL="${VITE_API_URL:-http://localhost:3000}/openapi.json"
echo "Fetching $URL"
curl -fsSL "$URL" | python3 -m json.tool > openapi/openapi.json
echo "Wrote openapi/openapi.json"
