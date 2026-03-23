# leit-index

Phase 1 indexing and segment access for Leit.

This crate provides:

- `InMemoryIndexBuilder` to build an in-memory inverted index
- `InMemoryIndex` to hold immutable retrieval data
- `SegmentView` to validate and read serialized segments from `&[u8]`
- `ExecutionWorkspace` to plan and execute queries with reusable scratch state
- `Option<SearchScorer>` to choose scored or unscored execution
- `SearchScorer` to choose the ranking policy for execution

The public surface stays small. Query planning lives in `leit-query`, but most
callers can stay at the `leit-index` layer by planning and executing through an
`ExecutionWorkspace`.

Typical Phase 1 flow:

```rust
use leit_collect::{CountCollector, TopKCollector};
use leit_index::{ExecutionWorkspace, SearchScorer};

let mut workspace = ExecutionWorkspace::new();
let plan = workspace.plan(&index, "title:rust OR body:retrieval")?;
let mut top_k = TopKCollector::new(10);
let mut count = CountCollector::new();
let mut collectors: [&mut dyn leit_collect::Collector<u32>; 2] = [&mut top_k, &mut count];
workspace.execute(
    &index,
    &plan,
    Some(SearchScorer::bm25()),
    &mut collectors,
)?;
let hits = top_k.finish();
let count = count.finish();
```

This crate is structured for `no_std + alloc` builds, with `std` enabled by
default for the current Phase 1 path.

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
- `alloc` - Enable alloc support (automatically enabled with `std`)
- `serde` - Enable serde serialization support
