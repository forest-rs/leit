# Examples

Small runnable examples for the current Leit stack.

- [`basic_search`](./basic_search): minimal end-to-end indexing and search, including field aliases, explicit scorer selection, and the default Unicode normalization path.
- [`explicit_execution`](./explicit_execution): plan a query once, then execute it explicitly with different collectors while inspecting plan metadata and execution stats.

Run any example from the workspace root:

```bash
cargo run -p basic_search
cargo run -p explicit_execution
```
