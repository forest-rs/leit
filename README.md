# Leit

A modular Rust library for building small, in-memory lexical search systems.

Leit is a workspace of small crates for building full-text retrieval systems.
The current codebase implements a Phase 1 in-memory search stack with:

- query planning
- Unicode-aware text analysis
- explicit scorer selection in the in-memory index execution path
- BM25 scoring wired through `leit_index` via explicit scorer selection
- BM25F scoring with cross-field aggregation and configurable per-field weights
- postings storage and cursor traits (execution currently uses index-internal
  postings; cursor-based traversal is Phase 2 work)
- top-k collection
- reciprocal-rank fusion
- in-memory indexing and segment validation

The crate boundaries are intentional. Each crate owns one concern and exposes a
small public surface.

## Quick start

The complete, runnable introduction is the [`basic_search` example](examples/basic_search/src/main.rs). It configures per-field Unicode analyzers, builds an immutable in-memory index, and runs ranked queries with BM25:

```bash
cargo run -p basic_search
```

For explicit planning, multiple collectors, and execution statistics, run the [`explicit_execution` example](examples/explicit_execution/src/main.rs).

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

The workspace is centered on the in-memory Phase 1 path. `InMemoryIndex` is
immutable after construction; rebuilding an index is the current replacement
workflow, and there is no durable update or delete lifecycle yet. `SegmentView`
validates borrowed serialized data, while `SegmentIndex` is currently a thin
wrapper and is not an execution backend for `ExecutionWorkspace`.

The crate boundaries leave room for additional storage backends, analysis
strategies, and scoring methods as later phases add them.

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
