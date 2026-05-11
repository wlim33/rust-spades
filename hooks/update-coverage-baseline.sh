#!/usr/bin/env bash
# Manually bump coverage-baseline.json after intentional coverage changes.
# Runs tarpaulin, computes per-crate coverage, writes a new baseline,
# and prints the diff so the change can be reviewed before committing.
#
# Env:
#   COVERAGE_CHECK_USE_EXISTING=1   reuse the existing tarpaulin report
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

export PATH="$HOME/.cargo/bin:$PATH"

BASELINE_FILE="$REPO_ROOT/coverage-baseline.json"
TARPAULIN_DIR="$REPO_ROOT/target/tarpaulin"
REPORT_FILE="$TARPAULIN_DIR/tarpaulin-report.json"
CRATES=(spades-core spades-server)
TODAY="$(date -u +%Y-%m-%d)"

if ! command -v cargo-tarpaulin >/dev/null 2>&1; then
    echo "error: cargo-tarpaulin not found on PATH" >&2
    echo "  install:  cargo install cargo-tarpaulin" >&2
    exit 1
fi

if [ "${COVERAGE_CHECK_USE_EXISTING:-0}" = "1" ]; then
    if [ ! -f "$REPORT_FILE" ]; then
        echo "error: COVERAGE_CHECK_USE_EXISTING=1 set but no report at $REPORT_FILE" >&2
        exit 1
    fi
    echo "==> reusing existing tarpaulin report at $REPORT_FILE" >&2
else
    echo "==> cargo tarpaulin --workspace (30-90s)" >&2
    cargo tarpaulin --workspace --out Json \
        --output-dir "$TARPAULIN_DIR" --skip-clean >&2
    if [ ! -f "$REPORT_FILE" ]; then
        echo "error: tarpaulin did not produce $REPORT_FILE" >&2
        exit 1
    fi
fi

crate_pct() {
    local crate="$1"
    jq -r --arg crate "$crate" '
        [ .files[]
          | select((.path | join("/")) | contains("/crates/" + $crate + "/src/"))
          | { covered, coverable } ]
        | { covered:   ([.[] | .covered]   | add // 0),
            coverable: ([.[] | .coverable] | add // 0) }
        | if .coverable == 0 then "0.0"
          else (((.covered * 1000) / .coverable | floor) / 10 | tostring)
          end
    ' "$REPORT_FILE"
}

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
