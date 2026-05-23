#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
echo "=== cargo-deny (workspace) ==="
cargo deny check
echo "=== cargo-deny (runner) ==="
cd runner && cargo deny check --config ../deny.toml
