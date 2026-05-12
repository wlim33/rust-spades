#!/usr/bin/env bash
# One-time setup (idempotent) for the deploy host. Run as a user with sudo.
# Assumes Debian/Ubuntu.
#
# Usage: bash setup.sh
#
# Re-run safely after pulling changes to pick up systemd-unit updates.
# See the "Next steps" epilogue at the end for GitHub Actions wiring.
set -euo pipefail

DEPLOY_USER="${DEPLOY_USER:-deploy}"
INSTALL_DIR="${INSTALL_DIR:-/opt/spades-server}"
DATA_DIR="${DATA_DIR:-/var/lib/spades}"
ENV_FILE="${ENV_FILE:-/etc/spades/env}"

# The script lives at <repo>/deploy/setup.sh; everything it installs lives
# alongside it. We don't need the rest of the source tree on the VPS.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "==> Installing runtime prerequisites"
sudo apt-get update
sudo apt-get install -y curl ca-certificates

echo "==> Creating deploy user (if missing)"
if ! id -u "$DEPLOY_USER" >/dev/null 2>&1; then
    sudo adduser --system --group --shell /bin/bash --home "/home/$DEPLOY_USER" "$DEPLOY_USER"
    sudo mkdir -p "/home/$DEPLOY_USER/.ssh"
    sudo chown "$DEPLOY_USER:$DEPLOY_USER" "/home/$DEPLOY_USER/.ssh"
    sudo chmod 700 "/home/$DEPLOY_USER/.ssh"
fi

echo "==> Creating $INSTALL_DIR/bin"
sudo mkdir -p "$INSTALL_DIR/bin"
sudo chown -R "$DEPLOY_USER:$DEPLOY_USER" "$INSTALL_DIR"

echo "==> Creating data dir $DATA_DIR"
sudo mkdir -p "$DATA_DIR"
sudo chown "$DEPLOY_USER:$DEPLOY_USER" "$DATA_DIR"

echo "==> Creating $ENV_FILE (if missing)"
sudo mkdir -p "$(dirname "$ENV_FILE")"
if [ ! -f "$ENV_FILE" ]; then
    sudo install -m 0640 -o root -g "$DEPLOY_USER" "$SCRIPT_DIR/env.template" "$ENV_FILE"
    echo "    -- wrote template to $ENV_FILE; edit it with your real secrets before restarting."
else
    echo "    -- $ENV_FILE already exists; leaving it alone."
fi

echo "==> Installing systemd unit"
sudo cp "$SCRIPT_DIR/spades-server.service" /etc/systemd/system/spades-server.service

# Remove the legacy CORS drop-in if it exists — env file replaces it.
sudo rm -f /etc/systemd/system/spades-server.service.d/cors.conf
sudo rmdir /etc/systemd/system/spades-server.service.d 2>/dev/null || true

sudo systemctl daemon-reload
sudo systemctl enable spades-server

echo "==> Granting passwordless 'systemctl restart spades-server' to $DEPLOY_USER"
SUDOERS_FILE="/etc/sudoers.d/spades-deploy"
echo "$DEPLOY_USER ALL=(root) NOPASSWD: /bin/systemctl restart spades-server, /bin/systemctl is-active spades-server" \
    | sudo tee "$SUDOERS_FILE" >/dev/null
sudo chmod 440 "$SUDOERS_FILE"

cat <<EOF

==> Done.

Next steps:
  1. Edit $ENV_FILE with your real secrets:
       sudo -e $ENV_FILE
  2. Add your GitHub Actions deploy public key to:
       /home/$DEPLOY_USER/.ssh/authorized_keys
  3. After the first GitHub Actions deploy lands a binary in
     $INSTALL_DIR/bin/, the server will start. Trigger manually with:
       sudo systemctl restart spades-server

EOF
