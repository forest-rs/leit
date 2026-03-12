---
id: lei-ugb6
status: closed
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


## Notes

**2026-03-12T04:13:51Z**

Added a public block-aware cursor seam in leit_postings: BlockCursorState plus BlockCursor over the existing DocCursor/TfCursor layering. The in-memory cursor exposes singleton blocks rather than reporting unsupported. Verified with cargo test -p leit_postings, cargo test -p leit-integration-tests --test phase1_readiness, and cargo clippy -p leit_postings --all-targets -- -D warnings.

**2026-03-12T05:38:07Z**

Implemented in commit 21394a6. leit_postings now exposes BlockCursor and BlockCursorState, with invariant coverage for the singleton in-memory seam.
