---
id: lq-fq2p
status: closed
deps: []
links: []
created: 2026-09-06T03:44:41Z
type: bug
priority: 1
assignee: Bruce Mitchener
---
# Harden typed query planning for landing

Review b99befc typed-query entry points, reproduce context boost validation gaps, and pin boundary behavior.

## Design

Keep signatures and dependencies unchanged. Validate the effective typed term boost including PlanningContext default_boost; document phrase approximation and migration semantics in a query-owned ADR.

## Acceptance Criteria

Regression rejects non-finite and negative effective boosts; cycle and literal/filter coverage passes; workspace fmt, taplo, typos, clippy, tests and docs pass.


## Notes

**2026-09-06T03:51:56Z**

Review scope: detailed review of b99befc against 2aa33d8 (six-file typed-query slice); full-stack workspace validation against origin/main 9116f81. The remote branch carries seven prerequisite commits (merge/serialization and reusable execution). Those prerequisites were inspected selectively, not given a complete independent correctness or safety audit. Recommendation: land the typed slice with this fix once its prerequisite stack is approved; no additional typed-query blocker found.

Must, fixed: lower_user_node validated explicit boost chains but lower_term_node subsequently multiplied by unchecked context.default_boost. A regression on the original code returned Ok with NaN term multipliers; finite MAX * 2 also overflowed unchecked. Validate effective term/phrase multipliers before shared lowering. Zero and ordinary finite products still work. No signatures, production dependencies, or unsafe code added.

Should, addressed: phrase builder documentation incorrectly said the node was representation-only. Builder/workspace docs now state AND approximation, ignored order/slop, cross-field matching, syntax-free term lookup with dictionary-defined analysis, and possible planning errors. Query-owned ADR 0001 captures these public semantics, migration for the new InvalidBoost variant and error node meaning, and an index-local document links to it.

Should, follow-up: max_depth metadata excludes TermExpansion levels (a two-field single term reports depth 1 despite a two-level plan), matching the pre-existing textual path. No current executor consumes this metadata for correctness. Reconcile the two planners and QueryProgram documentation in a follow-up; do not delay this typed slice for that.

Could: replace the five-second wall-clock assertion in shared_diamond_dag_plans_quickly with a deterministic work-bound check. Planning still allocates and expands shared subtrees within the node budget; reusable execution is not allocation-free planning.

Unsafe Watch: no unsafe in the typed slice or review fix. The prerequisite stack includes a benchmark GlobalAlloc wrapper and mmap test-lifetime changes; full-stack passing tests do not substitute for a complete independent safety audit.

Validation: reproduced the invalid-boost regression failing before the fix; cargo test -p leit_query --lib passed after the fix. Final cargo test --workspace --all-features passed 529 tests before the documentation correction, 0 failed, 0 ignored. Added unit coverage for invalid context products, zero/finite boosts, boolean/boost cycles, literal colon/parenthesis/operator terms, multiple filter slots, BM25/BM25F, and workspace reuse after errors and filtering. cargo clippy --workspace --all-targets --all-features -- -D warnings, cargo fmt --all -- --check, taplo fmt --check, typos, cargo doc --no-deps, cargo check -p leit_query -p leit_index --no-default-features, and git diff --check all passed. Tests ran on the full stacked branch in an isolated review worktree.

**2026-09-06T05:44:13Z**

Correction before landing: typed planning bypasses query syntax, but term analysis belongs to the dictionary implementation. InMemoryIndex applies its configured field analyzer and resolves exactly one resulting token. Corrected planner/workspace rustdoc, the ADR, index documentation, and earlier review wording. Added typed fielded/unfielded search coverage for uppercase and decomposed Unicode input, empty input, and multi-token input. No runtime behavior changed. Revalidated cargo test --workspace --all-features (530 passed, 0 failed, 0 ignored), strict workspace/all-target/all-feature clippy, cargo doc --no-deps, cargo fmt --all -- --check, taplo fmt --check, typos, and git diff --check; all passed. Amend the existing review fix commit and update origin with an explicit force-with-lease against eda6c042ede068b29e74db3bc9c8d2044f083cdb.
