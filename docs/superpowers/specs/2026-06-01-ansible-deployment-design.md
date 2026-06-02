# Ansible Deployment — Design Spec

**Date:** 2026-06-01
**Status:** Approved (pending implementation plan)
**Author:** William Lim

## Goal

Replace the current split deployment system — a hand-rolled bootstrap shell
script (`deploy/install-docker.sh`) plus ~100 lines of inline `ssh`/`scp`/`docker
compose`/`wrangler` glue inside one GitHub Actions job — with a single
**Ansible**-driven pipeline that is declarative, idempotent, and runnable both
from CI and from a laptop.

### Motivating pain points (all four selected)

- **Brittle CI glue** — the inline bash in `deploy.yml`'s `ship` job is fragile,
  hard to read, and can't be tested without pushing to `master`.
- **Want declarative state** — prefer describing desired server state so
  re-running converges and drift is visible.
- **Manual server setup** — `install-docker.sh` + by-hand cert/`.env` steps
  aren't repeatable; rebuilding the VPS or adding a server is painful.
- **Run deploys locally** — deploy/manage the server from a laptop on demand,
  not only via a push to `master`.

## Current system (baseline being replaced)

- **Backend:** Docker Compose (`spades-server` + Caddy) on a single Hetzner VPS
  (`5.161.99.196`); images pulled from `ghcr.io/wlim33/spades`, SHA-pinned.
- **Frontend:** Cloudflare Pages via `wrangler`.
- **Deploy engine:** GitHub Actions (`.github/workflows/deploy.yml`) — builds the
  image, SCPs `docker-compose.yml` + `Caddyfile` to the VPS, SSHes in to run
  `docker compose pull/up`, then deploys the frontend.
- **One-time bootstrap:** `deploy/install-docker.sh`.
- **Manual steps:** Cloudflare Origin CA certs copied by hand
  (`deploy/origin-certs.md`); `/opt/spades/.env` created once and never updated.
- **Server state on VPS:** `/opt/spades/{docker-compose.yml, Caddyfile, .env,
  certs/}` and `/var/lib/spades/games.sqlite`.

## Design decisions (locked during brainstorming)

| Decision | Choice | Rationale |
|---|---|---|
| CI vs Ansible | **Ansible does everything** (build → push → deploy → frontend); CI/laptop just trigger one playbook | Uniformity; one command everywhere. Accept loss of GHA layer-cache nicety. |
| Build host | **Control node** (`delegate_to: localhost` — GHA runner or laptop), push to ghcr; VPS only pulls | Keeps VPS lean, build env disposable, preserves immutable SHA tags. |
| Inventory | **Single prod host, future-ready** (group_vars/host_vars layering) | One host today; adding staging/second box is a new inventory entry. |
| Secrets | **Ansible Vault in repo** | Single source of truth; makes `.env` *and* Origin certs declarative; fixes "never updated" `.env`. |
| Playbook structure | **Two playbooks, role-based** (`provision.yml` + `deploy.yml`) | Matches cadence split (provision rarely, deploy constantly); roles testable in isolation. |

## Architecture

### Repository layout

A new top-level `ansible/` directory; the existing `deploy/` artifacts are
absorbed into it as templates and then removed (see Migration).

```
ansible/
  ansible.cfg                 # inventory path, ssh settings, vault password file hook
  inventory/
    production.yml            # spades-prod -> 5.161.99.196, ansible_user=deploy
  group_vars/
    all/
      vars.yml                # non-secret: domain, image repo, paths, ports
      vault.yml               # ansible-vault encrypted: tokens, oauth, smtp, origin cert+key
  host_vars/
    spades-prod.yml           # host-specific overrides (currently thin)
  provision.yml               # play: hosts=production, roles=[common]
  deploy.yml                  # play: build/push (localhost) -> backend (vps) -> frontend (localhost)
  roles/
    common/                   # host bootstrap (replaces install-docker.sh)
      tasks/main.yml
      handlers/main.yml
    backend/                  # app + caddy on the VPS
      tasks/main.yml
      templates/
        docker-compose.yml.j2 # from deploy/docker-compose.yml
        Caddyfile.j2          # from deploy/Caddyfile
        env.j2                # from deploy/env.template — NOW managed
      handlers/main.yml
    frontend/                 # build web/dist + wrangler pages deploy (runs on localhost)
      tasks/main.yml
  README.md                   # how to provision/deploy, local + CI
```

- `docker-compose.yml`, `Caddyfile`, `env.template` move from `deploy/` into
  `roles/*/templates/` as Jinja2 templates; hardcoded values (domain, image tag,
  origin) become variables.
- The root `Dockerfile` stays put (it is source, not deploy plumbing).

### `provision.yml` — the `common` role (rare path)

Declarative replacement for `install-docker.sh`. Idempotent; re-running
converges with no changes.

1. **Docker install** — Docker apt repo + GPG key; install `docker-ce`,
   `docker-ce-cli`, `containerd.io`, `docker-compose-plugin` via
   `ansible.builtin.apt`.
2. **`deploy` user** — system user, in `docker` group, with CI/laptop public SSH
   key in `authorized_keys`.
3. **Directory tree** — `/opt/spades`, `/opt/spades/certs`, `/var/lib/spades`
   (latter owned `1000:1000` to match container UID), correct modes.
4. **Legacy cleanup** — remove old systemd units / sudoers entries (as
   `install-docker.sh` did) so a mid-migration server converges cleanly.
5. **ghcr login** — `docker login ghcr.io` on the VPS using the vault token so
   `compose pull` works.

Does **not** lay down `.env`, certs, compose, or Caddyfile — those belong to the
`backend` role so they update on every deploy, not only at bootstrap. This split
fixes the "`.env` created once and never updated" problem.

### `deploy.yml` — three plays (frequent path)

**Play 1 — build & push (`localhost`):**
- `docker buildx build` the root `Dockerfile` → tag
  `ghcr.io/wlim33/spades:<image_tag>` (+ `:latest`).
- `image_tag` defaults to `git rev-parse --short HEAD`; CI overrides with
  `-e image_tag=$GITHUB_SHA`. Local and CI deploys are otherwise identical.
- Push both tags to ghcr (login via vault token). Immutable SHA tag preserved
  for rollback.

**Play 2 — `backend` role (`spades-prod`):**
1. Template `docker-compose.yml.j2`, `Caddyfile.j2`, `env.j2` →
   `/opt/spades/`. Each is checksummed; a change triggers the restart handler.
2. Drop Origin CA **cert + key** from vault → `/opt/spades/certs/` (modes
   `0640`/`0600`, owner `deploy`). Last manual step eliminated.
3. `docker compose pull` (the pinned `image_tag`).
4. `docker compose up -d --remove-orphans`.
5. **Health gate** — poll `http://127.0.0.1:3000/health` until healthy or
   ~60s timeout; fail the play if it never goes healthy.

**Play 3 — `frontend` role (`localhost`):**
- `pnpm install --frozen-lockfile && pnpm build` → `web/dist`.
- `wrangler pages deploy web/dist --project-name=spades` (CF token from vault).
- Smoke-check `https://app.wlim.dev/` and `https://spades.wlim.dev/health`.

**Rollback:** `ansible-playbook deploy.yml -e image_tag=<good-sha> --tags backend`
re-pins without rebuilding (tags skip the build/frontend plays).

**Control-node prerequisites** (Plays 1 & 3 run on `localhost`): Docker + buildx,
and pnpm/node. True on the GHA runner; documented for laptop use in the README.

### Variables & Vault model

**`group_vars/all/vars.yml`** (plaintext, committed) — non-secret knobs
previously hardcoded across compose/Caddyfile/env:

```yaml
spades_domain: spades.wlim.dev
app_domain: app.wlim.dev
cors_allow_origin: "https://app.wlim.dev"
image_repo: ghcr.io/wlim33/spades
image_tag: latest            # overridden per-deploy via -e
app_dir: /opt/spades
data_dir: /var/lib/spades
cf_pages_project: spades
```

**`group_vars/all/vault.yml`** (`ansible-vault` encrypted, committed) — all
secrets, consumed by `env.j2`, the ghcr/CF login tasks, and the cert tasks:

```
vault_ghcr_token
vault_cf_api_token
vault_cf_account_id
vault_google_oauth_client_id / vault_google_oauth_client_secret
vault_github_oauth_client_id / vault_github_oauth_client_secret
vault_smtp_host / vault_smtp_user / vault_smtp_pass / vault_smtp_from
vault_origin_cert   # full PEM, multiline
vault_origin_key    # full PEM key, multiline
```

**Vault password delivery:**
- **Local:** `--ask-vault-pass`, or `ANSIBLE_VAULT_PASSWORD_FILE` → gitignored file.
- **CI:** GHA writes a secret to a temp file and sets `ANSIBLE_VAULT_PASSWORD_FILE`.
- `ansible.cfg` wires the default so commands stay short.

`env.j2` renders `/opt/spades/.env` from these — OAuth/SMTP config becomes
declarative and versioned (encrypted); editing a secret + re-deploying actually
updates the server.

**Convention:** SSH **private** keys and the vault password itself never go into
the repo — only app/service secrets go into Vault. The deploy SSH key and vault
password stay as GHA secrets / local files, so the committed encrypted blob is
safe even if the vault password is rotated independently of host access.

### CI integration

The `lint`/`ci`/`e2e`/`coverage`/`audit` jobs stay unchanged. The `ship` job's
inline bash collapses to a thin Ansible trigger:

```yaml
ship:
  needs: [lint, ci, e2e, coverage]
  if: github.ref == 'refs/heads/master'
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: docker/setup-buildx-action@v3        # control-node build
    - uses: pnpm/action-setup@v4                 # control-node frontend build
    - uses: actions/setup-node@v4
    - run: pipx install ansible-core
    - name: Write vault password + SSH key
      run: |
        echo "${{ secrets.ANSIBLE_VAULT_PASSWORD }}" > ~/.vault-pass
        mkdir -p ~/.ssh && echo "${{ secrets.DEPLOY_SSH_KEY }}" > ~/.ssh/id_deploy
        chmod 600 ~/.ssh/id_deploy ~/.vault-pass
        echo "${{ secrets.DEPLOY_KNOWN_HOSTS }}" > ~/.ssh/known_hosts
    - name: Deploy
      working-directory: ansible
      env:
        ANSIBLE_VAULT_PASSWORD_FILE: ~/.vault-pass
      run: ansible-playbook deploy.yml -e image_tag=${GITHUB_SHA::7}
```

**GHA secrets after migration:**
- **Kept:** `DEPLOY_SSH_KEY`, `DEPLOY_KNOWN_HOSTS`, `DEPLOY_HOST` (now inventory
  data; could move into inventory).
- **New:** `ANSIBLE_VAULT_PASSWORD` (single key unlocking everything).
- **Removed from GHA, now in Vault:** `CLOUDFLARE_API_TOKEN`,
  `CLOUDFLARE_ACCOUNT_ID`, and the ghcr/OAuth/SMTP values.

Deploy logic is now identical in CI and on a laptop (`ansible-playbook
deploy.yml`), and reviewable as structured tasks instead of heredoc bash.

## Migration & cleanup

Ordered so nothing breaks mid-flight; old and new run in parallel until proven.

1. **Build `ansible/` alongside the existing system** — write roles/playbooks,
   port `docker-compose.yml`/`Caddyfile`/`env.template` to `.j2` templates. The
   current GHA `ship` job keeps working untouched.
2. **Populate Vault** — create + encrypt `vault.yml` with the
   Cloudflare/ghcr/OAuth/SMTP values and the Origin cert+key. Add
   `ANSIBLE_VAULT_PASSWORD` to GHA secrets.
3. **Dry-run against prod** — `provision.yml --check --diff` then `deploy.yml
   --check --diff`. The already-converged server should show near-zero changes;
   templated files should diff cleanly. Proof the port is faithful before any
   mutation.
4. **First real run** — `provision.yml` (converges host), then `deploy.yml` for
   an actual deploy from the laptop. Confirm health + smoke checks.
5. **Cut over CI** — replace the `ship` job with the Ansible trigger. Push a
   trivial commit; confirm CI deploys identically.
6. **Delete old plumbing** (after two green CI deploys):
   - `deploy/install-docker.sh` (→ `common` role)
   - `deploy/Caddyfile`, `docker-compose.yml`, `deploy/env.template` (→ templates)
   - Fold `deploy/origin-certs.md` + `SERVER.md` deploy steps into
     `ansible/README.md` (now mostly automated)
   - Drop unused GHA secrets (`CLOUDFLARE_*`)
   - `web/scripts/deploy-cf-pages.sh` (already archived/legacy)

Net: `deploy/` largely disappears; `ansible/` is the single home for everything
deploy-related.

## Testing & verification

1. **Static gates (CI on every PR):** `ansible-lint`, `ansible-playbook
   --syntax-check` (both playbooks), `yamllint`, and a vault sanity check
   (`ansible-vault view` succeeds — CI-only throwaway, not the real gating
   password).
2. **`--check --diff` dry runs** — primary correctness tool; run against prod
   before mutating changes. Wired as a manual `workflow_dispatch` job for
   on-demand dry-runs from the Actions tab.
3. **Idempotency check** — run `deploy.yml` twice; second run reports
   `changed=0` for config tasks (templates, dirs, certs). Explicit step in the
   migration's first-run validation.
4. **Health + smoke gates** — already in `deploy.yml`: `/health` poll fails the
   play if the container never goes healthy; post-deploy curls on `app.wlim.dev`
   + `spades.wlim.dev/health` are the e2e proof. Stronger than today — a red gate
   aborts with a clear failed task, not a buried bash exit code.
5. **Molecule** — out of scope. Container-based role testing is overkill for a
   single-host deploy; the `--check`/idempotency/health trio covers it. Noted in
   README as a future option.

## Out of scope

- Staging environment (inventory is future-ready but not stood up now).
- Molecule role testing.
- Moving the image build off the control node / onto the VPS.
- VPS-build (no-registry) flow — ghcr stays the artifact store.
```
