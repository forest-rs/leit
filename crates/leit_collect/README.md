# leit-collect

Collectors for Leit search results.

This crate provides:

- `Collector` as the result collection trait
- `CollectorSink` as the execution-facing abstraction for one collector or many
- collector-level `needs_scores` and `requires_exhaustive_matches` flags
- `TopKCollector` for bounded top-k collection with skip checks
- `CountCollector` for counting matching hits
- object-safe collectors so one execution can drive any number of collectors

`TopKCollector` keeps the current minimum score so query execution can skip
hits that cannot enter the result set. `CountCollector` collects doc IDs
without requiring scores.

This crate works in `no_std + alloc`. `std` is enabled by default.

## Running tests

From the workspace root:

```bash
cargo test -p leit_collect
```
