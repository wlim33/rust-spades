# Tier 3b — Pin Prod Image Off `:latest` — Design

- **Date:** 2026-05-26
- **Status:** Design approved; implementation pending
- **Scope:** `docker-compose.yml` image tag + `SERVER.md` docs. The second of three Tier 3 sub-projects (Tier 3a clap shipped; Tier 3c provisioning is separate).
- **Approach chosen:** error-only (no persisted tag), per the simpler of the two options.

## Context

`docker-compose.yml` pins the server image as `ghcr.io/wlim33/spades:${IMAGE_TAG:-latest}`. The deploy (`.github/workflows/deploy.yml` ship job) builds + pushes `:<sha>` and `:latest`, then runs `IMAGE_TAG=<sha> docker compose pull && up` — passing the tag **inline**. `/opt/spades/.env` is never touched by deploy scripts, so the pinned tag is not persisted anywhere.

The footgun: a **manual** `docker compose up` or `docker compose pull` on the VPS *without* `IMAGE_TAG` set silently resolves to `:latest` — which may be a different image than what's intentionally deployed. (A VPS reboot is safe: `restart: unless-stopped` restarts the existing container from its already-resolved image and does not re-run `docker compose up`.)

## Goal

Make `docker-compose.yml` **require** an explicit `IMAGE_TAG` — fail loudly instead of silently defaulting to `:latest`.

## Non-goals (per the chosen approach / YAGNI)

- **Not** persisting `IMAGE_TAG` to `/opt/spades/.env` (the rejected "persist" option). The deploy keeps passing it inline.
- **No** change to `.github/workflows/deploy.yml` — it already sets `IMAGE_TAG` inline, so it continues to work.
- **Not** removing the `:latest` push from the deploy build step — it's harmless once compose never defaults to it; removing it is unrelated churn.
- Not touching Tier 3c (provisioning).

## Design

### A. `docker-compose.yml` (one line)

Change the `spades-server` service image from:
```yaml
    image: ghcr.io/wlim33/spades:${IMAGE_TAG:-latest}
```
to:
```yaml
    image: ghcr.io/wlim33/spades:${IMAGE_TAG:?IMAGE_TAG must be set — the deploy passes the commit SHA; refusing to default to :latest}
```

`${VAR:?err}` makes `docker compose` exit with `err` whenever `IMAGE_TAG` is unset or empty.

### B. `SERVER.md`

Update the deploy/rollback section to reflect the new requirement:
1. Note that `docker-compose.yml` now **requires** `IMAGE_TAG` (no silent `:latest`).
2. **Ops caveat:** because `:?` is interpolated on every compose invocation, ad-hoc commands need a value too — e.g. `IMAGE_TAG=x docker compose logs` / `ps` (any value works for read-only commands; the image isn't pulled). The deploy is unaffected (it sets `IMAGE_TAG` inline).
3. Confirm the rollback one-liner stays accurate and is the intended mechanism: `ssh deploy@$VPS 'cd /opt/spades && IMAGE_TAG=<old-sha> docker compose up -d --pull always'`.

## Consequence (documented tradeoff)

`${IMAGE_TAG:?}` is evaluated when compose loads the file for **any** subcommand, so `docker compose ps/logs/down` also require `IMAGE_TAG` to be set. This is the accepted cost of the error-only approach (the persist-to-`.env` alternative would have avoided it). Workaround for ad-hoc ops: prefix any value (`IMAGE_TAG=x docker compose logs`). The deploy and reboot paths are unaffected.

## Files changed

| File | Change |
|------|--------|
| `docker-compose.yml` | 1 line — `${IMAGE_TAG:-latest}` → `${IMAGE_TAG:?...}` |
| `SERVER.md` | deploy/rollback note (requirement + ops caveat) |

No `deploy.yml`, `crates/**`, or web changes; no overlap with the uncommitted WIP.

## Verification

- `docker compose -f docker-compose.yml config` with **no** `IMAGE_TAG` → exits non-zero, error mentions `IMAGE_TAG must be set`.
- `IMAGE_TAG=testtag docker compose -f docker-compose.yml config` → resolves the image to `ghcr.io/wlim33/spades:testtag`.
- (Run locally if Docker Compose is installed; otherwise validated on the next deploy, which sets `IMAGE_TAG` inline, and by reading the rendered diff.)
- Confirm the deploy step `IMAGE_TAG=${SHORT_SHA} docker compose pull && up` still interpolates (it does — `IMAGE_TAG` is set there).

## Risk

Low. The deploy path and reboot behavior are unchanged; the only behavior change is that a `docker compose` command without `IMAGE_TAG` now fails loudly instead of silently using `:latest`.

## Out of scope / remaining Tier 3

- **Tier 3c — smoother provisioning** (`install-docker.sh`) — separate spec.
