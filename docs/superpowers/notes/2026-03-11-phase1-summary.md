# Phase 1 Summary

## Scope

This change finishes the Phase 1 in-memory index split, fixes the verified
correctness bugs, and tightens the workspace test and lint baseline.

## Key decisions

- `leit_index` keeps a build-time and query-time split through
  `InMemoryIndexBuilder`, `InMemoryIndex`, and `ExecutionWorkspace`.
- `PlannedQueryProgram::try_new` now rejects invalid graphs, including cycles
  and unreachable nodes.
- `Score` now preserves its finite invariant by saturating arithmetic to the
  valid range instead of allowing `NaN` or infinity.
- Text normalization now uses Unicode canonicalization plus configurable case
  mapping through `UnicodeNormalizer`.
- Full case folding is available as an opt-in mode. NFC plus lowercase remains
  the default.

## Verification

- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`
- `cargo doc --no-deps`

`taplo` and `typos` are still required by project policy, but those binaries
are not installed in this environment.
