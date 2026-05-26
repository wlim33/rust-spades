# Frictionless Local Dev (Tier 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make local development a one-command experience — `make dev` runs the full stack, `make e2e` auto-starts the backend — with no new tool dependency beyond `make`.

**Architecture:** A root `Makefile` becomes the single entrypoint, wrapping the existing `cargo` and `pnpm` toolchains. `make dev` runs the backend + Vite together in one `.ONESHELL` bash recipe with a `trap 'kill 0'` for clean teardown. `web/playwright.config.ts` gains a second `webServer` entry so Playwright boots the backend itself for e2e. No production/runtime code changes.

**Tech Stack:** GNU make, bash, cargo (Rust), pnpm + Vite + Playwright (web). Work continues on the existing branch `dx/frictionless-local-dev` (where the design spec is already committed).

**Spec:** `docs/superpowers/specs/2026-05-26-frictionless-local-dev-design.md`

**Prerequisite check:** confirm you are on the right branch before starting.

Run: `git branch --show-current`
Expected: `dx/frictionless-local-dev`

---

### Task 1: Create the root `Makefile`

**Files:**
- Create: `Makefile`

- [ ] **Step 1: Write the Makefile**

Create `Makefile` at the repo root with exactly this content (note: recipe bodies must be indented with **tabs**, not spaces — make requires tabs):

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

- [ ] **Step 2: Verify `make` lists targets**

Run: `make`
Expected: a list of targets with descriptions, e.g.:
```
  help         List targets
  dev          Backend + frontend together (Ctrl-C stops both)
  backend      Only the backend (DB= for in-memory)
  ...
  clean        Remove the local dev DB
```
If you get `*** missing separator. Stop.`, a recipe line is indented with spaces instead of a tab — fix the indentation.

- [ ] **Step 3: Verify recipe expansion without running the slow targets**

Run: `make -n dev`
Expected (the variables expanded, no execution):
```
trap 'kill 0' EXIT INT TERM
RUST_LOG=info cargo run -p spades-server -- --port 3000 --db dev.sqlite --insecure-cookies --cors-allow-origin http://localhost:5173 &
curl -sf --retry 120 --retry-delay 1 --retry-connrefused http://localhost:3000/health >/dev/null
pnpm -C web dev
```

Run: `make -n backend DB=`
Expected: the `cargo run` line has **no** `--db` flag (in-memory):
```
RUST_LOG=info cargo run -p spades-server -- --port 3000  --insecure-cookies --cors-allow-origin http://localhost:5173
```

- [ ] **Step 4: Verify `make dev` brings up both servers and tears down cleanly**

Run this self-contained check. It frees the ports, brings the stack up, confirms both are live, stops `make`, then confirms both ports are released. Keeping it in one shell lets `wait` block until `make` exits — no `sleep` needed:

```bash
lsof -ti:3000 -ti:5173 | xargs kill 2>/dev/null || true   # ensure a clean start

make dev > /tmp/makedev.log 2>&1 &
MAKE_PID=$!

curl -sf --retry 120 --retry-delay 1 --retry-connrefused http://localhost:3000/health && echo "backend-ok"
curl -sf --retry 60  --retry-delay 1 --retry-connrefused http://localhost:5173/ -o /dev/null && echo "frontend-ok"

kill "$MAKE_PID" 2>/dev/null      # recipe's `trap 'kill 0'` stops backend + frontend
wait "$MAKE_PID" 2>/dev/null || true

lsof -ti:3000 -ti:5173 && echo "STILL LISTENING — investigate" || echo "both ports free"
lsof -ti:3000 -ti:5173 | xargs kill 2>/dev/null || true   # belt-and-suspenders for later steps
```
Expected output includes `backend-ok`, `frontend-ok`, and `both ports free`.

Authoritative interactive test (the real-world gesture): run `make dev` in a terminal, watch both servers come up, press **Ctrl-C**, then in another shell run `lsof -ti:3000 -ti:5173` and confirm it prints nothing.

If a listener survives (usually an orphaned `cargo run` child), the teardown signal did not reach the process group — confirm `.ONESHELL:` is present (so the trap and the backgrounded server share one shell) and the `dev` recipe's first line is exactly `@trap 'kill 0' EXIT INT TERM`.

- [ ] **Step 5: Commit**

```bash
git add Makefile
git commit -m "chore(dev): add root Makefile for one-command local dev"
```

---

### Task 2: Playwright auto-starts the backend

**Files:**
- Modify: `web/playwright.config.ts:12-17` (the `webServer` object)

- [ ] **Step 1: Confirm e2e currently fails without a manually-started backend (RED)**

Make sure no backend is running.
Run: `lsof -ti:3000`
Expected: no output.

Install the browser if you have not already (one-time):
Run: `pnpm -C web exec playwright install chromium`

Run the e2e suite:
Run: `pnpm -C web test:e2e`
Expected: FAIL. With no backend up, the `apiUp` fixture in `tests/e2e/setup.ts` cannot reach `http://localhost:3000/health` and throws `rust-spades not reachable at http://localhost:3000/health` (Playwright only started Vite, not the backend).

- [ ] **Step 2: Change `webServer` from a single server to an array of two**

In `web/playwright.config.ts`, replace this exact block:

```ts
  webServer: {
    command: 'pnpm dev',
    url: 'http://localhost:5173',
    reuseExistingServer: !process.env.CI,
    timeout: 30_000,
  },
```

with:

```ts
  webServer: [
    {
      command: 'make -C .. backend DB=',
      url: 'http://localhost:3000/health',
      reuseExistingServer: !process.env.CI,
      timeout: 120_000,
    },
    {
      command: 'pnpm dev',
      url: 'http://localhost:5173',
      reuseExistingServer: !process.env.CI,
      timeout: 30_000,
    },
  ],
```

(`make -C ..` runs the root Makefile from `web/`, where Playwright executes. `DB=` makes the backend use an in-memory database, giving each e2e run a clean slate. The 120s timeout covers a first-time `cargo` compile. `url` is polled until it returns 2xx/3xx.)

- [ ] **Step 3: Verify e2e now passes with zero manual setup (GREEN)**

Confirm no backend is running first.
Run: `lsof -ti:3000`
Expected: no output.

Run: `pnpm -C web test:e2e`
Expected: PASS. Playwright starts the backend (`make -C .. backend DB=`) and Vite, waits for `:3000/health` and `:5173`, then runs the specs. At minimum `tests/e2e/smoke.spec.ts` passes. After the run, Playwright tears down both servers.

Also confirm it works through the Makefile target:
Run: `make e2e`
Expected: PASS (same behavior).

- [ ] **Step 4: Commit**

```bash
git add web/playwright.config.ts
git commit -m "test(e2e): auto-start the backend via Playwright webServer"
```

---

### Task 3: Ignore the dev database and document the workflow

**Files:**
- Modify: `.gitignore` (append)
- Modify: `readme.md` (insert a section after the "Server Mode" section, before "## Bidding")

- [ ] **Step 1: Confirm `dev.sqlite` is NOT ignored yet (RED)**

Run: `git check-ignore dev.sqlite`
Expected: no output and a non-zero exit (the file is not currently ignored).

- [ ] **Step 2: Append the dev-DB ignore rule to `.gitignore`**

Add these lines to the end of `.gitignore` (current last line is `*.deploy.env`):

```gitignore

# Local dev database (created by `make dev`).
/dev.sqlite*
```

- [ ] **Step 3: Verify the ignore rule works (GREEN)**

Run: `git check-ignore dev.sqlite dev.sqlite-wal dev.sqlite-shm`
Expected: all three paths are printed (each is now ignored):
```
dev.sqlite
dev.sqlite-wal
dev.sqlite-shm
```

- [ ] **Step 4: Add a "Local development" section to `readme.md`**

In `readme.md`, insert the following section immediately after the line `See [SERVER.md](SERVER.md) for the full API reference.` (the end of the "Server Mode" section) and immediately before `## Bidding`:

````markdown
## Local development

Run the full stack (Rust server + web UI) with one command. One-time setup:

```bash
pnpm -C web install
pnpm -C web exec playwright install chromium   # for e2e tests
```

Then, from the repo root:

```bash
make dev     # backend on :3000 + Vite UI on :5173 (Ctrl-C stops both)
make test    # cargo + web unit/component tests
make e2e     # web end-to-end tests (auto-starts the backend)
make         # list all targets
```

The dev server writes to a local `dev.sqlite` (git-ignored). `make clean` removes it.

````

- [ ] **Step 5: Verify the section is present and renders**

Run: `grep -n "## Local development" readme.md`
Expected: one match.

Run: `grep -nc 'make dev' readme.md`
Expected: at least `1`.

- [ ] **Step 6: Commit**

```bash
git add .gitignore readme.md
git commit -m "docs: document make-based local dev; ignore dev.sqlite"
```

---

### Final verification (whole-feature smoke)

- [ ] **Step 1: Run the full local workflow once, end to end**

```bash
make                 # prints target list
make test            # cargo + web unit/component tests pass
make e2e             # e2e passes with no manually-started backend
```
Expected: `make` lists targets; `make test` is green; `make e2e` is green.

- [ ] **Step 2: Confirm only the intended files changed on this branch**

Run: `git diff --stat master -- Makefile web/playwright.config.ts .gitignore readme.md`
Expected: exactly those four files (plus the already-committed spec) differ from `master`. No production/runtime source files (`crates/**`) appear.
