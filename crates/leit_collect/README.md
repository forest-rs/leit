# leit-collect

Collectors for Leit search results.

This crate provides:

- `Collector` as the result collection trait
- `TopKCollector` for bounded top-k collection with skip checks
- `CountCollector` for counting matching hits

`TopKCollector` keeps the current minimum score so query execution can skip
hits that cannot enter the result set.

## Running tests

From the workspace root:

```bash
cargo test -p leit_collect
```
