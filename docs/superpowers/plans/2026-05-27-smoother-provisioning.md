# Tier 3c — Smoother Provisioning (docs/output polish) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rewrite `install-docker.sh`'s printed "Next steps" into a prioritized, accurate checklist (fixing the post-Tier-3b deploy command), and add the missing deploy-SSH-key clause to `SERVER.md`'s setup note.

**Architecture:** Two text-only edits — the final `cat <<EOF … EOF` block in `deploy/install-docker.sh` (printed output; the script's provisioning logic is untouched) and one clause in `SERVER.md`. No code, no behavior change.

**Tech Stack:** Bash heredoc, Markdown.

**Branch:** `dx/provisioning-docs` (spec already committed there).

**Spec:** `docs/superpowers/specs/2026-05-27-smoother-provisioning-design.md`

**Guardrail:** The tree has unrelated uncommitted WIP (web/**, `.wrangler/`, etc.). Commit ONLY `deploy/install-docker.sh` and `SERVER.md`, via pathspec (message before `--`). NEVER `git add -A`/`.`/`-a`/`--amend`.

---

### Task 1: Polish the provisioning guidance

**Files:**
- Modify: `deploy/install-docker.sh` (the final `cat <<EOF` block)
- Modify: `SERVER.md` (the "One-time VPS setup" note)

- [ ] **Step 1: Rewrite the `install-docker.sh` "Next steps" heredoc**

Replace this exact block (it's the trailing `cat <<EOF … EOF`):
```bash
cat <<EOF

==> Done.

Next steps:
  1. Edit $SPADES_DIR/.env with real secrets:
       sudo -u $DEPLOY_USER -e $SPADES_DIR/.env
  2. Install the Cloudflare Origin CA cert and key into $SPADES_DIR/certs/.
     See deploy/origin-certs.md for instructions.
  3. Add the GitHub Actions deploy public key to:
       /home/$DEPLOY_USER/.ssh/authorized_keys
  4. The first push to master triggers the workflow. The workflow ssh's
     in and runs (from $SPADES_DIR):
       docker compose pull && docker compose up -d

EOF
```
with:
```bash
cat <<EOF

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

EOF
```
(Unquoted heredoc, so `$SPADES_DIR`/`$DEPLOY_USER` still expand at runtime as before; everything else is literal — `IMAGE_TAG=<sha>` and `&&` are printed verbatim.)

- [ ] **Step 2: Add the deploy-SSH-key clause to `SERVER.md`**

Replace this exact line (the "One-time VPS setup" note):
```
**One-time VPS setup:** `bash deploy/install-docker.sh` (installs Docker + compose plugin, creates `/opt/spades` with compose.yml, Caddyfile, an empty `certs/` dir, and `.env` from `deploy/env.template`, cleans up any legacy systemd unit). Then mint a Cloudflare Origin CA cert and install it into `/opt/spades/certs/` (see `deploy/origin-certs.md`) and set Cloudflare SSL/TLS mode to **Full (strict)**.
```
with:
```
**One-time VPS setup:** `bash deploy/install-docker.sh` (installs Docker + compose plugin, creates `/opt/spades` with compose.yml, Caddyfile, an empty `certs/` dir, and `.env` from `deploy/env.template`, cleans up any legacy systemd unit). Then mint a Cloudflare Origin CA cert and install it into `/opt/spades/certs/` (see `deploy/origin-certs.md`), add the GitHub Actions deploy public key to `/home/deploy/.ssh/authorized_keys`, and set Cloudflare SSL/TLS mode to **Full (strict)**.
```

- [ ] **Step 3: Verify**

```bash
bash -n deploy/install-docker.sh && echo "syntax-ok"
grep -n 'REQUIRED' deploy/install-docker.sh
grep -n 'docker compose up -d' deploy/install-docker.sh
grep -n 'authorized_keys' SERVER.md
```
Expected:
- `syntax-ok` (the script still parses).
- The `[REQUIRED]` lines appear (new block in place).
- The only `docker compose up -d` line is the `IMAGE_TAG=<sha> docker compose pull && IMAGE_TAG=<sha> docker compose up -d` one — **no** bare `docker compose up -d` without `IMAGE_TAG`.
- `SERVER.md`'s setup note now references `/home/deploy/.ssh/authorized_keys`.

(No execution — the heredoc is printed text and the script's `apt-get`/`install`/`adduser` logic is unchanged and must not be run here.)

- [ ] **Step 4: Commit**

```bash
git add deploy/install-docker.sh SERVER.md
git commit -m "docs(deploy): prioritized provisioning checklist; align IMAGE_TAG + ssh-key steps" -- deploy/install-docker.sh SERVER.md
```

---

### Final verification

- [ ] **Step 1: Scope check**

Run: `git diff --name-only master..HEAD`
Expected: only `deploy/install-docker.sh`, `SERVER.md`, and the spec/plan docs. No `crates/**`, web, `docker-compose.yml`, or `deploy.yml` changes.

- [ ] **Step 2: Provisioning logic untouched**

Run: `git diff master..HEAD -- deploy/install-docker.sh | grep -E '^[-+]' | grep -vE '^[-+]{3} ' | grep -E 'apt-get|adduser|usermod|mkdir|install -|systemctl|rm -f|chown'`
Expected: no output — the diff touches only the printed heredoc, not any provisioning command.
