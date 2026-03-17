# Leit

A modular Rust search library.

Leit is a workspace of small crates for building full-text retrieval systems.
The current codebase implements a Phase 1 in-memory search stack with:

- query planning
- Unicode-aware text analysis
- explicit scorer selection in the in-memory index execution path
- BM25 scoring wired through `leit_index` via explicit scorer selection
- BM25F scorer available; per-field scoring works but cross-field aggregation
  is not yet wired into the execution path
- postings storage and cursor traits (execution currently uses index-internal
  postings; cursor-based traversal is Phase 2 work)
- top-k collection
- reciprocal-rank fusion
- in-memory indexing and segment validation

The crate boundaries are intentional. Each crate owns one concern and exposes a
small public surface.

## `no_std` and `alloc`

The library crates are designed to work in `no_std` environments. They enable
`std` by default, but the core search crates can be built with
`default-features = false` for `no_std + alloc` targets.

That applies to the main library path:

- `leit_core`
- `leit_text`
- `leit_query`
- `leit_postings`
- `leit_score`
- `leit_collect`
- `leit_fusion`
- `leit_index`

The integration-test crate is part of the workspace for cross-crate coverage.
Its test suites run under `std`, even though the crate itself can be built with
default features disabled.

## Workspace crates

- `leit_core`: shared identifiers, scores, hits, and workspace traits
- `leit_text`: tokenization and Unicode normalization
- `leit_query`: query construction and planning
- `leit_postings`: postings storage and cursor traits
- `leit_score`: lexical scoring algorithms
- `leit_collect`: result collectors
- `leit_fusion`: result fusion
- `leit_index`: in-memory indexing, explicit query execution, and segment access
- `leit_integration_tests`: cross-crate integration coverage

## Current status

The workspace is centered on the in-memory Phase 1 path. The public APIs and
tests are already set up so later phases can swap in more storage backends,
analysis strategies, and scoring methods without collapsing the crate
boundaries.

## Verification

From the workspace root:

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
cargo doc --no-deps
```

The library crates also support `no_std + alloc` builds with:

```bash
cargo build --workspace --exclude leit_integration_tests --exclude leit_benchmark --no-default-features
```

## PR Preparation

Before pushing a PR update, run the same local gates that CI expects:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo doc --workspace --locked --all-features --no-deps --document-private-items
cargo test --workspace --all-features
```

If a change updates dependencies or workspace membership, make sure the relevant
commit also includes the corresponding `Cargo.lock` update.

For the full PR-prep workflow, including commit hygiene and optional Jujutsu
notes for `jj`-managed Git repos, see [docs/pr-preparation.md](docs/pr-preparation.md).

## License

Licensed under either Apache-2.0 or MIT.
