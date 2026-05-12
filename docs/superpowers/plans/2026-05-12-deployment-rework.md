# Deployment Rework Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace today's laptop-driven, build-on-VPS deploy with a single GitHub Actions workflow that triggers on push-to-main, runs in a monorepo (spades-ts merged into `web/`), pushes a prebuilt binary to the VPS over SSH, and ships the frontend to Cloudflare Pages via wrangler.

**Architecture:** One workflow file (`.github/workflows/deploy.yml`) with two jobs: `ci` (test + build, runs on push/PR/dispatch) and `deploy` (gated to push-to-main, SSH push of backend then wrangler push of frontend). The VPS holds binaries only — never source. A new `deploy/remote-swap.sh` performs the atomic symlink swap and auto-reverts on `/health` failure. Runtime secrets live in `/etc/spades/env` loaded by systemd, not in the deploy path.

**Tech Stack:**
- Rust workspace (`spades`, `spades-server` crates) — release binary built on `ubuntu-latest`, target `x86_64-unknown-linux-gnu`
- TypeScript + Vite + pnpm in `web/`
- GitHub Actions: `actions/checkout@v4`, `actions/setup-node@v4`, `pnpm/action-setup@v4`, `Swatinem/rust-cache@v2`, `actions/upload-artifact@v4`, `webfactory/ssh-agent@v0.9.0`, `cloudflare/wrangler-action@v3`
- VPS: Debian/Ubuntu, systemd, SQLite at `/var/lib/spades/games.sqlite`

**Source spec:** `docs/superpowers/specs/2026-05-12-deployment-rework-design.md`

---

## File Structure

**New files:**
- `web/` — entire spades-ts repo merged in via `git subtree add` (preserves history)
- `web/scripts/deploy-cf-pages.sh` — archived copy of spades-ts's old `bin/deploy`; never invoked by the new flow
- `deploy/remote-swap.sh` — invoked by the GH Action over SSH after `scp`. Performs atomic symlink swap, restart, health-check with auto-revert, prune.
- `deploy/tests/test-remote-swap.sh` — bash test harness for `remote-swap.sh` (mocks systemctl + health endpoint)
- `deploy/env.template` — example `/etc/spades/env` showing every variable the server reads
- `.github/workflows/deploy.yml` — single workflow, two jobs

**Modified files:**
- `deploy/setup.sh` — drop Rust install / git clone / cargo build; add `/etc/spades/env` provisioning; replace CORS systemd drop-in with `EnvironmentFile`
- `deploy/spades-server.service` — add `EnvironmentFile=/etc/spades/env`
- `bin/deploy` — local build for linux target, `scp`, invoke `remote-swap.sh` (no remote build)
- `bin/rollback` — unchanged (verify still works against new bin/ layout)
- `SERVER.md` — note the new deployment path; remove references to laptop-builds-on-VPS
- `.gitignore` — already excludes `bin/deploy`, `bin/deploy-all`, `bin/rollback` (currently gitignored). After this rework, `bin/deploy` and `bin/rollback` should become tracked. Adjust gitignore accordingly. **`bin/deploy-all` is removed entirely.**

**Removed files:**
- `bin/deploy-all` — no longer relevant (one workflow does both)

---

## Task 1: Pre-flight — capture VPS specifics

**Files:** none (research only; record outcomes in commit message of Task 2)

**Why:** The workflow needs a build target matching the VPS's libc. Choose `x86_64-unknown-linux-gnu` (default) or `x86_64-unknown-linux-musl` (if glibc mismatch).

- [ ] **Step 1: SSH in and capture arch/libc/distro**

```bash
ssh deploy@$DEPLOY_HOST 'uname -m && ldd --version | head -1 && cat /etc/os-release | head -3'
```

Expected output: `x86_64`, a glibc version (e.g. `ldd (Debian GLIBC 2.36-9+deb12u4) 2.36`), and the distro name. Record both.

- [ ] **Step 2: Decide the Rust target**

If `ldd --version` reports glibc ≥ 2.31 (any recent Debian/Ubuntu), use `x86_64-unknown-linux-gnu`. If anything weird (Alpine, super-old distro), use `x86_64-unknown-linux-musl` instead. Note the choice — every later task referencing the target uses this value.

- [ ] **Step 3: Confirm pnpm version used by spades-ts**

```bash
grep -E '"packageManager"|"pnpm"' /Users/wlim/Projects/spades-ts/package.json
```

Record the pnpm version (e.g., `pnpm@9.x`). The workflow will pin this exact version.

- [ ] **Step 4: Confirm wrangler invocation**

```bash
grep -E "wrangler|CF_PAGES_PROJECT" /Users/wlim/Projects/spades-ts/bin/deploy
```

Record the exact `wrangler pages deploy` flags used today (project name, branch, output dir). These will be replicated in the workflow.

---

## Task 2: Monorepo merge — bring spades-ts in under `web/`

**Files:**
- Create: `web/` (subtree)
- (Optional) Modify: `.gitignore` if spades-ts has root-level ignores that conflict

- [ ] **Step 1: Verify both repos are clean and pushed**

```bash
cd /Users/wlim/Projects/rust-spades && git status --porcelain && git log origin/master..HEAD --oneline
cd /Users/wlim/Projects/spades-ts && git status --porcelain && git log origin/main..HEAD --oneline
```

Both `git status --porcelain` must be empty. Both `git log` diffs must be empty (no unpushed commits). If not, push first.

- [ ] **Step 2: Add spades-ts as a subtree**

From `/Users/wlim/Projects/rust-spades`:

```bash
git subtree add --prefix=web https://github.com/wlim33/spades-ts.git main
```

(Note: rust-spades's main branch is `master`, spades-ts's is `main` — confirm by checking `git branch -a` in spades-ts before running this.)

Expected: a merge commit on master that adds every file from spades-ts under `web/`, preserving history (no `--squash`).

- [ ] **Step 3: Verify the merge**

```bash
ls web/package.json web/src web/vite.config.ts
git log --oneline -5
```

`web/package.json` etc. exist. `git log` shows the subtree merge plus prior spades-ts commits.

- [ ] **Step 4: Run frontend build/tests in place**

```bash
cd web && corepack enable && pnpm install --frozen-lockfile && pnpm test && pnpm build
```

All tests pass. `web/dist/` is created. Confirms the merge is functional before any further changes.

- [ ] **Step 5: Commit**

The `git subtree add` already created a commit, so this is verification only:

```bash
git log -1 --stat | head -20
```

The single commit you see should be the subtree merge. Note its SHA.

---

## Task 3: Archive the old frontend deploy script

**Files:**
- Move: `web/bin/deploy` → `web/scripts/deploy-cf-pages.sh`

- [ ] **Step 1: Move the file**

```bash
mkdir -p web/scripts
git mv web/bin/deploy web/scripts/deploy-cf-pages.sh
```

- [ ] **Step 2: Remove `web/bin/` if it's now empty**

```bash
ls web/bin/ 2>/dev/null && rmdir web/bin/ 2>/dev/null || true
```

- [ ] **Step 3: Prepend an archive notice to the script**

Open `web/scripts/deploy-cf-pages.sh`. Insert this comment block right after the shebang (after `set -euo pipefail`):

```bash
# ARCHIVED — the live deploy path is .github/workflows/deploy.yml.
# This script is kept for reference and as an emergency-only manual deploy.
# To use it: cd web && bash scripts/deploy-cf-pages.sh (requires wrangler login or CLOUDFLARE_API_TOKEN).
```

- [ ] **Step 4: Commit**

```bash
git add web/scripts/deploy-cf-pages.sh
git commit -m "deploy: archive web/bin/deploy as web/scripts/deploy-cf-pages.sh"
```

---

## Task 4: Write `deploy/remote-swap.sh` (TDD)

**Files:**
- Create: `deploy/remote-swap.sh`
- Create: `deploy/tests/test-remote-swap.sh`

**Why TDD:** This script's auto-revert behavior is the main robustness improvement. Worth testing.

- [ ] **Step 1: Write the test harness skeleton**

Create `deploy/tests/test-remote-swap.sh`:

```bash
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
    # $1 = 200 or 500
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

    # Total binaries should be <= 5 (live + 4 others kept).
    local count
    count=$(ls -1 "$TMP/bin/spades-server."* 2>/dev/null | wc -l | tr -d ' ')
    [ "$count" -le 5 ] || fail "expected <=5 binaries after prune, got $count"
    # The live binary must still be there.
    [ -x "$TMP/bin/spades-server.eeeeeeeeeeee" ] || fail "live binary was pruned"
    pass "prune keeps last 5"
    rm -rf "$TMP"
}

test_happy_path_swaps_symlink
test_health_failure_auto_reverts
test_missing_binary_fails_fast
test_prune_keeps_last_5
echo "all remote-swap tests passed"
```

- [ ] **Step 2: Run the tests — they should fail (script doesn't exist yet)**

```bash
chmod +x deploy/tests/test-remote-swap.sh
bash deploy/tests/test-remote-swap.sh
```

Expected: fails because `deploy/remote-swap.sh` does not exist. This is the failing-test step.

- [ ] **Step 3: Write `deploy/remote-swap.sh`**

Create `deploy/remote-swap.sh`:

```bash
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

echo "==> swapping symlink -> spades-server.${SHORT_SHA}"
ln -sfn "spades-server.${SHORT_SHA}" bin/spades-server-current.new
mv -Tf bin/spades-server-current.new bin/spades-server-current

echo "==> restarting spades-server"
$SYSTEMCTL restart spades-server
sleep 1

echo "==> health-checking $HEALTH_URL"
HEALTHY=0
for i in 1 2 3 4 5; do
    if curl -fsS --max-time 5 "$HEALTH_URL" >/dev/null 2>&1; then
        HEALTHY=1
        break
    fi
    sleep 1
done

if [ "$HEALTHY" -eq 0 ]; then
    echo "health check failed, reverting to ${PREV:-<none>}" >&2
    if [ -n "$PREV" ]; then
        ln -sfn "$PREV" bin/spades-server-current.new
        mv -Tf bin/spades-server-current.new bin/spades-server-current
        $SYSTEMCTL restart spades-server || true
    fi
    exit 1
fi

echo "==> pruning bin/spades-server.* to last $KEEP (including live)"
LIVE="$(readlink bin/spades-server-current)"
(
    set +o pipefail
    ls -1t bin/spades-server.* 2>/dev/null \
        | grep -Fxv "bin/$LIVE" \
        | tail -n +"$KEEP" \
        | xargs -r rm -f --
) || true

echo "==> remote-swap done: $SHORT_SHA"
```

```bash
chmod +x deploy/remote-swap.sh
```

- [ ] **Step 4: Run shellcheck**

```bash
shellcheck deploy/remote-swap.sh
```

Expected: no warnings. (If shellcheck isn't installed: `brew install shellcheck`.)

- [ ] **Step 5: Run the test harness — all four tests should pass**

```bash
bash deploy/tests/test-remote-swap.sh
```

Expected output: four `PASS:` lines and `all remote-swap tests passed`.

- [ ] **Step 6: Commit**

```bash
git add deploy/remote-swap.sh deploy/tests/test-remote-swap.sh
git commit -m "deploy: remote-swap.sh + tests (auto-revert on health failure)"
```

---

## Task 5: Add EnvironmentFile to systemd unit

**Files:**
- Modify: `deploy/spades-server.service`

- [ ] **Step 1: Edit the unit file**

Open `deploy/spades-server.service`. Replace the `[Service]` block so it reads:

```ini
[Service]
Type=simple
User=deploy
Group=deploy
WorkingDirectory=/opt/spades-server
EnvironmentFile=/etc/spades/env
ExecStart=/opt/spades-server/bin/spades-server-current --port 3000 --db /var/lib/spades/games.sqlite
Restart=on-failure
RestartSec=5s

# Lock down: read-only filesystem except for the data dir
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/spades
NoNewPrivileges=true
PrivateTmp=true
```

Remove the old commented-out `# Environment=CORS_ALLOW_ORIGIN=...` line — replaced by the env file.

- [ ] **Step 2: Commit**

```bash
git add deploy/spades-server.service
git commit -m "deploy: load runtime env from /etc/spades/env"
```

---

## Task 6: Create `/etc/spades/env` template

**Files:**
- Create: `deploy/env.template`

- [ ] **Step 1: Write the template**

Create `deploy/env.template`:

```sh
# /etc/spades/env — runtime env for spades-server.
# Mode 0640, owned root:deploy. Created by hand on the VPS; never touched by
# deploy scripts. Loaded by systemd via EnvironmentFile= in spades-server.service.

# CORS origin(s), comma-separated.
CORS_ALLOW_ORIGIN=https://app.wlim.dev

# OAuth callback base — must match what's registered with each provider.
OAUTH_REDIRECT_BASE_URL=https://spades.wlim.dev

# Google OAuth (leave both empty to disable Google sign-in).
GOOGLE_OAUTH_CLIENT_ID=
GOOGLE_OAUTH_CLIENT_SECRET=

# GitHub OAuth (leave both empty to disable GitHub sign-in).
GITHUB_OAUTH_CLIENT_ID=
GITHUB_OAUTH_CLIENT_SECRET=

# SMTP (leave any empty to disable email).
SMTP_HOST=
SMTP_PORT=587
SMTP_USER=
SMTP_PASS=
SMTP_FROM=
SMTP_STARTTLS=true
```

- [ ] **Step 2: Commit**

```bash
git add deploy/env.template
git commit -m "deploy: env.template documenting every runtime variable"
```

---

## Task 7: Update `deploy/setup.sh`

**Files:**
- Modify: `deploy/setup.sh`

- [ ] **Step 1: Rewrite the script**

Open `deploy/setup.sh` and replace its contents with:

```bash
#!/usr/bin/env bash
# One-time setup (idempotent) for the deploy host. Run as a user with sudo.
# Assumes Debian/Ubuntu.
#
# Usage: bash setup.sh
#
# Re-run safely after pulling changes to pick up systemd-unit updates.
# See the "Next steps" epilogue at the end for GitHub Actions wiring.
set -euo pipefail

DEPLOY_USER="${DEPLOY_USER:-deploy}"
INSTALL_DIR="${INSTALL_DIR:-/opt/spades-server}"
DATA_DIR="${DATA_DIR:-/var/lib/spades}"
ENV_FILE="${ENV_FILE:-/etc/spades/env}"

# The script lives at <repo>/deploy/setup.sh; everything it installs lives
# alongside it. We don't need the rest of the source tree on the VPS.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "==> Installing runtime prerequisites"
sudo apt-get update
sudo apt-get install -y curl ca-certificates

echo "==> Creating deploy user (if missing)"
if ! id -u "$DEPLOY_USER" >/dev/null 2>&1; then
    sudo adduser --system --group --shell /bin/bash --home "/home/$DEPLOY_USER" "$DEPLOY_USER"
    sudo mkdir -p "/home/$DEPLOY_USER/.ssh"
    sudo chown "$DEPLOY_USER:$DEPLOY_USER" "/home/$DEPLOY_USER/.ssh"
    sudo chmod 700 "/home/$DEPLOY_USER/.ssh"
fi

echo "==> Creating $INSTALL_DIR/bin"
sudo mkdir -p "$INSTALL_DIR/bin"
sudo chown -R "$DEPLOY_USER:$DEPLOY_USER" "$INSTALL_DIR"

echo "==> Creating data dir $DATA_DIR"
sudo mkdir -p "$DATA_DIR"
sudo chown "$DEPLOY_USER:$DEPLOY_USER" "$DATA_DIR"

echo "==> Creating $ENV_FILE (if missing)"
sudo mkdir -p "$(dirname "$ENV_FILE")"
if [ ! -f "$ENV_FILE" ]; then
    sudo install -m 0640 -o root -g "$DEPLOY_USER" "$SCRIPT_DIR/env.template" "$ENV_FILE"
    echo "    -- wrote template to $ENV_FILE; edit it with your real secrets before restarting."
else
    echo "    -- $ENV_FILE already exists; leaving it alone."
fi

echo "==> Installing systemd unit"
sudo cp "$SCRIPT_DIR/spades-server.service" /etc/systemd/system/spades-server.service

# Remove the legacy CORS drop-in if it exists — env file replaces it.
sudo rm -f /etc/systemd/system/spades-server.service.d/cors.conf
sudo rmdir /etc/systemd/system/spades-server.service.d 2>/dev/null || true

sudo systemctl daemon-reload
sudo systemctl enable spades-server

echo "==> Granting passwordless 'systemctl restart spades-server' to $DEPLOY_USER"
SUDOERS_FILE="/etc/sudoers.d/spades-deploy"
echo "$DEPLOY_USER ALL=(root) NOPASSWD: /bin/systemctl restart spades-server, /bin/systemctl is-active spades-server" \
    | sudo tee "$SUDOERS_FILE" >/dev/null
sudo chmod 440 "$SUDOERS_FILE"

cat <<EOF

==> Done.

Next steps:
  1. Edit $ENV_FILE with your real secrets:
       sudo -e $ENV_FILE
  2. Add your GitHub Actions deploy public key to:
       /home/$DEPLOY_USER/.ssh/authorized_keys
     (See plan Task 9.)
  3. After the first GitHub Actions deploy lands a binary in
     $INSTALL_DIR/bin/, the server will start. Trigger manually with:
       sudo systemctl restart spades-server

EOF
```

- [ ] **Step 2: Lint with shellcheck**

```bash
shellcheck deploy/setup.sh
```

Expected: no warnings.

- [ ] **Step 3: Commit**

```bash
git add deploy/setup.sh
git commit -m "deploy: setup.sh no longer builds or clones — VPS holds binaries only"
```

---

## Task 8: Apply changes to the VPS (manual operator step)

**Files:** none in this repo — operations on the VPS only.

**Why a task, not just an aside:** Future tasks (workflow validation) depend on this. If you skip it, the first GH Action run will fail.

- [ ] **Step 1: Push the spec/plan commits to origin/master**

```bash
git -C /Users/wlim/Projects/rust-spades push origin master
```

(The setup.sh script is pulled from your local laptop in Step 2; pushing isn't strictly required for this step, but the rest of the deploy chain needs it.)

- [ ] **Step 2: Copy the new setup files to the VPS**

```bash
scp deploy/setup.sh deploy/spades-server.service deploy/env.template "deploy@$DEPLOY_HOST:/tmp/spades-setup/"
```

(Create `/tmp/spades-setup/` on the server first if needed: `ssh deploy@$DEPLOY_HOST mkdir -p /tmp/spades-setup`.)

- [ ] **Step 3: Run setup.sh on the VPS**

```bash
ssh deploy@$DEPLOY_HOST 'cd /tmp/spades-setup && bash setup.sh'
```

Expected: the script creates `/opt/spades-server/bin/`, installs the systemd unit, writes `/etc/spades/env` from the template, and prints "Done." It does NOT start the server (no binary yet).

- [ ] **Step 4: Fill in `/etc/spades/env`**

```bash
ssh deploy@$DEPLOY_HOST 'sudo -e /etc/spades/env'
```

Fill in real values for SMTP, OAuth, CORS. Save.

- [ ] **Step 5: Verify the existing live binary still works**

The previous deploy left a binary at `/opt/spades-server/bin/spades-server-current` (symlink). After the systemd unit reload, the service should still come up fine:

```bash
ssh deploy@$DEPLOY_HOST 'sudo systemctl restart spades-server && sleep 2 && sudo systemctl is-active spades-server && curl -fsS http://127.0.0.1:3000/health'
```

Expected: `active` followed by `ok`. If health check fails, inspect `journalctl -u spades-server -n 50` — most likely cause is a missing env var in `/etc/spades/env`.

- [ ] **Step 6: Generate SSH key for GitHub Actions and authorize it**

On your laptop:

```bash
ssh-keygen -t ed25519 -f ~/.ssh/spades-deploy-gha -N "" -C "github-actions@spades"
cat ~/.ssh/spades-deploy-gha.pub
```

On the VPS:

```bash
ssh deploy@$DEPLOY_HOST 'cat >> ~/.ssh/authorized_keys'
# paste the public key, then Ctrl-D
```

Test the new key:

```bash
ssh -i ~/.ssh/spades-deploy-gha deploy@$DEPLOY_HOST 'echo ok'
```

Expected: `ok`. Keep the **private** key on your laptop only; it'll be set as a GitHub secret in Task 10.

- [ ] **Step 7: Capture the VPS host key for known_hosts pinning**

```bash
ssh-keyscan -t ed25519 "$DEPLOY_HOST"
```

Save the output line — you'll paste it as `DEPLOY_KNOWN_HOSTS` in Task 10.

---

## Task 9: Write the GitHub Actions workflow

**Files:**
- Create: `.github/workflows/deploy.yml`

- [ ] **Step 1: Create the directory and workflow file**

```bash
mkdir -p .github/workflows
```

Create `.github/workflows/deploy.yml`:

```yaml
name: deploy

on:
  push:
    branches: [master]
  pull_request:
    branches: [master]
  workflow_dispatch:

concurrency:
  group: deploy-${{ github.ref }}
  cancel-in-progress: false

env:
  RUST_TARGET: x86_64-unknown-linux-gnu

jobs:
  ci:
    name: test + build
    runs-on: ubuntu-latest
    timeout-minutes: 30
    steps:
      - uses: actions/checkout@v4

      - name: Set short SHA
        id: sha
        run: echo "short=$(git rev-parse --short=12 HEAD)" >> "$GITHUB_OUTPUT"

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ env.RUST_TARGET }}

      - name: Cargo cache
        uses: Swatinem/rust-cache@v2

      - name: cargo test
        run: cargo test --workspace --locked

      - name: cargo build --release
        run: cargo build --release --locked -p spades-server --target ${{ env.RUST_TARGET }}

      - name: Stage backend artifact
        run: |
          mkdir -p artifact/backend
          cp target/${{ env.RUST_TARGET }}/release/spades-server \
             artifact/backend/spades-server.${{ steps.sha.outputs.short }}

      - name: pnpm setup
        uses: pnpm/action-setup@v4
        with:
          run_install: false

      - name: Node setup
        uses: actions/setup-node@v4
        with:
          node-version: '20'
          cache: pnpm
          cache-dependency-path: web/pnpm-lock.yaml

      - name: pnpm install
        run: pnpm install --frozen-lockfile
        working-directory: web

      - name: pnpm test
        run: pnpm test
        working-directory: web

      - name: pnpm build
        run: pnpm build
        working-directory: web

      - name: Upload backend binary
        uses: actions/upload-artifact@v4
        with:
          name: spades-server-${{ steps.sha.outputs.short }}
          path: artifact/backend/spades-server.${{ steps.sha.outputs.short }}
          retention-days: 90
          if-no-files-found: error

      - name: Upload frontend bundle
        uses: actions/upload-artifact@v4
        with:
          name: spades-web-${{ steps.sha.outputs.short }}
          path: web/dist
          retention-days: 90
          if-no-files-found: error

  deploy:
    name: ship
    needs: ci
    if: github.event_name == 'push' && github.ref == 'refs/heads/master'
    runs-on: ubuntu-latest
    timeout-minutes: 15
    steps:
      - uses: actions/checkout@v4

      - name: Set short SHA
        id: sha
        run: echo "short=$(git rev-parse --short=12 HEAD)" >> "$GITHUB_OUTPUT"

      - name: Download backend binary
        uses: actions/download-artifact@v4
        with:
          name: spades-server-${{ steps.sha.outputs.short }}
          path: artifact/backend

      - name: Download frontend bundle
        uses: actions/download-artifact@v4
        with:
          name: spades-web-${{ steps.sha.outputs.short }}
          path: web/dist

      - name: Configure SSH
        uses: webfactory/ssh-agent@v0.9.0
        with:
          ssh-private-key: ${{ secrets.DEPLOY_SSH_KEY }}

      - name: Pin host key
        run: |
          mkdir -p ~/.ssh
          echo "${{ secrets.DEPLOY_KNOWN_HOSTS }}" >> ~/.ssh/known_hosts
          chmod 644 ~/.ssh/known_hosts

      - name: scp binary to VPS
        run: |
          chmod 0755 artifact/backend/spades-server.${{ steps.sha.outputs.short }}
          scp artifact/backend/spades-server.${{ steps.sha.outputs.short }} \
            deploy@${{ secrets.DEPLOY_HOST }}:/opt/spades-server/bin/

      - name: Remote swap + health check
        run: |
          ssh deploy@${{ secrets.DEPLOY_HOST }} \
            DEPLOY_PATH=/opt/spades-server \
            SHORT_SHA=${{ steps.sha.outputs.short }} \
            bash -s < deploy/remote-swap.sh

      - name: Deploy frontend to Cloudflare Pages
        uses: cloudflare/wrangler-action@v3
        with:
          apiToken: ${{ secrets.CLOUDFLARE_API_TOKEN }}
          accountId: ${{ secrets.CLOUDFLARE_ACCOUNT_ID }}
          command: pages deploy web/dist --project-name=spades --branch=main --commit-dirty=true

      - name: Smoke check
        run: |
          curl -fsS --max-time 10 https://app.wlim.dev/ >/dev/null
          curl -fsS --max-time 10 https://spades.wlim.dev/health >/dev/null
```

**Notes for the implementing engineer:**
- Branch is `master` for rust-spades — not `main`. Adjust if your default branch differs.
- The Cloudflare Pages project's "production branch" is `main` (per the archived `web/scripts/deploy-cf-pages.sh` defaults). Confirm this with `wrangler pages project list` if uncertain.
- The deploy job only runs on push to master. PRs validate ci only.

- [ ] **Step 2: Lint the workflow**

```bash
npx -y action-validator .github/workflows/deploy.yml
```

(Or use the `rhysd/actionlint` action locally with `actionlint`.)

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/deploy.yml
git commit -m "ci: single workflow deploys backend to VPS, frontend to CF Pages"
```

---

## Task 10: Configure GitHub Actions secrets

**Files:** none in repo — configuration in GitHub.

**Why a task:** Without these, the deploy job will fail.

- [ ] **Step 1: Set the five secrets**

Using `gh` CLI from the repo root:

```bash
# Private key generated in Task 8 Step 6:
gh secret set DEPLOY_SSH_KEY < ~/.ssh/spades-deploy-gha

# Host or IP of the VPS:
gh secret set DEPLOY_HOST -b "<your-vps-hostname-or-ip>"

# Output of `ssh-keyscan` from Task 8 Step 7:
gh secret set DEPLOY_KNOWN_HOSTS -b "<paste the full ssh-keyscan line here>"

# Cloudflare API token with Pages:Edit scope:
gh secret set CLOUDFLARE_API_TOKEN -b "<token>"

# Cloudflare account ID (find in CF dashboard, right sidebar):
gh secret set CLOUDFLARE_ACCOUNT_ID -b "<account-id>"
```

- [ ] **Step 2: Verify**

```bash
gh secret list
```

Expected: all five names present.

---

## Task 11: Validate the workflow on a PR

**Files:** none in repo — but you'll create a throwaway PR.

- [ ] **Step 1: Open a no-op PR to exercise the `ci` job**

```bash
git checkout -b test/deploy-workflow-validate
# Make a trivial change (e.g., a typo in a comment in deploy/remote-swap.sh).
git commit -am "test: validate deploy workflow"
git push -u origin test/deploy-workflow-validate
gh pr create --base master --title "test: validate deploy workflow" --body "Validates the new CI workflow. Do not merge."
```

- [ ] **Step 2: Watch the CI job run**

```bash
gh run watch
```

Expected: the `ci` job passes (test + build for both backend and frontend, artifacts uploaded). The `deploy` job is skipped because `github.event_name == 'pull_request'`.

- [ ] **Step 3: Close the PR without merging**

```bash
gh pr close --delete-branch
```

- [ ] **Step 4: Trigger a real deploy via workflow_dispatch on a known-good SHA**

To avoid surprising the production server with an untested change, trigger a deploy of the current `master` HEAD (which has the same binary already running) via:

```bash
gh workflow run deploy.yml --ref master
gh run watch
```

Expected: the full workflow runs end-to-end. The `deploy` job scp's the new binary, the symlink swaps, health passes, frontend deploys, smoke check passes. Since the code is identical to what was running, behavior is unchanged.

- [ ] **Step 5: Verify the swap happened on the VPS**

```bash
ssh deploy@$DEPLOY_HOST 'readlink /opt/spades-server/bin/spades-server-current'
```

Expected: `spades-server.<short-sha-of-master-HEAD>`.

---

## Task 12: Rework `bin/deploy` as break-glass-only

**Files:**
- Modify: `bin/deploy`

- [ ] **Step 1: Rewrite `bin/deploy`**

Open `bin/deploy` and replace with:

```bash
#!/usr/bin/env bash
# Break-glass local deploy. Use when GitHub Actions is unavailable.
# Builds the release binary locally (cross-compile to linux-gnu), ships it
# to the VPS, runs the same remote-swap script the workflow uses.
#
# Required:
#   DEPLOY_HOST   hostname/IP (or set in .deploy.env)
# Optional:
#   DEPLOY_USER   default: deploy
#   DEPLOY_PATH   default: /opt/spades-server
#   RUST_TARGET   default: x86_64-unknown-linux-gnu
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
if [ -f "$REPO_ROOT/.deploy.env" ]; then
    # shellcheck disable=SC1091
    source "$REPO_ROOT/.deploy.env"
fi

: "${DEPLOY_HOST:?DEPLOY_HOST not set — export it or add it to .deploy.env}"
DEPLOY_USER="${DEPLOY_USER:-deploy}"
DEPLOY_PATH="${DEPLOY_PATH:-/opt/spades-server}"
RUST_TARGET="${RUST_TARGET:-x86_64-unknown-linux-gnu}"

cd "$REPO_ROOT"

if [ -n "$(git status --porcelain)" ]; then
    echo "Error: working tree is dirty. Commit or stash before deploying." >&2
    git status --short >&2
    exit 1
fi

SHORT_SHA="$(git rev-parse --short=12 HEAD)"

echo "==> Building release binary for $RUST_TARGET"
# `rustup target add` is idempotent; safe to run every time.
rustup target add "$RUST_TARGET"
cargo build --release --locked -p spades-server --target "$RUST_TARGET"

LOCAL_BIN="target/$RUST_TARGET/release/spades-server"
[ -x "$LOCAL_BIN" ] || { echo "build produced no binary at $LOCAL_BIN" >&2; exit 1; }

echo "==> scp -> $DEPLOY_USER@$DEPLOY_HOST:$DEPLOY_PATH/bin/spades-server.$SHORT_SHA"
scp "$LOCAL_BIN" "$DEPLOY_USER@$DEPLOY_HOST:$DEPLOY_PATH/bin/spades-server.$SHORT_SHA"

echo "==> Running remote-swap.sh"
ssh "$DEPLOY_USER@$DEPLOY_HOST" \
    DEPLOY_PATH="$DEPLOY_PATH" \
    SHORT_SHA="$SHORT_SHA" \
    bash -s < "$REPO_ROOT/deploy/remote-swap.sh"

echo "==> Break-glass deploy complete: $SHORT_SHA"
echo "    (Push to master via PR + CI for normal deploys.)"
```

```bash
chmod +x bin/deploy
```

- [ ] **Step 2: Lint**

```bash
shellcheck bin/deploy
```

Expected: no warnings.

- [ ] **Step 3: Verify the script syntactically — do NOT run it against prod**

```bash
bash -n bin/deploy
```

Expected: no output (clean syntax). To smoke-test against a non-prod target later, point `DEPLOY_HOST` at a throwaway box.

---

## Task 13: Commit `bin/deploy` rework and remove `bin/deploy-all`

**Files:**
- Modify: `bin/deploy` (already changed in Task 12)
- Remove: `bin/deploy-all`
- Verify (no edit): `bin/rollback`

The `bin/` scripts are already tracked in git (commit `fc95074`). No `.gitignore` change needed.

- [ ] **Step 1: Remove `bin/deploy-all`**

```bash
git rm bin/deploy-all
```

- [ ] **Step 2: Confirm `bin/rollback` still works against the new layout**

Read `bin/rollback`. It targets `bin/spades-server.<sha>` files on the VPS and flips `bin/spades-server-current`. The new flow uses the same paths, so `bin/rollback` continues to work unchanged.

```bash
shellcheck bin/rollback
```

Expected: no warnings.

- [ ] **Step 3: Commit Task 12's rework together with the deletion**

```bash
git add bin/deploy
git commit -m "deploy: bin/deploy is now break-glass-only (build locally, scp, run remote-swap)

bin/deploy-all removed: the GitHub Actions workflow ships both backend and
frontend from one push, so the chained-script wrapper is no longer needed."
```

---

## Task 14: Update `SERVER.md`

**Files:**
- Modify: `SERVER.md`

- [ ] **Step 1: Find the deploy section**

```bash
grep -n "deploy\|systemd\|setup.sh" SERVER.md
```

- [ ] **Step 2: Add/replace a "Deployment" section**

Add this section to `SERVER.md` (location: near the bottom, after "Architecture"):

```markdown
## Deployment

The live deploy path is `.github/workflows/deploy.yml`. Push to `master` and the workflow runs:

1. `cargo test --workspace` + `pnpm --dir web test`
2. `cargo build --release --target x86_64-unknown-linux-gnu` + `pnpm --dir web build`
3. Upload both artifacts (90-day retention) for rollback
4. `scp` binary to VPS, run `deploy/remote-swap.sh` (atomic symlink swap, restart, `/health` poll, auto-revert on failure)
5. `wrangler pages deploy web/dist` to Cloudflare Pages
6. Smoke check `https://app.wlim.dev/` and `https://spades.wlim.dev/health`

**Rollback** in order of preference:
- `git revert <bad-sha> && git push` — the action redeploys the prior state.
- `gh workflow run deploy.yml --ref <good-sha>` — re-runs the action against an older SHA; downloads its cached artifact if within 90 days.
- `bin/rollback <short-sha>` from the laptop — instant symlink flip on the VPS, works for any binary still on disk (last 5 retained).
- Frontend-only: `wrangler pages deployment list` + `... activate <id>`, or the CF dashboard.

**Break-glass deploy** (CI down): `bin/deploy` from the laptop builds the binary locally (`x86_64-unknown-linux-gnu`) and runs the same `remote-swap.sh` over SSH.

**Server-side runtime config** lives in `/etc/spades/env` (loaded by systemd, never touched by deploys). Template: `deploy/env.template`.

**One-time VPS setup:** `bash deploy/setup.sh` on the box (creates `deploy` user, systemd unit, sudoers entry, env file from template).
```

- [ ] **Step 3: Commit**

```bash
git add SERVER.md
git commit -m "docs: describe new deploy flow in SERVER.md"
```

---

## Task 15: First real deploy

**Files:** none — push the accumulated commits to master.

- [ ] **Step 1: Push master**

```bash
git push origin master
```

- [ ] **Step 2: Watch the workflow**

```bash
gh run watch
```

Expected: full ci + deploy run, both green.

- [ ] **Step 3: Verify in production**

```bash
curl -fsS https://app.wlim.dev/
curl -fsS https://spades.wlim.dev/health
ssh deploy@$DEPLOY_HOST 'readlink /opt/spades-server/bin/spades-server-current && ls /opt/spades-server/bin/'
```

Expected: smoke endpoints return 200. The symlink points to a `spades-server.<sha>` matching `git rev-parse --short=12 HEAD`. The `bin/` listing shows at most 5 binaries.

- [ ] **Step 4: Done — celebrate or rollback**

If anything looks wrong, revert the merge commit and push: `git revert -m 1 <subtree-merge-sha> && git push`. The workflow will deploy the revert.

---

## Self-review notes

**Spec coverage check (against `docs/superpowers/specs/2026-05-12-deployment-rework-design.md`):**
- Architecture diagram → Task 9 (workflow). ✓
- Monorepo merge into `web/` (no squash) → Task 2. ✓
- `git subtree add` mechanism → Task 2 Step 2. ✓
- Cloudflare Pages git integration disconnect → not needed (none exists; confirmed in design). ✓
- One workflow file with `push: master` + `workflow_dispatch` → Task 9. ✓ (PR trigger added for CI validation; spec was silent on this and it's a clear improvement — pull requests run ci only, never deploy.)
- All 10 workflow steps → Task 9. ✓
- All 5 GH secrets → Task 10. ✓
- `deploy/remote-swap.sh` with auto-revert → Task 4. ✓
- Failure-mode table → covered by Task 4 tests + Task 9 ordering. ✓
- `deploy/setup.sh` changes (drop Rust/git-clone/build, add env file, replace CORS drop-in) → Task 7. ✓
- `/etc/spades/env` template → Task 6. ✓
- Systemd `EnvironmentFile=` → Task 5. ✓
- `bin/deploy` break-glass rework → Task 12. ✓
- `bin/deploy-all` removed → Task 13. ✓
- `bin/rollback` unchanged, verified → Task 13. ✓
- 4 rollback paths → Task 14 (SERVER.md documentation). ✓
- VPS provisioning + manual `/etc/spades/env` fill-in → Task 8. ✓
- Open items (arch confirmation, pnpm version, wrangler flags) → Task 1. ✓
- spades-ts repo archive (recommendation only) → noted in spec; not a code task here. Operator can archive whenever convenient.

**No placeholder scan:** All code blocks are complete. All `<placeholder>` markers are user-supplied values (DEPLOY_HOST, secrets), which is appropriate.

**Type consistency:** `remote-swap.sh` uses `DEPLOY_PATH`, `SHORT_SHA`, `SYSTEMCTL`, `HEALTH_URL`, `KEEP` — same names used everywhere it's invoked (`bin/deploy` Step 1, workflow Step "Remote swap").

**Caveat for the implementer:** rust-spades's default branch is `master`, spades-ts's is `main`. The workflow uses `master`. The Cloudflare Pages "production branch" defaults to `main` in the wrangler invocation. These are intentional and not bugs.
