#!/usr/bin/env bash
set -euo pipefail

# ARCHIVED — the live deploy path is .github/workflows/deploy.yml.
# This script is kept for reference and as an emergency-only manual deploy.
# To use it: cd web && bash scripts/deploy-cf-pages.sh (requires wrangler login or CLOUDFLARE_API_TOKEN).

# Build the SPA locally and publish it to Cloudflare Pages.
# Runs from your laptop only — no git connection in the Pages project, no CI runners.
#
# Optional:
#   CF_PAGES_PROJECT       Cloudflare Pages project name (default: spades)
#   CF_PAGES_BRANCH        deployment branch label (default: main)
#   CLOUDFLARE_API_TOKEN   token with Pages:Edit; if unset, prior `wrangler login` is used
#   DEPLOY_SMOKE_URL       URL to smoke-check after deploy (default: https://app.wlim.dev/)
#
# Put `export CLOUDFLARE_API_TOKEN=...` in your shell rc, or source a local
# .deploy.env (gitignored).

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT/web"

if [ -f "$REPO_ROOT/.deploy.env" ]; then
    # shellcheck disable=SC1091
    source "$REPO_ROOT/.deploy.env"
fi

CF_PAGES_PROJECT="${CF_PAGES_PROJECT:-spades}"
CF_PAGES_BRANCH="${CF_PAGES_BRANCH:-main}"
DEPLOY_SMOKE_URL="${DEPLOY_SMOKE_URL:-https://app.wlim.dev/}"

LOCAL_SHA="$(git rev-parse HEAD)"
SHORT_SHA="${LOCAL_SHA:0:12}"

echo "==> Deploying spades @ $SHORT_SHA to Cloudflare Pages ($CF_PAGES_PROJECT)"

# Refuse if the working tree is dirty.
if [ -n "$(git status --porcelain)" ]; then
    echo "Error: local working tree is dirty. Commit or stash before deploying." >&2
    git status --short >&2
    exit 1
fi

# Verify the SHA is on the remote (matches the backend script's guarantee).
DEPLOY_BRANCH="$(git rev-parse --abbrev-ref HEAD)"
if ! git fetch --quiet origin "$DEPLOY_BRANCH" 2>/dev/null; then
    echo "Error: cannot fetch origin/$DEPLOY_BRANCH. Push first." >&2
    exit 1
fi
if ! git merge-base --is-ancestor "$LOCAL_SHA" "origin/$DEPLOY_BRANCH"; then
    echo "Error: HEAD is not on origin/$DEPLOY_BRANCH yet. Push first." >&2
    exit 1
fi

echo "==> pnpm install --frozen-lockfile"
pnpm install --frozen-lockfile

echo "==> pnpm build"
pnpm build

if [ ! -f dist/index.html ]; then
    echo "Error: dist/index.html missing after build" >&2
    exit 1
fi

echo "==> wrangler pages deploy dist/"
pnpm exec wrangler pages deploy dist \
    --project-name="$CF_PAGES_PROJECT" \
    --branch="$CF_PAGES_BRANCH" \
    --commit-hash="$LOCAL_SHA"

echo "==> smoke check $DEPLOY_SMOKE_URL"
# Pages propagation is usually instant but give it a few tries.
for i in 1 2 3 4 5; do
    if curl -fsS --max-time 5 "$DEPLOY_SMOKE_URL" | grep -q '<title'; then
        echo "==> smoke OK"
        echo "==> Deploy complete: $SHORT_SHA"
        exit 0
    fi
    sleep 2
done

echo "Warning: smoke check did not find a <title> at $DEPLOY_SMOKE_URL" >&2
echo "         The deploy succeeded but the URL may not be live yet (DNS/CDN propagation)." >&2
exit 1
