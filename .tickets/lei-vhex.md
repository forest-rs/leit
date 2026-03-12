---
id: lei-vhex
status: closed
deps: [lei-z4d1]
links: []
created: 2026-03-12T01:12:50Z
type: task
priority: 2
assignee: Norman Nunley, Jr
---
# Reconcile execution API with Phase 1 intent

Align leit_index public execution/search APIs with the explicit workspace-plus-scorer direction captured in the handoff and design documents.

## Acceptance Criteria

The public execution path is explicit about planner output, workspace ownership, collector usage, and scorer selection or its temporary absence.\nSearch behavior tests are updated to match the chosen Phase 1 API.\nInvariant coverage confirms no ranking or reuse regressions.


## Notes

**2026-03-12T05:36:45Z**

Implemented explicit workspace-driven execution in leit_index. ExecutionWorkspace now owns planning and execution, SearchScorer makes ranking policy explicit, scorerless index search entry points are removed, and invariant coverage was updated across leit_index, leit-integration-tests, and leit_benchmark. Verified with cargo fmt --all, cargo test -p leit_index -p leit-integration-tests -p leit_benchmark, and cargo clippy -p leit_index -p leit-integration-tests -p leit_benchmark --all-targets -- -D warnings.
