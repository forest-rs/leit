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
//!
//! # Quick start
//!
//! Configure an analyzer for each indexed field, register the names used by
//! textual queries, build the immutable in-memory index, and search it:
//!
//! ```
//! use leit_core::FieldId;
//! use leit_index::{
//!     ExecutionWorkspace, InMemoryIndexBuilder, NoFilter, PlanOptions, SearchScorer,
//! };
//! use leit_text::{Analyzer, FieldAnalyzers, UnicodeNormalizer, WhitespaceTokenizer};
//!
//! # fn main() -> Result<(), leit_index::IndexError> {
//! let title = FieldId::new(1);
//! let mut analyzers = FieldAnalyzers::new();
//! analyzers.set(
//!     title,
//!     Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
//! );
//!
//! let mut builder = InMemoryIndexBuilder::new(analyzers);
//! builder.register_field_alias(title, "title");
//! builder.index_document(1, &[(title, "Rust retrieval")])?;
//! let index = builder.build_index();
//!
//! let mut workspace = ExecutionWorkspace::new();
//! let hits = workspace.search(
//!     &index,
//!     "title:rust",
//!     10,
//!     SearchScorer::bm25(),
//!     PlanOptions::default(),
//!     &NoFilter,
//! )?;
//! assert_eq!(hits.len(), 1);
//! # Ok(())
//! # }
//! ```

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
pub use search::{ExecutionStats, ExecutionWorkspace, PlanOptions, SearchScorer};
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
