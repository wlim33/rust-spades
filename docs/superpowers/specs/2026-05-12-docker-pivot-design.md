# Docker pivot — design

Date: 2026-05-12

## Why this exists

The bash-based deploy from `2026-05-12-deployment-rework-design.md` shipped but the first real deploy got bitten by manual scp file-ordering: `setup.sh`'s content ended up at `/etc/systemd/system/spades-server.service`, breaking the unit. The bash flow is correct in principle but has too many small surfaces (systemd unit, env file, sudoers, scp staging, symlink swap, prune, auto-revert) — every step is a hand-managed thing on the VPS.

Container runtimes solved this years ago. This pivot replaces the bash plumbing with a Dockerfile + docker compose on the VPS. The CI workflow shrinks; the VPS only needs Docker installed.

The bash work is not wasted — `remote-swap.sh` taught us what auto-revert needs to look like, and the env-file approach carries over to Docker — but the orchestration moves to Docker.

## Goals

- Push to `master` builds an image, pushes it to ghcr.io, and the VPS pulls + restarts in one step.
- The VPS holds: Docker, a `docker-compose.yml`, a `.env` file with secrets, and SQLite data. Nothing else.
- Rollback is `docker compose pull <old-sha> && docker compose up -d`. No symlinks.
- Frontend deploy path is unchanged: `wrangler pages deploy` from the same workflow.
- Single GitHub Actions workflow.
- "Local-first" still applies: the VPS is yours, the registry is your account's ghcr.io, the laptop is the place you push from.

## Non-goals

- Zero-downtime deploys. A 3–5 s restart per deploy is fine.
- Kubernetes, Docker Swarm, multi-host orchestration. Single VPS.
- Multi-environment (staging). Single prod.
- DB backups (still punted; cron-job follow-on).

## Architecture

```
                push to master
                     │
                     ▼
        ┌──────────────────────────┐
        │   GitHub Actions runner  │
        │   (deploy.yml, 2 jobs)   │
        │                          │
        │   ci:  cargo test        │
        │        pnpm test+build   │
        │   ship: docker build+push│
        │        ssh: compose pull │
        │        wrangler deploy   │
        │        smoke check       │
        └────┬────────┬────────────┘
             │        │
             │        └────────────┐
             │ docker push         │ wrangler pages
             ▼                     ▼
        ┌──────────┐         ┌──────────────────┐
        │ ghcr.io  │         │ Cloudflare Pages │
        │ /wlim33  │         │ app.wlim.dev     │
        │ /spades  │         └──────────────────┘
        │ :<sha>   │
        │ :latest  │
        └────┬─────┘
             │
             │ ssh + docker compose pull
             ▼
        ┌─────────────────────────────────────┐
        │  VPS                                │
        │  ┌──────────────────────────────┐   │
        │  │ docker compose (1 service)   │   │
        │  │   image: .../spades:latest   │   │
        │  │   env_file: /opt/spades/.env │   │
        │  │   volumes:                   │   │
        │  │     /var/lib/spades:/data    │   │
        │  │   ports: 3000:3000           │   │
        │  │   healthcheck: /health       │   │
        │  │   restart: unless-stopped    │   │
        │  └──────────────────────────────┘   │
        └─────────────────────────────────────┘
                       ▲
                       │
                       └── browser ── spades.wlim.dev
```

## File layout after this change

```
rust-spades/
├── Dockerfile                        # NEW
├── .dockerignore                     # NEW
├── docker-compose.yml                # NEW (template; copied to VPS at /opt/spades/)
├── deploy/
│   ├── install-docker.sh             # NEW (replaces setup.sh)
│   └── env.template                  # KEPT (now also the compose .env template)
├── .github/workflows/
│   └── deploy.yml                    # REWRITTEN
├── SERVER.md                         # Deployment section rewritten
└── ... (everything else unchanged)
```

**Files deleted by the pivot:**

- `deploy/setup.sh` — replaced by `install-docker.sh`
- `deploy/spades-server.service` — systemd no longer manages the service
- `deploy/remote-swap.sh` — Docker manages the swap
- `deploy/tests/test-remote-swap.sh` and the `deploy/tests/` directory
- `bin/deploy` — break-glass is now `docker buildx build --push` + `ssh ... compose up -d`
- `bin/rollback` — replaced by `docker compose pull spades:<sha> && docker compose up -d`

## Dockerfile

Multi-stage, ~40 lines.

```dockerfile
# syntax=docker/dockerfile:1.7

# ---- builder ---------------------------------------------------------------
FROM rust:1.85-bookworm AS builder
WORKDIR /src

# Cache cargo deps separately from source.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Build only the server crate (lib crates compile transitively).
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p spades-server \
    && cp target/release/spades-server /spades-server

# ---- runtime ---------------------------------------------------------------
FROM debian:12-slim AS runtime

# Install only what the binary needs at runtime.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

# Non-root user (uid 1000 to match common bind-mount expectations).
RUN useradd --system --uid 1000 --user-group --no-create-home spades

COPY --from=builder /spades-server /usr/local/bin/spades-server

USER spades
WORKDIR /data
EXPOSE 3000

# /data is bind-mounted from the host's /var/lib/spades for SQLite persistence.
ENV DATABASE_URL=/data/games.sqlite

ENTRYPOINT ["/usr/local/bin/spades-server"]
CMD ["--port", "3000", "--db", "/data/games.sqlite"]
```

`.dockerignore` (next to the Dockerfile):

```
target/
**/node_modules/
**/dist/
**/.git/
**/.github/
**/docs/
**/web/
**/deploy/
**/.deploy.env
**/*.deploy.env
**/test-results/
**/.DS_Store
```

The `.dockerignore` keeps the build context small — only `Cargo.toml`, `Cargo.lock`, and `crates/` (the Rust workspace) need to be sent to the daemon.

## docker-compose.yml

Lives in the repo at the root. The CI workflow copies it to `/opt/spades/docker-compose.yml` on the VPS as part of the first deploy (or it's placed there once by `install-docker.sh`).

```yaml
services:
  spades-server:
    image: ghcr.io/wlim33/spades:${IMAGE_TAG:-latest}
    container_name: spades-server
    restart: unless-stopped
    ports:
      - "3000:3000"
    volumes:
      - /var/lib/spades:/data
    env_file:
      - /opt/spades/.env
    healthcheck:
      test: ["CMD", "curl", "-fsS", "http://127.0.0.1:3000/health"]
      interval: 10s
      timeout: 5s
      retries: 6
      start_period: 10s
```

Key choices:

- **`image: ...:${IMAGE_TAG:-latest}`** — defaults to `:latest` for normal deploys; rollback overrides via `IMAGE_TAG=<old-sha> docker compose up -d`.
- **`container_name: spades-server`** — stable name; useful for `docker logs`, `docker exec`.
- **`restart: unless-stopped`** — auto-restart on crash, but a manual `docker compose stop` keeps it stopped.
- **`ports: "3000:3000"`** — Cloudflare Proxy still fronts this. Could bind to `127.0.0.1:3000:3000` to require CF for access, but matches today's posture (port open).
- **`env_file: /opt/spades/.env`** — absolute path so the file lives outside the compose file's directory. Mode 0640, root:root or similar on the VPS.
- **`healthcheck`** — six tries × 10 s = ~60 s grace period. Restart policy reacts to repeated unhealthy state.

## deploy/install-docker.sh

One-time bootstrap on a fresh VPS. Idempotent.

```bash
#!/usr/bin/env bash
# Idempotent VPS bootstrap. Installs Docker + compose plugin, creates the
# /opt/spades directory with compose.yml + .env template + data dir.
# Assumes Debian/Ubuntu.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEPLOY_USER="${DEPLOY_USER:-deploy}"
SPADES_DIR="${SPADES_DIR:-/opt/spades}"
DATA_DIR="${DATA_DIR:-/var/lib/spades}"

echo "==> Installing Docker"
if ! command -v docker >/dev/null 2>&1; then
    sudo apt-get update
    sudo apt-get install -y ca-certificates curl gnupg
    sudo install -m 0755 -d /etc/apt/keyrings
    curl -fsSL https://download.docker.com/linux/debian/gpg \
        | sudo gpg --dearmor -o /etc/apt/keyrings/docker.gpg
    sudo chmod a+r /etc/apt/keyrings/docker.gpg
    echo \
      "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] \
       https://download.docker.com/linux/debian $(. /etc/os-release && echo \"$VERSION_CODENAME\") stable" \
      | sudo tee /etc/apt/sources.list.d/docker.list >/dev/null
    sudo apt-get update
    sudo apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
fi

echo "==> Creating deploy user (if missing)"
if ! id -u "$DEPLOY_USER" >/dev/null 2>&1; then
    sudo adduser --system --group --shell /bin/bash --home "/home/$DEPLOY_USER" "$DEPLOY_USER"
    sudo mkdir -p "/home/$DEPLOY_USER/.ssh"
    sudo chown "$DEPLOY_USER:$DEPLOY_USER" "/home/$DEPLOY_USER/.ssh"
    sudo chmod 700 "/home/$DEPLOY_USER/.ssh"
fi

echo "==> Adding $DEPLOY_USER to docker group"
sudo usermod -aG docker "$DEPLOY_USER"

echo "==> Creating $SPADES_DIR and $DATA_DIR"
sudo mkdir -p "$SPADES_DIR" "$DATA_DIR"
sudo chown "$DEPLOY_USER:$DEPLOY_USER" "$SPADES_DIR"
# SQLite must be writable by the container's uid 1000.
sudo chown 1000:1000 "$DATA_DIR"

echo "==> Installing docker-compose.yml"
sudo install -m 0644 -o "$DEPLOY_USER" -g "$DEPLOY_USER" \
    "$SCRIPT_DIR/../docker-compose.yml" "$SPADES_DIR/docker-compose.yml"

echo "==> Creating .env (if missing)"
if [ ! -f "$SPADES_DIR/.env" ]; then
    sudo install -m 0640 -o "$DEPLOY_USER" -g "$DEPLOY_USER" \
        "$SCRIPT_DIR/env.template" "$SPADES_DIR/.env"
    echo "    -- wrote template to $SPADES_DIR/.env; edit it with real secrets before the first deploy."
else
    echo "    -- $SPADES_DIR/.env already exists; leaving it alone."
fi

# Clean up legacy systemd unit if present.
sudo systemctl disable --now spades-server 2>/dev/null || true
sudo rm -f /etc/systemd/system/spades-server.service
sudo rm -rf /etc/systemd/system/spades-server.service.d
sudo systemctl daemon-reload || true
sudo rm -f /etc/sudoers.d/spades-deploy

cat <<EOF

==> Done.

Next steps:
  1. Edit $SPADES_DIR/.env with real secrets:
       sudo -u $DEPLOY_USER -e $SPADES_DIR/.env
  2. Add the GitHub Actions deploy public key to:
       /home/$DEPLOY_USER/.ssh/authorized_keys
  3. The first push to master triggers the workflow. Once the image
     is in ghcr.io, the workflow ssh's in and runs:
       cd $SPADES_DIR && docker compose pull && docker compose up -d

EOF
```

Note: the script also removes the legacy systemd unit and sudoers entry from the bash-flow attempt, so the pivot leaves the VPS in a clean state.

## GitHub Actions workflow

Two jobs:

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

jobs:
  ci:
    name: test + build frontend
    runs-on: ubuntu-latest
    timeout-minutes: 30
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo test --workspace --locked

      - uses: pnpm/action-setup@v4
        with:
          run_install: false
      - uses: actions/setup-node@v4
        with:
          node-version: '20'
          cache: pnpm
          cache-dependency-path: web/pnpm-lock.yaml
      - run: pnpm install --frozen-lockfile
        working-directory: web
      - run: pnpm test
        working-directory: web
      - run: pnpm build
        working-directory: web

      - uses: actions/upload-artifact@v4
        with:
          name: spades-web-${{ github.sha }}
          path: web/dist
          retention-days: 90
          if-no-files-found: error

  ship:
    name: ship
    needs: ci
    if: github.event_name == 'push' && github.ref == 'refs/heads/master'
    runs-on: ubuntu-latest
    timeout-minutes: 20
    permissions:
      contents: read
      packages: write
    steps:
      - uses: actions/checkout@v4

      - name: Set short SHA
        id: sha
        run: echo "short=$(git rev-parse --short=12 HEAD)" >> "$GITHUB_OUTPUT"

      - uses: docker/setup-buildx-action@v3

      - name: Log in to ghcr.io
        uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - name: Build and push backend image
        uses: docker/build-push-action@v6
        with:
          context: .
          push: true
          tags: |
            ghcr.io/wlim33/spades:${{ steps.sha.outputs.short }}
            ghcr.io/wlim33/spades:latest
          cache-from: type=gha
          cache-to: type=gha,mode=max

      - name: Configure SSH
        uses: webfactory/ssh-agent@v0.9.0
        with:
          ssh-private-key: ${{ secrets.DEPLOY_SSH_KEY }}

      - name: Pin host key
        run: |
          mkdir -p ~/.ssh
          echo "${{ secrets.DEPLOY_KNOWN_HOSTS }}" >> ~/.ssh/known_hosts
          chmod 644 ~/.ssh/known_hosts

      - name: Copy compose file to VPS
        run: scp docker-compose.yml deploy@${{ secrets.DEPLOY_HOST }}:/opt/spades/docker-compose.yml

      - name: Pull + restart on VPS
        run: |
          ssh deploy@${{ secrets.DEPLOY_HOST }} bash <<EOF
            set -euo pipefail
            cd /opt/spades
            echo "${{ secrets.GITHUB_TOKEN }}" | docker login ghcr.io -u ${{ github.actor }} --password-stdin
            IMAGE_TAG=${{ steps.sha.outputs.short }} docker compose pull
            IMAGE_TAG=${{ steps.sha.outputs.short }} docker compose up -d --remove-orphans
            # Wait up to 60s for health to flip to healthy.
            for i in \$(seq 1 12); do
              status=\$(docker inspect --format='{{.State.Health.Status}}' spades-server 2>/dev/null || echo unknown)
              if [ "\$status" = "healthy" ]; then exit 0; fi
              sleep 5
            done
            echo "Container did not reach healthy state" >&2
            docker compose logs --tail=100 spades-server >&2
            exit 1
          EOF

      - name: Download frontend bundle
        uses: actions/download-artifact@v4
        with:
          name: spades-web-${{ github.sha }}
          path: web/dist

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

Notable bits:

- **No backend artifact upload** — the image registry IS the artifact store.
- **`GITHUB_TOKEN` for ghcr.io push** — `permissions: packages: write` on the job; no PAT needed.
- **Pin-by-SHA on deploy, also publish `:latest`** — `:latest` lets an operator run `docker compose pull && docker compose up -d` on the VPS for an interactive deploy without knowing a SHA. `:<sha>` is what the workflow uses, ensuring rollbacks are deterministic.
- **Wait-for-healthy loop** — the workflow polls Docker's healthcheck instead of curling `/health` directly. This avoids race conditions during the brief restart window.
- **CF Pages deploy comes AFTER the backend is healthy** — same ordering rationale as before.

## Secrets

| Where | Name | Purpose |
|---|---|---|
| GitHub Actions | `DEPLOY_SSH_KEY` | private key, `deploy@vps` |
| GitHub Actions | `DEPLOY_HOST` | VPS hostname/IP |
| GitHub Actions | `DEPLOY_KNOWN_HOSTS` | pinned host key |
| GitHub Actions | `CLOUDFLARE_API_TOKEN` | Pages:Edit |
| GitHub Actions | `CLOUDFLARE_ACCOUNT_ID` | wrangler needs it |
| VPS `/opt/spades/.env` | all backend runtime envs | SMTP, OAuth, CORS, etc. — same content as `deploy/env.template` |

`GITHUB_TOKEN` is the built-in token; no separate secret for ghcr.io push. The token's `packages: write` scope is granted by the workflow's `permissions:` block.

## Rollback

In order of likely use:

1. **`git revert <bad-sha> && git push`** — workflow runs, builds, deploys. Default path.
2. **`workflow_dispatch` against an older SHA** — `gh workflow run deploy.yml --ref <good-sha>` rebuilds (or uses GHA layer cache) and ships.
3. **Manual `compose` flip on the VPS** — for the fastest backend-only rollback:
   ```bash
   ssh deploy@$VPS 'cd /opt/spades && IMAGE_TAG=<old-short-sha> docker compose up -d --pull always'
   ```
   No new image is built; Docker pulls the previously-pushed `:<sha>` tag from ghcr.io. Health-check + auto-restart kick in as normal.
4. **Frontend-only**: `wrangler pages deployment list` + activate, or CF dashboard.

There's no "last 5 binaries on disk" idea anymore — ghcr.io retains every image SHA we've ever pushed (per its retention policy, which is "forever" by default for non-anonymous uploads).

## Migration from the current broken state

The VPS currently has:
- A broken `/etc/systemd/system/spades-server.service` (bash content)
- Possibly an in-progress `/etc/spades/env`
- The OLD `/opt/spades-server/` directory with the original cargo-built binary

The migration is:

1. **From your laptop:** push the new `Dockerfile`, `docker-compose.yml`, `.dockerignore`, `deploy/install-docker.sh`, `deploy/env.template`, and the rewritten workflow.
2. **Manually copy the new files to the VPS** (`scp deploy/install-docker.sh deploy/env.template docker-compose.yml deploy@$VPS:/tmp/`) and run `bash /tmp/install-docker.sh`. The script removes the broken systemd unit, installs Docker, and sets up `/opt/spades`.
3. **Fill in `/opt/spades/.env`** (`sudo -u deploy -e /opt/spades/.env`).
4. **Push to master.** First real Docker deploy runs.

The old `/opt/spades-server/` directory becomes dead weight — you can `rm -rf` it once you've confirmed the new flow works. SQLite at `/var/lib/spades/games.sqlite` is preserved (just gets bind-mounted into the container).

## Failure modes

| Scenario | Behavior |
|---|---|
| Tests fail in `ci` | `ship` job never runs |
| Docker build fails | Push doesn't happen; nothing changes on the VPS |
| `docker push` fails (rate limit, auth) | Workflow fails; no deploy |
| `compose pull` fails (image not found) | Old container keeps running; workflow fails loudly |
| Healthcheck never goes healthy | Wait-loop exits non-zero, workflow fails. Container itself stays running (probably restart-looping). Operator inspects with `docker logs` and decides: revert, or fix forward |
| CF Pages deploy fails after backend healthy | Backend on new SHA, frontend on old. Workflow fails; same recovery as before |
| ghcr.io is down | Push fails. Pulled images keep working. Operator can deploy from a stale `:latest` if needed |
| VPS reboots | `restart: unless-stopped` brings the container back automatically |

## Things this design explicitly does NOT do

- DB backups — still cron-job follow-on.
- Multi-environment / staging — single prod.
- Zero-downtime rolling deploys — accept 3–5 s gap.
- Container resource limits (mem, cpu) — add later if needed.
- Log shipping beyond `docker logs` / journald — out of scope.
- Image signing / attestation / SBOM — out of scope.
- Reverse proxy in front of port 3000 (nginx/caddy/traefik) — Cloudflare Proxy already does TLS + edge; out of scope.

## Open items for the implementation plan

- Confirm `rust:1.85-bookworm` is actually pulled (the docker hub registry must have it). If not, fall back to `rust:1.84-bookworm` and pin `Cargo.toml`'s `rust-version` accordingly.
- Verify `ghcr.io/wlim33/spades` is an allowed image path (might require the user to enable package permissions on the GitHub user account once).
- Confirm Docker on the VPS distro/version supports BuildKit cache mounts (`--mount=type=cache`). Anything from docker-ce 23+ is fine.
- Decide whether to expose port 3000 publicly or bind to 127.0.0.1 only (depends on whether Cloudflare Proxy is in "orange-cloud" mode — currently yes per the spec, so port can be public).
