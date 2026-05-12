#!/usr/bin/env bash
# Invoked over SSH from the GitHub Actions workflow after the new binary
# has been scp'd to ${DEPLOY_PATH}/bin/spades-server.${SHORT_SHA}.
#
# Required env:
#   DEPLOY_PATH   on-disk install dir (e.g. /opt/spades-server)
#   SHORT_SHA     12-char short SHA matching the binary filename
# Optional env (defaults shown):
#   SYSTEMCTL     "sudo /bin/systemctl"     (override for tests)
#   HEALTH_URL    "http://127.0.0.1:3000/health"
#   KEEP          5                          (binaries to retain after prune)
set -euo pipefail

: "${DEPLOY_PATH:?DEPLOY_PATH is required}"
: "${SHORT_SHA:?SHORT_SHA is required}"
SYSTEMCTL="${SYSTEMCTL:-sudo /bin/systemctl}"
HEALTH_URL="${HEALTH_URL:-http://127.0.0.1:3000/health}"
KEEP="${KEEP:-5}"

cd "$DEPLOY_PATH"
NEW="bin/spades-server.${SHORT_SHA}"
if [ ! -x "$NEW" ]; then
    echo "missing or non-executable: $NEW" >&2
    exit 1
fi

PREV="$(readlink bin/spades-server-current || true)"
chmod 0755 "$NEW"

# Atomic symlink swap that works on both Linux (mv -T) and macOS (rm + ln).
swap_symlink() {
    local target=$1
    ln -sfn "$target" bin/spades-server-current.new
    # GNU mv supports -T; BSD mv does not. Use rename(2) via mv on a non-existent
    # destination, falling back to rm + mv.
    if mv -Tf bin/spades-server-current.new bin/spades-server-current 2>/dev/null; then
        return 0
    fi
    rm -f bin/spades-server-current
    mv bin/spades-server-current.new bin/spades-server-current
}

echo "==> swapping symlink -> spades-server.${SHORT_SHA}"
swap_symlink "spades-server.${SHORT_SHA}"

echo "==> restarting spades-server"
$SYSTEMCTL restart spades-server
sleep 1

echo "==> health-checking $HEALTH_URL"
HEALTHY=0
for _attempt in 1 2 3 4 5; do
    if curl -fsS --max-time 5 "$HEALTH_URL" >/dev/null 2>&1; then
        HEALTHY=1
        break
    fi
    sleep 1
done

if [ "$HEALTHY" -eq 0 ]; then
    echo "health check failed, reverting to ${PREV:-<none>}" >&2
    if [ -n "$PREV" ]; then
        swap_symlink "$PREV"
        $SYSTEMCTL restart spades-server || true
    fi
    exit 1
fi

echo "==> pruning bin/spades-server.* to last $KEEP (including live)"
LIVE="$(readlink bin/spades-server-current)"
# Sort by mtime (newest first), skip the live binary, remove older excess copies.
_keep=0
while IFS= read -r _f; do
    if [ "$_f" = "bin/$LIVE" ]; then continue; fi
    _keep=$(( _keep + 1 ))
    if [ "$_keep" -ge "$KEEP" ]; then
        rm -f "$_f"
    fi
done < <(find bin -maxdepth 1 -name 'spades-server.*' -not -name '*.new' \
    -printf '%T@ %p\n' 2>/dev/null | sort -rn | awk '{print $2}')

echo "==> remote-swap done: $SHORT_SHA"
