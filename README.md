# spades-ts

TypeScript SPA front-end for the [rust-spades](https://github.com/wlim/rust-spades) game server.

## Status

Functional. Anonymous Quick Play / Play with Computers / Play with Friends all work end-to-end, plus email-password + OAuth (Google / GitHub) auth, profile pages, and game history. Design and plans are in `docs/superpowers/specs/` and `docs/superpowers/plans/`.

## Dev

```sh
pnpm install
pnpm dev        # http://localhost:5173
```

Requires a running rust-spades server at `VITE_API_URL` (defaults to `http://localhost:3000` in dev).

```sh
cd ../rust-spades
cargo run -p spades-server -- --port 3000 --insecure-cookies \
  --cors-allow-origin http://localhost:5173
```

## Scripts

|                 |                                         |
| --------------- | --------------------------------------- |
| `pnpm dev`      | Vite dev server                         |
| `pnpm build`    | Type-check + production build → `dist/` |
| `pnpm preview`  | Serve the production build locally      |
| `pnpm test`     | Unit + component tests                  |
| `pnpm test:e2e` | Playwright end-to-end tests             |
| `pnpm lint`     | ESLint                                  |
| `pnpm format`   | Prettier write                          |

## Deploy

Production bundle is plain static files; serve from the same origin as `rust-spades` to avoid CORS and cookie-domain issues.

Two ways to host:

1. **rust-spades serves static** (recommended): run rust-spades with `--static-dir /srv/spades/public`. The server falls back to `index.html` for unknown paths that aren't API routes.

2. **Reverse proxy in front** (Caddy / nginx): serve `/srv/spades/public` for `/`, proxy `/games`, `/auth`, `/users`, `/matchmaking`, `/challenges`, `/player`, `/openapi.json` to rust-spades.

Either way, deploy with:

```sh
DEPLOY_HOST=wlim@spades.wlim.dev DEPLOY_PATH=/srv/spades/public ./scripts/deploy.sh
```

The script builds locally, ships via rsync, and swaps atomically.
