#!/bin/bash
# Run kernel integration tests via QEMU
# Usage: ./run_tests.sh [test_name]
#   test_name: basic_boot, heap_allocation, file_system, should_panic, stack_overflow
#
# Environment:
#   TIMEOUT           Per-test timeout in seconds (default: 90)
#   CI                If set, enables CI-friendly output (no color)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
KERNEL_DIR="$SCRIPT_DIR/kernel"
RUNNER_DIR="$SCRIPT_DIR/runner"
TEST_RUNNER="$RUNNER_DIR/target/x86_64-unknown-linux-gnu/debug/test-runner"
TARGET_DIR="$SCRIPT_DIR/target/x86_64-unknown-none/debug/deps"

TIMEOUT="${TIMEOUT:-90}"
PASSED=0
FAILED=0
START_TIME=$(date +%s)

# Color helpers (off in CI)
if [ -n "${CI:-}" ]; then
    BOLD=""; RED=""; GREEN=""; YELLOW=""; NC=""
else
    BOLD="$(tput bold 2>/dev/null || echo '')"
    RED="$(tput setaf 1 2>/dev/null || echo '')"
    GREEN="$(tput setaf 2 2>/dev/null || echo '')"
    YELLOW="$(tput setaf 3 2>/dev/null || echo '')"
    NC="$(tput sgr0 2>/dev/null || echo '')"
fi

say() { echo -e "${BOLD}==>${NC} $*"; }
ok()  { echo -e "    ${GREEN}✓${NC} $*"; }
fail() { echo -e "    ${RED}✗${NC} $*"; }

# Build the test-runner if needed
build_test_runner() {
    if [ ! -f "$TEST_RUNNER" ]; then
        say "Building test-runner..."
        (cd "$RUNNER_DIR" && cargo build --bin test-runner)
    fi
}

# Build the kernel first (needed for runner's build.rs cache)
build_kernel() {
    say "Building kernel..."
    (cd "$KERNEL_DIR" && cargo build --target x86_64-unknown-none)
}

run_test() {
    local test_name="$1"
    say "Running test: ${test_name}"

    # Build the test binary
    (cd "$KERNEL_DIR" && cargo build --test "$test_name" --target x86_64-unknown-none)

    # Find the test binary
    local binary
    binary=$(ls "$TARGET_DIR/${test_name}-"* 2>/dev/null | grep -v '\.d$' | head -1)

    if [ -z "$binary" ]; then
        fail "Could not find test binary for ${test_name}"
        FAILED=$((FAILED + 1))
        return 1
    fi

    # Run via test-runner with timeout
    local exit_code=0
    timeout "$TIMEOUT" "$TEST_RUNNER" "$binary" || exit_code=$?

    case $exit_code in
        0)
            ok "${test_name}"
            PASSED=$((PASSED + 1))
            return 0
            ;;
        1)
            fail "${test_name} (FAILED)"
            FAILED=$((FAILED + 1))
            return 1
            ;;
        124)
            fail "${test_name} (TIMEOUT after ${TIMEOUT}s)"
            FAILED=$((FAILED + 1))
            return 1
            ;;
        *)
            fail "${test_name} (ERROR: exit code ${exit_code})"
            FAILED=$((FAILED + 1))
            return 1
            ;;
    esac
}

print_summary() {
    local elapsed=$(($(date +%s) - START_TIME))
    local total=$((PASSED + FAILED))

    echo ""
    echo "${BOLD}────────────────────────────────────────${NC}"
    echo -n "${BOLD}Results:${NC} "
    if [ "$FAILED" -eq 0 ]; then
        echo "${GREEN}All ${total} tests passed${NC} (${elapsed}s)"
    else
        echo "${RED}${FAILED}/${total} failed${NC}, ${GREEN}${PASSED} passed${NC} (${elapsed}s)"
    fi
    echo "${BOLD}────────────────────────────────────────${NC}"
}

# --- Main ---

build_kernel
build_test_runner

if [ -n "${1:-}" ]; then
    run_test "$1" || true
    print_summary
    [ "$FAILED" -eq 0 ] || exit 1
else
    for test in basic_boot heap_allocation file_system should_panic; do
        run_test "$test" || true
    done

    # stack_overflow is flaky — skip by default
    # run_test stack_overflow || true

    print_summary
    [ "$FAILED" -eq 0 ] || exit 1
fi
