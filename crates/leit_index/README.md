# leit-index

Phase 1 indexing and segment access for Leit.

This crate provides:

- `InMemoryIndexBuilder` to build an in-memory inverted index
- `InMemoryIndex` to search that index
- `SegmentView` to validate and read serialized segments from `&[u8]`
- `ExecutionWorkspace` to reuse search scratch state

The public surface stays small. Query planning lives in `leit-query`, but most
callers can search through `InMemoryIndex` without building planner contexts.

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
