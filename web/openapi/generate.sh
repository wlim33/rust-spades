#!/usr/bin/env bash
set -euo pipefail
pnpm exec openapi-typescript openapi/openapi.json -o src/api/schema.d.ts
pnpm exec prettier --write src/api/schema.d.ts
