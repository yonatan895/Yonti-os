#!/bin/bash
# Run kernel integration tests via QEMU.
# Usage: ./run_tests.sh [test_name]
#   test_name: all, should_panic
#
# Supports Cargo-built ELFs.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
RUNNER_DIR="$SCRIPT_DIR/runner"
KERNEL_DIR="$SCRIPT_DIR/kernel"
TEST_RUNNER="$RUNNER_DIR/target/x86_64-unknown-linux-gnu/debug/test-runner"
TARGET_DIR="$SCRIPT_DIR/target/x86_64-unknown-none/debug/deps"
TARGET="x86_64-unknown-none"

TIMEOUT="${TIMEOUT:-90}"
PASSED=0
FAILED=0
START_TIME=$(date +%s)

if [ -n "${CI:-}" ]; then
    BOLD=""; RED=""; GREEN=""; NC=""
else
    BOLD="$(tput bold 2>/dev/null || echo '')"
    RED="$(tput setaf 1 2>/dev/null || echo '')"
    GREEN="$(tput setaf 2 2>/dev/null || echo '')"
    NC="$(tput sgr0 2>/dev/null || echo '')"
fi

say() { echo -e "${BOLD}==>${NC} $*"; }
ok()  { echo -e "    ${GREEN}✓${NC} $*"; }
fail() { echo -e "    ${RED}✗${NC} $*"; }

build_test_runner() {
    if [ ! -f "$TEST_RUNNER" ]; then
        say "Building test-runner..."
        (cd "$RUNNER_DIR" && cargo build --no-default-features --bin test-runner)
    fi
}

build_all_elfs() {
    say "Building test ELFs (cargo)..."
    (cd "$KERNEL_DIR" && cargo build --tests --target "$TARGET")
}

find_elf() {
    local test_name="$1"
    # Cargo-built ELFs
    ls -t "$TARGET_DIR/${test_name}-"* 2>/dev/null | grep -v '\.d$' | head -1
}

run_one_test() {
    local test_name="$1"
    say "Running test: ${test_name}"

    local binary
    binary=$(find_elf "$test_name")

    if [ -z "$binary" ]; then
        fail "Binary not found for ${test_name}"
        FAILED=$((FAILED + 1))
        return 1
    fi

    local exit_code=0
    timeout "$TIMEOUT" "$TEST_RUNNER" "$binary" || exit_code=$?

    case $exit_code in
        0) ok "${test_name}" ; PASSED=$((PASSED + 1)) ;;
        1) fail "${test_name} (FAILED)" ; FAILED=$((FAILED + 1)) ;;
        124) fail "${test_name} (TIMEOUT)" ; FAILED=$((FAILED + 1)) ;;
        *) fail "${test_name} (exit ${exit_code})" ; FAILED=$((FAILED + 1)) ;;
    esac
}

print_summary() {
    local t=$((PASSED + FAILED))
    local e=$(($(date +%s) - START_TIME))
    echo ""
    echo "${BOLD}────────────────────────────────────────${NC}"
    echo -n "${BOLD}Results:${NC} "
    if [ "$FAILED" -eq 0 ]; then
        echo "${GREEN}All ${t} passed${NC} (${e}s)"
    else
        echo "${RED}${FAILED}/${t} failed${NC} (${e}s)"
    fi
    echo "${BOLD}────────────────────────────────────────${NC}"
}

# ── Main ───────────────────────────────────────────────────────────

build_test_runner
build_all_elfs

if [ -n "${1:-}" ]; then
    run_one_test "$1" || true
else
    run_one_test "all" || true
    run_one_test "should_panic" || true
fi

print_summary
[ "$FAILED" -eq 0 ] || exit 1
