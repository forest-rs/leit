#!/usr/bin/env bash
# Run clippy for wasm32 target.
set -euo pipefail

mise exec -- rustup target add wasm32-unknown-unknown 2>/dev/null || true

mise exec -- cargo hack clippy \
    --exclude leit_benchmark \
    --exclude leit_integration_tests \
    --workspace --locked --target wasm32-unknown-unknown \
    --optional-deps --each-feature --ignore-unknown-features \
    --features std \
    -- -D warnings
