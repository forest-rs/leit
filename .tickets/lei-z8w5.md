---
id: lei-z8w5
status: open
deps: [lei-z4d1]
links: []
created: 2026-03-12T01:12:50Z
type: task
priority: 2
assignee: Norman Nunley, Jr
---
# Add Phase 1 benchmark harness

Create a real benchmark/wind-tunnel style crate for the current in-memory Phase 1 stack so Phase 1 has an executable performance harness rather than only a spec README.

## Acceptance Criteria

A benchmark crate exists in the workspace and builds.\nIt runs at least one deterministic indexing/query workload over the current stack.\nDocs explain how to run it and keep benchmark concerns out of kernel crates.

