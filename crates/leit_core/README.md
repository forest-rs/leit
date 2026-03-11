# leit-core

Core types and traits shared across the Leit workspace.

This crate provides:

- typed identifiers such as `FieldId`, `TermId`, and `QueryNodeId`
- `EntityId` for application-defined document identifiers
- `Score` for finite retrieval scores
- `ScoredHit` for scored results
- `ScratchSpace` and `Workspace` for reusable execution state

Most other Leit crates depend on this crate and should not redefine these
types locally.

This crate is `no_std`-ready. `std` is enabled by default, but embedded builds
can disable default features.
