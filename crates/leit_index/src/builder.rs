// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;

use leit_core::{FieldId, TermId};
use leit_text::FieldAnalyzers;

use crate::error::IndexError;
use crate::memory::{FieldMetadata, InMemoryIndex, PostingBlock, PostingEntry, TermEntry};

/// Build-time contract for index construction.
pub trait IndexBuilder {
    /// Built index type produced by `finish`.
    type Output;

    /// Register a user-facing field name for query planning.
    fn register_field_name(&mut self, field_id: FieldId, name: &str);

    /// Add a single document to the builder.
    fn add_document(&mut self, doc_id: u32, fields: &[(FieldId, &str)]) -> Result<(), IndexError>;

    /// Finish building and return an immutable index artifact.
    fn finish(self) -> Self::Output;
}

#[derive(Debug)]
pub(crate) struct BuildState {
    pub(crate) documents: BTreeSet<u32>,
    pub(crate) terms_to_ids: BTreeMap<(FieldId, String), TermId>,
    pub(crate) term_entries: Vec<TermEntry>,
    pub(crate) postings: BTreeMap<TermId, Vec<PostingEntry>>,
    pub(crate) field_stats: BTreeMap<FieldId, FieldMetadata>,
    pub(crate) field_names: BTreeMap<String, FieldId>,
    pub(crate) field_doc_lengths: BTreeMap<(u32, FieldId), u32>,
    pub(crate) next_term_id: u32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct BlockConfig {
    pub(crate) postings_block_size: usize,
}

impl Default for BlockConfig {
    fn default() -> Self {
        Self {
            postings_block_size: crate::memory::DEFAULT_POSTINGS_BLOCK_SIZE,
        }
    }
}

impl BuildState {
    pub(crate) const fn new() -> Self {
        Self {
            documents: BTreeSet::new(),
            terms_to_ids: BTreeMap::new(),
            term_entries: Vec::new(),
            postings: BTreeMap::new(),
            field_stats: BTreeMap::new(),
            field_names: BTreeMap::new(),
            field_doc_lengths: BTreeMap::new(),
            next_term_id: 0,
        }
    }
}

/// Mutable Phase 1 index builder.
#[derive(Debug)]
pub struct InMemoryIndexBuilder {
    analyzers: FieldAnalyzers,
    block_config: BlockConfig,
    state: BuildState,
}

impl InMemoryIndexBuilder {
    /// Create an empty builder using the supplied per-field analyzers.
    pub const fn new(analyzers: FieldAnalyzers) -> Self {
        Self {
            analyzers,
            block_config: BlockConfig {
                postings_block_size: crate::memory::DEFAULT_POSTINGS_BLOCK_SIZE,
            },
            state: BuildState::new(),
        }
    }

    /// Register a user-facing field name for query planning.
    pub fn register_field_alias(&mut self, field_id: FieldId, name: &str) {
        <Self as IndexBuilder>::register_field_name(self, field_id, name);
    }

    /// Add a single document to the builder.
    pub fn index_document(
        &mut self,
        doc_id: u32,
        fields: &[(FieldId, &str)],
    ) -> Result<(), IndexError> {
        <Self as IndexBuilder>::add_document(self, doc_id, fields)
    }

    /// Finish building and return an immutable index artifact.
    pub fn build_index(self) -> InMemoryIndex {
        <Self as IndexBuilder>::finish(self)
    }
}

impl IndexBuilder for InMemoryIndexBuilder {
    type Output = InMemoryIndex;

    fn register_field_name(&mut self, field_id: FieldId, name: &str) {
        self.state.field_names.insert(name.into(), field_id);
    }

    fn add_document(&mut self, doc_id: u32, fields: &[(FieldId, &str)]) -> Result<(), IndexError> {
        if self.state.documents.contains(&doc_id) {
            return Err(IndexError::DuplicateDocument(doc_id));
        }

        let mut pending_fields = BTreeMap::<FieldId, (BTreeMap<String, u32>, u32)>::new();

        for &(field_id, text) in fields {
            let analyzer = self
                .analyzers
                .get(field_id)
                .ok_or(IndexError::MissingAnalyzer(field_id))?;
            let analyzed_tokens = analyzer.analyze(text);

            let mut frequencies = BTreeMap::<String, u32>::new();
            let mut field_token_count = 0_u32;
            for (_, normalized) in analyzed_tokens {
                field_token_count = field_token_count
                    .checked_add(1)
                    .ok_or(IndexError::ValueOutOfRange)?;
                let entry = frequencies.entry(normalized).or_insert(0);
                *entry = entry.checked_add(1).ok_or(IndexError::ValueOutOfRange)?;
            }
            let (existing_frequencies, existing_token_count) = pending_fields
                .entry(field_id)
                .or_insert_with(|| (BTreeMap::new(), 0));
            *existing_token_count = existing_token_count
                .checked_add(field_token_count)
                .ok_or(IndexError::ValueOutOfRange)?;
            for (term, term_freq) in frequencies {
                let entry = existing_frequencies.entry(term).or_insert(0);
                *entry = entry
                    .checked_add(term_freq)
                    .ok_or(IndexError::ValueOutOfRange)?;
            }
        }

        self.state.documents.insert(doc_id);

        for (field_id, (frequencies, field_token_count)) in pending_fields {
            self.state
                .field_doc_lengths
                .insert((doc_id, field_id), field_token_count);
            let stats = self
                .state
                .field_stats
                .entry(field_id)
                .or_insert(FieldMetadata {
                    field_id,
                    doc_count: 0,
                    total_terms: 0,
                });
            stats.doc_count = stats
                .doc_count
                .checked_add(1)
                .ok_or(IndexError::ValueOutOfRange)?;
            stats.total_terms = stats
                .total_terms
                .checked_add(field_token_count)
                .ok_or(IndexError::ValueOutOfRange)?;

            for (term, term_freq) in frequencies {
                let term_id = if let Some(existing) =
                    self.state.terms_to_ids.get(&(field_id, term.clone()))
                {
                    *existing
                } else {
                    let term_id = TermId::new(self.state.next_term_id);
                    self.state.next_term_id = self
                        .state
                        .next_term_id
                        .checked_add(1)
                        .ok_or(IndexError::ValueOutOfRange)?;
                    self.state
                        .terms_to_ids
                        .insert((field_id, term.clone()), term_id);
                    self.state.term_entries.push(TermEntry {
                        field_id,
                        term_id,
                        term,
                    });
                    term_id
                };

                self.state
                    .postings
                    .entry(term_id)
                    .or_default()
                    .push(PostingEntry { doc_id, term_freq });
            }
        }

        Ok(())
    }

    fn finish(self) -> Self::Output {
        let posting_blocks = build_posting_blocks(
            &self.state.term_entries,
            &self.state.postings,
            &self.state.field_doc_lengths,
            self.block_config.postings_block_size,
        );
        InMemoryIndex::new(
            self.analyzers,
            self.state.documents,
            self.state.terms_to_ids,
            self.state.term_entries,
            self.state.postings,
            posting_blocks,
            self.state.field_stats,
            self.state.field_names,
            self.state.field_doc_lengths,
        )
    }
}

pub(crate) fn build_posting_blocks(
    term_entries: &[TermEntry],
    postings: &BTreeMap<TermId, Vec<PostingEntry>>,
    field_doc_lengths: &BTreeMap<(u32, FieldId), u32>,
    postings_block_size: usize,
) -> BTreeMap<TermId, Vec<PostingBlock>> {
    let mut blocks = BTreeMap::new();
    let block_size = postings_block_size.max(1);
    for (&term_id, term_postings) in postings {
        debug_assert!(
            (term_id.as_u32() as usize) < term_entries.len(),
            "term_entries and postings must be in sync"
        );
        let field = term_entries
            .get(term_id.as_u32() as usize)
            .map_or(FieldId::new(0), |entry| entry.field_id);
        let mut term_blocks = Vec::new();
        let mut start = 0;
        while start < term_postings.len() {
            let end = core::cmp::min(start.saturating_add(block_size), term_postings.len());
            let mut end_doc = term_postings[start].doc_id;
            let mut max_term_freq = 0;
            let mut min_doc_length = u32::MAX;
            for posting in &term_postings[start..end] {
                end_doc = posting.doc_id;
                max_term_freq = max_term_freq.max(posting.term_freq);
                min_doc_length = min_doc_length.min(
                    field_doc_lengths
                        .get(&(posting.doc_id, field))
                        .copied()
                        .unwrap_or_default(),
                );
            }
            term_blocks.push(PostingBlock {
                start,
                end,
                end_doc,
                max_term_freq,
                min_doc_length: if min_doc_length == u32::MAX {
                    0
                } else {
                    min_doc_length
                },
            });
            start = end;
        }
        blocks.insert(term_id, term_blocks);
    }
    blocks
}
