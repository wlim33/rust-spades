# Ansible Deployment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the hand-rolled VPS bootstrap script and the inline `ssh`/`scp`/`docker compose`/`wrangler` glue in GitHub Actions with a single Ansible pipeline (`provision.yml` + `deploy.yml`) that runs identically from CI and from a laptop.

**Architecture:** A new top-level `ansible/` directory holds two playbooks. `provision.yml` (the `common` role) bootstraps a host: Docker, the `deploy` user, directories, ghcr login. `deploy.yml` runs three plays — build+push the image on the control node → render config and converge the stack on the VPS (`backend` role) → build and ship the frontend from the control node (`frontend` role). Ansible Vault holds app/service secrets in-repo (encrypted); the origin IP, SSH key, and vault password stay as environment/GHA secrets and are never committed.

**Tech Stack:** ansible-core (pure `ansible.builtin` modules + `command` for docker verbs — no extra collections), Docker + buildx, Caddy, Cloudflare Pages (`wrangler`), GitHub Actions.

---

## Important context for the executor

**Two classes of task in this plan:**

- **BUILD tasks (Phases 1–4, 9):** create/lint files. Fully doable by an automated agent with no production access. They use a throwaway **dev vault password** (`ansible/.vault-pass-dev`, gitignored) and **placeholder** secret values so `ansible-lint` / `--syntax-check` pass.
- **OPERATOR tasks (Phases 5–8), marked `[OPERATOR]`:** require the real vault password, real secret values, SSH access to the live VPS, and/or mutating production. An automated subagent must NOT attempt these — they need a human with the credentials. Claude assists, the operator runs them.

**Secret split (do not deviate):**
- **In-repo, encrypted (Ansible Vault, `group_vars/all/vault.yml`):** ghcr token, Cloudflare API token + account ID, Google/GitHub OAuth, SMTP, and the Origin CA **cert + key**.
- **Never in repo (env vars / GHA secrets):** `DEPLOY_HOST` (origin IP — kept secret because Cloudflare hides the origin), `DEPLOY_SSH_KEY`, `DEPLOY_KNOWN_HOSTS`, `ANSIBLE_VAULT_PASSWORD`.

**Control-node prerequisites** (the machine running the playbook — GHA runner or laptop): `ansible-core`, Docker + buildx, Node + `pnpm`/`npx`, `ssh`. Documented in `ansible/README.md` (Task 18).

**Old system stays live the entire time.** Nothing in the current GitHub Actions `ship` job is touched until Phase 7. Phases 1–6 build and validate the new path in parallel.

---

## File structure

```
ansible/
  ansible.cfg                          # Task 1
  .gitignore                           # Task 1
  inventory/production.yml             # Task 2
  group_vars/all/vars.yml              # Task 2
  group_vars/all/vault.yml             # Task 3 (encrypted)
  host_vars/spades-prod.yml            # Task 2
  provision.yml                        # Task 5
  deploy.yml                           # Task 10
  roles/
    common/tasks/main.yml              # Task 4
    common/handlers/main.yml           # Task 4
    backend/tasks/main.yml             # Task 8
    backend/handlers/main.yml          # Task 8
    backend/templates/docker-compose.yml.j2   # Task 6
    backend/templates/Caddyfile.j2            # Task 6
    backend/templates/env.j2                  # Task 7
    frontend/tasks/main.yml            # Task 9
  README.md                            # Task 18
.github/workflows/ansible.yml          # Task 11 (static checks + dispatch dry-run)
.github/workflows/deploy.yml           # Task 15 (ship job rewritten)
```

---

## Phase 1 — Scaffold (BUILD)

### Task 1: Ansible config + gitignore + dev vault password

**Files:**
- Create: `ansible/ansible.cfg`
- Create: `ansible/.gitignore`
- Create: `ansible/.yamllint`

- [ ] **Step 1: Create `ansible/ansible.cfg`**

```ini
[defaults]
inventory = inventory/production.yml
roles_path = roles
host_key_checking = True
retry_files_enabled = False
stdout_callback = yaml
nocows = True
interpreter_python = auto_silent

[ssh_connection]
pipelining = True
```

- [ ] **Step 2: Create `ansible/.gitignore`**

```gitignore
# Vault password files and anything locally decrypted are never committed.
.vault-pass
.vault-pass-dev
*.decrypted
```

- [ ] **Step 2b: Create `ansible/.yamllint`**

Relaxes line-length (long comments/folded strings) and keeps Ansible-friendly
truthy handling. `ansible-lint` auto-discovers this file.

```yaml
---
extends: default

rules:
  braces:
    max-spaces-inside: 1
  octal-values:
    forbid-implicit-octal: true
    forbid-explicit-octal: true
  line-length:
    max: 160
    level: warning
  truthy:
    allowed-values: ['true', 'false']
    check-keys: false
  comments:
    min-spaces-from-content: 1
  comments-indentation: disable
  document-start: disable
```

The `braces`/`octal-values` rules align with what `ansible-lint` expects (so it
keeps fix-mode enabled); `document-start: disable` avoids requiring `---` on
every file (the encrypted `vault.yml` can't carry one). The non-fatal
"Decryption failed" WARNINGs `ansible-lint` prints for `vault.yml` are a known
quirk and do not fail the run.

- [ ] **Step 3: Create the throwaway dev vault password (gitignored, build-phase only)**

Run:
```bash
printf 'devpass\n' > ansible/.vault-pass-dev
chmod 600 ansible/.vault-pass-dev
git check-ignore ansible/.vault-pass-dev
```
Expected: prints `ansible/.vault-pass-dev` (confirms it is ignored — it must never be committed).

- [ ] **Step 4: Verify ansible is installed**

Run: `cd ansible && ansible --version`
Expected: prints `ansible [core 2.1x...]`. If missing: `pipx install ansible-core`.

- [ ] **Step 5: Commit**

```bash
git add ansible/ansible.cfg ansible/.gitignore
git commit -m "chore(ansible): scaffold config and gitignore"
```

---

### Task 2: Inventory and non-secret variables

**Files:**
- Create: `ansible/inventory/production.yml`
- Create: `ansible/group_vars/all/vars.yml`
- Create: `ansible/host_vars/spades-prod.yml`

- [ ] **Step 1: Create `ansible/inventory/production.yml`**

The origin IP is supplied at runtime via `DEPLOY_HOST` — never hardcoded.

```yaml
all:
  children:
    production:
      hosts:
        spades-prod:
          ansible_host: "{{ lookup('env', 'DEPLOY_HOST') }}"
          ansible_user: deploy
```

- [ ] **Step 2: Create `ansible/group_vars/all/vars.yml`**

These are the non-secret knobs previously hardcoded across compose/Caddyfile/env.

```yaml
# --- domains / app config ---------------------------------------------------
spades_domain: spades.wlim.dev
app_domain: app.wlim.dev
cors_allow_origin: "https://app.wlim.dev"
oauth_redirect_base_url: "https://spades.wlim.dev"

# --- image -----------------------------------------------------------------
image_repo: ghcr.io/wlim33/spades
image_tag: latest          # overridden per-deploy: ansible-playbook deploy.yml -e image_tag=<sha>

# --- registry --------------------------------------------------------------
ghcr_registry: ghcr.io
ghcr_username: wlim33

# --- paths on the VPS ------------------------------------------------------
app_dir: /opt/spades
data_dir: /var/lib/spades
container_uid: 1000

# --- cloudflare pages ------------------------------------------------------
cf_pages_project: spades
cf_pages_branch: main

# --- buildx cache: empty for laptop; CI overrides with the gha backend -----
buildx_cache_args: ""
```

- [ ] **Step 3: Create `ansible/host_vars/spades-prod.yml`**

Thin today; the seam for a future second host.

```yaml
# Host-specific overrides for spades-prod. Currently none beyond inventory.
```

- [ ] **Step 4: Verify inventory parses**

Run: `cd ansible && ansible-inventory --list`
Expected: JSON listing the `production` group with host `spades-prod`. `ansible_host` shows empty (because `DEPLOY_HOST` is unset locally) — that is fine for parsing.

- [ ] **Step 5: Commit**

```bash
git add ansible/inventory ansible/group_vars/all/vars.yml ansible/host_vars
git commit -m "feat(ansible): inventory and non-secret variables"
```

---

### Task 3: Vault file with placeholder secrets

**Files:**
- Create: `ansible/group_vars/all/vault.yml` (encrypted)

- [ ] **Step 1: Write the plaintext placeholder vault to a temp file**

Create `/tmp/vault-plain.yml` with this content:

```yaml
# Encrypted with ansible-vault. Build-phase values are placeholders;
# the operator replaces them with real secrets in Phase 5 (Task 12).
vault_ghcr_token: "REPLACE_ME"
vault_cf_api_token: "REPLACE_ME"
vault_cf_account_id: "REPLACE_ME"
vault_google_oauth_client_id: ""
vault_google_oauth_client_secret: ""
vault_github_oauth_client_id: ""
vault_github_oauth_client_secret: ""
vault_smtp_host: ""
vault_smtp_port: "587"
vault_smtp_user: ""
vault_smtp_pass: ""
vault_smtp_from: ""
vault_smtp_starttls: "true"
vault_origin_cert: |
  -----BEGIN CERTIFICATE-----
  REPLACE_ME
  -----END CERTIFICATE-----
vault_origin_key: |
  -----BEGIN PRIVATE KEY-----
  REPLACE_ME
  -----END PRIVATE KEY-----
```

- [ ] **Step 2: Encrypt it into place with the dev password**

Run:
```bash
cd ansible
ansible-vault encrypt /tmp/vault-plain.yml \
  --vault-password-file .vault-pass-dev \
  --output group_vars/all/vault.yml
rm /tmp/vault-plain.yml
```
Expected: `Encryption successful`; `group_vars/all/vault.yml` now begins with `$ANSIBLE_VAULT;1.1;AES256`.

- [ ] **Step 3: Verify it decrypts and the keys are present**

Run:
```bash
cd ansible && ansible-vault view group_vars/all/vault.yml --vault-password-file .vault-pass-dev
```
Expected: prints the YAML above (placeholder values).

- [ ] **Step 4: Commit**

```bash
git add ansible/group_vars/all/vault.yml
git commit -m "feat(ansible): encrypted vault with placeholder secrets"
```

---

## Phase 2 — Provision (`common` role) (BUILD)

### Task 4: `common` role — host bootstrap

**Files:**
- Create: `ansible/roles/common/tasks/main.yml`
- Create: `ansible/roles/common/handlers/main.yml`

This is the declarative replacement for `deploy/install-docker.sh`. Every task is idempotent.

- [ ] **Step 1: Create `ansible/roles/common/handlers/main.yml`**

```yaml
- name: Reload systemd
  become: true
  ansible.builtin.systemd:
    daemon_reload: true
```

(Handler/task names use Sentence case to satisfy ansible-lint `name[casing]`;
registered vars are role-prefixed to satisfy `var-naming[no-role-prefix]`.)

- [ ] **Step 2: Create `ansible/roles/common/tasks/main.yml`**

```yaml
# --- Docker engine ---------------------------------------------------------
- name: Install prerequisites for the Docker apt repo
  become: true
  ansible.builtin.apt:
    name:
      - ca-certificates
      - curl
      - gnupg
    state: present
    update_cache: true

- name: Create apt keyrings directory
  become: true
  ansible.builtin.file:
    path: /etc/apt/keyrings
    state: directory
    mode: "0755"

- name: Install Docker's GPG key (ascii-armored, used directly via signed-by)
  become: true
  ansible.builtin.get_url:
    url: https://download.docker.com/linux/debian/gpg
    dest: /etc/apt/keyrings/docker.asc
    mode: "0644"

- name: Add the Docker apt repository
  become: true
  ansible.builtin.apt_repository:
    repo: >-
      deb [arch={{ ansible_facts.architecture | replace('x86_64', 'amd64') }}
      signed-by=/etc/apt/keyrings/docker.asc]
      https://download.docker.com/linux/debian
      {{ ansible_facts.distribution_release }} stable
    filename: docker
    state: present

- name: Install Docker Engine + compose plugin
  become: true
  ansible.builtin.apt:
    name:
      - docker-ce
      - docker-ce-cli
      - containerd.io
      - docker-buildx-plugin
      - docker-compose-plugin
    state: present
    update_cache: true

# --- deploy user -----------------------------------------------------------
- name: Create the deploy system user
  become: true
  ansible.builtin.user:
    name: "{{ ansible_user }}"
    system: true
    shell: /bin/bash
    home: "/home/{{ ansible_user }}"
    create_home: true
    groups: docker
    append: true

# --- directories -----------------------------------------------------------
- name: Create the app directory
  become: true
  ansible.builtin.file:
    path: "{{ app_dir }}"
    state: directory
    owner: "{{ ansible_user }}"
    group: "{{ ansible_user }}"
    mode: "0755"

- name: Create the certs directory
  become: true
  ansible.builtin.file:
    path: "{{ app_dir }}/certs"
    state: directory
    owner: "{{ ansible_user }}"
    group: "{{ ansible_user }}"
    mode: "0700"

- name: Create the data directory (owned by the container UID)
  become: true
  ansible.builtin.file:
    path: "{{ data_dir }}"
    state: directory
    owner: "{{ container_uid }}"
    group: "{{ container_uid }}"
    mode: "0755"

# --- ghcr login (so the VPS can pull the private image) --------------------
- name: Log the deploy user into ghcr.io
  ansible.builtin.command:
    cmd: docker login {{ ghcr_registry }} -u {{ ghcr_username }} --password-stdin
    stdin: "{{ vault_ghcr_token }}"
  register: common_ghcr_login
  changed_when: "'Login Succeeded' in common_ghcr_login.stdout"

# --- legacy cleanup (from the old bash flow) -------------------------------
- name: Disable the legacy spades-server systemd unit
  become: true
  ansible.builtin.systemd:
    name: spades-server
    state: stopped
    enabled: false
  failed_when: false

- name: Remove legacy systemd unit + sudoers artifacts
  become: true
  ansible.builtin.file:
    path: "{{ item }}"
    state: absent
  loop:
    - /etc/systemd/system/spades-server.service
    - /etc/systemd/system/spades-server.service.d
    - /etc/sudoers.d/spades-deploy
  notify: reload systemd
```

- [ ] **Step 3: Lint the role**

Run: `cd ansible && ANSIBLE_VAULT_PASSWORD_FILE=.vault-pass-dev ansible-lint roles/common`
Expected: no errors. (Warnings about `failed_when: false` are acceptable; fix any `[E...]` errors reported.)

- [ ] **Step 4: Commit**

```bash
git add ansible/roles/common
git commit -m "feat(ansible): common role for host bootstrap"
```

---

### Task 5: `provision.yml` playbook

**Files:**
- Create: `ansible/provision.yml`

- [ ] **Step 1: Create `ansible/provision.yml`**

```yaml
- name: Provision the spades host
  hosts: production
  gather_facts: true
  roles:
    - common
```

- [ ] **Step 2: Syntax-check (needs the dev vault password to load vars)**

Run:
```bash
cd ansible && ansible-playbook provision.yml --syntax-check --vault-password-file .vault-pass-dev
```
Expected: `playbook: provision.yml` with no errors.

- [ ] **Step 3: Commit**

```bash
git add ansible/provision.yml
git commit -m "feat(ansible): provision playbook"
```

---

## Phase 3 — Backend role (BUILD)

### Task 6: Compose + Caddyfile templates

**Files:**
- Create: `ansible/roles/backend/templates/docker-compose.yml.j2`
- Create: `ansible/roles/backend/templates/Caddyfile.j2`

- [ ] **Step 1: Create `ansible/roles/backend/templates/docker-compose.yml.j2`**

The image tag is now rendered directly (no `IMAGE_TAG` env indirection needed).

```yaml
services:
  spades-server:
    image: {{ image_repo }}:{{ image_tag }}
    container_name: spades-server
    restart: unless-stopped
    expose:
      - "3000"
    volumes:
      - {{ data_dir }}:/data
    env_file:
      - {{ app_dir }}/.env
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
      - {{ app_dir }}/Caddyfile:/etc/caddy/Caddyfile:ro
      - {{ app_dir }}/certs:/etc/caddy/certs:ro
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

- [ ] **Step 2: Create `ansible/roles/backend/templates/Caddyfile.j2`**

Domain and cert filenames are now variables.

```caddy
{
    auto_https off
}

{{ spades_domain }}:443 {
    tls /etc/caddy/certs/{{ spades_domain }}.pem /etc/caddy/certs/{{ spades_domain }}.key
    encode gzip
    reverse_proxy spades-server:3000 {
        header_up X-Real-IP {http.request.remote.host}
        header_up X-Forwarded-For {http.request.header.X-Forwarded-For}
        header_up X-Forwarded-Proto https
    }
}
```

- [ ] **Step 3: Commit**

```bash
git add ansible/roles/backend/templates/docker-compose.yml.j2 ansible/roles/backend/templates/Caddyfile.j2
git commit -m "feat(ansible): backend compose and Caddyfile templates"
```

---

### Task 7: `.env` template (now managed)

**Files:**
- Create: `ansible/roles/backend/templates/env.j2`

- [ ] **Step 1: Create `ansible/roles/backend/templates/env.j2`**

```jinja
# Managed by Ansible (roles/backend/templates/env.j2). Do not edit on the VPS —
# changes are overwritten on the next deploy. Source values from Ansible Vault.
CORS_ALLOW_ORIGIN={{ cors_allow_origin }}
OAUTH_REDIRECT_BASE_URL={{ oauth_redirect_base_url }}

GOOGLE_OAUTH_CLIENT_ID={{ vault_google_oauth_client_id }}
GOOGLE_OAUTH_CLIENT_SECRET={{ vault_google_oauth_client_secret }}

GITHUB_OAUTH_CLIENT_ID={{ vault_github_oauth_client_id }}
GITHUB_OAUTH_CLIENT_SECRET={{ vault_github_oauth_client_secret }}

SMTP_HOST={{ vault_smtp_host }}
SMTP_PORT={{ vault_smtp_port }}
SMTP_USER={{ vault_smtp_user }}
SMTP_PASS={{ vault_smtp_pass }}
SMTP_FROM={{ vault_smtp_from }}
SMTP_STARTTLS={{ vault_smtp_starttls }}
```

- [ ] **Step 2: Commit**

```bash
git add ansible/roles/backend/templates/env.j2
git commit -m "feat(ansible): managed .env template sourced from vault"
```

---

### Task 8: `backend` role tasks + handlers

**Files:**
- Create: `ansible/roles/backend/tasks/main.yml`
- Create: `ansible/roles/backend/handlers/main.yml`

- [ ] **Step 1: Create `ansible/roles/backend/handlers/main.yml`**

```yaml
- name: Restart caddy
  ansible.builtin.command:
    cmd: docker compose restart caddy
    chdir: "{{ app_dir }}"
  changed_when: true
```

(Sentence-case name per `name[casing]`; the matching `notify:` lines below use
the same `Restart caddy` string.)

- [ ] **Step 2: Create `ansible/roles/backend/tasks/main.yml`**

```yaml
# --- render config onto the VPS -------------------------------------------
- name: Render docker-compose.yml
  ansible.builtin.template:
    src: docker-compose.yml.j2
    dest: "{{ app_dir }}/docker-compose.yml"
    owner: "{{ ansible_user }}"
    group: "{{ ansible_user }}"
    mode: "0644"

- name: Render .env
  ansible.builtin.template:
    src: env.j2
    dest: "{{ app_dir }}/.env"
    owner: "{{ ansible_user }}"
    group: "{{ ansible_user }}"
    mode: "0640"

- name: Render Caddyfile
  ansible.builtin.template:
    src: Caddyfile.j2
    dest: "{{ app_dir }}/Caddyfile"
    owner: "{{ ansible_user }}"
    group: "{{ ansible_user }}"
    mode: "0644"
  notify: Restart caddy

# --- Origin CA cert + key from vault --------------------------------------
- name: Install the Origin CA certificate
  ansible.builtin.copy:
    content: "{{ vault_origin_cert }}"
    dest: "{{ app_dir }}/certs/{{ spades_domain }}.pem"
    owner: "{{ ansible_user }}"
    group: "{{ ansible_user }}"
    mode: "0640"
  no_log: true
  notify: Restart caddy

- name: Install the Origin CA private key
  ansible.builtin.copy:
    content: "{{ vault_origin_key }}"
    dest: "{{ app_dir }}/certs/{{ spades_domain }}.key"
    owner: "{{ ansible_user }}"
    group: "{{ ansible_user }}"
    mode: "0600"
  no_log: true
  notify: Restart caddy

# --- converge the stack ----------------------------------------------------
- name: Log into ghcr.io for the pull
  ansible.builtin.command:
    cmd: docker login {{ ghcr_registry }} -u {{ ghcr_username }} --password-stdin
    stdin: "{{ vault_ghcr_token }}"
  changed_when: false
  no_log: true

- name: Pull the pinned image
  ansible.builtin.command:
    cmd: docker compose pull
    chdir: "{{ app_dir }}"
  changed_when: true

- name: Bring the stack up
  ansible.builtin.command:
    cmd: docker compose up -d --remove-orphans
    chdir: "{{ app_dir }}"
  changed_when: true

# --- health gate -----------------------------------------------------------
- name: Wait for the backend to report healthy
  ansible.builtin.uri:
    url: http://127.0.0.1:3000/health
    status_code: 200
  register: health
  retries: 12
  delay: 5
  until: health.status == 200
```

- [ ] **Step 3: Lint the role**

Run: `cd ansible && ANSIBLE_VAULT_PASSWORD_FILE=.vault-pass-dev ansible-lint roles/backend`
Expected: no `[E...]` errors.

- [ ] **Step 4: Commit**

```bash
git add ansible/roles/backend/tasks ansible/roles/backend/handlers
git commit -m "feat(ansible): backend role converges the stack with health gate"
```

---

## Phase 4 — Build + frontend + deploy playbook (BUILD)

### Task 9: `frontend` role

**Files:**
- Create: `ansible/roles/frontend/tasks/main.yml`

Runs on the control node (`localhost`). `frontend_repo_root` is computed from
`playbook_dir` (role-prefixed to satisfy ansible-lint `var-naming`).

- [ ] **Step 1: Create `ansible/roles/frontend/tasks/main.yml`**

```yaml
- name: Resolve the repository root
  ansible.builtin.set_fact:
    frontend_repo_root: "{{ playbook_dir }}/.."

- name: Install web dependencies
  ansible.builtin.command:
    cmd: pnpm install --frozen-lockfile
    chdir: "{{ frontend_repo_root }}/web"
  changed_when: true

- name: Build the frontend bundle
  ansible.builtin.command:
    cmd: pnpm build
    chdir: "{{ frontend_repo_root }}/web"
  changed_when: true

- name: Deploy to Cloudflare Pages
  ansible.builtin.command:
    cmd: >-
      npx --yes wrangler pages deploy web/dist
      --project-name={{ cf_pages_project }}
      --branch={{ cf_pages_branch }}
      --commit-dirty=true
    chdir: "{{ frontend_repo_root }}"
  environment:
    CLOUDFLARE_API_TOKEN: "{{ vault_cf_api_token }}"
    CLOUDFLARE_ACCOUNT_ID: "{{ vault_cf_account_id }}"
  changed_when: true
  no_log: true

- name: Smoke-check the frontend
  ansible.builtin.uri:
    url: "https://{{ app_domain }}/"
    status_code: 200

- name: Smoke-check the backend health endpoint
  ansible.builtin.uri:
    url: "https://{{ spades_domain }}/health"
    status_code: 200
```

- [ ] **Step 2: Lint the role**

Run: `cd ansible && ANSIBLE_VAULT_PASSWORD_FILE=.vault-pass-dev ansible-lint roles/frontend`
Expected: no `[E...]` errors.

- [ ] **Step 3: Commit**

```bash
git add ansible/roles/frontend
git commit -m "feat(ansible): frontend role builds and deploys to cloudflare pages"
```

---

### Task 10: `deploy.yml` — three plays

**Files:**
- Create: `ansible/deploy.yml`

- [ ] **Step 1: Create `ansible/deploy.yml`**

```yaml
# Play 1 — build and push the image from the control node.
- name: Build and push the backend image
  hosts: localhost
  connection: local
  gather_facts: false
  tags: [build]
  tasks:
    - name: Log into ghcr.io
      ansible.builtin.command:
        cmd: docker login {{ ghcr_registry }} -u {{ ghcr_username }} --password-stdin
        stdin: "{{ vault_ghcr_token }}"
      changed_when: false
      no_log: true

    - name: Buildx build and push (sha + latest tags)
      ansible.builtin.command:
        cmd: >-
          docker buildx build
          --tag {{ image_repo }}:{{ image_tag }}
          --tag {{ image_repo }}:latest
          {{ buildx_cache_args }}
          --push
          {{ playbook_dir }}/..
      changed_when: true

# Play 2 — converge the backend stack on the VPS.
- name: Deploy the backend
  hosts: production
  gather_facts: true
  tags: [backend]
  roles:
    - backend

# Play 3 — build and ship the frontend from the control node.
- name: Deploy the frontend
  hosts: localhost
  connection: local
  gather_facts: false
  tags: [frontend]
  roles:
    - frontend
```

- [ ] **Step 2: Syntax-check**

Run:
```bash
cd ansible && ansible-playbook deploy.yml --syntax-check --vault-password-file .vault-pass-dev
```
Expected: `playbook: deploy.yml` with no errors.

- [ ] **Step 3: Lint the whole tree**

Run: `cd ansible && ANSIBLE_VAULT_PASSWORD_FILE=.vault-pass-dev ansible-lint`
Expected: `Passed` / no `[E...]` errors.

- [ ] **Step 4: Commit**

```bash
git add ansible/deploy.yml
git commit -m "feat(ansible): deploy playbook (build -> backend -> frontend)"
```

---

### Task 11: CI workflow for Ansible static checks + manual dry-run

**Files:**
- Create: `.github/workflows/ansible.yml`

- [ ] **Step 1: Create `.github/workflows/ansible.yml`**

The `dry-run` job is `workflow_dispatch`-only and uses real secrets to run `--check --diff` against prod without mutating it.

```yaml
name: ansible

on:
  pull_request:
    branches: [master]
    paths: ['ansible/**', '.github/workflows/ansible.yml']
  workflow_dispatch:

jobs:
  static:
    name: lint + syntax
    runs-on: ubuntu-latest
    timeout-minutes: 10
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-python@v5
        with:
          python-version: '3.12'
      - name: Install ansible + linters
        run: pipx install ansible-core && pipx inject ansible-core ansible-lint && pip install yamllint
      - name: yamllint
        run: yamllint -c ansible/.yamllint ansible
      - name: Write CI vault password
        run: echo "${{ secrets.ANSIBLE_VAULT_PASSWORD }}" > "$RUNNER_TEMP/vault-pass"
      - name: ansible-lint
        working-directory: ansible
        env:
          ANSIBLE_VAULT_PASSWORD_FILE: ${{ runner.temp }}/vault-pass
        run: ansible-lint
      - name: syntax-check
        working-directory: ansible
        env:
          ANSIBLE_VAULT_PASSWORD_FILE: ${{ runner.temp }}/vault-pass
        run: |
          ansible-playbook provision.yml --syntax-check
          ansible-playbook deploy.yml --syntax-check

  dry-run:
    name: deploy --check (manual)
    if: github.event_name == 'workflow_dispatch'
    runs-on: ubuntu-latest
    timeout-minutes: 15
    steps:
      - uses: actions/checkout@v4
      - run: pipx install ansible-core
      - name: SSH + vault setup
        run: |
          mkdir -p ~/.ssh
          echo "${{ secrets.DEPLOY_SSH_KEY }}" > ~/.ssh/id_deploy && chmod 600 ~/.ssh/id_deploy
          echo "${{ secrets.DEPLOY_KNOWN_HOSTS }}" > ~/.ssh/known_hosts
          echo "${{ secrets.ANSIBLE_VAULT_PASSWORD }}" > "$RUNNER_TEMP/vault-pass"
      - name: ansible-playbook deploy.yml --check --diff --tags backend
        working-directory: ansible
        env:
          DEPLOY_HOST: ${{ secrets.DEPLOY_HOST }}
          ANSIBLE_VAULT_PASSWORD_FILE: ${{ runner.temp }}/vault-pass
          ANSIBLE_PRIVATE_KEY_FILE: ~/.ssh/id_deploy
        run: ansible-playbook deploy.yml --check --diff --tags backend
```

Note: `--check` skips the `command`-based docker tasks (they only run when not in check mode is not automatic — see Task 17 for the explicit guard added before enabling this job against prod). For the first introduction this job is wired but only exercised after Task 17.

- [ ] **Step 2: yamllint locally**

Run: `yamllint -c ansible/.yamllint ansible .github/workflows/ansible.yml`
Expected: no errors. (If `yamllint` is missing: `brew install yamllint`.)

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ansible.yml
git commit -m "ci(ansible): static checks on PRs and manual dry-run"
```

---

## Phase 5 — Vault population [OPERATOR]

> **[OPERATOR]** These steps need the real vault password and real secret values. An automated agent must not perform them.

### Task 12: Replace placeholders with real secrets and re-key to the real password

**Files:**
- Modify: `ansible/group_vars/all/vault.yml` (re-encrypted)

- [ ] **Step 1: Choose the real vault password and store it locally (gitignored)**

```bash
printf '%s\n' 'YOUR-REAL-VAULT-PASSWORD' > ansible/.vault-pass
chmod 600 ansible/.vault-pass
```

- [ ] **Step 2: Edit the vault with real values**

Run: `cd ansible && ansible-vault edit group_vars/all/vault.yml --vault-password-file .vault-pass-dev`

Fill in real values for every `REPLACE_ME`:
- `vault_ghcr_token`: a GitHub PAT with `read:packages` **and** `write:packages` (push happens from the control node, pull on the VPS).
- `vault_cf_api_token`: Cloudflare token with Pages:Edit.
- `vault_cf_account_id`: Cloudflare account ID.
- `vault_origin_cert` / `vault_origin_key`: the full PEM blocks of the Cloudflare Origin CA cert + key (generate per the old `deploy/origin-certs.md` if you don't have them).
- OAuth/SMTP: real values if used, else leave empty strings.

- [ ] **Step 3: Re-key the vault from the dev password to the real password**

```bash
cd ansible && ansible-vault rekey group_vars/all/vault.yml \
  --vault-password-file .vault-pass-dev \
  --new-vault-password-file .vault-pass
```
Expected: `Rekey successful`.

- [ ] **Step 4: Delete the dev password file — it must not unlock prod secrets anymore**

```bash
rm ansible/.vault-pass-dev
```

- [ ] **Step 5: Verify the real password decrypts and the dev one no longer exists**

```bash
cd ansible && ansible-vault view group_vars/all/vault.yml --vault-password-file .vault-pass | head -5
```
Expected: real (non-placeholder) values print.

- [ ] **Step 6: Commit the re-encrypted vault**

```bash
git add ansible/group_vars/all/vault.yml
git commit -m "chore(ansible): real secrets in vault (re-keyed)"
```

- [ ] **Step 7: Add CI secrets in GitHub**

In the repo settings → Secrets and variables → Actions, add/confirm:
- `ANSIBLE_VAULT_PASSWORD` = the real vault password from Step 1.
- `DEPLOY_HOST`, `DEPLOY_SSH_KEY`, `DEPLOY_KNOWN_HOSTS` = (already exist from the old workflow — leave them).

---

## Phase 6 — Validate against prod [OPERATOR]

> **[OPERATOR]** Requires SSH access to the live VPS. The old GitHub Actions deploy is still the source of truth; these steps must not break it.

### Task 13: Dry-run and faithful-port verification

- [ ] **Step 1: Export the connection env locally**

```bash
export DEPLOY_HOST=<origin-ip>
export ANSIBLE_VAULT_PASSWORD_FILE="$PWD/ansible/.vault-pass"
```

- [ ] **Step 2: Dry-run provision (host is already converged → expect near-zero changes)**

Run: `cd ansible && ansible-playbook provision.yml --check --diff`
Expected: completes; `changed` count is small (Docker already installed, user exists, dirs exist). Review every reported change — a surprise change means the role diverges from the live host; reconcile before continuing.

- [ ] **Step 3: Dry-run the backend deploy against the live config**

Run: `cd ansible && ansible-playbook deploy.yml --check --diff --tags backend -e image_tag=$(git rev-parse --short=12 HEAD)`
Expected: the `--diff` for `docker-compose.yml`, `.env`, and `Caddyfile` shows only formatting/whitespace deltas against what is already on `/opt/spades`. A semantic diff (a changed value, a missing line) means the template is not a faithful port — fix the template and re-run. This is the core proof before any mutation.

- [ ] **Step 4: Record findings**

Note any intentional diffs (e.g. the `.env` is now fully managed vs. hand-edited). Confirm they are acceptable before the first real run.

---

### Task 14: First real run from the laptop

- [ ] **Step 1: Provision for real (idempotent; converges the host)**

Run: `cd ansible && ansible-playbook provision.yml`
Expected: completes green.

- [ ] **Step 2: Real deploy from the laptop**

Run: `cd ansible && ansible-playbook deploy.yml -e image_tag=$(git rev-parse --short=12 HEAD)`
Expected: image builds + pushes, backend converges, health gate passes, frontend deploys, both smoke checks return 200.

- [ ] **Step 3: Idempotency check (spec requirement) — config tasks report `changed=0`**

Run: `cd ansible && ansible-playbook deploy.yml -e image_tag=$(git rev-parse --short=12 HEAD) --tags backend`
Expected: in the recap, the three `template` tasks and the two cert `copy` tasks report **ok** (not changed). The docker `command` tasks are expected to report changed (they are actions, not config) — that matches the spec's "changed=0 for config tasks (templates, dirs, certs)" scope.

- [ ] **Step 4: Confirm the live site**

```bash
curl -fsS https://app.wlim.dev/ >/dev/null && echo app-ok
curl -fsS https://spades.wlim.dev/health >/dev/null && echo health-ok
```
Expected: `app-ok` and `health-ok`.

---

## Phase 7 — CI cutover [OPERATOR]

### Task 15: Replace the GitHub Actions `ship` job with the Ansible trigger

**Files:**
- Modify: `.github/workflows/deploy.yml` (replace the `ship` job only; leave `lint`/`ci`/`e2e`/`coverage`/`audit` untouched)

- [ ] **Step 1: Replace the entire `ship:` job in `.github/workflows/deploy.yml` with:**

```yaml
  ship:
    name: ship
    needs: [lint, ci, e2e, coverage]
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

      - uses: pnpm/action-setup@v4
        with:
          package_json_file: web/package.json
          run_install: false
      - uses: actions/setup-node@v4
        with:
          node-version: '20'
          cache: pnpm
          cache-dependency-path: web/pnpm-lock.yaml

      - name: Install ansible-core
        run: pipx install ansible-core

      - name: SSH + vault setup
        run: |
          mkdir -p ~/.ssh
          echo "${{ secrets.DEPLOY_SSH_KEY }}" > ~/.ssh/id_deploy && chmod 600 ~/.ssh/id_deploy
          echo "${{ secrets.DEPLOY_KNOWN_HOSTS }}" > ~/.ssh/known_hosts
          echo "${{ secrets.ANSIBLE_VAULT_PASSWORD }}" > "$RUNNER_TEMP/vault-pass"

      - name: Deploy with Ansible
        working-directory: ansible
        env:
          DEPLOY_HOST: ${{ secrets.DEPLOY_HOST }}
          ANSIBLE_VAULT_PASSWORD_FILE: ${{ runner.temp }}/vault-pass
          ANSIBLE_PRIVATE_KEY_FILE: ~/.ssh/id_deploy
        run: |
          ansible-playbook deploy.yml \
            -e image_tag=${{ steps.sha.outputs.short }} \
            -e buildx_cache_args="--cache-from type=gha --cache-to type=gha,mode=max"
```

- [ ] **Step 2: Confirm nothing else in the file changed**

Run: `git diff .github/workflows/deploy.yml`
Expected: only the `ship` job body differs; `lint`/`ci`/`e2e`/`coverage`/`audit` are byte-for-byte unchanged.

- [ ] **Step 3: Commit on a branch and open a PR (do not push straight to master)**

```bash
git add .github/workflows/deploy.yml
git commit -m "ci: deploy via Ansible instead of inline ssh/scp glue"
```

- [ ] **Step 4: Merge and watch the first CI deploy**

After merge to `master`, watch the `ship` job. Expected: image pushed, backend converged, health gate green, frontend deployed, smoke checks pass — identical outcome to the old job.

- [ ] **Step 5: Trigger a second deploy (trivial commit) and confirm green again**

Two consecutive green CI deploys is the gate for Phase 8 deletion.

---

## Phase 8 — Delete the old plumbing [OPERATOR]

### Task 16: Remove superseded files and unused secrets

**Files:**
- Delete: `deploy/install-docker.sh`, `docker-compose.yml`, `deploy/Caddyfile`, `deploy/env.template`, `web/scripts/deploy-cf-pages.sh`
- Modify: `deploy/origin-certs.md` (fold into `ansible/README.md`, then delete) — handled in Task 18

- [ ] **Step 1: Confirm two green CI deploys have happened (Phase 7 Step 5).** Do not proceed otherwise.

- [ ] **Step 2: Delete the superseded files**

```bash
git rm deploy/install-docker.sh docker-compose.yml deploy/Caddyfile deploy/env.template web/scripts/deploy-cf-pages.sh
```

- [ ] **Step 3: Verify nothing else references them**

Run: `grep -rn -e 'install-docker.sh' -e 'deploy/Caddyfile' -e 'deploy/env.template' -e 'deploy-cf-pages.sh' --exclude-dir=.git .`
Expected: only matches inside `docs/superpowers/` (this plan/spec) and `ansible/README.md`. Any reference in `Makefile`, `SERVER.md`, or a workflow must be updated/removed in the same commit.

- [ ] **Step 4: Drop the now-unused GitHub secrets**

In repo settings → Actions secrets, delete `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID` (now in the vault).

- [ ] **Step 5: Commit**

```bash
git commit -m "chore: remove deploy plumbing superseded by ansible"
```

---

## Phase 9 — Hardening + docs (BUILD, except where noted)

### Task 17: Guard docker `command` tasks under check mode

So the `--check` dry-run (Task 11 `dry-run` job, Task 13) never tries to run docker against prod and the diff stays meaningful.

**Files:**
- Modify: `ansible/roles/backend/tasks/main.yml`
- Modify: `ansible/deploy.yml`
- Modify: `ansible/roles/frontend/tasks/main.yml`

- [ ] **Step 1: Add `check_mode: false` + skip to each docker/pnpm `command` task**

Add `when: not ansible_check_mode` to every **action** task — both the
`ansible.builtin.command` tasks (`docker ...`, `pnpm ...`, `npx ...`) AND the
`ansible.builtin.uri` health-gate / smoke-check tasks. In check mode the
container/site is not updated, so an unguarded `uri` would either hit the
network or fail against stale state and break the dry-run. Specifically guard:

- `roles/backend/tasks/main.yml`: "Log into ghcr.io for the pull", "Pull the
  pinned image", "Bring the stack up", **and** "Wait for the backend to report
  healthy" (the `uri` gate).
- `deploy.yml` Play 1: "Log into ghcr.io" and "buildx build and push".
- `roles/frontend/tasks/main.yml`: "Install web dependencies", "Build the
  frontend bundle", "Deploy to Cloudflare Pages", **and** both "Smoke-check ..."
  `uri` tasks.

Leave only the `template`, `copy`, and `file` tasks unguarded — those are the
config-state tasks whose `--check --diff` output is the whole point of the dry
run. Example for the pull task:

```yaml
- name: Pull the pinned image
  ansible.builtin.command:
    cmd: docker compose pull
    chdir: "{{ app_dir }}"
  changed_when: true
  when: not ansible_check_mode
```

- [ ] **Step 2: Re-lint**

Run: `cd ansible && ansible-lint`
Expected: no `[E...]` errors.

- [ ] **Step 3: Syntax-check both playbooks**

Run:
```bash
cd ansible && ansible-playbook deploy.yml --syntax-check && ansible-playbook provision.yml --syntax-check
```
Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add ansible/roles/backend/tasks/main.yml ansible/deploy.yml ansible/roles/frontend/tasks/main.yml
git commit -m "fix(ansible): skip docker/pnpm actions in check mode"
```

---

### Task 18: `ansible/README.md` (absorbs `origin-certs.md` + SERVER.md deploy notes)

**Files:**
- Create: `ansible/README.md`
- Delete: `deploy/origin-certs.md` (content folded in)

- [ ] **Step 1: Create `ansible/README.md`**

````markdown
# Deployment (Ansible)

Single Ansible pipeline for the spades backend (Docker Compose on a Hetzner VPS)
and frontend (Cloudflare Pages). Runs identically from CI and a laptop.

## Control-node prerequisites

- `ansible-core` (`pipx install ansible-core`)
- Docker + buildx
- Node + `pnpm` / `npx`
- `ssh`

## Secrets

- **In-repo, encrypted** (`group_vars/all/vault.yml`, Ansible Vault): ghcr token,
  Cloudflare API token + account ID, Google/GitHub OAuth, SMTP, Origin CA cert + key.
  Edit with `ansible-vault edit group_vars/all/vault.yml`.
- **Never committed** (env / GitHub secrets): `DEPLOY_HOST` (origin IP — kept secret
  because Cloudflare hides the origin), `DEPLOY_SSH_KEY`, `DEPLOY_KNOWN_HOSTS`,
  `ANSIBLE_VAULT_PASSWORD`. The vault password lives in `ansible/.vault-pass` locally
  (gitignored); point `ANSIBLE_VAULT_PASSWORD_FILE` at it.

## Provision a host (rare)

```bash
export DEPLOY_HOST=<origin-ip>
export ANSIBLE_VAULT_PASSWORD_FILE="$PWD/.vault-pass"
ansible-playbook provision.yml
```

## Deploy (frequent)

```bash
export DEPLOY_HOST=<origin-ip>
export ANSIBLE_VAULT_PASSWORD_FILE="$PWD/.vault-pass"
ansible-playbook deploy.yml -e image_tag=$(git rev-parse --short=12 HEAD)
```

CI runs the same command (see `.github/workflows/deploy.yml`).

## Rollback

```bash
ansible-playbook deploy.yml -e image_tag=<good-sha> --tags backend
```

Re-pins the backend to a previously-pushed image without rebuilding or touching
the frontend.

## Dry run

```bash
ansible-playbook deploy.yml --check --diff --tags backend
```

## Cloudflare Origin CA cert

The VPS's Caddy terminates TLS with a Cloudflare Origin CA certificate (signed by
Cloudflare's private CA; only validates behind the Cloudflare proxy — no ACME, no
port 80, 15-year validity). The cert + key live in the vault (`vault_origin_cert`,
`vault_origin_key`) and are templated to `/opt/spades/certs/spades.wlim.dev.{pem,key}`
by the `backend` role. To regenerate: Cloudflare dashboard → SSL/TLS → Origin Server
→ Create Certificate (hostname `spades.wlim.dev`, ECDSA, 15 years), then paste the PEM
blocks into the vault and redeploy. Ensure Cloudflare SSL/TLS mode is **Full (strict)**.

## Future options

- Stand up a staging environment: add `inventory/staging.yml` + a `host_vars` entry.
- Add Molecule for container-based role testing (out of scope today).
````

- [ ] **Step 2: Delete the folded-in doc**

```bash
git rm deploy/origin-certs.md
```

- [ ] **Step 3: Update any lingering references**

Run: `grep -rn 'origin-certs.md' --exclude-dir=.git . | grep -v docs/superpowers`
Expected: no matches (or update `SERVER.md` to point at `ansible/README.md`).

- [ ] **Step 4: Commit**

```bash
git add ansible/README.md
git commit -m "docs(ansible): deployment runbook; fold in origin-certs notes"
```

---

## Self-review (completed during planning)

- **Spec coverage:** repo layout (Tasks 1–3, 6–10), `common`/provision (Tasks 4–5), `backend` role incl. health gate + vault-sourced certs/.env (Tasks 6–8), build-on-control-node + frontend (Tasks 9–10), Vault model (Tasks 3, 12), CI cutover (Task 15), migration order + deletions (Tasks 13–16), testing — ansible-lint/syntax/yamllint/dry-run/idempotency/health (Tasks 11, 13, 14, 17). All spec sections map to tasks.
- **Deviation from spec (flagged to user):** the origin IP (`DEPLOY_HOST`) stays an env/GHA secret rather than moving into committed inventory, because Cloudflare proxies the domain to hide the origin — committing the IP would be a security regression. Vault holds app/service secrets only.
- **Placeholder scan:** every file step contains complete content; no TBD/TODO.
- **Type/name consistency:** variable names (`image_repo`, `image_tag`, `app_dir`, `data_dir`, `spades_domain`, `vault_*`, `buildx_cache_args`, `cf_pages_*`) are used identically across `vars.yml`, templates, roles, and playbooks.
```
