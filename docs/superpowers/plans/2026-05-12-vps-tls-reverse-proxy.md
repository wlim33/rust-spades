# VPS TLS Reverse Proxy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a Caddy reverse proxy in the existing compose stack so the VPS terminates TLS for `spades.wlim.dev`, using a static Cloudflare Origin CA cert. Cloudflare can then run in "Full (strict)" and the deploy smoke check passes.

**Architecture:** Two services in `/opt/spades/docker-compose.yml`: `spades-server` (now internal-only, port `3000` exposed on the compose network) and `caddy` (`caddy:2-alpine`, publishes `:443`, mounts a static Origin CA cert/key). Cert is generated once in the Cloudflare dashboard and installed by an operator out-of-band. No ACME, no port 80.

**Tech Stack:** Caddy 2 (alpine image), Docker Compose v2, GitHub Actions deploy workflow (existing), Cloudflare Origin CA (operator-provisioned).

**Spec:** `docs/superpowers/specs/2026-05-12-vps-tls-reverse-proxy-design.md`

---

## File Structure

| File | Action | Purpose |
|---|---|---|
| `deploy/Caddyfile` | create | Declarative reverse-proxy config for `spades.wlim.dev` with static cert. |
| `deploy/origin-certs.md` | create | Operator docs: how to mint the Cloudflare Origin CA cert and install it on the VPS. |
| `docker-compose.yml` | modify | Drop host-port publish on `spades-server`; add `caddy` service; add named volumes. |
| `deploy/install-docker.sh` | modify | Install the Caddyfile under `/opt/spades/Caddyfile` and create `/opt/spades/certs/` (no contents). |
| `.github/workflows/deploy.yml` | modify | scp the Caddyfile alongside `docker-compose.yml` on every deploy. |

All files are config/docs. No Rust or TS code changes. No unit tests — verification is operational (commands listed at the end of the plan).

---

## Task 1: Add the Caddyfile

**Files:**
- Create: `deploy/Caddyfile`

- [ ] **Step 1: Create the Caddyfile**

Create `deploy/Caddyfile` with this exact content:

```caddy
{
    auto_https off
}

spades.wlim.dev:443 {
    tls /etc/caddy/certs/spades.wlim.dev.pem /etc/caddy/certs/spades.wlim.dev.key
    encode gzip
    reverse_proxy spades-server:3000 {
        header_up X-Real-IP {http.request.remote.host}
        header_up X-Forwarded-For {http.request.header.X-Forwarded-For}
        header_up X-Forwarded-Proto https
    }
}
```

Notes:
- `auto_https off` disables Caddy's automatic ACME — we're using a static cert.
- The cert/key paths are inside the container; the host mount lands them at `/etc/caddy/certs/`.

- [ ] **Step 2: Validate the Caddyfile syntax**

Run:
```bash
docker run --rm -v "$(pwd)/deploy/Caddyfile:/etc/caddy/Caddyfile:ro" caddy:2-alpine \
    caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile
```

Expected output (last line):
```
Valid configuration
```

If it errors, fix the Caddyfile until it validates.

- [ ] **Step 3: Commit**

```bash
git add deploy/Caddyfile
git commit -m "deploy: add Caddy reverse-proxy config for spades.wlim.dev"
```

---

## Task 2: Add the origin-cert operator docs

**Files:**
- Create: `deploy/origin-certs.md`

- [ ] **Step 1: Create the docs file**

Create `deploy/origin-certs.md` with this exact content:

```markdown
# Cloudflare Origin CA cert for spades.wlim.dev

The VPS's Caddy reverse proxy terminates TLS using a Cloudflare Origin CA
certificate. Origin CA certs are signed by Cloudflare's private CA — they
only validate when traffic arrives via the Cloudflare proxy. That's the
right trade for us: no ACME automation, 15-year validity, no port 80
required.

If you ever need to reach the origin without Cloudflare's proxy (gray
cloud, direct IP), the cert will not validate in a browser. Use
`curl --resolve` or `openssl s_client` for direct-origin debugging.

## Generating the cert

1. Cloudflare dashboard → SSL/TLS → Origin Server → Create Certificate.
2. Hostnames: `spades.wlim.dev` (add `*.wlim.dev` if you'll host more
   services on this VPS later).
3. Validity: 15 years.
4. Key type: ECDSA (preferred) or RSA 2048.
5. Download the certificate (PEM) and private key (PEM).

## Installing on the VPS

From a workstation that has the files:

```bash
scp spades.wlim.dev.pem myuser@<vps>:/tmp/
scp spades.wlim.dev.key myuser@<vps>:/tmp/
ssh myuser@<vps>
sudo install -d -m 0700 -o deploy -g deploy /opt/spades/certs
sudo install -m 0640 -o deploy -g deploy /tmp/spades.wlim.dev.pem /opt/spades/certs/
sudo install -m 0600 -o deploy -g deploy /tmp/spades.wlim.dev.key /opt/spades/certs/
rm /tmp/spades.wlim.dev.{pem,key}
```

The `caddy` service mounts `/opt/spades/certs` read-only at
`/etc/caddy/certs` and reads `spades.wlim.dev.pem` / `spades.wlim.dev.key`
from there.

## Cloudflare SSL/TLS mode

After the cert is installed and a deploy lands, set SSL/TLS mode to
**Full (strict)** in the Cloudflare dashboard (SSL/TLS → Overview).
Anything weaker leaves the cf→origin leg unencrypted (Flexible) or
unverified (Full).

## Renewing

Origin CA certs are valid for 15 years. There's nothing to automate.
When the cert nears expiry (set a calendar reminder), repeat the
generate-and-install steps above and `docker compose restart caddy`.
```

- [ ] **Step 2: Commit**

```bash
git add deploy/origin-certs.md
git commit -m "deploy: document Cloudflare Origin CA cert provisioning"
```

---

## Task 3: Update docker-compose.yml

**Files:**
- Modify: `docker-compose.yml` (full replace)

- [ ] **Step 1: Replace the compose file**

Replace the entire contents of `docker-compose.yml` with:

```yaml
services:
  spades-server:
    image: ghcr.io/wlim33/spades:${IMAGE_TAG:-latest}
    container_name: spades-server
    restart: unless-stopped
    expose:
      - "3000"
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

  caddy:
    image: caddy:2-alpine
    container_name: spades-caddy
    restart: unless-stopped
    depends_on:
      spades-server:
        condition: service_healthy
    ports:
      - "443:443"
    volumes:
      - /opt/spades/Caddyfile:/etc/caddy/Caddyfile:ro
      - /opt/spades/certs:/etc/caddy/certs:ro
      - caddy_data:/data
      - caddy_config:/config
    healthcheck:
      test: ["CMD", "wget", "-qO-", "http://127.0.0.1:2019/config/"]
      interval: 30s
      timeout: 5s
      retries: 3

volumes:
  caddy_data:
  caddy_config:
```

Key changes from the previous file:
- `spades-server`: `ports: ["3000:3000"]` → `expose: ["3000"]`. The backend is no longer reachable from outside the compose network.
- New `caddy` service: publishes 443, depends on `spades-server` being healthy, has its own healthcheck via the Caddy admin API on `127.0.0.1:2019` (admin is enabled by default, listening on loopback inside the container).
- New named volumes (`caddy_data`, `caddy_config`) — empty in our setup but kept for image symmetry.

- [ ] **Step 2: Validate compose syntax**

Run:
```bash
docker compose -f docker-compose.yml config >/dev/null && echo OK
```

Expected output:
```
OK
```

If it errors, fix the YAML until it validates.

- [ ] **Step 3: Commit**

```bash
git add docker-compose.yml
git commit -m "compose: front spades-server with Caddy for TLS termination"
```

---

## Task 4: Update install-docker.sh

**Files:**
- Modify: `deploy/install-docker.sh:50-57` (insert new steps after the `.env` install block)

- [ ] **Step 1: Read the current install-docker.sh**

Run:
```bash
cat deploy/install-docker.sh
```

Locate the block:
```bash
echo "==> Creating .env (if missing)"
if [ ! -f "$SPADES_DIR/.env" ]; then
    sudo install -m 0640 -o "$DEPLOY_USER" -g "$DEPLOY_USER" \
        "$SCRIPT_DIR/env.template" "$SPADES_DIR/.env"
    echo "    -- wrote template to $SPADES_DIR/.env; edit it with real secrets before the first deploy."
else
    echo "    -- $SPADES_DIR/.env already exists; leaving it alone."
fi
```

We add two new steps immediately after this block.

- [ ] **Step 2: Insert the Caddyfile install + certs dir creation**

Edit `deploy/install-docker.sh`. Immediately after the `.env` block above and before the next `echo "==> ..."`, insert:

```bash

echo "==> Installing Caddyfile"
sudo install -m 0644 -o "$DEPLOY_USER" -g "$DEPLOY_USER" \
    "$SCRIPT_DIR/Caddyfile" "$SPADES_DIR/Caddyfile"

echo "==> Creating certs directory (if missing)"
if [ ! -d "$SPADES_DIR/certs" ]; then
    sudo install -d -m 0700 -o "$DEPLOY_USER" -g "$DEPLOY_USER" "$SPADES_DIR/certs"
    echo "    -- created $SPADES_DIR/certs; drop spades.wlim.dev.pem and spades.wlim.dev.key here before the first deploy (see deploy/origin-certs.md)."
else
    echo "    -- $SPADES_DIR/certs already exists; leaving it alone."
fi
```

Notes:
- The Caddyfile install is unconditional (overwrites on re-run) because the repo is source-of-truth for it. Unlike `.env`, the Caddyfile has no secrets.
- The certs directory is created empty if absent; never overwritten.

- [ ] **Step 3: Update the "Next steps" message**

Find the block near the bottom of the file that lists next steps after running the script. It currently mentions editing `.env`. Add a line about installing the cert. The block looks roughly like:

```bash
cat <<EOM
==> Done.

Next steps:
  1. Edit $SPADES_DIR/.env with real secrets:
       sudo -u $DEPLOY_USER -e $SPADES_DIR/.env
...
EOM
```

Insert immediately after the `.env` line:

```
  2. Install the Cloudflare Origin CA cert and key into $SPADES_DIR/certs/.
     See deploy/origin-certs.md for instructions.
```

Renumber subsequent items if any.

- [ ] **Step 4: Bash syntax check**

Run:
```bash
bash -n deploy/install-docker.sh && echo OK
```

Expected output:
```
OK
```

- [ ] **Step 5: Commit**

```bash
git add deploy/install-docker.sh
git commit -m "deploy: bootstrap Caddyfile and certs dir in install-docker.sh"
```

---

## Task 5: Update the deploy workflow

**Files:**
- Modify: `.github/workflows/deploy.yml:105-106`

- [ ] **Step 1: Update the scp step**

Open `.github/workflows/deploy.yml`. Find the step:

```yaml
      - name: Copy compose file to VPS
        run: scp docker-compose.yml deploy@${{ secrets.DEPLOY_HOST }}:/opt/spades/docker-compose.yml
```

Replace it with:

```yaml
      - name: Copy compose file + Caddyfile to VPS
        run: |
          scp docker-compose.yml deploy@${{ secrets.DEPLOY_HOST }}:/opt/spades/docker-compose.yml
          scp deploy/Caddyfile    deploy@${{ secrets.DEPLOY_HOST }}:/opt/spades/Caddyfile
```

Nothing else in the workflow needs to change — the existing `docker compose pull && docker compose up -d --remove-orphans` will pick up the new service and the existing healthcheck-poll still gates on `spades-server` (not Caddy).

- [ ] **Step 2: Sanity-check the YAML**

Run:
```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/deploy.yml'))" && echo OK
```

Expected output:
```
OK
```

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/deploy.yml
git commit -m "ci: scp Caddyfile alongside docker-compose.yml on deploy"
```

---

## Task 6: Operator runbook (one-time, out-of-band)

These steps are NOT automated. They run by hand once per VPS before the next deploy. The plan must call them out so they don't get forgotten — a deploy without them will fail (Caddy will restart-loop because the cert files don't exist).

- [ ] **Step 1: Generate the Origin CA cert in the Cloudflare dashboard**

- Cloudflare dashboard → SSL/TLS → Origin Server → Create Certificate
- Hostnames: `spades.wlim.dev` (and optionally `*.wlim.dev`)
- Validity: 15 years
- Download cert and key as PEM. Save locally as `spades.wlim.dev.pem` and `spades.wlim.dev.key`.

- [ ] **Step 2: Install the cert on the VPS**

From a workstation with the PEM files:
```bash
scp spades.wlim.dev.pem myuser@<vps>:/tmp/
scp spades.wlim.dev.key myuser@<vps>:/tmp/
ssh myuser@<vps>
sudo install -d -m 0700 -o deploy -g deploy /opt/spades/certs
sudo install -m 0640 -o deploy -g deploy /tmp/spades.wlim.dev.pem /opt/spades/certs/
sudo install -m 0600 -o deploy -g deploy /tmp/spades.wlim.dev.key /opt/spades/certs/
rm /tmp/spades.wlim.dev.{pem,key}
```

Verify on the VPS:
```bash
sudo ls -la /opt/spades/certs/
```

Expected: two files, `spades.wlim.dev.pem` (mode `0640`) and `spades.wlim.dev.key` (mode `0600`), owned by `deploy:deploy`.

- [ ] **Step 3: Flip Cloudflare SSL/TLS mode to "Full (strict)"**

Cloudflare dashboard → SSL/TLS → Overview → set encryption mode to **Full (strict)**.

- [ ] **Step 4: Open port 443 on the VPS firewall (if ufw is enabled)**

```bash
sudo ufw status   # confirm ufw is active first
sudo ufw allow 443/tcp
```

If `ufw status` returns `Status: inactive`, skip this step.

Port 3000 stays closed — and compose no longer publishes it, so even if the firewall is permissive, nothing on the host listens on `:3000` from outside.

---

## Task 7: Trigger the deploy and verify end-to-end

- [ ] **Step 1: Push to master**

After Tasks 1–5 have been committed and Task 6 has been completed:
```bash
git push origin master
```

This kicks off `.github/workflows/deploy.yml`.

- [ ] **Step 2: Watch the workflow**

```bash
gh run watch
```

Wait for the workflow to complete. The final "Smoke check" step is the one we expect to start passing.

If the workflow fails before the smoke check, debug per the failing step. If it fails on the smoke check, continue to Step 3 to gather diagnostics.

- [ ] **Step 3: End-to-end verification through Cloudflare**

From your workstation:
```bash
curl -fsS https://spades.wlim.dev/health
```

Expected: HTTP 200, response body matches what `spades-server`'s `/health` endpoint returns (likely `ok` or similar).

If you get HTTP 525, Cloudflare can't TLS-handshake with origin — check that the cert files exist on the VPS (Task 6 Step 2) and that Caddy is running:
```bash
ssh deploy@<vps>
cd /opt/spades && docker compose ps
docker compose logs caddy --tail=50
```

- [ ] **Step 4: Verify Caddy → backend on the VPS**

```bash
ssh deploy@<vps>
docker exec spades-caddy wget -qO- http://spades-server:3000/health
```

Expected: same body as Step 3.

- [ ] **Step 5: Verify cert identity**

On the VPS:
```bash
echo | openssl s_client -connect localhost:443 -servername spades.wlim.dev 2>/dev/null \
    | openssl x509 -noout -issuer -subject -dates
```

Expected:
- `issuer=CN = Cloudflare Origin SSL Certificate Authority` (or similar — must mention Cloudflare Origin)
- `subject` includes `spades.wlim.dev`
- `notAfter` ~15 years from today

- [ ] **Step 6: Verify port 3000 is no longer publicly reachable**

From your workstation (NOT the VPS):
```bash
curl --max-time 5 "http://<vps-ip>:3000/health"
```

Expected: connection refused or timeout. If it returns 200, compose is still publishing port 3000 — re-check `docker-compose.yml` and re-deploy.
