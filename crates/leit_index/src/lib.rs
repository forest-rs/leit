// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

#![no_std]

//! Index construction and segment access for Leit.
//!
//! Phase 1 keeps this crate concrete:
//! - `InMemoryIndex` builds a small in-memory inverted index
//! - `ExecutionWorkspace` plans and executes queries against that index
//! - `Option<SearchScorer>` chooses scored or unscored execution per query
//! - `SearchScorer` makes ranking policy explicit at execution time
//! - `SegmentView` opens and validates a borrowed segment from `&[u8]`
//!
//! The borrowed-open seam is the important extension point for future
//! acquisition crates such as mmap-backed segment loaders.

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

mod builder;
mod codec;
mod cursor;
mod error;
mod index_surface;
mod memory;
mod merge;
mod merge_policy;
#[cfg(feature = "bench-internals")]
mod reference_execution;
mod search;
mod segment;
mod segment_format;
mod segment_index;
mod serialization;

pub use builder::{InMemoryIndexBuilder, IndexBuilder};
pub use error::{IndexError, SegmentError, ValidationMode};
pub use index_surface::{
    ExecutableIndex, FieldStatsView, PlanningIndex, PostingBlockView, TermEntryView,
};
pub use leit_core::{FilterEvaluator, FilterSlotId, NoFilter};
pub use leit_postings::codec::CodecId;
pub use leit_query::{BooleanOp, QueryBuilder, UserQueryNode, UserQueryProgram};
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub use memory::BenchmarkScratchCapacities;
pub use memory::{InMemoryIndex, PostingEntry};
pub use merge::{MergeError, MergeRejected, MergedIndex, PreparedMerge, prepare_merge};
pub use merge_policy::{SegmentSummary, select_merge_candidates};
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub use reference_execution::ReferenceExecutionIndex;
pub use search::{ExecutionStats, ExecutionWorkspace, SearchScorer};
#[expect(
    deprecated,
    reason = "DirectorySegmentView is a deprecated Phase 1 artifact; kept for frozen compatibility"
)]
pub use segment::DirectorySegmentView;
#[expect(
    deprecated,
    reason = "SectionKind is a deprecated Phase 1 artifact; kept for frozen compatibility"
)]
pub use segment::SectionKind;
pub use segment_format::migrate::migrate_to_current;
#[cfg(feature = "mmap")]
pub use segment_format::mmap::{MmapError, MmapSegment, MmapSegmentViewBuilder};
pub use segment_format::{
    BlockMetadataReader, FORMAT_VERSION, FieldTableReader, HEADER_SIZE, LexiconReader, MAGIC,
    PostingsDataReader, PostingsTableReader, SegmentHeader, SegmentView,
};
pub use segment_index::SegmentIndex;
pub use serialization::{PreparedSegment, SegmentWriteError, prepare_serialization};
