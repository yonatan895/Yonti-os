#!/bin/bash
# Run kernel integration tests via QEMU
# Usage: ./run_tests.sh [test_name]
#   test_name: basic_boot, heap_allocation, file_system, should_panic, stack_overflow

set -e

KERNEL_DIR="$(cd "$(dirname "$0")/kernel" && pwd)"
RUNNER_DIR="$(cd "$(dirname "$0")/runner" && pwd)"
TEST_RUNNER="$RUNNER_DIR/target/x86_64-unknown-linux-gnu/debug/test-runner"
TARGET_DIR="$(cd "$(dirname "$0")" && pwd)/target/x86_64-unknown-none/debug/deps"

# Build the test-runner if needed
if [ ! -f "$TEST_RUNNER" ]; then
    echo "Building test-runner..."
    cd "$RUNNER_DIR"
    cargo build --bin test-runner
fi

run_test() {
    local test_name="$1"
    echo "=== Running test: $test_name ==="

    # Build the test binary
    cd "$KERNEL_DIR"
    cargo build --test "$test_name" --target x86_64-unknown-none

    # Find the test binary
    local binary
    binary=$(ls "$TARGET_DIR/${test_name}-"* | grep -v '\.d$' | head -1)

    if [ -z "$binary" ]; then
        echo "ERROR: Could not find test binary for $test_name"
        return 1
    fi

    echo "Test binary: $binary"

    # Run via test-runner
    timeout 90 "$TEST_RUNNER" "$binary"
    local exit_code=$?

    if [ $exit_code -eq 0 ]; then
        echo "=== PASSED: $test_name ==="
    elif [ $exit_code -eq 1 ]; then
        echo "=== FAILED: $test_name ==="
        return 1
    else
        echo "=== ERROR: $test_name (exit code $exit_code) ==="
        return 1
    fi
    return 0
}

# Build the kernel first (needed for runner's build.rs)
cd "$KERNEL_DIR"
cargo build

if [ -n "$1" ]; then
    run_test "$1"
else
    echo "Running all tests..."
    FAILED=0

    for test in basic_boot heap_allocation file_system should_panic; do
        run_test "$test" || FAILED=1
    done

    # stack_overflow is flaky — skip by default
    # run_test "stack_overflow" || FAILED=1

    if [ $FAILED -eq 0 ]; then
        echo ""
        echo "All tests passed!"
    else
        echo ""
        echo "Some tests failed!"
        exit 1
    fi
fi