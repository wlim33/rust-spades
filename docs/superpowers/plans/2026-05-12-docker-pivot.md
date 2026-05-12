# Docker Pivot Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** Replace the bash + systemd deploy flow with a Docker image built in CI, pushed to ghcr.io, and pulled by docker compose on the VPS.

**Architecture:** One workflow file builds the image and pushes to `ghcr.io/wlim33/spades:<sha>` + `:latest`, then SSHes into the VPS to `docker compose pull && docker compose up -d`. Frontend still goes to Cloudflare Pages from the same workflow. The VPS holds Docker, a compose file at `/opt/spades/docker-compose.yml`, an `.env` file at `/opt/spades/.env`, and SQLite at `/var/lib/spades/games.sqlite` (bind-mounted into the container at `/data`).

**Tech Stack:** Rust 1.85 (Edition 2024), `rust:1.85-bookworm` builder image, `debian:12-slim` runtime, docker compose plugin, ghcr.io, GitHub Actions (`docker/build-push-action@v6`, `docker/setup-buildx-action@v3`, `docker/login-action@v3`, `webfactory/ssh-agent@v0.9.0`, `cloudflare/wrangler-action@v3`).

**Source spec:** `docs/superpowers/specs/2026-05-12-docker-pivot-design.md`

---

## File Structure

**New files:**
- `Dockerfile` (repo root)
- `.dockerignore` (repo root)
- `docker-compose.yml` (repo root; copied to `/opt/spades/` on the VPS)
- `deploy/install-docker.sh`

**Modified files:**
- `.github/workflows/deploy.yml` — rewritten
- `SERVER.md` — Deployment section rewritten
- `deploy/env.template` — unchanged (still describes runtime envs; serves as the `/opt/spades/.env` template)

**Removed files:**
- `deploy/setup.sh`
- `deploy/spades-server.service`
- `deploy/remote-swap.sh`
- `deploy/tests/test-remote-swap.sh` (and `deploy/tests/` if it becomes empty)
- `bin/deploy`
- `bin/rollback`

---

## Task 1: Add `.dockerignore` + `Dockerfile`

**Files:**
- Create: `/Users/wlim/Projects/rust-spades/.dockerignore`
- Create: `/Users/wlim/Projects/rust-spades/Dockerfile`

- [ ] **Step 1: Write `.dockerignore`** (Write tool):

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

- [ ] **Step 2: Write `Dockerfile`** (Write tool):

```dockerfile
# syntax=docker/dockerfile:1.7

# ---- builder ---------------------------------------------------------------
FROM rust:1.85-bookworm AS builder
WORKDIR /src

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p spades-server \
    && cp target/release/spades-server /spades-server

# ---- runtime ---------------------------------------------------------------
FROM debian:12-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --system --uid 1000 --user-group --no-create-home spades

COPY --from=builder /spades-server /usr/local/bin/spades-server

USER spades
WORKDIR /data
EXPOSE 3000

ENV DATABASE_URL=/data/games.sqlite

ENTRYPOINT ["/usr/local/bin/spades-server"]
CMD ["--port", "3000", "--db", "/data/games.sqlite"]
```

- [ ] **Step 3: Static validation only (no docker build).**

```bash
# Just parse the Dockerfile, don't build it.
which docker >/dev/null 2>&1 && docker build --check . 2>&1 || echo "(docker not available locally — workflow will validate)"
```

Building the image locally would take many minutes and require a working Docker daemon — neither necessary at this step. The workflow validates by actually building.

- [ ] **Step 4: Commit:**

```bash
cd /Users/wlim/Projects/rust-spades
git add .dockerignore Dockerfile
git commit -m "docker: multi-stage Dockerfile (rust:1.85 → debian:12-slim)"
```

---

## Task 2: Add `docker-compose.yml`

**Files:**
- Create: `/Users/wlim/Projects/rust-spades/docker-compose.yml`

- [ ] **Step 1: Write the compose file** (Write tool):

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

- [ ] **Step 2: Validate YAML:**

```bash
python3 -c "import yaml; yaml.safe_load(open('docker-compose.yml'))" && echo "yaml: valid"
```

Expected: `yaml: valid`.

- [ ] **Step 3: Commit:**

```bash
git add docker-compose.yml
git commit -m "docker: compose file with healthcheck + bind-mounted SQLite"
```

---

## Task 3: Add `deploy/install-docker.sh`

**Files:**
- Create: `/Users/wlim/Projects/rust-spades/deploy/install-docker.sh`

- [ ] **Step 1: Write the bootstrap script** (Write tool):

```bash
#!/usr/bin/env bash
# Idempotent VPS bootstrap. Installs Docker + compose plugin, creates the
# /opt/spades directory with compose.yml + .env template + data dir.
# Removes the legacy systemd unit + sudoers entry from the previous bash flow.
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
    codename="$(. /etc/os-release && echo "$VERSION_CODENAME")"
    arch="$(dpkg --print-architecture)"
    echo "deb [arch=$arch signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/debian $codename stable" \
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

echo "==> Cleaning up legacy bash-flow artifacts"
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
  3. The first push to master triggers the workflow. The workflow ssh's
     in and runs (from /opt/spades):
       docker compose pull && docker compose up -d

EOF
```

- [ ] **Step 2: Make executable + shellcheck:**

```bash
chmod +x /Users/wlim/Projects/rust-spades/deploy/install-docker.sh
shellcheck /Users/wlim/Projects/rust-spades/deploy/install-docker.sh
bash -n /Users/wlim/Projects/rust-spades/deploy/install-docker.sh
```

Expected: shellcheck clean, syntax clean.

- [ ] **Step 3: Commit:**

```bash
git add deploy/install-docker.sh
git commit -m "deploy: install-docker.sh — VPS bootstrap (replaces setup.sh)"
```

---

## Task 4: Rewrite `.github/workflows/deploy.yml`

**Files:**
- Modify: `/Users/wlim/Projects/rust-spades/.github/workflows/deploy.yml`

- [ ] **Step 1: Overwrite the file** with this content (Write tool):

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

      - name: cargo test
        run: cargo test --workspace --locked

      - uses: pnpm/action-setup@v4
        with:
          run_install: false

      - uses: actions/setup-node@v4
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

      - name: Upload frontend bundle
        uses: actions/upload-artifact@v4
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

      - uses: webfactory/ssh-agent@v0.9.0
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
        env:
          SHORT_SHA: ${{ steps.sha.outputs.short }}
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          GH_ACTOR: ${{ github.actor }}
          VPS: ${{ secrets.DEPLOY_HOST }}
        run: |
          ssh "deploy@${VPS}" bash <<EOF
            set -euo pipefail
            cd /opt/spades
            echo "${GH_TOKEN}" | docker login ghcr.io -u "${GH_ACTOR}" --password-stdin
            IMAGE_TAG=${SHORT_SHA} docker compose pull
            IMAGE_TAG=${SHORT_SHA} docker compose up -d --remove-orphans
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

- [ ] **Step 2: Validate YAML:**

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/deploy.yml'))" && echo "yaml: valid"
```

Expected: `yaml: valid`.

- [ ] **Step 3: Commit:**

```bash
git add .github/workflows/deploy.yml
git commit -m "ci: workflow builds + pushes Docker image, VPS does compose pull"
```

---

## Task 5: Delete bash-flow artifacts

**Files:**
- Delete: `deploy/setup.sh`
- Delete: `deploy/spades-server.service`
- Delete: `deploy/remote-swap.sh`
- Delete: `deploy/tests/test-remote-swap.sh`
- Delete: `deploy/tests/` (the directory, once empty)
- Delete: `bin/deploy`
- Delete: `bin/rollback`

- [ ] **Step 1: Remove tracked files:**

```bash
cd /Users/wlim/Projects/rust-spades
git rm deploy/setup.sh deploy/spades-server.service deploy/remote-swap.sh deploy/tests/test-remote-swap.sh bin/deploy bin/rollback
```

- [ ] **Step 2: Remove the now-empty `deploy/tests/` directory:**

```bash
rmdir deploy/tests 2>/dev/null || true
ls deploy/
```

The `ls` should show only `install-docker.sh` and `env.template`.

- [ ] **Step 3: Check `bin/` is now empty (or close to it):**

```bash
ls bin/ 2>/dev/null
```

If `bin/` is empty after the deletions, also remove the directory:

```bash
rmdir bin 2>/dev/null || true
```

- [ ] **Step 4: Commit:**

```bash
git status --short
git commit -m "deploy: drop bash + systemd artifacts (replaced by Docker flow)"
```

---

## Task 6: Update `SERVER.md`

**Files:**
- Modify: `/Users/wlim/Projects/rust-spades/SERVER.md`

- [ ] **Step 1: Locate the existing "Deployment" section** (added in the previous rework). Use Edit tool to replace the entire section with:

```markdown
## Deployment

The live deploy path is `.github/workflows/deploy.yml`. Push to `master` and the workflow:

1. `cargo test --workspace` + `pnpm --dir web test`
2. `pnpm --dir web build` (uploads `web/dist` as a workflow artifact)
3. `docker buildx build` → push to `ghcr.io/wlim33/spades:<sha>` and `:latest`
4. SSH to the VPS: `cd /opt/spades && docker compose pull && docker compose up -d`, then wait for the container's healthcheck to flip to `healthy`
5. `wrangler pages deploy web/dist` to Cloudflare Pages
6. Smoke check `https://app.wlim.dev/` and `https://spades.wlim.dev/health`

**On the VPS**, the entire backend is:

```
/opt/spades/docker-compose.yml   # the service definition
/opt/spades/.env                 # runtime envs (SMTP, OAuth, CORS); never deployed
/var/lib/spades/games.sqlite     # bind-mounted into the container at /data
```

Nothing compiles on the VPS; nothing is symlinked. `docker compose` handles restarts and healthchecks.

**Rollback** in order of preference:
- `git revert <bad-sha> && git push` — workflow redeploys the prior state.
- `gh workflow run deploy.yml --ref <good-sha>` — re-runs with cached image layers.
- `ssh deploy@$VPS 'cd /opt/spades && IMAGE_TAG=<short-sha> docker compose up -d --pull always'` — instant pin to any previously-pushed image SHA.
- Frontend-only: `wrangler pages deployment list` + `... activate <id>`, or the CF dashboard.

**One-time VPS setup:** `bash deploy/install-docker.sh` (installs Docker + compose plugin, creates `/opt/spades` with compose.yml + `.env` from `deploy/env.template`, cleans up any legacy systemd unit).

**Image tags:** every deploy pushes both `:<short-sha>` (immutable, used for rollback) and `:latest` (mutable; what the VPS pulls by default). Images live forever in ghcr.io.
```

- [ ] **Step 2: Verify the file still parses as Markdown** (just read it back):

```bash
head -50 /Users/wlim/Projects/rust-spades/SERVER.md
```

- [ ] **Step 3: Commit:**

```bash
git add SERVER.md
git commit -m "docs: rewrite SERVER.md Deployment section for Docker flow"
```

---

## Task 7 (operator): Migrate the VPS

**Manual; no code changes.** The current VPS has a broken systemd unit from the bash-flow attempt. This task replaces it with the Docker flow.

- [ ] **Step 1: SCP the bootstrap files to the VPS:**

```bash
cd /Users/wlim/Projects/rust-spades
scp deploy/install-docker.sh deploy/env.template docker-compose.yml deploy@$DEPLOY_HOST:/tmp/
```

(Verify each file landed cleanly: `ssh deploy@$DEPLOY_HOST 'head -3 /tmp/install-docker.sh /tmp/env.template /tmp/docker-compose.yml'`.)

- [ ] **Step 2: Run the bootstrap:**

```bash
# install-docker.sh expects to find docker-compose.yml at ../docker-compose.yml,
# so co-locate them in /tmp/spades-setup/ before running.
ssh deploy@$DEPLOY_HOST '
  mkdir -p /tmp/spades-setup/deploy &&
  cp /tmp/install-docker.sh /tmp/env.template /tmp/spades-setup/deploy/ &&
  cp /tmp/docker-compose.yml /tmp/spades-setup/ &&
  bash /tmp/spades-setup/deploy/install-docker.sh
'
```

Expected: Docker installs (or skips if already present), `/opt/spades` is created with compose.yml, `.env` is written from template, legacy systemd unit and sudoers entry are removed.

- [ ] **Step 3: Fill `/opt/spades/.env`:**

```bash
ssh deploy@$DEPLOY_HOST 'sudo -u deploy -e /opt/spades/.env'
```

At minimum confirm `CORS_ALLOW_ORIGIN` and `OAUTH_REDIRECT_BASE_URL` are correct. Other envs (SMTP, OAuth) only matter if those features are in use.

- [ ] **Step 4: Generate the GH-Actions SSH key (skip if already done in the previous rework):**

```bash
ls ~/.ssh/spades-deploy-gha 2>/dev/null && echo "key exists; skip" || \
    ssh-keygen -t ed25519 -f ~/.ssh/spades-deploy-gha -N "" -C "github-actions@spades"

# Authorize:
cat ~/.ssh/spades-deploy-gha.pub | ssh deploy@$DEPLOY_HOST 'cat >> ~/.ssh/authorized_keys'

# Confirm:
ssh -i ~/.ssh/spades-deploy-gha deploy@$DEPLOY_HOST 'docker --version'
```

The `docker --version` test confirms the deploy user is in the docker group (you may need to log out and back in once after `install-docker.sh` adds the group — if `docker --version` says "permission denied" on the socket, run `ssh deploy@$DEPLOY_HOST 'newgrp docker; docker --version'` or reboot the VPS).

- [ ] **Step 5: Set / confirm GitHub Actions secrets** (skip names that are already set from the previous rework):

```bash
gh secret list

# If any are missing:
gh secret set DEPLOY_SSH_KEY < ~/.ssh/spades-deploy-gha
gh secret set DEPLOY_HOST -b "<vps-hostname-or-ip>"
gh secret set DEPLOY_KNOWN_HOSTS -b "<ssh-keyscan -t ed25519 \$DEPLOY_HOST output>"
gh secret set CLOUDFLARE_API_TOKEN -b "<token>"
gh secret set CLOUDFLARE_ACCOUNT_ID -b "<account id>"
```

No new secrets vs. the previous rework — `GITHUB_TOKEN` is built-in for the ghcr.io push.

---

## Task 8 (operator): First Docker deploy

- [ ] **Step 1: Push master:**

```bash
git push origin master
```

- [ ] **Step 2: Watch the workflow:**

```bash
gh run watch
```

Expected: `ci` job passes (cargo + pnpm tests, web bundle artifact), `ship` job builds the Docker image, pushes both tags to ghcr.io, SSHes into the VPS to `compose pull && up -d`, waits for healthy, deploys CF Pages, smoke-checks both URLs.

- [ ] **Step 3: Verify in production:**

```bash
curl -fsS https://app.wlim.dev/ >/dev/null && echo "frontend OK"
curl -fsS https://spades.wlim.dev/health && echo
ssh deploy@$DEPLOY_HOST 'docker ps --filter name=spades-server --format "{{.Image}} {{.Status}}"'
```

Expected: smoke endpoints return 200; `docker ps` shows the running container's image SHA matches the just-deployed commit, and its status includes `(healthy)`.

- [ ] **Step 4: Clean up the legacy install-dir (optional):**

After confirming the Docker container is serving traffic correctly:

```bash
ssh deploy@$DEPLOY_HOST 'sudo rm -rf /opt/spades-server /etc/spades'
```

The Docker setup uses `/opt/spades` (not `-server`) and `/opt/spades/.env` (not `/etc/spades/env`), so the old paths are dead.

---

## Self-review notes

**Spec coverage (against `docs/superpowers/specs/2026-05-12-docker-pivot-design.md`):**
- Dockerfile → Task 1. ✓
- `.dockerignore` → Task 1. ✓
- docker-compose.yml → Task 2. ✓
- install-docker.sh (bootstrap with legacy cleanup) → Task 3. ✓
- Workflow rewrite (ci + ship, image tags, ghcr.io login via GITHUB_TOKEN, wait-for-healthy loop, CF Pages, smoke) → Task 4. ✓
- Deletions of bash-flow files → Task 5. ✓
- SERVER.md rewrite → Task 6. ✓
- VPS migration → Task 7. ✓
- First deploy → Task 8. ✓
- Open items (Rust image existence, ghcr.io package permission, Docker version on VPS, port-exposure choice) — surfaced in the spec for awareness; Task 1's image will fail the workflow if `rust:1.85-bookworm` doesn't exist (easy fix: drop to 1.84).

**Placeholder scan:** All code blocks contain complete content. The few `<your-vps-hostname>` and `<token>` placeholders in operator tasks are intentional user inputs.

**Type/name consistency:** `IMAGE_TAG` env var used in compose file matches what the workflow sets. Image path `ghcr.io/wlim33/spades` used in workflow, compose, and SERVER.md. `/opt/spades`, `/var/lib/spades`, `/data` paths used consistently across compose, install-docker.sh, Dockerfile.
