# Frictionless Local Dev (Tier 1) — Design

- **Date:** 2026-05-26
- **Status:** Design approved; implementation pending
- **Scope:** Local developer experience only. No production code, CI, or deploy-pipeline changes.

## Context

Running rust-spades locally today is a manual, two-terminal ritual with no task runner
(there is no `Makefile`/`justfile`/root `package.json`):

1. Terminal 1: `cargo run -p spades-server -- --port 3000 --insecure-cookies --cors-allow-origin http://localhost:5173`
   — the exact flags must be remembered.
2. Terminal 2: `pnpm -C web dev`.

The frontend's e2e suite (`web/tests/e2e/`) needs that backend running, but Playwright only
starts the Vite dev server — so `pnpm -C web test:e2e` fails unless the developer has
separately started the server. (A related guard bug in `web/tests/e2e/setup.ts` was already
fixed this session: it now probes `/health` instead of the POST-only `/games`.)

This is the first sub-project of a larger "simpler / more robust / more elegant" roadmap; it
was chosen first because it is zero production risk and high daily value.

## Goals

- **One command** brings up the full stack: `make dev` runs backend + frontend together and a
  single Ctrl-C tears both down cleanly (no orphaned `:3000` listener).
- **e2e needs zero manual setup**: `make e2e` / `pnpm -C web test:e2e` auto-starts the backend.
- **One discoverable home for commands**: `make` with no args lists targets; `dev`, `test`,
  `e2e`, `fmt`, `lint`, `check`, `build` all live in one place and wrap both the cargo and pnpm
  toolchains.
- **No new tool dependency** beyond `make` (already ubiquitous; chosen over `just` to avoid an
  install step).

## Non-goals (YAGNI)

- No process manager or `concurrently`/`npm-run-all` dependency — plain `make` + bash only.
- No `scripts/` directory — the one tricky recipe is handled inline (see §B).
- No changes to CI (`/.github/workflows/deploy.yml`), the deploy pipeline, or production code.
  Running e2e/coverage in CI is **Tier 2**; config unification, the `:latest` pin, and
  provisioning are **Tier 3**.

## Design

### A. `Makefile` (repo root, new)

Self-documenting, with overridable variables and a single DRY definition of the dev-server
command reused by `dev`, `backend`, and (via `make -C ..`) Playwright.

```makefile
# Makefile — local dev for rust-spades
.DEFAULT_GOAL := help
.ONESHELL:
.SHELLFLAGS := -eu -o pipefail -c

PORT     ?= 3000
DB       ?= dev.sqlite          # set DB= (empty) for an in-memory database
WEB      ?= http://localhost:5173
RUST_LOG ?= info
DB_FLAG  := $(if $(DB),--db $(DB),)
SERVER   := cargo run -p spades-server -- --port $(PORT) $(DB_FLAG) \
            --insecure-cookies --cors-allow-origin $(WEB)

help:        ## List targets
	@grep -hE '^[a-zA-Z_-]+:.*## ' $(MAKEFILE_LIST) \
	  | awk 'BEGIN{FS=":.*## "}{printf "  \033[36m%-12s\033[0m %s\n",$$1,$$2}'

dev:         ## Backend + frontend together (Ctrl-C stops both)
	@trap 'kill 0' EXIT INT TERM
	RUST_LOG=$(RUST_LOG) $(SERVER) &
	curl -sf --retry 120 --retry-delay 1 --retry-connrefused http://localhost:$(PORT)/health >/dev/null
	pnpm -C web dev

backend:     ## Only the backend (DB= for in-memory)
	RUST_LOG=$(RUST_LOG) $(SERVER)
web:         ## Only the Vite dev server
	pnpm -C web dev

test: test-rust test-web   ## All tests (rust + web unit/component)
test-rust:   ## cargo test (workspace)
	cargo test --workspace
test-web:    ## web unit + component tests
	pnpm -C web test
e2e:         ## web e2e (Playwright auto-starts the backend)
	pnpm -C web test:e2e

fmt:         ## Format rust + web
	cargo fmt
	pnpm -C web format
fmt-check:   ## Verify formatting (rust + web)
	cargo fmt --check
	pnpm -C web format:check
lint:        ## Lint rust + web
	cargo clippy --workspace --all-targets -- -D warnings
	pnpm -C web lint
check: fmt-check lint test  ## Pre-push gate (formatting + lint + tests)

build:       ## Release build (server binary + web bundle)
	cargo build --release -p spades-server
	pnpm -C web build
clean:       ## Remove the local dev DB
	rm -f dev.sqlite dev.sqlite-wal dev.sqlite-shm

.PHONY: help dev backend web test test-rust test-web e2e fmt fmt-check lint check build clean
```

Notes:
- Variables are overridable: `make dev PORT=4000`, `make backend DB=` (in-memory).
- `check` mirrors the existing `hooks/pre-push` intent (clippy + tests) plus a format check; it
  intentionally omits the tarpaulin coverage ratchet, which stays in the hook (and becomes a
  Tier 2 CI concern).
- Web script names are confirmed against `web/package.json`: `format`, `format:check`, `lint`,
  `test`, `test:e2e`, `build` all exist.

### B. `make dev` — concurrency & teardown

`.ONESHELL` runs the whole recipe in a single bash process, so a `trap 'kill 0' EXIT INT TERM`
plus backgrounding the server gives clean teardown: Ctrl-C (or any exit) signals the entire
process group, killing the backgrounded backend too — no orphaned `:3000` listener. The
`curl --retry --retry-connrefused` health-gate means Vite only launches once the backend
answers `/health`, so logs are ordered and the first proxied API call never races a
still-compiling server. `.SHELLFLAGS := -eu -o pipefail -c` ensures a failed health-gate aborts
the recipe (firing the trap) instead of silently starting Vite against a dead backend.

Dependency-free and contained to the `Makefile`. **Alternative considered:** a `scripts/dev.sh`
helper — rejected to keep the change to a single file; the inline recipe is small and readable.

### C. Playwright auto-starts the backend (`web/playwright.config.ts`, edit)

`webServer` changes from a single Vite entry to an array of two servers. Playwright starts both
and waits for each `url` before running tests:

```ts
webServer: [
  {
    command: 'make -C .. backend DB=',          // fresh in-memory DB per run
    url: 'http://localhost:3000/health',
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,                            // allow first cargo compile
  },
  {
    command: 'pnpm dev',
    url: 'http://localhost:5173',
    reuseExistingServer: !process.env.CI,
    timeout: 30_000,
  },
],
```

Effects:
- `make e2e` / `pnpm -C web test:e2e` works with **zero manual setup**.
- `DB=` (in-memory) gives each e2e run a clean database, addressing the previously-noted
  test-data-accumulation gap (e2e tests create real users/games with no cleanup).
- The `Makefile` remains the single source of truth for how the backend launches.
- `reuseExistingServer: !CI` means a developer already running `make dev` is reused locally; CI
  (Tier 2) will set `reuseExistingServer:false` and get backend startup for free — this edit is
  the hook that later enables e2e-in-CI.
- The fixed `apiUp` guard in `setup.ts` stays as a cheap sanity check for the reuse case.

### D. Supporting changes

- `.gitignore` (edit): add `dev.sqlite*` (the default local dev database).
- Docs (edit, one section): a **Local development** section in `readme.md`: one-time setup
  (`pnpm -C web install` and `pnpm -C web exec playwright install chromium`), then `make dev`,
  `make test`, `make e2e`. Replaces the remembered-flags ritual.

## Files changed

| File | Change |
|------|--------|
| `Makefile` | new |
| `web/playwright.config.ts` | edit — `webServer` becomes a 2-server array |
| `.gitignore` | edit — add `dev.sqlite*` |
| `readme.md` | edit — add Local development section |

No production/runtime code is touched.

## Verification (evidence before "done")

1. `make` (no args) prints the target list.
2. `make dev` brings up both servers; `curl :3000/health` → 200 and `curl :5173/` → 200 (proxy
   reaches backend); a single Ctrl-C frees **both** `:3000` and `:5173` (confirm via `lsof`).
3. `make test` runs cargo + web unit/component suites green.
4. `make e2e` (with **no** backend pre-started; chromium installed once via
   `pnpm -C web exec playwright install chromium`) auto-starts the backend and runs the e2e
   smoke spec green.
5. `make backend DB=` starts in-memory (log shows the in-memory warning, no `dev.sqlite`
   created).

## Out of scope / future roadmap

- **Tier 2 — make the suites run & stay enforced:** wire e2e into CI (reuses §C), enforce the
  coverage ratchet in CI instead of opt-in pre-push.
- **Tier 3 — deploy/prod robustness:** unify server config (replace hand-rolled `std::env::args`
  parsing in `main.rs` with a typed config), pin prod compose off `:latest`, smooth out
  `install-docker.sh` provisioning.
