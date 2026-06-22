#!/usr/bin/env bash
# Manually bump coverage-baseline.json after intentional coverage changes.
# Runs cargo-llvm-cov, computes per-crate coverage, writes a new baseline,
# and prints the diff so the change can be reviewed before committing.
#
# Env:
#   COVERAGE_CHECK_USE_EXISTING=1   reuse the existing coverage report
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

export PATH="$HOME/.cargo/bin:$PATH"

BASELINE_FILE="$REPO_ROOT/coverage-baseline.json"
REPORT_DIR="$REPO_ROOT/target/llvm-cov"
REPORT_FILE="$REPORT_DIR/coverage.json"
CRATES=(spades-core spades-server trick-notation)
TODAY="$(date -u +%Y-%m-%d)"

# shellcheck source=hooks/coverage-lib.sh
source "$REPO_ROOT/hooks/coverage-lib.sh"

run_coverage

# Build the new baseline JSON, one crate at a time, preserving key order.
new_baseline=$(jq -n '{}')
for crate in "${CRATES[@]}"; do
    pct=$(crate_pct "$crate")
    new_baseline=$(echo "$new_baseline" | jq \
        --arg crate "$crate" --argjson pct "$pct" --arg today "$TODAY" \
        '. + { ($crate): { line_coverage_pct: $pct, last_updated: $today } }')
done

new_file=$(mktemp)
echo "$new_baseline" | jq '.' > "$new_file"

if [ -f "$BASELINE_FILE" ]; then
    echo "" >&2
    echo "==> baseline diff" >&2
    diff -u "$BASELINE_FILE" "$new_file" || true
fi

mv "$new_file" "$BASELINE_FILE"

echo "" >&2
echo "  baseline updated: $BASELINE_FILE" >&2
echo "  commit it:  git add coverage-baseline.json && git commit" >&2
