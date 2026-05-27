# Tier 3b — Pin Prod Image Off `:latest` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `docker-compose.yml` require an explicit `IMAGE_TAG` (no silent `:latest` default) and update `SERVER.md` to match.

**Architecture:** One-line change to the compose `image:` (`${IMAGE_TAG:-latest}` → `${IMAGE_TAG:?...}`) plus two `SERVER.md` doc edits (the deploy command and the "Image tags" note). `deploy.yml` is unchanged — it already passes `IMAGE_TAG=<sha>` inline. No code, no tests.

**Tech Stack:** docker compose, Markdown.

**Branch:** `dx/pin-prod-image` (spec already committed there).

**Spec:** `docs/superpowers/specs/2026-05-26-pin-prod-image-design.md`

**Guardrail:** The tree has unrelated uncommitted WIP (web/**, `.wrangler/`, deletions, an old transcript-plan doc). Commit ONLY `docker-compose.yml` and `SERVER.md`, via pathspec (message before `--`). NEVER `git add -A`/`.`/`-a`/`--amend`.

---

### Task 1: Require explicit `IMAGE_TAG` in compose + align docs

**Files:**
- Modify: `docker-compose.yml:3`
- Modify: `SERVER.md` (deploy step, line ~373; "Image tags" note, line ~397)

- [ ] **Step 1: Pin the compose image**

In `docker-compose.yml`, replace:
```
    image: ghcr.io/wlim33/spades:${IMAGE_TAG:-latest}
```
with:
```
    image: ghcr.io/wlim33/spades:${IMAGE_TAG:?IMAGE_TAG must be set — the deploy passes the commit SHA; refusing to default to :latest}
```

- [ ] **Step 2: Update the deploy command in `SERVER.md`**

In `SERVER.md`, replace:
```
4. SSH to the VPS: `cd /opt/spades && docker compose pull && docker compose up -d`, then wait for the container's healthcheck to flip to `healthy`
```
with:
```
4. SSH to the VPS with the pinned tag: `cd /opt/spades && IMAGE_TAG=<sha> docker compose pull && IMAGE_TAG=<sha> docker compose up -d`, then wait for the container's healthcheck to flip to `healthy`
```

- [ ] **Step 3: Update the "Image tags" note in `SERVER.md`**

In `SERVER.md`, replace:
```
**Image tags:** every deploy pushes both `:<short-sha>` (immutable, used for rollback) and `:latest` (mutable; what the VPS pulls by default). Images live forever in ghcr.io.
```
with:
```
**Image tags:** every deploy pushes both `:<short-sha>` (immutable, used for rollback) and `:latest` (mutable). `docker-compose.yml` pins the image to `${IMAGE_TAG:?...}`, so compose **requires** an explicit `IMAGE_TAG` and never silently falls back to `:latest`; the deploy and rollback commands pass the SHA. Because compose interpolates the file on every invocation, ad-hoc commands also need a value — e.g. `IMAGE_TAG=x docker compose logs` (any value works for read-only commands; nothing is pulled). Images live forever in ghcr.io.
```

(The **Rollback** bullet at line ~392 already uses `IMAGE_TAG=<short-sha> docker compose up -d --pull always`, which stays correct — no change needed there.)

- [ ] **Step 4: Verify**

Confirm the compose line changed and the file is still valid YAML:
```bash
grep -n 'IMAGE_TAG' docker-compose.yml
ruby -ryaml -e "YAML.load_file('docker-compose.yml'); puts 'yaml-ok'"
```
Expected: the `image:` line shows `${IMAGE_TAG:?...}` (no `:-latest`); prints `yaml-ok`.

Confirm the docs updated:
```bash
grep -nE 'IMAGE_TAG=<sha>|requires.*IMAGE_TAG|never silently' SERVER.md
```
Expected: the updated deploy command and the new "Image tags" wording both appear.

Behavior check (only if Docker Compose is installed locally):
```bash
IMAGE_TAG=testtag docker compose -f docker-compose.yml config 2>/dev/null | grep 'image: ghcr.io/wlim33/spades'
```
Expected: `image: ghcr.io/wlim33/spades:testtag` (interpolation resolves with a tag). A bare `docker compose -f docker-compose.yml config` (no `IMAGE_TAG`) exits non-zero, citing the `IMAGE_TAG must be set` message. (Locally, `config` may *also* warn about the missing `/opt/spades/.env` env_file — that's an environment artifact, not this change.) If Docker Compose isn't installed here, this is exercised on the next deploy, which sets `IMAGE_TAG=<sha>` inline — the deploy path is unaffected.

- [ ] **Step 5: Commit**

```bash
git add docker-compose.yml SERVER.md
git commit -m "deploy: require explicit IMAGE_TAG; stop compose defaulting to :latest" -- docker-compose.yml SERVER.md
```

---

### Final verification

- [ ] **Step 1: Scope check**

Run: `git diff --name-only master..HEAD`
Expected: only `docker-compose.yml`, `SERVER.md`, and the spec/plan docs. No `crates/**`, web, `.wrangler/`, or `deploy.yml` changes.

- [ ] **Step 2: Sanity — the deploy still resolves**

Confirm the workflow's deploy step still sets `IMAGE_TAG` (so `:?` resolves there):
```bash
grep -n 'IMAGE_TAG=' .github/workflows/deploy.yml
```
Expected: the `ship` job's `IMAGE_TAG=${SHORT_SHA} docker compose pull` / `up` lines are present and unchanged — the pin is satisfied on every real deploy.
