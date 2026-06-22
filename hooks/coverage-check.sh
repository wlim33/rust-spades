#!/usr/bin/env bash
# Coverage regression gate. Runs cargo-llvm-cov, classifies coverage per crate,
# fails if any crate dropped below its baseline in coverage-baseline.json.
#
# Env:
#   COVERAGE_CHECK_USE_EXISTING=1   reuse the existing coverage report instead
#                                   of rerunning (useful for testing this script)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

export PATH="$HOME/.cargo/bin:$PATH"

BASELINE_FILE="$REPO_ROOT/coverage-baseline.json"
REPORT_DIR="$REPO_ROOT/target/llvm-cov"
REPORT_FILE="$REPORT_DIR/coverage.json"
CRATES=(spades-core spades-server trick-notation)

# Shared by update-coverage-baseline.sh. Produces $REPORT_FILE (llvm-cov JSON
# export format) and defines crate_pct/baseline_pct.
# shellcheck source=hooks/coverage-lib.sh
source "$REPO_ROOT/hooks/coverage-lib.sh"

if [ ! -f "$BASELINE_FILE" ]; then
    echo "error: $BASELINE_FILE not found" >&2
    echo "  bootstrap:  hooks/update-coverage-baseline.sh" >&2
    exit 1
fi

run_coverage

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
