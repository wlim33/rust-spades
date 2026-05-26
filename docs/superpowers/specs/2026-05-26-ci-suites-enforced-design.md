# Tier 2 — CI Suites Run & Stay Enforced — Design

- **Date:** 2026-05-26
- **Status:** Design approved; implementation pending
- **Scope:** CI pipeline (`.github/workflows/deploy.yml`) + a one-line Playwright config tweak. No production/runtime code.
- **Predecessor:** Tier 1 (`2026-05-26-frictionless-local-dev-design.md`) — its Playwright auto-start change is what makes the e2e CI job simple.

## Context

Two test suites exist but aren't enforced by the active pipeline:

1. **e2e runs nowhere.** `.github/workflows/deploy.yml`'s `ci` job runs `cargo test --workspace --locked` + `pnpm test` (unit+component) + `pnpm build`. The Playwright e2e suite is never executed in CI. (A dead `web/.github/workflows/ci.yml` that *looked* like it ran e2e was inert and has been removed.)
2. **Coverage is opt-in / local-only.** `hooks/coverage-check.sh` runs `cargo tarpaulin` and ratchets per-crate line coverage against `coverage-baseline.json`, but it only fires from the pre-push hook, and only for contributors who ran `git config core.hooksPath hooks`. There is no remote enforcement.

This is Tier 2 of the roadmap. Goal: make both suites run in CI and **block the production deploy** on failure.

## Goals

- The Playwright e2e suite runs in CI on every push and PR.
- The coverage ratchet (`hooks/coverage-check.sh`) runs in CI and fails the build on any per-crate regression below `coverage-baseline.json`.
- The deploy (`ship` job) runs only when tests, e2e, and coverage all pass.
- e2e flake is curbed with CI-only Playwright retries.

## Non-goals (YAGNI)

- No new test content (no new e2e specs, no coverage-raising tests).
- No change to what deploys or how (the `ship` job's build/scp/ssh/wrangler steps are untouched — only its `needs:` changes).
- No removal of the local pre-push hook — it stays for fast local feedback; CI becomes the enforcement point.
- Not touching Tier 3 (server config unification, `:latest` pin, provisioning).
- Not configuring GitHub branch-protection required-checks (a repo setting, outside the workflow file). The workflow makes the checks *run* on PRs; making them *required to merge* is a separate manual setting the maintainer can enable later.

## Design

Two new **parallel jobs** in `deploy.yml`, inserted between `ci` and `ship`. Separate jobs (rather than extending `ci`) run concurrently and make failures unambiguous.

### A. `e2e` job

Tier 1's `webServer` array means that in CI (`CI=true`, set automatically by GitHub Actions → `reuseExistingServer: false`) Playwright starts the backend (`make -C .. backend DB=`, a `cargo run` debug build, in-memory DB) and Vite itself — so no manual server-start step is needed.

```yaml
  e2e:
    name: e2e
    runs-on: ubuntu-latest
    timeout-minutes: 20
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - uses: pnpm/action-setup@v4
        with:
          package_json_file: web/package.json
      - uses: actions/setup-node@v4
        with:
          node-version: '20'
          cache: pnpm
          cache-dependency-path: web/pnpm-lock.yaml
      - name: pnpm install
        run: pnpm install --frozen-lockfile
        working-directory: web
      - name: Install Playwright browser
        run: pnpm exec playwright install --with-deps chromium
        working-directory: web
      - name: e2e
        run: make e2e
      - name: Upload Playwright report
        if: failure()
        uses: actions/upload-artifact@v4
        with:
          name: playwright-report
          path: web/playwright-report/
          retention-days: 14
```

`make e2e` runs from the repo root (`pnpm -C web test:e2e`). `make` is preinstalled on `ubuntu-latest`; the Rust toolchain + `rust-cache` cover the `cargo run` backend.

To curb browser-test flake, `web/playwright.config.ts` changes `retries: 0` to:
```ts
  retries: process.env.CI ? 2 : 0,
```
Retries apply only in CI; local runs keep `0`.

### B. `coverage` job

Reuses `hooks/coverage-check.sh` verbatim — it runs `cargo tarpaulin --workspace --out Json`, computes per-crate line coverage with `jq`, compares to `coverage-baseline.json`, and exits non-zero on any regression. `jq` is preinstalled on `ubuntu-latest`. `cargo-tarpaulin` is installed as a prebuilt binary to avoid a multi-minute `cargo install` compile.

```yaml
  coverage:
    name: coverage
    runs-on: ubuntu-latest
    timeout-minutes: 20
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Install cargo-tarpaulin
        uses: taiki-e/install-action@v2
        with:
          tool: cargo-tarpaulin
      - name: coverage gate
        run: hooks/coverage-check.sh
```

Deliberate minor cost: tarpaulin re-runs the Rust tests instrumented, so the workspace suite runs both in `ci` (fast plain feedback) and here. Accepted for clean separation; the suite is small.

### C. Wiring

The `ship` job's dependency changes from:
```yaml
  ship:
    needs: ci
```
to:
```yaml
  ship:
    needs: [ci, e2e, coverage]
```
Nothing else in `ship` changes. `ship` already only runs on push to `master`; now it additionally requires `e2e` and `coverage` to pass.

Both new jobs run on every workflow trigger (push to `master`, PRs to `master`, `workflow_dispatch`) — identical to `ci` — so PRs surface the checks too.

## Files changed

| File | Change |
|------|--------|
| `.github/workflows/deploy.yml` | add `e2e` + `coverage` jobs; change `ship` `needs: ci` → `needs: [ci, e2e, coverage]` |
| `web/playwright.config.ts` | `retries: 0` → `retries: process.env.CI ? 2 : 0` |

No production/runtime code touched.

## Verification

CI changes cannot be fully exercised locally without pushing, and pushing `master` triggers the deploy. Therefore:

- **Local (pre-merge):** confirm the workflow YAML is valid (parses); confirm the referenced commands work — `make e2e` (via Tier 1's verified path) and `hooks/coverage-check.sh` (runs tarpaulin, passes against the committed baseline); confirm `web/playwright.config.ts` still type-checks and `playwright test --list` loads.
- **Full validation:** happens on the next push/PR. The **safe way to exercise the new jobs without deploying is a PR** — it runs `ci` + `e2e` + `coverage` but not `ship`. On committed `master`, e2e passes (the home heading is present) and coverage is at/above baseline (`spades-core` 97.6%, `spades-server` 76.9%).

## Out of scope / future

- **Tier 3 — deploy/prod robustness:** unify server config (replace hand-rolled `std::env::args` parsing in `main.rs` with a typed config), pin prod compose off `:latest`, smooth out `install-docker.sh` provisioning.
- Optional later: enable GitHub branch-protection required checks; collapse the double Rust-test run by making the coverage job the sole Rust-test runner.
