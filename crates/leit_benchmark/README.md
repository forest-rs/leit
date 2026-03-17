# leit-benchmark

Deterministic wind-tunnel benchmark harnesses for the current Phase 1 Leit
stack.

This crate is intentionally separate from the kernel crates so timing and
benchmark-driver concerns do not leak into retrieval code.

Current scope:

- one fixed in-memory indexing/query scenario
- a small executable entry point
- stable result-shape tests for the benchmark fixture

Run it with:

```bash
cargo run -p leit_benchmark
```
