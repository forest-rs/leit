# leit-index

Phase 1 indexing and segment access for Leit.

This crate provides:

- `InMemoryIndexBuilder` to build an in-memory inverted index
- `InMemoryIndex` to hold immutable retrieval data
- `SegmentView` to validate and read serialized segments from `&[u8]`
- `ExecutionWorkspace` to plan and execute queries with reusable scratch state
- `SearchScorer` to choose the ranking policy for execution

The public surface stays small. Query planning lives in `leit-query`, but most
callers can stay at the `leit-index` layer by planning and executing through an
`ExecutionWorkspace`.

Typical Phase 1 flow:

```rust
use leit_collect::{collectors, CountCollector, TopKCollector};
use leit_index::{ExecutionWorkspace, NoFilter, PlanOptions, SearchScorer};

let mut workspace = ExecutionWorkspace::new();
let plan = workspace.plan(
    &index,
    "title:rust OR body:retrieval",
    PlanOptions::default(),
    &NoFilter,
)?;
let mut top_k = TopKCollector::new(10);
let mut count = CountCollector::new();
let mut collectors = collectors([&mut top_k, &mut count]);
workspace.execute(
    &index,
    &plan,
    Some(SearchScorer::bm25()),
    &NoFilter,
    &mut collectors,
)?;
```

`ExecutionWorkspace` accepts one `PlanOptions` value on every planning and
search call. Use `PlanOptions::default()` for ordinary queries; provide
field weights when BM25F ranking of unqualified terms expanded across multiple
default fields needs per-field control. Explicitly fielded terms and terms
planned against a single default field use weight `1.0`. Fields absent from the
map also use `1.0`; supplied weights must be finite and non-negative, and zero
is valid. The same options shape applies to textual and typed queries and to
custom-collector or convenience-search paths.

Planning controls live on the workspace boundary so it can combine
index-derived default fields, per-query configuration, and external filter
slots consistently for both textual and typed queries. Typed callers therefore
do not need to construct a lower-level `PlanningContext` or reproduce filter
wrapping. One options value also keeps new controls from multiplying the
workspace method surface.

Migration from the earlier weighted methods is direct: pass
`PlanOptions::new().with_default_field_weights(weights)` to `plan`,
`plan_program`, `search`, or `search_program` instead of calling
`plan_with_field_weights` or `search_bm25f_with_field_weights`. `PlanOptions`
fields are private so future planning controls can be added without breaking
callers. Existing calls gain a `PlanOptions::default()` argument, and
`SearchScorer::bm25f()` still selects BM25F scoring.

The complete version of this flow is compiled and run by the workspace's
[`basic_search` example](https://github.com/forest-rs/leit/blob/main/examples/basic_search/src/main.rs). The analyzer
configured for a field is applied while indexing and while resolving query
terms, so both sides use the same normalization rules.

This crate is structured for `no_std + alloc` builds, with `std` enabled by
default. The `mmap` feature adds memory-mapped segment access and requires
`std`.

`InMemoryIndex` is immutable after construction. `ExecutionWorkspace` currently
executes plans only against `InMemoryIndex`; `SegmentIndex` is a thin wrapper
around `SegmentView` while the serialized format is still gaining the metadata
needed for direct execution. There is no durable update or delete lifecycle in
Phase 1.

## Segment Format

The Phase 1 segment format contains:

- fixed magic and version
- document, term, and field counts
- a section directory
- sections for term dictionary, field metadata, postings metadata, and postings payload

`SegmentView::open` validates the buffer once and then exposes borrowed access to
the declared sections.

## Features

- `std` - Enable standard library support (enabled by default)
- `mmap` - Enable memory-mapped segment access (also enables `std`)
- `bench-internals` - Expose benchmark-only posting snapshots
