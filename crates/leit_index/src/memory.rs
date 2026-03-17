// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;
use core::ops::{AddAssign, MulAssign};

use leit_collect::Collector;
use leit_core::{FieldId, QueryNodeId, Score, ScoredHit, TermId};
use leit_query::{ExecutionPlan, FieldRegistry, QueryNode, QueryProgram, TermDictionary};
use leit_text::FieldAnalyzers;

use crate::codec::encode_segment;
use crate::error::IndexError;
use crate::search::{ExecutionStats, SearchScorer};

pub(crate) const DEFAULT_POSTINGS_BLOCK_SIZE: usize = 2;

#[derive(Clone, Debug)]
pub(crate) struct TermEntry {
    pub(crate) field_id: FieldId,
    pub(crate) term_id: TermId,
    pub(crate) term: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PostingEntry {
    pub(crate) doc_id: u32,
    pub(crate) term_freq: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FieldMetadata {
    pub(crate) field_id: FieldId,
    pub(crate) doc_count: u32,
    pub(crate) total_terms: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PostingBlock {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) end_doc: u32,
    pub(crate) max_term_freq: u32,
    pub(crate) min_doc_length: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct EvalResult {
    pub(crate) matches: BTreeSet<u32>,
    pub(crate) scores: BTreeMap<u32, Score>,
}

impl EvalResult {
    pub(crate) fn from_scores(scores: BTreeMap<u32, Score>) -> Self {
        let matches = scores.keys().copied().collect();
        Self { matches, scores }
    }

    const fn from_matches(matches: BTreeSet<u32>) -> Self {
        Self {
            matches,
            scores: BTreeMap::new(),
        }
    }
}

fn is_non_unit_boost(boost: f32) -> bool {
    debug_assert!(boost.is_finite(), "boost must be finite");
    (boost - 1.0).abs() > f32::EPSILON
}

const fn u32_to_f32(value: u32) -> f32 {
    value as f32
}

/// An immutable searchable in-memory Phase 1 index.
#[derive(Debug)]
pub struct InMemoryIndex {
    pub(crate) analyzers: FieldAnalyzers,
    pub(crate) documents: BTreeSet<u32>,
    pub(crate) terms_to_ids: BTreeMap<(FieldId, String), TermId>,
    pub(crate) term_entries: Vec<TermEntry>,
    pub(crate) postings: BTreeMap<TermId, Vec<PostingEntry>>,
    pub(crate) posting_blocks: BTreeMap<TermId, Vec<PostingBlock>>,
    pub(crate) field_stats: BTreeMap<FieldId, FieldMetadata>,
    pub(crate) field_names: BTreeMap<String, FieldId>,
    pub(crate) field_doc_lengths: BTreeMap<(u32, FieldId), u32>,
}

impl InMemoryIndex {
    pub(crate) fn new(
        analyzers: FieldAnalyzers,
        documents: BTreeSet<u32>,
        terms_to_ids: BTreeMap<(FieldId, String), TermId>,
        term_entries: Vec<TermEntry>,
        postings: BTreeMap<TermId, Vec<PostingEntry>>,
        posting_blocks: BTreeMap<TermId, Vec<PostingBlock>>,
        field_stats: BTreeMap<FieldId, FieldMetadata>,
        field_names: BTreeMap<String, FieldId>,
        field_doc_lengths: BTreeMap<(u32, FieldId), u32>,
    ) -> Self {
        Self {
            analyzers,
            documents,
            terms_to_ids,
            term_entries,
            postings,
            posting_blocks,
            field_stats,
            field_names,
            field_doc_lengths,
        }
    }

    /// Serialize the current index into a single validated segment buffer.
    pub fn to_segment_bytes(&self) -> Result<Vec<u8>, IndexError> {
        encode_segment(self)
    }

    pub(crate) fn document_count(&self) -> u32 {
        u32::try_from(self.documents.len()).unwrap_or(u32::MAX)
    }

    pub(crate) fn term_entries(&self) -> &[TermEntry] {
        &self.term_entries
    }

    pub(crate) const fn field_stats(&self) -> &BTreeMap<FieldId, FieldMetadata> {
        &self.field_stats
    }

    pub(crate) const fn postings(&self) -> &BTreeMap<TermId, Vec<PostingEntry>> {
        &self.postings
    }

    fn avg_field_doc_length(&self, field: FieldId) -> f32 {
        let Some(stats) = self.field_stats.get(&field) else {
            return 0.0;
        };
        if stats.doc_count == 0 {
            return 0.0;
        }
        u32_to_f32(stats.total_terms) / u32_to_f32(stats.doc_count)
    }

    pub(crate) fn default_field(&self) -> FieldId {
        self.field_stats
            .values()
            .map(|stats| stats.field_id)
            .min()
            .or_else(|| self.field_names.values().min().copied())
            .unwrap_or(FieldId::new(0))
    }

    pub(crate) fn evaluate_plan(
        &self,
        plan: &ExecutionPlan,
        scorer: SearchScorer,
        stats: &mut ExecutionStats,
    ) -> Result<EvalResult, IndexError> {
        self.evaluate_node(plan.program.root(), &plan.program, scorer, stats)
    }

    fn evaluate_node(
        &self,
        node_id: QueryNodeId,
        program: &QueryProgram,
        scoring: SearchScorer,
        stats: &mut ExecutionStats,
    ) -> Result<EvalResult, IndexError> {
        let Some(node) = program.get(node_id) else {
            return Ok(EvalResult::default());
        };

        match node {
            QueryNode::Term { field, term, boost } => {
                Ok(self.eval_term(*field, *term, *boost, scoring, stats))
            }
            QueryNode::Or { children, boost } => {
                let mut matches = BTreeSet::new();
                let mut results = BTreeMap::new();
                for child in children {
                    let child_result = self.evaluate_node(*child, program, scoring, stats)?;
                    matches.extend(child_result.matches);
                    for (doc_id, score) in child_result.scores {
                        let entry = results.entry(doc_id).or_insert(Score::ZERO);
                        AddAssign::add_assign(entry, score);
                    }
                }
                if is_non_unit_boost(*boost) {
                    for score in results.values_mut() {
                        MulAssign::mul_assign(score, *boost);
                    }
                }
                Ok(EvalResult {
                    matches,
                    scores: results,
                })
            }
            QueryNode::And { children, boost } => {
                let mut iter = children.iter();
                let Some(first) = iter.next() else {
                    return Ok(EvalResult::default());
                };
                let first_result = self.evaluate_node(*first, program, scoring, stats)?;
                let mut matches = first_result.matches.clone();
                let mut child_results = Vec::new();
                child_results.push(first_result);
                for child in iter {
                    let child_result = self.evaluate_node(*child, program, scoring, stats)?;
                    matches.retain(|doc_id| child_result.matches.contains(doc_id));
                    child_results.push(child_result);
                }
                let mut results = BTreeMap::new();
                for child_result in child_results {
                    for (doc_id, child_score) in child_result.scores {
                        if matches.contains(&doc_id) {
                            let entry = results.entry(doc_id).or_insert(Score::ZERO);
                            AddAssign::add_assign(entry, child_score);
                        }
                    }
                }
                if is_non_unit_boost(*boost) {
                    for score in results.values_mut() {
                        MulAssign::mul_assign(score, *boost);
                    }
                }
                Ok(EvalResult {
                    matches,
                    scores: results,
                })
            }
            QueryNode::Not { child } => {
                let child_matches = self.evaluate_node(*child, program, scoring, stats)?.matches;
                let mut matches = BTreeSet::new();
                for doc_id in &self.documents {
                    if !child_matches.contains(doc_id) {
                        matches.insert(*doc_id);
                    }
                }
                Ok(EvalResult::from_matches(matches))
            }
            QueryNode::ConstantScore { child, score } => {
                let mut result = self.evaluate_node(*child, program, scoring, stats)?;
                result.scores.clear();
                let safe_score = Score::try_from(*score).unwrap_or(Score::ZERO);
                for doc_id in &result.matches {
                    result.scores.insert(*doc_id, safe_score);
                }
                Ok(result)
            }
        }
    }

    fn score_posting(
        &self,
        posting: &PostingEntry,
        field: FieldId,
        boost: f32,
        scoring: SearchScorer,
        avg_doc_length: f32,
        doc_count: u32,
        doc_frequency: u32,
    ) -> Score {
        let doc_length = self
            .field_doc_lengths
            .get(&(posting.doc_id, field))
            .copied()
            .unwrap_or_default();
        let mut score = scoring.score_term(
            field,
            posting.term_freq,
            doc_length,
            avg_doc_length,
            doc_count,
            doc_frequency,
        );
        if is_non_unit_boost(boost) {
            MulAssign::mul_assign(&mut score, boost);
        }
        score
    }

    fn eval_term(
        &self,
        field: FieldId,
        term: TermId,
        boost: f32,
        scoring: SearchScorer,
        stats: &mut ExecutionStats,
    ) -> EvalResult {
        let mut results = BTreeMap::new();
        let Some(postings) = self.postings.get(&term) else {
            return EvalResult::default();
        };

        let avg_doc_length = self.avg_field_doc_length(field);
        let doc_count = self.document_count();
        let doc_frequency = u32::try_from(postings.len()).unwrap_or(u32::MAX);

        for posting in postings {
            stats.scored_postings = stats.scored_postings.saturating_add(1);
            let score = self.score_posting(
                posting,
                field,
                boost,
                scoring,
                avg_doc_length,
                doc_count,
                doc_frequency,
            );
            results.insert(posting.doc_id, score);
        }

        EvalResult::from_scores(results)
    }

    pub(crate) fn collect_result<C: Collector<u32>>(
        result: EvalResult,
        collector: &mut C,
        stats: &mut ExecutionStats,
    ) {
        for doc_id in result.matches {
            let score = result.scores.get(&doc_id).copied().unwrap_or(Score::ZERO);
            if collector.can_skip(score) {
                continue;
            }
            collector.collect(ScoredHit::new(doc_id, score));
            stats.collected_hits = stats.collected_hits.saturating_add(1);
        }
    }

    pub(crate) fn try_execute_root<C: Collector<u32>>(
        &self,
        plan: &ExecutionPlan,
        scoring: SearchScorer,
        collector: &mut C,
        stats: &mut ExecutionStats,
    ) -> Result<bool, IndexError> {
        let Some(node) = plan.program.get(plan.program.root()) else {
            return Ok(true);
        };
        match node {
            QueryNode::Term { field, term, boost } => {
                self.collect_term(*field, *term, *boost, scoring, collector, stats);
                Ok(true)
            }
            QueryNode::ConstantScore { child, score } => {
                let mut result = self.evaluate_node(*child, &plan.program, scoring, stats)?;
                result.scores.clear();
                let score = Score::try_from(*score).unwrap_or(Score::ZERO);
                if collector.can_skip(score) {
                    return Ok(true);
                }
                for doc_id in result.matches {
                    collector.collect(ScoredHit::new(doc_id, score));
                    stats.collected_hits = stats.collected_hits.saturating_add(1);
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn collect_term<C: Collector<u32>>(
        &self,
        field: FieldId,
        term: TermId,
        boost: f32,
        scoring: SearchScorer,
        collector: &mut C,
        stats: &mut ExecutionStats,
    ) {
        let Some(postings) = self.postings.get(&term) else {
            return;
        };
        let Some(blocks) = self.posting_blocks.get(&term) else {
            return;
        };

        let avg_doc_length = self.avg_field_doc_length(field);
        let doc_count = self.document_count();
        let doc_frequency = u32::try_from(postings.len()).unwrap_or(u32::MAX);

        for block in blocks {
            // Block-max pruning is only valid for non-negative boosts.
            // Negative boost inverts the upper bound, making it a lower bound.
            if boost >= 0.0
                && let Some(threshold) = collector.threshold()
            {
                let bound = Self::block_upper_bound(
                    *block,
                    field,
                    boost,
                    scoring,
                    avg_doc_length,
                    doc_count,
                    doc_frequency,
                );
                if bound < threshold {
                    stats.skipped_blocks = stats.skipped_blocks.saturating_add(1);
                    continue;
                }
            }

            for posting in &postings[block.start..block.end] {
                stats.scored_postings = stats.scored_postings.saturating_add(1);
                let score = self.score_posting(
                    posting,
                    field,
                    boost,
                    scoring,
                    avg_doc_length,
                    doc_count,
                    doc_frequency,
                );
                if collector.can_skip(score) {
                    continue;
                }
                collector.collect(ScoredHit::new(posting.doc_id, score));
                stats.collected_hits = stats.collected_hits.saturating_add(1);
            }
        }
    }

    fn block_upper_bound(
        block: PostingBlock,
        field: FieldId,
        boost: f32,
        scoring: SearchScorer,
        avg_doc_length: f32,
        doc_count: u32,
        doc_frequency: u32,
    ) -> Score {
        let mut bound = scoring.score_term(
            field,
            block.max_term_freq,
            block.min_doc_length,
            avg_doc_length,
            doc_count,
            doc_frequency,
        );
        if is_non_unit_boost(boost) {
            MulAssign::mul_assign(&mut bound, boost);
        }
        bound
    }
}

impl FieldRegistry for InMemoryIndex {
    fn resolve_field(&self, field: &str) -> Option<FieldId> {
        self.field_names.get(field).copied()
    }
}

impl TermDictionary for InMemoryIndex {
    fn resolve_term(&self, field: FieldId, term: &str) -> Option<TermId> {
        let analyzer = self.analyzers.get(field)?;
        let analyzed_tokens = analyzer.analyze(term);
        if analyzed_tokens.len() != 1 {
            return None;
        }
        let normalized = analyzed_tokens[0].1.as_str();
        self.terms_to_ids.get(&(field, normalized.into())).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    use crate::builder::build_posting_blocks;

    #[test]
    fn posting_blocks_respect_configured_block_size() {
        let term_id = TermId::new(0);
        let term_entries = vec![TermEntry {
            field_id: FieldId::new(1),
            term_id,
            term: String::from("alpha"),
        }];
        let postings = BTreeMap::from([(
            term_id,
            vec![
                PostingEntry {
                    doc_id: 1,
                    term_freq: 3,
                },
                PostingEntry {
                    doc_id: 2,
                    term_freq: 2,
                },
                PostingEntry {
                    doc_id: 3,
                    term_freq: 1,
                },
            ],
        )]);
        let field_doc_lengths = BTreeMap::from([
            ((1, FieldId::new(1)), 5),
            ((2, FieldId::new(1)), 7),
            ((3, FieldId::new(1)), 9),
        ]);

        let singleton_blocks =
            build_posting_blocks(&term_entries, &postings, &field_doc_lengths, 1);
        let pair_blocks = build_posting_blocks(&term_entries, &postings, &field_doc_lengths, 2);

        assert_eq!(singleton_blocks[&term_id].len(), 3);
        assert_eq!(pair_blocks[&term_id].len(), 2);
        assert_eq!(
            pair_blocks[&term_id][0],
            PostingBlock {
                start: 0,
                end: 2,
                end_doc: 2,
                max_term_freq: 3,
                min_doc_length: 5,
            }
        );
    }
}
