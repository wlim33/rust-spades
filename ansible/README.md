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
  (gitignored); `ansible.cfg` reads it by default, and `ANSIBLE_VAULT_PASSWORD_FILE`
  overrides it (CI sets that env var).

First run from a laptop: `host_key_checking` is on, so add the host key to
`~/.ssh/known_hosts` before the first `provision.yml` (e.g.
`ssh-keyscan <origin-ip> >> ~/.ssh/known_hosts`) or the connection will prompt.

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

CI runs the same command from the `ship` job in `.github/workflows/deploy.yml`
(after the migration cutover); `.github/workflows/ansible.yml` runs the static
checks on PRs and the manual `--check` dry-run. Until the cutover lands, the old
`ship` job is still the live deploy path.

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
