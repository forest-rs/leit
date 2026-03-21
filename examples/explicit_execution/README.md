# explicit_execution

Plan once, then execute that planned query explicitly.

It shows how to:

- build an in-memory index
- plan a query through `ExecutionWorkspace`
- inspect a little plan metadata before execution
- run the same plan with `TopKCollector`
- run the same plan again with `CountCollector`
- read the execution stats recorded by the workspace

Run it from the workspace root:

```bash
cargo run -p explicit_execution
```
