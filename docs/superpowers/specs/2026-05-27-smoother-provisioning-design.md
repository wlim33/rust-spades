# Tier 3c — Smoother Provisioning (docs/output polish) — Design

- **Date:** 2026-05-27
- **Status:** Design approved; implementation pending
- **Scope:** `deploy/install-docker.sh` printed guidance + one `SERVER.md` clause. The last of three Tier 3 sub-projects (3a clap and 3b `:latest` pin shipped).
- **Approach chosen:** output/docs polish only (no preflight script, no cert-install flags).

## Context

`deploy/install-docker.sh` ends by printing a "Next steps" block listing four manual first-run actions (edit `.env`, install cert, add ssh key, first deploy). Two problems:

1. **Flat priority / unclear required-vs-optional.** Editing `.env` is mostly optional (the template pre-sets `CORS_ALLOW_ORIGIN` + `OAUTH_REDIRECT_BASE_URL`; OAuth/SMTP are opt-in), while installing the Origin CA cert is **required** (Caddy won't start without it, taking the site down). The current block doesn't convey this.
2. **Stale deploy command (post-Tier-3b).** The block shows `docker compose pull && docker compose up -d`. After Tier 3b pinned the image to `${IMAGE_TAG:?...}`, a bare `docker compose up` fails — the command must pass `IMAGE_TAG`. The workflow already does; the printed guidance is now misleading.

Separately, `SERVER.md`'s "One-time VPS setup" note (line ~395) lists install-docker.sh + cert + Full(strict) but **omits the deploy-SSH-key step** that the script's checklist includes.

This is the last roadmap item; it's low-risk because the script's printed text is `echo`'d output and the actual provisioning logic (`install`/`mkdir`/`chown`/cleanup) is untouched.

## Goal

Make the first-run guidance clear, prioritized, and accurate — and align `SERVER.md` with it.

## Non-goals (per the chosen approach / YAGNI)

- No preflight/validation script or `--check` mode (rejected option 1).
- No `--cert`/`--key`/`--ssh-key` flags or any change to what the script *does* (rejected option 2).
- No change to `install-docker.sh`'s provisioning logic — only its final printed block.

## Design

### A. `deploy/install-docker.sh` — rewrite the final "Next steps" heredoc

Replace the current `cat <<EOF ... EOF` block (the one beginning `==> Done.`) with a prioritized, accurate checklist. It keeps using the existing `$DEPLOY_USER` / `$SPADES_DIR` variables:

```
==> Bootstrap complete. Before the first deploy, finish these on the VPS:

  [REQUIRED] 1. Install the Cloudflare Origin CA cert + key into
                $SPADES_DIR/certs/ as spades.wlim.dev.pem and spades.wlim.dev.key —
                Caddy won't start (site stays down) without them. See deploy/origin-certs.md.
  [REQUIRED] 2. Add the GitHub Actions deploy public key to
                /home/$DEPLOY_USER/.ssh/authorized_keys (the workflow ssh's in as $DEPLOY_USER).
  [optional] 3. Edit $SPADES_DIR/.env  (sudo -u $DEPLOY_USER -e $SPADES_DIR/.env):
                CORS_ALLOW_ORIGIN and OAUTH_REDIRECT_BASE_URL are pre-set; fill the
                GOOGLE_/GITHUB_ OAuth and SMTP_ vars only if you want sign-in / email.
  [after 1st deploy] 4. Set Cloudflare SSL/TLS mode to Full (strict).

Then push to master — the workflow builds the image, ssh's in, and runs (from $SPADES_DIR):
    IMAGE_TAG=<sha> docker compose pull && IMAGE_TAG=<sha> docker compose up -d
(docker-compose.yml requires IMAGE_TAG; the workflow passes the commit SHA.)
```

Changes vs. today: reorders so the two **REQUIRED** steps (cert, ssh key) lead; states *why* the cert is required; marks `.env` optional and explains the pre-set vars; adds the Full(strict) step (previously only in docs); and corrects the deploy command to include `IMAGE_TAG` (Tier 3b consistency).

### B. `SERVER.md` — align the "One-time VPS setup" note

The note currently reads (line ~395):
> ... Then mint a Cloudflare Origin CA cert and install it into `/opt/spades/certs/` (see `deploy/origin-certs.md`) and set Cloudflare SSL/TLS mode to **Full (strict)**.

Add the deploy-SSH-key step so it matches the script's checklist — i.e. also mention adding the GitHub Actions deploy public key to `/home/deploy/.ssh/authorized_keys`. One clause; no other `SERVER.md` change.

## Files changed

| File | Change |
|------|--------|
| `deploy/install-docker.sh` | rewrite the final "Next steps" `cat <<EOF` block only |
| `SERVER.md` | add the deploy-SSH-key clause to the One-time VPS setup note |

No change to the script's provisioning actions; no `docker-compose.yml`/`deploy.yml`/code changes.

## Verification

- `bash -n deploy/install-docker.sh` → no syntax errors.
- `grep -n 'REQUIRED' deploy/install-docker.sh` → the new prioritized block is present.
- `grep -n 'docker compose up -d' deploy/install-docker.sh` → the only occurrence is the `IMAGE_TAG=<sha> docker compose ...` line (no bare `docker compose up -d` without `IMAGE_TAG`).
- `grep -n 'authorized_keys' SERVER.md` → the setup note now references the deploy ssh key.
- No execution needed — the heredoc is printed text; provisioning logic is unchanged.

## Risk

Negligible. Printed guidance + a docs clause; the script's behavior is identical. No runtime, deploy, or code impact.

## Outcome

Completes Tier 3 (3a config, 3b image pin, 3c provisioning) and the overall "simpler / more robust / more elegant" roadmap (Tiers 1–3).
