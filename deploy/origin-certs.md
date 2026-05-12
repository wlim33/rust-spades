# Cloudflare Origin CA cert for spades.wlim.dev

The VPS's Caddy reverse proxy terminates TLS using a Cloudflare Origin CA
certificate. Origin CA certs are signed by Cloudflare's private CA — they
only validate when traffic arrives via the Cloudflare proxy. That's the
right trade for us: no ACME automation, 15-year validity, no port 80
required.

If you ever need to reach the origin without Cloudflare's proxy (gray
cloud, direct IP), the cert will not validate in a browser. Use
`curl --resolve` or `openssl s_client` for direct-origin debugging.

## Generating the cert

1. Cloudflare dashboard → SSL/TLS → Origin Server → Create Certificate.
2. Hostnames: `spades.wlim.dev` (add `*.wlim.dev` if you'll host more
   services on this VPS later).
3. Validity: 15 years.
4. Key type: ECDSA (preferred) or RSA 2048.
5. Download the certificate (PEM) and private key (PEM).

## Installing on the VPS

From a workstation that has the files:

```bash
scp spades.wlim.dev.pem myuser@<vps>:/tmp/
scp spades.wlim.dev.key myuser@<vps>:/tmp/
ssh myuser@<vps>
sudo install -d -m 0700 -o deploy -g deploy /opt/spades/certs
sudo install -m 0640 -o deploy -g deploy /tmp/spades.wlim.dev.pem /opt/spades/certs/
sudo install -m 0600 -o deploy -g deploy /tmp/spades.wlim.dev.key /opt/spades/certs/
rm /tmp/spades.wlim.dev.{pem,key}
```

The `caddy` service mounts `/opt/spades/certs` read-only at
`/etc/caddy/certs` and reads `spades.wlim.dev.pem` / `spades.wlim.dev.key`
from there.

## Cloudflare SSL/TLS mode

After the cert is installed and a deploy lands, set SSL/TLS mode to
**Full (strict)** in the Cloudflare dashboard (SSL/TLS → Overview).
Anything weaker leaves the cf→origin leg unencrypted (Flexible) or
unverified (Full).

## Renewing

Origin CA certs are valid for 15 years. There's nothing to automate.
When the cert nears expiry (set a calendar reminder), repeat the
generate-and-install steps above and `docker compose restart caddy`.
