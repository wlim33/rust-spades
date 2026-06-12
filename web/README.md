# spades-ts

TypeScript SPA front-end for the [rust-spades](https://github.com/wlim33/rust-spades) game server.

## Status

Functional. Anonymous Quick Play / Play with Computers / Play with Friends all work end-to-end, plus email-password + OAuth (Google / GitHub) auth, profile pages, and game history.

## Design system

CSS-only, no component framework or runtime CSS-in-JS. Light and dark themes are driven entirely by a `[data-theme]` attribute on `<html>` (the toggle persists and follows `prefers-color-scheme`).

- **Tokens** — `src/ui/tokens.css`: semantic color / space / radius / shadow / type-scale tokens, defined per theme. Everything else references tokens; avoid raw hex/px.
- **Type** — self-hosted via Fontsource: Fraunces (display), Hanken Grotesk (text), IBM Plex Mono (numerals).
- **Icons** — vendored Remix Icons (Apache-2.0, `LICENSE`) and Lucide game glyphs (ISC/MIT, `LICENSE-lucide`) in `src/ui/icons/*.svg`, inlined at build time (no runtime dep) through the `icon()` helper in `src/ui/icon.ts`.
- **Cards** — CC0 playing-card faces from me.uk, vendored under `public/cards/` (regeneration notes in `public/cards/SOURCE.md`).
- **Primitives** — `src/ui/design.css` + `src/ui/components/`: the `.panel` card surface, `.seg` segmented control, `.btn` / `button()`, `formField`, `authCard`, plus the felt table, bid bar, and live clocks — all token-driven and theme-aware.

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

Deploys are automatic: push to `master` and the repo's [`deploy.yml`](../.github/workflows/deploy.yml) workflow builds `dist/` and publishes it to Cloudflare Pages (`app.wlim.dev`) after the backend ships. The API lives on its own origin (`spades.wlim.dev`, Caddy → Docker on the VPS); CORS and cookie settings are configured server-side as part of the backend deployment. Runbook and rollback: [SERVER.md → Deployment](../SERVER.md#deployment).

Emergency manual deploy from a laptop (archived path; needs `wrangler` auth):

```sh
bash scripts/deploy-cf-pages.sh
```

The script refuses dirty or unpushed trees, builds, deploys to the Pages project, and smoke-checks `https://app.wlim.dev/`.
