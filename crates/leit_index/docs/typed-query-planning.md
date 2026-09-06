# Typed query planning

The workspace typed entry points follow the query-owned
[typed planning decision](../../leit_query/docs/adr-0001-typed-query-planning.md),
including dictionary-defined term analysis, Phase 1 phrase approximation, boost validation,
and migration guidance. All declared external filter slots wrap the typed plan.
