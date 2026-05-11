#!/usr/bin/env bash
# One-time setup for the deploy host. Run as a user with sudo. Assumes Debian/Ubuntu.
#
# Usage: bash setup.sh
#
# After this finishes, generate an SSH keypair locally:
#   ssh-keygen -t ed25519 -f spades-deploy -N ""
# Append `spades-deploy.pub` to /home/deploy/.ssh/authorized_keys on the server,
# and set GitHub repo secrets:
#   DEPLOY_HOST       = <server hostname or IP>
#   DEPLOY_USER       = deploy
#   DEPLOY_SSH_KEY    = <contents of spades-deploy (private key)>
set -euo pipefail

REPO_URL="${REPO_URL:-https://github.com/wlim33/rust-spades.git}"
DEPLOY_USER="${DEPLOY_USER:-deploy}"
INSTALL_DIR="${INSTALL_DIR:-/opt/spades-server}"
DATA_DIR="${DATA_DIR:-/var/lib/spades}"

echo "==> Installing build prerequisites"
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libssl-dev curl git

echo "==> Creating deploy user (if missing)"
if ! id -u "$DEPLOY_USER" >/dev/null 2>&1; then
    sudo adduser --system --group --shell /bin/bash --home "/home/$DEPLOY_USER" "$DEPLOY_USER"
    sudo mkdir -p "/home/$DEPLOY_USER/.ssh"
    sudo chown "$DEPLOY_USER:$DEPLOY_USER" "/home/$DEPLOY_USER/.ssh"
    sudo chmod 700 "/home/$DEPLOY_USER/.ssh"
fi

echo "==> Cloning $REPO_URL into $INSTALL_DIR"
sudo mkdir -p "$INSTALL_DIR"
sudo chown "$DEPLOY_USER:$DEPLOY_USER" "$INSTALL_DIR"
if [ ! -d "$INSTALL_DIR/.git" ]; then
    sudo -u "$DEPLOY_USER" git clone "$REPO_URL" "$INSTALL_DIR"
fi

echo "==> Installing Rust for $DEPLOY_USER (if missing)"
if ! sudo -u "$DEPLOY_USER" test -x "/home/$DEPLOY_USER/.cargo/bin/cargo"; then
    sudo -u "$DEPLOY_USER" bash -c 'curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable --profile minimal --no-modify-path'
fi

echo "==> First build (release)"
sudo -u "$DEPLOY_USER" bash -c "export PATH=\"\$HOME/.cargo/bin:\$PATH\" && cd $INSTALL_DIR && cargo build --release -p spades-server"

echo "==> Creating data dir $DATA_DIR"
sudo mkdir -p "$DATA_DIR"
sudo chown "$DEPLOY_USER:$DEPLOY_USER" "$DATA_DIR"

echo "==> Installing systemd unit"
sudo cp "$INSTALL_DIR/deploy/spades-server.service" /etc/systemd/system/spades-server.service
sudo systemctl daemon-reload
sudo systemctl enable spades-server

echo "==> Granting passwordless 'systemctl restart spades-server' to $DEPLOY_USER"
SUDOERS_FILE="/etc/sudoers.d/spades-deploy"
echo "$DEPLOY_USER ALL=(root) NOPASSWD: /bin/systemctl restart spades-server, /bin/systemctl is-active spades-server" \
    | sudo tee "$SUDOERS_FILE" >/dev/null
sudo chmod 440 "$SUDOERS_FILE"

echo "==> Starting spades-server"
sudo systemctl start spades-server
sudo systemctl --no-pager status spades-server | head -n 15

cat <<EOF

==> Done.

Next steps (on your laptop, not the server):
  1. ssh-keygen -t ed25519 -f spades-deploy -N ""
  2. Append spades-deploy.pub to /home/$DEPLOY_USER/.ssh/authorized_keys on the server
  3. Set GitHub repo secrets:
       DEPLOY_HOST    = <this server's hostname or IP>
       DEPLOY_USER    = $DEPLOY_USER
       DEPLOY_SSH_KEY = (paste contents of spades-deploy, the private key)
  4. Push to master. The Deploy workflow should pull + rebuild + restart.

The service listens on port 3000. If you want CORS, run:
  sudo systemctl edit spades-server
and add e.g.:
  [Service]
  Environment=CORS_ALLOW_ORIGIN=https://your.app
then 'sudo systemctl restart spades-server'.
EOF
