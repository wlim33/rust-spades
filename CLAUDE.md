# rust-spades

Four-player Spades: pure-Rust game engine + axum multiplayer server + TypeScript SPA.
Cargo workspace (`crates/`) + pnpm app (`web/`). `master` is the deploy branch.

## Commands

```bash
make dev     # backend :3000 + Vite UI :5173 together (Ctrl-C stops both)
make test    # cargo test --workspace + web unit/component tests
make e2e     # Playwright e2e (auto-starts the backend)
make check   # pre-push gate: fmt-check + clippy -D warnings + all tests
make         # list every target
```

Web-only scripts run via `pnpm -C web <script>`: `dev`, `test`, `test:e2e`, `lint`, `format`, `build`.
One-time setup: `pnpm -C web install && pnpm -C web exec playwright install chromium`.

## Layout

- `crates/spades-core` — game state machine, published to crates.io as `spades`. No async deps;
  the `openapi` feature gates oasgen derives. Unit tests live inside `src/tests/` (intentional —
  they count as covered lines, see docs/coverage.md).
- `crates/spades-server` — axum HTTP/WS/SSE server. Library in `src/` (auth/, matchmaking,
  game_manager, …), binary in `src/bin/server/` (handlers/, ws.rs, dto.rs). SQLite via `--db` is optional.
- `web/` — framework-less SPA: lit-html + @preact/signals-core + navaid + openapi-fetch.
  State in `src/state/`, card animation in `src/cards/`, design system in `src/ui/` (web/README.md).
- Docs: SERVER.md (full API + deploy runbook + rollback), docs/coverage.md (coverage ratchet).

## Gotchas

- **OpenAPI codegen is two committed artifacts.** After changing server endpoints/DTOs: start the
  server, `pnpm -C web openapi:fetch`, then `pnpm -C web openapi:generate`; commit both
  `web/openapi/openapi.json` and `web/src/api/schema.d.ts`. CI's `openapi:check` only verifies
  schema.d.ts ↔ openapi.json — a stale openapi.json vs the live server is NOT caught.
- **Version bumps touch three files**: `workspace.package.version` in the root Cargo.toml, the
  `spades = { path = …, version = "…" }` pin in crates/spades-server/Cargo.toml, AND `version` in
  web/package.json (kept in lockstep with the workspace version).
- **ESLint uses flat config** (`web/eslint.config.js`); ESLint 10 removed the legacy `.eslintrc`
  system and the `ESLINT_USE_FLAT_CONFIG` escape hatch.
- **Coverage ratchet**: per-crate line coverage vs committed `coverage-baseline.json`, enforced by
  the opt-in pre-push hook and CI. Intentional drops: `hooks/update-coverage-baseline.sh`, commit.
- **CORS is off by default**; dev needs `--insecure-cookies --cors-allow-origin http://localhost:5173`
  (the Makefile passes these). `dev.sqlite` is the git-ignored dev DB; `make clean` removes it.

## Web game-event invariants (each was a prod bug)

- WS events trigger async hand fetches that can resolve out of order; the per-game `seq` cursor in
  `web/src/state/game.ts` drops stale ones. Never drop an event on hand-fetch failure — degrade to
  the bounded polling fallback (`web/src/state/game-sync.ts`).
- Card animations run through the FIFO queue in `web/src/cards/orchestrator.ts`; the play POST
  fires inside the play step, and steps read game state at execution time, not enqueue time.
- `Trick(n)` in server state means rotation n, not trick index.
- Styling uses only tokens from `web/src/ui/tokens.css` (no raw hex/px); accent color is reserved
  for interactive elements.

## Deploy

Push to `master` → `.github/workflows/deploy.yml` runs the full gate (lint, tests, e2e, coverage,
audit), then the `ship` job: Docker image → ghcr → VPS compose swap + Cloudflare Pages. Runbook
and rollback: SERVER.md → Deployment. `ansible/` is a staged replacement pipeline, NOT the live path.
