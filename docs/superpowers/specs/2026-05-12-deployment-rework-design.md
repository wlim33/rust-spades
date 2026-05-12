# Deployment rework — design

Date: 2026-05-12

## Problem

Today's deploy is laptop-driven across two repos:

- **rust-spades** ships the backend to a single VPS via `bin/deploy`, which SSHes in, `git reset --hard`s to the pushed SHA, **runs `cargo build --release` on the VPS**, swaps an atomic symlink, and `systemctl restart`s. Old binaries are pruned to the last 5.
- **spades-ts** ships the frontend to Cloudflare Pages via `bin/deploy` (wrangler).
- `bin/deploy-all` chains the two so a laptop can ship both with one command.

Pain points the rework targets, in priority order (per user):

1. **Two-repo coordination.** A "deploy" requires two pushes, two clean working trees, two laptops-have-the-checkout assumptions, and `bin/deploy-all` to glue them.
2. **Server compiles Rust on every deploy.** Slow on a small VPS, eats RAM/CPU during the swap window, requires a Rust toolchain on prod.
3. **No CI gate.** Nothing automated stops a deploy of code that doesn't build or whose tests fail.

Not in scope (explicitly punted):

- DB backups (`/var/lib/spades/games.sqlite`) — separate cron-job concern.
- Observability beyond `/health` and journald logs.
- Multi-environment (staging) — single prod environment.
- TLS termination — Cloudflare Proxy handles it.

## Goals and constraints

- **Local-first** in the sense of self-hosted infrastructure (single VPS + Cloudflare), not in the sense of laptop-triggered deploys.
- **Push to `main` = auto-deploy.** PR review on `main` is the gate; CI is the test.
- **Robust and simple as possible.** Fewest moving parts that still survives a bad deploy.
- **One GitHub Action.** A single `.github/workflows/deploy.yml`.
- **Cloudflare for domain + Pages for frontend; single VPS for backend.** No other infra introduced.

## Architecture

```
                 push to main
                     │
                     ▼
        ┌──────────────────────────┐
        │  GitHub Actions runner   │
        │  (deploy.yml, one job)   │
        │                          │
        │  1. cargo test           │
        │  2. pnpm test            │
        │  3. cargo build --release│
        │  4. pnpm build           │
        └────────┬────────┬────────┘
                 │        │
                 │        └─────────────┐
                 │ scp + ssh            │ wrangler pages deploy
                 ▼                      ▼
        ┌─────────────────┐    ┌──────────────────┐
        │   VPS           │    │ Cloudflare Pages │
        │   spades-server │    │ app.wlim.dev     │
        │   :3000         │    │                  │
        └─────────────────┘    └──────────────────┘
                 ▲                      │
                 │                      │
                 └── browser ◀──────────┘
                     (spades.wlim.dev)
```

Key properties:

- **One repo, one commit, one workflow, one shippable unit.** Backend and frontend deploy from the same SHA.
- **No code compiles on the VPS.** The Rust toolchain becomes optional (kept for emergency manual builds).
- **Backend before frontend.** The frontend never ships if the backend deploy failed health-check. Old-frontend-talking-to-new-backend is the safer direction (API additions are non-breaking; API removals require an intentional deprecation).
- **Workflow artifact retention** archives every built binary on GitHub for ~90 days. Within that window, rollback is a `workflow_dispatch` re-run against an older SHA. Beyond that, the VPS still keeps the last N binaries on disk for instant local symlink-flip.
- **`bin/deploy` from laptop stays as the break-glass path** — laptop builds for `x86_64-unknown-linux-gnu`, scp's binary, runs the swap script.

## Repository shape after merge

```
rust-spades/
├── Cargo.toml              # workspace root
├── crates/                 # backend crates
├── web/                    # ← spades-ts contents (package.json, src/, dist/, ...)
├── bin/
│   ├── deploy              # break-glass: local build → push to VPS
│   └── rollback            # unchanged: symlink-flip on VPS
├── deploy/
│   ├── setup.sh            # one-time VPS provisioning
│   ├── spades-server.service
│   └── remote-swap.sh      # NEW: invoked by GH Action over SSH
├── .github/workflows/
│   └── deploy.yml          # NEW: the single workflow
└── ...
```

### Monorepo merge

`git subtree add --prefix=web https://github.com/wlim33/spades-ts.git main` (no `--squash` — preserves full frontend history; user is sole contributor, blame is occasionally useful).

After merge:

1. spades-ts's `bin/deploy` is moved to `web/scripts/deploy-cf-pages.sh` for archival reference; it is no longer the live deploy path.
2. The GH Action builds and ships the frontend from `web/`.
3. spades-ts the repo is archived on GitHub with a README pointer to rust-spades. **Not deleted** — keeps history reachable.
4. No Cloudflare Pages git integration exists today, so no integration needs to be disconnected. The action becomes the sole deployer via `wrangler pages deploy`.

## The GitHub Action

One file: `.github/workflows/deploy.yml`. One job, sequential steps.

**Triggers:**

- `push: branches: [main]` — primary continuous-deploy path.
- `workflow_dispatch` — manual re-run, optionally targeting a specific SHA. Used for fast rollback.

**Steps, in order:**

1. **Checkout** at the pushed SHA.
2. **Setup toolchains** — Rust stable (with `Swatinem/rust-cache` or equivalent), Node + pnpm (with `pnpm/action-setup` and `actions/setup-node` cache).
3. **Test backend** — `cargo test --workspace --locked`. Hard fail.
4. **Test frontend** — `pnpm --dir web install --frozen-lockfile && pnpm --dir web test`. Hard fail.
5. **Build backend** — `cargo build --release -p spades-server --locked --target x86_64-unknown-linux-gnu`. (Target confirmed against VPS arch during plan execution; switch to `musl` only if glibc mismatch is observed.)
6. **Build frontend** — `pnpm --dir web build` (Vite emits `web/dist/`).
7. **Upload binary as workflow artifact** — `actions/upload-artifact@v4` with name `spades-server-<sha>`, retention 90 days.
8. **Deploy backend** — SSH-based push:
   - `scp` the binary to `${DEPLOY_PATH}/bin/spades-server.<short-sha>`.
   - `ssh` invokes `deploy/remote-swap.sh` with `DEPLOY_PATH` and `SHORT_SHA` in the environment.
   - Step fails if the remote script exits non-zero (which includes auto-revert on health failure).
9. **Deploy frontend** — `wrangler pages deploy web/dist/ --project-name=spades --branch=main`. Only runs if step 8 exited 0.
10. **Post-deploy smoke check** — `curl -fsS https://app.wlim.dev/` and `curl -fsS https://spades.wlim.dev/health`. Either failing fails the workflow.

**Secrets (GitHub repository secrets):**

| Name | Purpose |
|---|---|
| `DEPLOY_SSH_KEY` | Private key for `deploy@vps` |
| `DEPLOY_HOST` | VPS hostname or IP |
| `DEPLOY_KNOWN_HOSTS` | Pinned host key, written to `~/.ssh/known_hosts` on the runner before SSH |
| `CLOUDFLARE_API_TOKEN` | Scope: Pages:Edit |
| `CLOUDFLARE_ACCOUNT_ID` | Required by wrangler |

**Failure semantics:**

| Failure point | What changes | What rolls back |
|---|---|---|
| Tests fail | Nothing deploys | N/A |
| Backend `scp` or pre-swap fails | Symlink unchanged | N/A — old binary still serving |
| Backend health-check fails after swap | `remote-swap.sh` flips symlink back, restarts | Automatic |
| Frontend deploy fails | Backend on new version, frontend on old | Manual: `wrangler` rollback or revert commit |
| Smoke check fails | Both deployed, one is failing | Investigate; revert commit if needed |

## Server-side script

`deploy/remote-swap.sh`, invoked by the action over SSH. Replaces the build-on-server portion of today's `bin/deploy`.

```bash
#!/usr/bin/env bash
# Invoked as:
#   ssh deploy@vps DEPLOY_PATH=... SHORT_SHA=... bash -s < deploy/remote-swap.sh
# Assumes the new binary has already been scp'd to
#   ${DEPLOY_PATH}/bin/spades-server.${SHORT_SHA}
set -euo pipefail

cd "$DEPLOY_PATH"
NEW="bin/spades-server.${SHORT_SHA}"
[ -x "$NEW" ] || { echo "missing $NEW"; exit 1; }

PREV="$(readlink bin/spades-server-current || true)"
chmod 0755 "$NEW"

ln -sfn "spades-server.${SHORT_SHA}" bin/spades-server-current.new
mv -Tf bin/spades-server-current.new bin/spades-server-current

sudo /bin/systemctl restart spades-server
sleep 1

for i in 1 2 3 4 5; do
    curl -fsS --max-time 5 http://127.0.0.1:3000/health >/dev/null && break
    if [ "$i" = 5 ]; then
        echo "health failed, reverting to $PREV" >&2
        ln -sfn "$PREV" bin/spades-server-current.new
        mv -Tf bin/spades-server-current.new bin/spades-server-current
        sudo /bin/systemctl restart spades-server
        exit 1
    fi
    sleep 1
done

# Prune: keep the last 5 binaries, including the live one.
LIVE="$(readlink bin/spades-server-current)"
(
    set +o pipefail
    ls -1t bin/spades-server.* 2>/dev/null \
        | grep -Fxv "bin/$LIVE" \
        | tail -n +5 \
        | xargs -r rm -f --
) || true
```

Key differences from today's `bin/deploy` remote block:

- **No `git fetch`, no `cargo build`.** The VPS holds binaries only; source of truth for code is GitHub, source of truth for what runs is the symlink.
- **Auto-revert on health-check failure.** Today's script fails the deploy but leaves the broken symlink in place; the new one flips back automatically. This is the primary "robust" win.

## `deploy/setup.sh` changes

Today's `setup.sh` provisions a fresh Ubuntu/Debian VPS end-to-end. After the rework:

- **Remove:** Rust toolchain install, initial `cargo build`, repo `git clone` into `/opt/spades-server`. The VPS no longer needs the source tree.
- **Keep:** deploy user creation, systemd unit install, data directory creation, sudoers entry for `systemctl restart`.
- **Replace:** the CORS systemd drop-in (`/etc/systemd/system/spades-server.service.d/cors.conf`) with a single `EnvironmentFile=/etc/spades/env` on the systemd unit. All runtime env vars live there.
- **Add:** `/etc/spades/env` (mode 0640, owned `root:deploy`) — created by the operator once, never touched by deploy scripts.

**`/etc/spades/env` contents (template):**

```
SMTP_HOST=...
SMTP_USER=...
SMTP_PASS=...
SMTP_FROM=...
SMTP_PORT=587
SMTP_STARTTLS=true
GOOGLE_OAUTH_CLIENT_ID=...
GOOGLE_OAUTH_CLIENT_SECRET=...
GITHUB_OAUTH_CLIENT_ID=...
GITHUB_OAUTH_CLIENT_SECRET=...
OAUTH_REDIRECT_BASE_URL=https://spades.wlim.dev
CORS_ALLOW_ORIGIN=https://app.wlim.dev
```

## `bin/deploy` (laptop) — break-glass path

Reworked to mirror the CI flow rather than build on the server:

1. Refuse if working tree is dirty (unchanged).
2. `cargo build --release --target x86_64-unknown-linux-gnu -p spades-server` locally.
3. `scp` the binary to the VPS.
4. SSH-invoke `deploy/remote-swap.sh`.

The script no longer requires the local SHA to be on `origin/main`, since this is the emergency path. It does still refuse a dirty tree to keep deploys traceable.

`bin/deploy-all` is removed — there is no "deploy frontend from laptop" path in the new model. If frontend needs an emergency push, run `wrangler pages deploy web/dist/` directly.

`bin/rollback` is unchanged — still flips the symlink on the VPS to any binary still present in `bin/`.

## Rollback paths, in order of likely use

1. **Push a revert commit to `main`.** `git revert <bad-sha> && git push`. The action runs and ships the revert. Default path.
2. **Re-run the action on a known-good SHA.** `gh workflow run deploy.yml -r <good-sha>`. Within the 90-day artifact window, the binary for that SHA is downloaded from the prior run; outside it, rebuilt from source.
3. **Instant local symlink flip** via `bin/rollback <short-sha>` from the laptop. Works for any binary still on the box (last 5). Survives a CI outage.
4. **Frontend-only rollback.** `wrangler pages deployment list` + `wrangler pages deployment activate <id>`, or the CF dashboard. Backend stays on its current version.

## Failure modes considered

| Scenario | Behavior |
|---|---|
| GH Actions is down | Use `bin/deploy` from laptop (break-glass). |
| VPS is down | All deploys fail; no traffic served. Frontend may still serve stale UI from CF Pages but it has no backend to talk to. |
| Health-check fails after swap | Auto-revert in `remote-swap.sh`. |
| Frontend deploy fails after backend succeeded | Backend on new, frontend on old. Old frontend → new backend is the safer direction. Surface as workflow failure; fix forward or revert commit. |
| SSH key compromised | Rotate key in `/home/deploy/.ssh/authorized_keys` and in GH secret. Blast radius bounded by the `deploy` user's sudoers entry (`systemctl restart spades-server` only). |
| Wrong VPS arch / glibc | Caught at first deploy (`spades-server` binary fails to exec). Switch action target to `x86_64-unknown-linux-musl` and redeploy. |

## Open items deferred to implementation plan

- Confirm VPS architecture (`uname -m`, `ldd --version`) to choose `gnu` vs `musl` target.
- Confirm pnpm version used in spades-ts and lock it in the workflow.
- Confirm `wrangler` invocation flags by reading current `spades-ts/bin/deploy`.
- Decide whether the spades-ts repo gets archived or deleted (recommend archive).
- Verify `actions/upload-artifact@v4` retention setting matches account defaults (90 days assumed).

## What this design does NOT include

- DB backups (`/var/lib/spades/games.sqlite`) — punted; suggest a cron-job follow-on.
- Observability beyond `/health` and journald.
- Multi-environment (staging).
- Build caching beyond the defaults provided by the cache actions.
- Migrations framework — server creates `games` table on startup if missing; no schema migrations needed today.
