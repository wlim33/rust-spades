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
    # shellcheck source=/dev/null
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
  2. Install the Cloudflare Origin CA cert and key into $SPADES_DIR/certs/.
     See deploy/origin-certs.md for instructions.
  3. Add the GitHub Actions deploy public key to:
       /home/$DEPLOY_USER/.ssh/authorized_keys
  4. The first push to master triggers the workflow. The workflow ssh's
     in and runs (from $SPADES_DIR):
       docker compose pull && docker compose up -d

EOF
