# spades-ts

TypeScript SPA front-end for the [rust-spades](https://github.com/wlim/rust-spades) game server.

## Status

Scaffold only. See `docs/superpowers/specs/2026-05-11-spades-ts-design.md` for the design and `docs/superpowers/plans/` for staged implementation plans.

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
