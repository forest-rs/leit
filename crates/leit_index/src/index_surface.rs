// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use leit_core::{FieldId, TermId};
use leit_query::{FieldRegistry, TermDictionary};

use crate::PostingEntry;

/// Planner-facing surface shared by searchable index representations.
///
/// This is the smallest common seam that [`crate::ExecutionWorkspace`] needs in
/// order to lower textual queries without depending on a concrete
/// `InMemoryIndex`.
pub trait PlanningIndex: FieldRegistry + TermDictionary {
    /// Visit every field available for planning.
    fn for_each_field(&self, f: &mut dyn FnMut(FieldId));

    /// Visit the default fields used for unfielded term expansion.
    fn for_each_default_field(&self, f: &mut dyn FnMut(FieldId));
}

/// Full index/search substrate shared by execution-capable index representations.
///
/// This trait is intentionally derived from the current `InMemoryIndex` search
/// surface rather than from the higher-level `ExecutionWorkspace` facade.
pub trait ExecutableIndex: PlanningIndex {
    /// Total number of indexed documents.
    fn document_count(&self) -> u32;

    /// Metadata for one field, if present.
    fn field_stats(&self, field: FieldId) -> Option<FieldStatsView>;

    /// Indexed length of one document within one field.
    fn field_doc_length(&self, doc_id: u32, field: FieldId) -> u32;

    /// Visit every indexed document identifier.
    fn for_each_doc(&self, f: &mut dyn FnMut(u32));

    /// Term metadata by canonical term identifier.
    fn term_entry(&self, term: TermId) -> Option<TermEntryView<'_>>;

    /// Postings for one term in doc-sorted order.
    fn postings(&self, term: TermId) -> Option<&[PostingEntry]>;

    /// Visit block summaries for one term, if available.
    fn for_each_posting_block(&self, term: TermId, f: &mut dyn FnMut(PostingBlockView));
}

/// Borrowed field statistics view used by [`ExecutableIndex`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldStatsView {
    /// Canonical field identifier.
    pub field_id: FieldId,
    /// Number of documents containing at least one token in this field.
    pub doc_count: u32,
    /// Total indexed term count across all documents for this field.
    pub total_terms: u32,
}

/// Borrowed term metadata view used by [`ExecutableIndex`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TermEntryView<'a> {
    /// Field that owns this term.
    pub field_id: FieldId,
    /// Canonical term identifier.
    pub term_id: TermId,
    /// Normalized term text.
    pub term_text: &'a str,
}

/// Block summary view used by [`ExecutableIndex`] pruning paths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PostingBlockView {
    /// Inclusive postings-slice start index for this block.
    pub start: usize,
    /// Exclusive postings-slice end index for this block.
    pub end: usize,
    /// Maximum term frequency within the block.
    pub max_term_freq: u32,
    /// Minimum document length represented in the block.
    pub min_doc_length: u32,
}
