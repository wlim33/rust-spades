# VPS TLS reverse proxy — design

Date: 2026-05-12

## Why this exists

The compose stack from `2026-05-12-docker-pivot-design.md` publishes `spades-server:3000` directly on the host, plain HTTP. Cloudflare proxies `spades.wlim.dev` → VPS, but with SSL/TLS mode "Full (strict)" the cf→origin handshake fails because the origin doesn't speak TLS — the post-deploy smoke check returns HTTP 525.

The alternative ("Flexible" mode — cf↔browser HTTPS, cf↔origin plain HTTP) gets the smoke check green but leaves the cf→origin leg unencrypted across the public internet. Not catastrophic, but a real downgrade.

This spec adds a TLS-terminating reverse proxy on the VPS so the origin actually serves HTTPS. The proxy is Caddy, the cert is a Cloudflare Origin CA cert (static, 15-year), and the proxy runs as a second service in the existing compose stack.

## Goals

- Cloudflare can talk to origin in "Full (strict)" mode without warnings.
- No host port for `spades-server` — only `caddy:443` is reachable from outside the compose network.
- No ACME automation. The cert is static and lives for 15 years.
- Single deploy unit — Caddy and `spades-server` ship in the same `docker-compose.yml`.
- Bootstrap script (`deploy/install-docker.sh`) installs the Caddyfile and creates the cert directory but never touches the cert/key files (operator drops them in once).

## Non-goals

- Publicly-trusted origin cert. Origin CA cert is only valid because Cloudflare's proxy trusts it; that's a deliberate trade for zero cert automation.
- Bypassing Cloudflare. Direct origin access (gray cloud) would produce a cert warning. Documented as a constraint.
- HTTP→HTTPS redirect at origin. Cloudflare's "Always Use HTTPS" handles this at the edge; we don't expose port 80 on the VPS.
- Multi-domain wildcard cert. We provision for `spades.wlim.dev` only; future hosts get their own cert (or a single multi-SAN cert) by hand.

## Architecture

```
Browser ──HTTPS (public CA)──► Cloudflare (Full strict)
                                  │
                                  └──HTTPS (Origin CA)──► VPS:443  Caddy
                                                                    │
                                                                    └──HTTP──► spades-server:3000
                                                                                (internal compose network)
```

Two containers in `/opt/spades/docker-compose.yml`:

1. **`caddy`** — image `caddy:2-alpine`. Publishes `:443`. Mounts the Caddyfile and the cert/key directory read-only. Reverse-proxies all traffic to `spades-server:3000` over the internal compose network.
2. **`spades-server`** — unchanged backend, except `ports: ["3000:3000"]` is replaced with `expose: ["3000"]` so port 3000 is no longer reachable from the host.

DNS: `spades.wlim.dev` stays a Cloudflare-proxied A record pointing at the VPS IP. Cloudflare SSL/TLS mode is **Full (strict)**.

## Files

### New

**`deploy/Caddyfile`** — declarative config:

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

`auto_https off` because we're using a static cert, not ACME — keeps Caddy from negotiating certs on startup.

**`deploy/origin-certs.md`** — explains where the cert/key come from (Cloudflare dashboard → SSL/TLS → Origin Server → Create Certificate, 15-year validity), the install path on the VPS (`/opt/spades/certs/`), the required ownership (`deploy:deploy`) and modes (`0640` for the cert, `0600` for the key). Notes the constraint that the cert is only valid behind Cloudflare's proxy. (Single file at top of `deploy/` — no `certs/` directory in the repo, since the directory only exists on the VPS.)

### Changed

**`docker-compose.yml`** — drop host-port publish on `spades-server`, add `caddy` service:

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

`caddy_data` and `caddy_config` are kept for symmetry with the official image; they hold no ACME state in our setup but cost nothing.

**`deploy/install-docker.sh`** — append two steps after the `.env` install:

1. Install `deploy/Caddyfile` to `/opt/spades/Caddyfile` (mode `0644`, owned by `deploy:deploy`), idempotent (overwrite on re-run since the file is source-of-truth in the repo).
2. Create `/opt/spades/certs/` (mode `0700`, owned by `deploy:deploy`) if absent. Never touch its contents.

**`.github/workflows/deploy.yml`** — the "Copy compose file to VPS" step also scp's the Caddyfile:

```yaml
- name: Copy compose + Caddyfile to VPS
  run: |
    scp docker-compose.yml deploy@${{ secrets.DEPLOY_HOST }}:/opt/spades/docker-compose.yml
    scp deploy/Caddyfile    deploy@${{ secrets.DEPLOY_HOST }}:/opt/spades/Caddyfile
```

The healthcheck-poll loop on `spades-server` stays as-is. We don't gate the deploy on Caddy health (Caddy starts independently of backend readiness).

## One-time setup on the VPS

These run by hand once per VPS, outside the workflow:

1. Generate the Origin CA cert in the Cloudflare dashboard:
   - SSL/TLS → Origin Server → Create Certificate
   - Hostnames: `spades.wlim.dev` (and optionally `*.wlim.dev` for future services)
   - Validity: 15 years
   - Download cert and private key as PEM.

2. Install them on the VPS as the operator (not `deploy`):
   ```bash
   scp spades.wlim.dev.pem myuser@vps:/tmp/
   scp spades.wlim.dev.key myuser@vps:/tmp/
   ssh myuser@vps
   sudo install -d -m 0700 -o deploy -g deploy /opt/spades/certs
   sudo install -m 0640 -o deploy -g deploy /tmp/spades.wlim.dev.pem /opt/spades/certs/
   sudo install -m 0600 -o deploy -g deploy /tmp/spades.wlim.dev.key /opt/spades/certs/
   rm /tmp/spades.wlim.dev.{pem,key}
   ```

3. Flip Cloudflare SSL/TLS mode to **Full (strict)** (dashboard → SSL/TLS → Overview).

4. Open port 443 if a firewall is enabled:
   ```bash
   sudo ufw allow 443/tcp
   ```
   Port 3000 stays closed (and is no longer published by compose).

## Failure modes

| Symptom | Cause | Resolution |
|---|---|---|
| `caddy` restart loop | Cert/key missing at `/opt/spades/certs/` | Operator hasn't run the one-time setup. `docker compose logs caddy` shows the missing-file path. |
| 502 from `spades.wlim.dev` | `spades-server` unhealthy; Caddy is up but backend not | Same as today — workflow's poll on `spades-server.State.Health.Status` already catches this and fails the deploy. |
| HTTP 525 still | Cloudflare still in "Flexible" mode, or cert hostname doesn't match | Verify dashboard SSL/TLS mode; verify `openssl s_client` output (see Testing). |
| Cert warning in browser when gray-cloud testing | Origin CA is private to Cloudflare | Expected — documented in `deploy/origin-certs.md`. Use `curl --resolve` for direct-origin testing instead. |

## Testing

No unit tests — pure infra change with no Rust/TS code paths affected. Verification is operational:

1. **End-to-end through Cloudflare** (from a laptop):
   ```bash
   curl -fsS https://spades.wlim.dev/health
   ```
   Expect 200. This is what the workflow's existing smoke check already runs.

2. **Internal Caddy → backend path** (from the VPS):
   ```bash
   docker exec spades-caddy wget -qO- http://spades-server:3000/health
   ```
   Expect 200. Confirms compose-internal DNS and the backend's own health endpoint.

3. **Cert identity check** (from the VPS):
   ```bash
   echo | openssl s_client -connect localhost:443 -servername spades.wlim.dev 2>/dev/null \
     | openssl x509 -noout -issuer -subject -dates
   ```
   Issuer should be `CN=Cloudflare Origin SSL Certificate Authority`. Subject matches `spades.wlim.dev`. `notAfter` ~15 years out.

4. **Negative: port 3000 not exposed** (from outside the VPS):
   ```bash
   curl --max-time 5 http://<vps-ip>:3000/health
   ```
   Expect connection refused/timeout. Confirms `spades-server` is no longer publishing.
