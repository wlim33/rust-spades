#!/usr/bin/env bash
# Coverage regression gate. Runs tarpaulin, classifies coverage per crate,
# fails if any crate dropped below its baseline in coverage-baseline.json.
#
# Env:
#   COVERAGE_CHECK_USE_EXISTING=1   reuse the existing tarpaulin report instead
#                                   of rerunning (useful for testing this script)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

export PATH="$HOME/.cargo/bin:$PATH"

BASELINE_FILE="$REPO_ROOT/coverage-baseline.json"
TARPAULIN_DIR="$REPO_ROOT/target/tarpaulin"
REPORT_FILE="$TARPAULIN_DIR/tarpaulin-report.json"
CRATES=(spades-core spades-server)

if ! command -v cargo-tarpaulin >/dev/null 2>&1; then
    echo "error: cargo-tarpaulin not found on PATH" >&2
    echo "  install:  cargo install cargo-tarpaulin" >&2
    exit 1
fi

if [ ! -f "$BASELINE_FILE" ]; then
    echo "error: $BASELINE_FILE not found" >&2
    echo "  bootstrap:  hooks/update-coverage-baseline.sh" >&2
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

# Compute per-crate line coverage from tarpaulin-report.json.
# Each .files[] entry has .covered, .coverable, and .path (array of components).
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

baseline_pct() {
    local crate="$1"
    jq -r --arg crate "$crate" '.[$crate].line_coverage_pct // 0' "$BASELINE_FILE"
}

echo "" >&2
printf "  %-15s  %-8s  %-8s  %-8s  %s\n" "crate" "actual" "baseline" "delta" "status" >&2
printf "  %-15s  %-8s  %-8s  %-8s  %s\n" "-----" "------" "--------" "-----" "------" >&2

fail=0
bump_suggested=0
for crate in "${CRATES[@]}"; do
    a=$(crate_pct "$crate")
    b=$(baseline_pct "$crate")
    read -r delta verdict < <(awk -v a="$a" -v b="$b" '
        BEGIN {
            d = a - b
            if (a + 0 < b + 0)             v = "FAIL"
            else if (a + 0 > b + 0 + 0.5)  v = "BUMP"
            else                           v = "OK"
            printf "%+.1f %s\n", d, v
        }
    ')
    case "$verdict" in
        FAIL) fail=1 ;;
        BUMP) bump_suggested=1 ;;
    esac
    printf "  %-15s  %-8s  %-8s  %-8s  %s\n" \
        "$crate" "${a}%" "${b}%" "$delta" "$verdict" >&2
done

if [ "$fail" -eq 1 ]; then
    echo "" >&2
    echo "  coverage regression detected — push aborted" >&2
    echo "  if intentional:  hooks/update-coverage-baseline.sh && git add coverage-baseline.json" >&2
    echo "  to skip once:    SKIP_HOOKS=1 git push   (or git push --no-verify)" >&2
    exit 1
fi

if [ "$bump_suggested" -eq 1 ]; then
    echo "" >&2
    echo "  coverage improved beyond baseline — consider:" >&2
    echo "      hooks/update-coverage-baseline.sh" >&2
fi

echo "" >&2
echo "  coverage check passed." >&2
