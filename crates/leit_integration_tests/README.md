# Leit Integration Tests

Cross-crate integration tests for the Leit workspace.

## Structure

- `tests/phase1_readiness.rs` checks the core Phase 1 seams across crates.
- `tests/unicode_pipeline.rs` checks Unicode normalization and search behavior.

## Running Tests

From the workspace root:
```bash
cargo test -p leit-integration-tests
```

To run one suite:
```bash
cargo test -p leit-integration-tests --test phase1_readiness
```

## Test Coverage

These tests cover behaviors that cross crate boundaries:

- core entities and score semantics
- BM25 and BM25F reference behavior
- collector behavior
- query planning and execution seams
- Unicode analysis and search matching
