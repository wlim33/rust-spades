#!/usr/bin/env bash
set -euo pipefail

# ARCHIVED — the live deploy path is .github/workflows/deploy.yml.
# This script is kept for reference and as an emergency-only manual deploy.
# To use it: cd web && bash scripts/deploy-cf-pages.sh (requires wrangler login or CLOUDFLARE_API_TOKEN).

# Env vars expected:
#   DEPLOY_HOST   — ssh destination (e.g. wlim@spades.wlim.dev)
#   DEPLOY_PATH   — absolute path on the host where dist/ lands (e.g. /srv/spades-ts/public)
#
# Examples:
#   DEPLOY_HOST=wlim@spades.wlim.dev DEPLOY_PATH=/srv/spades/public ./scripts/deploy.sh
#
# Assumes the host runs rust-spades with --static-dir $DEPLOY_PATH (or a reverse
# proxy that serves $DEPLOY_PATH as static).

if [[ -z "${DEPLOY_HOST:-}" ]] || [[ -z "${DEPLOY_PATH:-}" ]]; then
  echo "DEPLOY_HOST and DEPLOY_PATH must be set" >&2
  exit 1
fi

echo "Building production bundle…"
pnpm install --frozen-lockfile
pnpm build

echo "Shipping to $DEPLOY_HOST:$DEPLOY_PATH"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cp -R dist/* "$tmp"/

# Stage to a temp dir on the host, then atomically swap.
ssh "$DEPLOY_HOST" "mkdir -p $DEPLOY_PATH.staging"
rsync -az --delete "$tmp"/ "$DEPLOY_HOST:$DEPLOY_PATH.staging/"
ssh "$DEPLOY_HOST" "rm -rf $DEPLOY_PATH.previous && \
  ( [ -d $DEPLOY_PATH ] && mv $DEPLOY_PATH $DEPLOY_PATH.previous || true ) && \
  mv $DEPLOY_PATH.staging $DEPLOY_PATH"

echo "Deployed."
