# explicit_execution

Plan once, then execute that planned query explicitly.

It shows how to:

- build an in-memory index
- plan a query through `ExecutionWorkspace`
- inspect a little plan metadata before execution
- run the same plan once with both `TopKCollector` and `CountCollector`
- reuse the plan for a count-only execution path that does not require scoring
- read the execution stats recorded by the workspace

Run it from the workspace root:

```bash
cargo run -p explicit_execution
```
