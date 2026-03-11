# leit-query

Query construction and planning for Leit.

This crate provides:

- `QueryBuilder` and fluent helpers for building query programs
- `QueryProgram` for inspection-friendly query trees
- `PlannedQueryProgram` and `ExecutionPlan` for execution-facing plans
- `Planner` for Phase 1 query parsing and lowering
- traits such as `FieldRegistry` and `TermDictionary` for planning context

The crate separates user-facing query structure from execution-facing plans so
index crates can depend on a calmer, validated surface.

## Running tests

From the workspace root:

```bash
cargo test -p leit_query
```
