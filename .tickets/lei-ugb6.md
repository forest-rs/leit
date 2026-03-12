---
id: lei-ugb6
status: open
deps: [lei-z4d1]
links: []
created: 2026-03-12T01:12:50Z
type: task
priority: 2
assignee: Norman Nunley, Jr
---
# Complete layered postings cursor seam

Add the public block-aware cursor extension seam expected by the Phase 1 handoff without overbuilding Phase 2 compression work.

## Acceptance Criteria

leit_postings exposes DocCursor, TfCursor, and a block-aware extension seam.\nThe in-memory implementation either supports the extension minimally or reports unsupported behavior explicitly.\nInvariant tests cover existing cursor traversal guarantees and the public extension contract.

