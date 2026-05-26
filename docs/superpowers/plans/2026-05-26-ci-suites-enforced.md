# Tier 2 — CI Suites Run & Stay Enforced — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Run the Playwright e2e suite and the coverage ratchet in CI, and block the production deploy unless tests, e2e, and coverage all pass.

**Architecture:** Add two parallel jobs (`e2e`, `coverage`) to `.github/workflows/deploy.yml` and gate the `ship` (deploy) job on all three (`needs: [ci, e2e, coverage]`). The `e2e` job reuses Tier 1's Playwright backend auto-start (so no manual server step); the `coverage` job reuses the existing `hooks/coverage-check.sh`. A one-line Playwright config change adds CI-only retries.

**Tech Stack:** GitHub Actions, Playwright (chromium), cargo-tarpaulin.

**Branch:** Work continues on `dx/ci-suites-enforced` (the design spec is already committed there).

**Spec:** `docs/superpowers/specs/2026-05-26-ci-suites-enforced-design.md`

**Verification reality (read before starting):** CI workflow changes cannot be fully executed locally, and pushing `master` triggers a real deploy. So local verification is limited to: (a) the workflow YAML parses, (b) the commands the jobs invoke exist/work, (c) the Playwright config still loads. **The authoritative validation is a PR** — it runs `ci` + `e2e` + `coverage` but NOT `ship` (which is `if: push to master`). Do NOT push `master` to "test" this.

**WIP guardrail:** The working tree has unrelated user changes (`crates/**`, `web/**`, staged `.wrangler/`, deleted `.travis.yml`/`web/.github/...`, modified `web/tests/e2e/setup.ts`). Commit ONLY the files each task names, via pathspec (`git commit -m "..." -- <paths>`). NEVER `git add -A` / `git add .` / `git commit -a` / `--amend`.

---

### Task 1: CI-only Playwright retries

**Files:**
- Modify: `web/playwright.config.ts:6`

- [ ] **Step 1: Make the edit**

In `web/playwright.config.ts`, change line 6 from:
```ts
  retries: 0,
```
to:
```ts
  retries: process.env.CI ? 2 : 0,
```
(GitHub Actions sets `CI=true`, so e2e gets 2 retries in CI to absorb browser-test flake; local runs keep 0.)

- [ ] **Step 2: Verify the config still type-checks and loads**

Run (from `web/`): `pnpm exec tsc --noEmit -p tsconfig.json`
Expected: exits 0, no output.

Run (from `web/`): `pnpm exec playwright test --list`
Expected: exits 0 and lists the e2e specs (7 tests across 5 files). A malformed config would throw here.

- [ ] **Step 3: Commit**

```bash
git add web/playwright.config.ts
git commit -m "test(e2e): retry twice in CI to absorb flake" -- web/playwright.config.ts
```

---

### Task 2: Add the `e2e` and `coverage` jobs

**Files:**
- Modify: `.github/workflows/deploy.yml` (insert two jobs between the `ci` job and the `ship` job)

- [ ] **Step 1: Insert the two jobs**

In `.github/workflows/deploy.yml`, find this exact text (end of the `ci` job, start of `ship`):
```yaml
          if-no-files-found: error

  ship:
    name: ship
```
and replace it with:
```yaml
          if-no-files-found: error

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
          run_install: false
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

  ship:
    name: ship
```

- [ ] **Step 2: Verify the workflow still parses as YAML**

Run: `ruby -ryaml -e "YAML.load_file('.github/workflows/deploy.yml'); puts 'yaml-ok'"`
Expected: prints `yaml-ok` (ruby ships with macOS; this catches indentation/structure errors). Optional stronger check if installed: `actionlint`.

- [ ] **Step 3: Verify the commands the jobs invoke actually exist**

The `e2e` job runs `make e2e`; confirm that target expands correctly:
Run: `make -n e2e`
Expected: prints `pnpm -C web test:e2e`.

The `coverage` job runs `hooks/coverage-check.sh`; confirm it exists and is executable:
Run: `test -x hooks/coverage-check.sh && echo "coverage-check ok"`
Expected: prints `coverage-check ok`.

(These prove the jobs reference real entrypoints. Running the jobs end-to-end happens in CI — see the Verification reality note. Running `hooks/coverage-check.sh` locally is optional and requires `cargo-tarpaulin` installed; it is the existing, already-verified pre-push gate.)

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/deploy.yml
git commit -m "ci: run e2e and coverage suites in CI" -- .github/workflows/deploy.yml
```

---

### Task 3: Gate the deploy on e2e + coverage

**Files:**
- Modify: `.github/workflows/deploy.yml` (the `ship` job's `needs`)

- [ ] **Step 1: Make the edit**

In `.github/workflows/deploy.yml`, change the `ship` job's dependency from:
```yaml
    needs: ci
```
to:
```yaml
    needs: [ci, e2e, coverage]
```
(`needs: ci` appears only once — on the `ship` job. The `ship` job's `if: github.event_name == 'push' && github.ref == 'refs/heads/master'` is unchanged, so the deploy still only runs on push to `master`, now additionally requiring `e2e` and `coverage` green.)

- [ ] **Step 2: Verify YAML parses and `needs` references valid jobs**

Run: `ruby -ryaml -e "YAML.load_file('.github/workflows/deploy.yml'); puts 'yaml-ok'"`
Expected: prints `yaml-ok`.

Run: `grep -nE '^\s*needs:' .github/workflows/deploy.yml`
Expected: one line — `    needs: [ci, e2e, coverage]`. Each listed id (`ci`, `e2e`, `coverage`) is a defined job in the file.

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/deploy.yml
git commit -m "ci: block deploy unless tests, e2e, and coverage pass" -- .github/workflows/deploy.yml
```

---

### Final verification (whole-feature)

- [ ] **Step 1: Workflow integrity**

Run: `ruby -ryaml -e "YAML.load_file('.github/workflows/deploy.yml'); puts 'yaml-ok'"`
Expected: `yaml-ok`.

Confirm the three jobs and the gate all exist:
Run: `grep -nE '^  (ci|e2e|coverage|ship):|needs:' .github/workflows/deploy.yml`
Expected: `ci:`, `e2e:`, `coverage:`, `ship:` job headers and the single `needs: [ci, e2e, coverage]`.

- [ ] **Step 2: Scope check**

Run: `git diff --stat master..HEAD -- .github/workflows/deploy.yml web/playwright.config.ts`
Expected: exactly those two files differ (plus the already-committed spec). No `crates/**` or other runtime files.

- [ ] **Step 3: Authoritative validation (after merge, via PR — not a `master` push)**

Open a PR for this branch (or push the branch and open one). Confirm the `ci`, `e2e`, and `coverage` jobs all run and pass, and that `ship` does NOT run (PRs are not `push` to `master`). Only after that is the enforcement proven end-to-end.
