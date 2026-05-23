#!/bin/bash
# Run kernel integration tests via QEMU (Bazel builds ELFs, Cargo builds test-runner).
# Usage: ./run_tests.sh [test_name]
#   test_name: all, should_panic
#
# Bazel-built ELFs: bazel-bin/kernel/{all_tests_elf,should_panic_elf}
# Cargo-built test-runner: runner/target/.../test-runner

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
RUNNER_DIR="$SCRIPT_DIR/runner"
TEST_RUNNER="$RUNNER_DIR/target/x86_64-unknown-linux-gnu/debug/test-runner"

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
        say "Building test-runner (cargo)..."
        (cd "$RUNNER_DIR" && cargo build --no-default-features --bin test-runner)
    fi
}

build_kernel_elfs() {
    say "Building kernel ELFs (bazel)..."
    bazel build --config=bare //kernel:all_tests_elf //kernel:should_panic_elf
}

run_one_test() {
    local test_name="$1"
    local binary_name="${test_name}_tests_elf"
    # should_panic binary doesn't have _tests suffix
    [ "$test_name" = "should_panic" ] && binary_name="${test_name}_elf"
    local binary="$SCRIPT_DIR/bazel-bin/kernel/${binary_name}"

    say "Running test: ${test_name}"
    if [ ! -f "$binary" ]; then
        fail "Binary not found: $binary"
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
build_kernel_elfs

if [ -n "${1:-}" ]; then
    run_one_test "$1" || true
else
    run_one_test "all" || true
    run_one_test "should_panic" || true
fi

print_summary
[ "$FAILED" -eq 0 ] || exit 1
