---
id: lei-vhex
status: open
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

