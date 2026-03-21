#!/usr/bin/env bash
# Run clippy for no_std targets using the local architecture.
set -euo pipefail

arch=$(uname -m)
case "$arch" in
    arm64|aarch64) target=aarch64-unknown-none ;;
    x86_64)        target=x86_64-unknown-none ;;
    *)             echo "unknown arch: $arch"; exit 1 ;;
esac

mise exec -- rustup target add "$target" 2>/dev/null || true

mise exec -- cargo hack clippy \
    --exclude leit_index \
    --exclude leit_benchmark \
    --exclude leit_integration_tests \
    --exclude basic_search \
    --exclude explicit_execution \
    --workspace --locked --optional-deps --each-feature \
    --exclude-features std,default,icu \
    --target "$target" \
    -- -D warnings
