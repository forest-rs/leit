# Leit

A modular Rust search library.

Leit is a workspace of small crates for building full-text retrieval systems.
The current codebase implements a Phase 1 in-memory search stack with:

- query planning
- Unicode-aware text analysis
- BM25 and BM25F scoring
- postings traversal
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

The integration-test crate is `std`-only by design.

## Workspace crates

- `leit_core`: shared identifiers, scores, hits, and workspace traits
- `leit_text`: tokenization and Unicode normalization
- `leit_query`: query construction and planning
- `leit_postings`: postings storage and cursor traits
- `leit_score`: lexical scoring algorithms
- `leit_collect`: result collectors
- `leit_fusion`: result fusion
- `leit_index`: in-memory indexing, search, and segment access
- `leit-integration-tests`: cross-crate integration coverage

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

## License

Licensed under either Apache-2.0 or MIT.
