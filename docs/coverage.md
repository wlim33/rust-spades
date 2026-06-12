# Coverage gate

The repo enforces a per-crate **line-coverage ratchet**: pushes that drop either `spades-core` or `spades-server` below the recorded baseline are rejected by the pre-push hook. The baseline lives in [`coverage-baseline.json`](../coverage-baseline.json) and is reviewed in PRs like source code.

## Enabling the gate locally

The hook is opt-in. Once per clone:

```bash
git config core.hooksPath hooks
brew install cargo-llvm-cov cargo-nextest   # or: cargo install cargo-llvm-cov cargo-nextest --locked
rustup component add llvm-tools
```

After that, every `git push` runs (in order):

1. `cargo clippy --workspace --all-targets -- -D warnings`
2. `cargo test --workspace --features spades-server/insecure-fast-hash`
3. `hooks/coverage-check.sh` — runs cargo-llvm-cov, compares against the baseline

(`cargo-nextest` is optional but recommended: the coverage run uses it to execute
test binaries in parallel, and falls back to `cargo test` without it. The
`insecure-fast-hash` feature swaps argon2 to throwaway params so auth tests
don't pay production hashing costs; it is never part of a deployed build.)

A failing coverage check looks like:

```
  crate            actual    baseline  delta     status
  -----            ------    --------  -----     ------
  spades-core      92.6%     97.2%     -4.6      FAIL
  spades-server    66.3%     66.3%     +0.0      OK

  coverage regression detected — push aborted
```

## What counts as a regression

Strict ratchet: any crate whose new line-coverage percentage is below its recorded baseline fails the push. There is no tolerance band on the failure side — llvm-cov's line counts are deterministic for a given source tree, so a sub-percent drop is a real drop.

On the improvement side there is a +0.5pp band before the hook suggests a bump, to avoid noisy "bump the baseline" hints for trivial gains.

## When coverage legitimately drops

Sometimes a regression is intentional (e.g., adding a `todo!()` for a future feature, or deleting test scaffolding that exercised dead code being removed). Two options:

1. **Lower the baseline deliberately:** run `hooks/update-coverage-baseline.sh`. It re-runs the coverage suite, rewrites `coverage-baseline.json` with the new numbers, and prints a diff so the change is visible in your next commit. Commit the file. Reviewers will see the drop and can question it.
2. **Skip the gate once:** `SKIP_HOOKS=1 git push` or `git push --no-verify`. Use sparingly; the next push from anyone else will still hit the same regression and fail.

## When coverage improves

Add tests, push. If the new coverage exceeds the baseline by >0.5pp, the hook prints a hint:

```
  coverage improved beyond baseline — consider:
      hooks/update-coverage-baseline.sh
```

Run that script, commit the updated baseline, and now the new floor is locked in — future regressions can't sneak back below it.

## Files

- [`coverage-baseline.json`](../coverage-baseline.json) — committed baseline, one entry per crate.
- [`hooks/coverage-check.sh`](../hooks/coverage-check.sh) — what pre-push runs.
- [`hooks/update-coverage-baseline.sh`](../hooks/update-coverage-baseline.sh) — manual bump tool.
- [`hooks/coverage-lib.sh`](../hooks/coverage-lib.sh) — shared coverage run + report parsing.

## Scope and limits

- Line coverage only. No branch coverage, no per-file or per-function thresholds.
- Two crates measured: `spades-core` (path: `crates/spades-core/src/`) and `spades-server` (path: `crates/spades-server/src/`). Test files under `src/tests/` are part of the source tree and intentionally count as covered.
- Enforced locally by the opt-in pre-push hook and remotely by CI: the `coverage` job in [`deploy.yml`](../.github/workflows/deploy.yml) runs `hooks/coverage-check.sh` on pushes to `master` and on pull requests, so regressions are caught even without the local hook. In CI this job doubles as the workspace test run — cargo-llvm-cov executes every test anyway, so there is no separate `cargo test` job (doctests, which nextest can't run, get their own step).
- Coverage runs add ~20-60s to `git push` on a warm target dir. Use `SKIP_HOOKS=1` if you need to push a docs-only change without waiting.
