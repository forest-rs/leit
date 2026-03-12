---
id: lei-zyol
status: open
deps: [lei-z4d1]
links: []
created: 2026-03-12T01:12:50Z
type: task
priority: 1
assignee: Norman Nunley, Jr
---
# Settle reusable collector lifecycle

Give leit_collect an explicit reusable per-query lifecycle and threshold contract, then update index execution to use it.

## Acceptance Criteria

Collector API exposes explicit begin/finish-style lifecycle and threshold semantics.\nA reused TopKCollector does not leak state across queries.\nInvariant tests cover collector reuse, threshold behavior, and deterministic ordering.


## Notes

**2026-03-12T02:24:47Z**

Added explicit collector lifecycle and threshold semantics. Collector now exposes begin_query, threshold, and finish; can_skip derives from threshold. TopKCollector clears state between queries and CountCollector resets per query. Verified with cargo test -p leit_collect, cargo test -p leit_index -p leit-integration-tests --test phase1_readiness, and cargo clippy -p leit_collect -p leit_index -p leit-integration-tests --all-targets -- -D warnings.
