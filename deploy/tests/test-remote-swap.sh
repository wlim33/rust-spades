#!/usr/bin/env bash
# Tests for deploy/remote-swap.sh.
# Strategy: build a fake DEPLOY_PATH in a tempdir, override SYSTEMCTL and the
# health endpoint, run the script, assert symlink state.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REMOTE_SWAP="$SCRIPT_DIR/../remote-swap.sh"

fail() { echo "FAIL: $*" >&2; exit 1; }
pass() { echo "PASS: $*"; }

# --- harness helpers ---------------------------------------------------------

setup_tmpdeploy() {
    TMP=$(mktemp -d)
    mkdir -p "$TMP/bin"
    # Fake "previous" binary that systemctl thinks is running.
    cat >"$TMP/bin/spades-server.aaaaaaaaaaaa" <<'EOS'
#!/bin/sh
exec sleep 999
EOS
    chmod +x "$TMP/bin/spades-server.aaaaaaaaaaaa"
    ln -sfn spades-server.aaaaaaaaaaaa "$TMP/bin/spades-server-current"
    echo "$TMP"
}

start_fake_health() {
    # $1 = 200 or 500, $2 = port
    local code=$1 port=$2
    python3 -c "
import http.server, sys
class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response($code); self.end_headers(); self.wfile.write(b'ok')
    def log_message(self, *a, **kw): pass
http.server.HTTPServer(('127.0.0.1', $port), H).serve_forever()
" &
    echo $!
}

stop_fake_health() {
    kill "$1" 2>/dev/null || true
    wait "$1" 2>/dev/null || true
}

# --- tests -------------------------------------------------------------------

test_happy_path_swaps_symlink() {
    local TMP; TMP=$(setup_tmpdeploy)
    cat >"$TMP/bin/spades-server.bbbbbbbbbbbb" <<'EOS'
#!/bin/sh
exec sleep 999
EOS
    chmod +x "$TMP/bin/spades-server.bbbbbbbbbbbb"

    local HEALTH_PID; HEALTH_PID=$(start_fake_health 200 33001)
    sleep 0.3

    DEPLOY_PATH="$TMP" \
    SHORT_SHA="bbbbbbbbbbbb" \
    SYSTEMCTL="true" \
    HEALTH_URL="http://127.0.0.1:33001/health" \
        bash "$REMOTE_SWAP" \
        || { stop_fake_health "$HEALTH_PID"; fail "script exited non-zero"; }

    stop_fake_health "$HEALTH_PID"

    local live; live=$(readlink "$TMP/bin/spades-server-current")
    [ "$live" = "spades-server.bbbbbbbbbbbb" ] || fail "expected bbbb live, got $live"
    pass "happy path swaps symlink"
    rm -rf "$TMP"
}

test_health_failure_auto_reverts() {
    local TMP; TMP=$(setup_tmpdeploy)
    cat >"$TMP/bin/spades-server.cccccccccccc" <<'EOS'
#!/bin/sh
exec sleep 999
EOS
    chmod +x "$TMP/bin/spades-server.cccccccccccc"

    local HEALTH_PID; HEALTH_PID=$(start_fake_health 500 33002)
    sleep 0.3

    DEPLOY_PATH="$TMP" \
    SHORT_SHA="cccccccccccc" \
    SYSTEMCTL="true" \
    HEALTH_URL="http://127.0.0.1:33002/health" \
        bash "$REMOTE_SWAP" \
        && { stop_fake_health "$HEALTH_PID"; fail "script should have exited non-zero"; } \
        || true

    stop_fake_health "$HEALTH_PID"

    local live; live=$(readlink "$TMP/bin/spades-server-current")
    [ "$live" = "spades-server.aaaaaaaaaaaa" ] || fail "expected revert to aaaa, got $live"
    pass "health failure auto-reverts"
    rm -rf "$TMP"
}

test_missing_binary_fails_fast() {
    local TMP; TMP=$(setup_tmpdeploy)
    # Don't create the binary for SHORT_SHA.

    DEPLOY_PATH="$TMP" \
    SHORT_SHA="dddddddddddd" \
    SYSTEMCTL="true" \
    HEALTH_URL="http://127.0.0.1:33003/health" \
        bash "$REMOTE_SWAP" \
        && fail "script should have exited non-zero" \
        || true

    local live; live=$(readlink "$TMP/bin/spades-server-current")
    [ "$live" = "spades-server.aaaaaaaaaaaa" ] || fail "symlink should be unchanged, got $live"
    pass "missing binary fails fast"
    rm -rf "$TMP"
}

test_prune_keeps_last_5() {
    local TMP; TMP=$(setup_tmpdeploy)
    # Pre-seed 6 old binaries with descending mtimes.
    for i in 1 2 3 4 5 6; do
        local name="spades-server.old$i"
        printf '#!/bin/sh\nsleep 999\n' >"$TMP/bin/$name"
        chmod +x "$TMP/bin/$name"
        touch -d "$i days ago" "$TMP/bin/$name"
    done
    # The new one we're swapping in.
    cat >"$TMP/bin/spades-server.eeeeeeeeeeee" <<'EOS'
#!/bin/sh
exec sleep 999
EOS
    chmod +x "$TMP/bin/spades-server.eeeeeeeeeeee"

    local HEALTH_PID; HEALTH_PID=$(start_fake_health 200 33004)
    sleep 0.3

    DEPLOY_PATH="$TMP" \
    SHORT_SHA="eeeeeeeeeeee" \
    SYSTEMCTL="true" \
    HEALTH_URL="http://127.0.0.1:33004/health" \
        bash "$REMOTE_SWAP"

    stop_fake_health "$HEALTH_PID"

    local count
    count=$(find "$TMP/bin" -maxdepth 1 -name 'spades-server.*' | wc -l | tr -d ' ')
    [ "$count" -le 5 ] || fail "expected <=5 binaries after prune, got $count"
    [ -x "$TMP/bin/spades-server.eeeeeeeeeeee" ] || fail "live binary was pruned"
    pass "prune keeps last 5"
    rm -rf "$TMP"
}

test_happy_path_swaps_symlink
test_health_failure_auto_reverts
test_missing_binary_fails_fast
test_prune_keeps_last_5
echo "all remote-swap tests passed"
