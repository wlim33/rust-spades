# Shared between coverage-check.sh and update-coverage-baseline.sh.
# Expects REPO_ROOT, REPORT_DIR, REPORT_FILE, BASELINE_FILE to be set.
#
# Coverage runs via cargo-llvm-cov (LLVM source-based instrumentation): tests
# execute at full speed and full parallelism, unlike tarpaulin's ptrace engine
# which ran binaries sequentially and slowed CPU-bound code dramatically.

# Produce $REPORT_FILE in llvm-cov's JSON export format, honoring
# COVERAGE_CHECK_USE_EXISTING=1.
run_coverage() {
    if [ "${COVERAGE_CHECK_USE_EXISTING:-0}" = "1" ]; then
        if [ ! -f "$REPORT_FILE" ]; then
            echo "error: COVERAGE_CHECK_USE_EXISTING=1 set but no report at $REPORT_FILE" >&2
            exit 1
        fi
        echo "==> reusing existing coverage report at $REPORT_FILE" >&2
        return
    fi

    if ! command -v cargo-llvm-cov >/dev/null 2>&1; then
        echo "error: cargo-llvm-cov not found on PATH" >&2
        echo "  install:  brew install cargo-llvm-cov cargo-nextest" >&2
        echo "       or:  cargo install cargo-llvm-cov cargo-nextest --locked" >&2
        exit 1
    fi

    mkdir -p "$REPORT_DIR"
    # nextest runs test binaries in parallel (cargo test runs them one at a
    # time); fall back to the plain runner when it isn't installed.
    if command -v cargo-nextest >/dev/null 2>&1; then
        echo "==> cargo llvm-cov nextest --workspace (~30-60s)" >&2
        cargo llvm-cov nextest --workspace \
            --features spades-server/insecure-fast-hash \
            --json --output-path "$REPORT_FILE" >&2
    else
        echo "==> cargo llvm-cov test --workspace (~1-2min; install cargo-nextest to parallelize)" >&2
        cargo llvm-cov test --workspace \
            --features spades-server/insecure-fast-hash \
            --json --output-path "$REPORT_FILE" >&2
    fi

    if [ ! -f "$REPORT_FILE" ]; then
        echo "error: cargo-llvm-cov did not produce $REPORT_FILE" >&2
        exit 1
    fi
}

# Per-crate line coverage (one decimal, truncated) from the llvm-cov JSON
# export: .data[0].files[] entries carry an absolute .filename and
# .summary.lines.{count,covered}.
crate_pct() {
    local crate="$1"
    jq -r --arg crate "$crate" '
        [ .data[0].files[]
          | select(.filename | contains("/crates/" + $crate + "/src/"))
          | .summary.lines ]
        | { covered: ([.[] | .covered] | add // 0),
            count:   ([.[] | .count]   | add // 0) }
        | if .count == 0 then "0.0"
          else (((.covered * 1000) / .count | floor) / 10 | tostring)
          end
    ' "$REPORT_FILE"
}

baseline_pct() {
    local crate="$1"
    jq -r --arg crate "$crate" '.[$crate].line_coverage_pct // 0' "$BASELINE_FILE"
}
