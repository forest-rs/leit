---
id: lei-fp20
status: closed
deps: [lei-z4d1]
links: []
created: 2026-03-12T01:12:50Z
type: task
priority: 1
assignee: Norman Nunley, Jr
---
# Canonicalize execution query handles

Refactor leit_query and leit_index so execution-facing query state uses canonical TermId handles after planning, matching the Phase 1 handoff requirement.

## Acceptance Criteria

Planner and execution tests prove equivalent terms resolve to stable canonical TermId handles.\nleit_index no longer depends on raw term strings once planning completes.\nInvariant tests cover connected, acyclic planned programs and canonical handle stability.


## Notes

**2026-03-12T02:19:52Z**

Implemented the execution-facing query type rename: user-facing builder output is now UserQueryProgram/UserQueryNode, while canonical TermId-based planned programs are now QueryProgram/QueryNode. Added a planner contract test asserting equivalent textual terms lower to the same canonical term handle. Verified with cargo test -p leit_query -p leit_index -p leit-integration-tests and cargo clippy -p leit_query -p leit_index --all-targets -- -D warnings.

**2026-03-12T05:37:57Z**

Implemented in commit 84b07c2. Execution-facing query programs now use canonical TermId handles with added planner and invariant coverage.
