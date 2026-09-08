# Typed query planning

The workspace typed entry points follow the query-owned
[typed planning decision](../../leit_query/docs/adr-0001-typed-query-planning.md),
including dictionary-defined term analysis, Phase 1 phrase approximation, boost validation,
and migration guidance. All declared external filter slots wrap the typed plan.

`ExecutionWorkspace` owns the high-level integration boundary: it derives
default fields from the index, applies per-query `PlanOptions`, and attaches
external filter slots before delegating typed lowering to `leit_query`. Typed
consumers should use `plan_program` rather than construct a `PlanningContext`
and reproduce those steps.
