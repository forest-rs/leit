// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::ops::{AddAssign, MulAssign};

use leit_collect::Collector;
use leit_core::{FieldId, FilterEvaluator, QueryNodeId, Score, ScoredHit, TermId};
#[cfg(test)]
use leit_core::{SegmentLocalDocId, TermFreq};
#[cfg(test)]
use leit_postings::codec::{BlockDeltaCodec, Codec, CodecId, DeltaVarintCodec};
use leit_postings::cursor::{
    CursorFactory, CursorStatus, DecodeScratch, DefaultCursorFactory, PostingsView, TfCursor,
};
use leit_query::{ExecutionPlan, FieldRegistry, QueryNode, QueryProgram, TermDictionary};
use leit_score::Bm25FScorer;
use leit_text::{AnalysisSchemaId, FieldAnalyzers};

use crate::cursor::MemPostingsCursor;
use crate::error::IndexError;
use crate::index_surface::{
    ExecutableIndex, FieldStatsView, PlanningIndex, PostingBlockView, TermEntryView,
};
use crate::search::{ExecutionStats, FieldHit, SearchScorer, score_bm25f_fields};
use crate::segment_format::writer::write_segment;

pub(crate) const DEFAULT_POSTINGS_BLOCK_SIZE: usize = 2;

#[derive(Clone, Debug)]
pub(crate) struct TermEntry {
    pub(crate) field_id: FieldId,
    pub(crate) term_id: TermId,
    pub(crate) term: String,
}

/// A single posting: a document ID and its term frequency for a term.
///
/// Postings are aggregated per term and stored in doc-sorted order (ascending doc ID).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PostingEntry {
    /// Document identifier (segment-local, u32).
    pub(crate) doc_id: u32,
    /// Term frequency (raw count of term occurrences in the document's field).
    pub(crate) term_freq: u32,
}

impl PostingEntry {
    /// Create an immutable posting value for an [`ExecutableIndex`].
    pub const fn new(doc_id: u32, term_freq: u32) -> Self {
        Self { doc_id, term_freq }
    }

    /// Return the segment-local document identifier.
    pub const fn doc_id(self) -> u32 {
        self.doc_id
    }

    /// Return the term frequency for this document.
    pub const fn term_freq(self) -> u32 {
        self.term_freq
    }
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

#[derive(Clone, Copy, Debug, Default)]
pub(crate) enum CursorSource<'a> {
    #[default]
    InMemory,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "prepared compressed source is retained as crate-private parity evidence"
        )
    )]
    Compressed {
        postings: &'a BTreeMap<TermId, Vec<u8>>,
        scratch: &'a RefCell<DecodeScratch>,
    },
}

#[derive(Clone, Debug, Default)]
struct EvaluationFrame {
    hits: Vec<ScoredHit<u32>>,
}

#[derive(Clone, Copy, Debug)]
enum WorkItem {
    Visit(QueryNodeId),
    ReduceChildren {
        node: QueryNodeId,
        next_child: usize,
        accumulator: usize,
    },
    FinishUnary(QueryNodeId),
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct DocFieldHit {
    pub(crate) doc_id: u32,
    pub(crate) field: FieldId,
    pub(crate) term_frequency: u32,
}

/// Query-owned buffers retained across planned executions.
#[derive(Clone, Debug)]
pub(crate) struct EvaluationScratch {
    work_stack: Vec<WorkItem>,
    frame_pool: Vec<EvaluationFrame>,
    free_frames: Vec<usize>,
    spare_frames: [usize; 2],
    pub(crate) terms: Vec<(FieldId, TermId, f32)>,
    pub(crate) fields: Vec<(FieldId, f32, f32)>,
    pub(crate) doc_hits: Vec<DocFieldHit>,
    pub(crate) field_hits: Vec<FieldHit>,
    pub(crate) scoring_fields: Vec<leit_score::FieldStats>,
}

/// Retained capacities of reusable query-execution storage.
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BenchmarkScratchCapacities {
    pub work_stack: usize,
    pub frame_pool: usize,
    pub free_frames: usize,
    pub frame_hits: Vec<usize>,
    pub terms: usize,
    pub fields: usize,
    pub doc_hits: usize,
    pub field_hits: usize,
    pub scoring_fields: usize,
    pub union_spare_hits: usize,
    pub intersection_spare_hits: usize,
}

#[cfg(feature = "bench-internals")]
impl EvaluationScratch {
    pub(crate) fn benchmark_capacities(&self) -> BenchmarkScratchCapacities {
        BenchmarkScratchCapacities {
            work_stack: self.work_stack.capacity(),
            frame_pool: self.frame_pool.capacity(),
            free_frames: self.free_frames.capacity(),
            frame_hits: self
                .frame_pool
                .iter()
                .map(|frame| frame.hits.capacity())
                .collect(),
            terms: self.terms.capacity(),
            fields: self.fields.capacity(),
            doc_hits: self.doc_hits.capacity(),
            field_hits: self.field_hits.capacity(),
            scoring_fields: self.scoring_fields.capacity(),
            union_spare_hits: self.frame_pool[self.spare_frames[0]].hits.capacity(),
            intersection_spare_hits: self.frame_pool[self.spare_frames[1]].hits.capacity(),
        }
    }
}

impl Default for EvaluationScratch {
    fn default() -> Self {
        Self {
            work_stack: Vec::new(),
            frame_pool: alloc::vec![EvaluationFrame::default(), EvaluationFrame::default()],
            free_frames: Vec::new(),
            spare_frames: [0, 1],
            terms: Vec::new(),
            fields: Vec::new(),
            doc_hits: Vec::new(),
            field_hits: Vec::new(),
            scoring_fields: Vec::new(),
        }
    }
}

impl EvaluationScratch {
    fn begin(&mut self) {
        self.work_stack.clear();
        self.free_frames.clear();
        for (index, frame) in self.frame_pool.iter_mut().enumerate() {
            frame.hits.clear();
            if !self.spare_frames.contains(&index) {
                self.free_frames.push(index);
            }
        }
    }

    fn acquire_frame(&mut self) -> usize {
        let index = self.free_frames.pop().unwrap_or_else(|| {
            self.frame_pool.push(EvaluationFrame::default());
            self.frame_pool.len() - 1
        });
        self.frame_pool[index].hits.clear();
        index
    }

    fn release_frame(&mut self, index: usize) {
        if self.spare_frames.contains(&index) {
            return;
        }
        self.frame_pool[index].hits.clear();
        self.free_frames.push(index);
    }
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

fn scratch_constant_score(hits: &mut [ScoredHit<u32>], score: Score) {
    for hit in hits {
        hit.score = score;
    }
}

fn scratch_filter(hits: &mut Vec<ScoredHit<u32>>, mut keep: impl FnMut(u32) -> bool) {
    hits.retain(|hit| keep(hit.id));
}

#[derive(Clone, Copy)]
enum MergeKind {
    Union,
    Intersection,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "difference is exercised by production-helper tests"
        )
    )]
    Difference,
}

fn reusable_frame_pair_mut(
    frames: &mut [EvaluationFrame],
    first: usize,
    second: usize,
) -> Option<(&mut EvaluationFrame, &mut EvaluationFrame)> {
    if first == second || first >= frames.len() || second >= frames.len() {
        return None;
    }
    if first < second {
        let (left, right) = frames.split_at_mut(second);
        Some((&mut left[first], &mut right[0]))
    } else {
        let (left, right) = frames.split_at_mut(first);
        Some((&mut right[0], &mut left[second]))
    }
}

fn copy_reusable_frame(frames: &mut [EvaluationFrame], source: usize, target: usize) -> bool {
    let Some((source, target)) = reusable_frame_pair_mut(frames, source, target) else {
        return false;
    };
    target.hits.clear();
    target.hits.extend_from_slice(&source.hits);
    true
}

fn merge_reusable_frames(
    scratch: &mut EvaluationScratch,
    left: usize,
    right: usize,
    kind: MergeKind,
) -> usize {
    let slot = match kind {
        MergeKind::Intersection => 1,
        MergeKind::Union | MergeKind::Difference => 0,
    };
    let output = scratch.spare_frames[slot];
    debug_assert!(
        left != right && left != output && right != output,
        "active frames and the reserved output frame must be distinct"
    );
    scratch.frame_pool[output].hits.clear();
    let mut left_index = 0;
    let mut right_index = 0;
    loop {
        let left_hit = scratch.frame_pool[left].hits.get(left_index).copied();
        let right_hit = scratch.frame_pool[right].hits.get(right_index).copied();
        match (left_hit, right_hit) {
            (Some(left_hit), Some(right_hit)) => match left_hit.id.cmp(&right_hit.id) {
                core::cmp::Ordering::Less => {
                    if !matches!(kind, MergeKind::Intersection) {
                        scratch.frame_pool[output].hits.push(left_hit);
                    }
                    left_index += 1;
                }
                core::cmp::Ordering::Greater => {
                    if matches!(kind, MergeKind::Union) {
                        scratch.frame_pool[output].hits.push(right_hit);
                    }
                    right_index += 1;
                }
                core::cmp::Ordering::Equal => {
                    if !matches!(kind, MergeKind::Difference) {
                        scratch.frame_pool[output].hits.push(ScoredHit::new(
                            left_hit.id,
                            left_hit.score + right_hit.score,
                        ));
                    }
                    left_index += 1;
                    right_index += 1;
                }
            },
            (Some(left_hit), None) => {
                if !matches!(kind, MergeKind::Intersection) {
                    scratch.frame_pool[output].hits.push(left_hit);
                }
                left_index += 1;
            }
            (None, Some(right_hit)) => {
                if matches!(kind, MergeKind::Union) {
                    scratch.frame_pool[output].hits.push(right_hit);
                }
                right_index += 1;
            }
            (None, None) => break,
        }
    }
    scratch.spare_frames[slot] = left;
    output
}

fn complement_reusable_frame(
    index: &InMemoryIndex,
    scratch: &mut EvaluationScratch,
    child: usize,
) -> usize {
    let output = scratch.spare_frames[0];
    debug_assert!(
        child != output,
        "active child and reserved complement frame must be distinct"
    );
    scratch.frame_pool[output].hits.clear();
    let mut child_index = 0;
    for doc_id in &index.documents {
        while scratch.frame_pool[child]
            .hits
            .get(child_index)
            .is_some_and(|hit| hit.id < *doc_id)
        {
            child_index += 1;
        }
        if scratch.frame_pool[child]
            .hits
            .get(child_index)
            .is_none_or(|hit| hit.id != *doc_id)
        {
            scratch.frame_pool[output]
                .hits
                .push(ScoredHit::zero(*doc_id));
        }
    }
    scratch.spare_frames[0] = child;
    output
}

fn apply_reusable_boost(frame: &mut EvaluationFrame, boost: f32) {
    if is_non_unit_boost(boost) {
        for hit in &mut frame.hits {
            MulAssign::mul_assign(&mut hit.score, boost);
        }
    }
}

fn fill_term_frame(
    index: &InMemoryIndex,
    field: FieldId,
    term: TermId,
    boost: f32,
    scoring: Option<SearchScorer>,
    frame: &mut EvaluationFrame,
    stats: &mut ExecutionStats,
) {
    frame.hits.clear();
    let Some(postings) = index.postings.get(&term) else {
        return;
    };
    let avg_doc_length = index.avg_field_doc_length(field);
    let doc_count = index.document_count();
    let doc_frequency = u32::try_from(postings.len()).unwrap_or(u32::MAX);
    for posting in postings {
        let score = if let Some(scorer) = scoring {
            stats.scored_postings = stats.scored_postings.saturating_add(1);
            index.score_doc_tf(
                posting.doc_id,
                posting.term_freq,
                field,
                boost,
                scorer,
                avg_doc_length,
                doc_count,
                doc_frequency,
            )
        } else {
            Score::ZERO
        };
        frame.hits.push(ScoredHit::new(posting.doc_id, score));
    }
}

fn evaluate_reusable_occurrence<F: FilterEvaluator<u32>>(
    index: &InMemoryIndex,
    program: &QueryProgram,
    root: QueryNodeId,
    scoring: Option<SearchScorer>,
    filter: &F,
    scratch: &mut EvaluationScratch,
    stats: &mut ExecutionStats,
) -> Result<usize, IndexError> {
    scratch.begin();
    scratch.work_stack.push(WorkItem::Visit(root));
    let mut completed = None;
    while let Some(item) = scratch.work_stack.pop() {
        match item {
            WorkItem::Visit(node_id) => {
                let Some(node) = program.get(node_id) else {
                    completed = Some(scratch.acquire_frame());
                    continue;
                };
                match node {
                    QueryNode::Term { field, term, boost } => {
                        let frame = scratch.acquire_frame();
                        fill_term_frame(
                            index,
                            *field,
                            *term,
                            *boost,
                            scoring,
                            &mut scratch.frame_pool[frame],
                            stats,
                        );
                        completed = Some(frame);
                    }
                    QueryNode::Or { children, boost }
                    | QueryNode::And { children, boost }
                    | QueryNode::TermExpansion {
                        children, boost, ..
                    } => {
                        if let Some(SearchScorer::Bm25F(scorer)) = scoring
                            && let Some(frame) = try_fill_bm25f_expansion_frame(
                                index, node, program, scorer, scratch, stats,
                            )
                        {
                            completed = Some(frame);
                            continue;
                        }
                        let accumulator = scratch.acquire_frame();
                        let Some(first) = children.first().copied() else {
                            apply_reusable_boost(&mut scratch.frame_pool[accumulator], *boost);
                            completed = Some(accumulator);
                            continue;
                        };
                        scratch.work_stack.push(WorkItem::ReduceChildren {
                            node: node_id,
                            next_child: 1,
                            accumulator,
                        });
                        scratch.work_stack.push(WorkItem::Visit(first));
                    }
                    QueryNode::Not { child } | QueryNode::ConstantScore { child, .. } => {
                        scratch.work_stack.push(WorkItem::FinishUnary(node_id));
                        scratch.work_stack.push(WorkItem::Visit(*child));
                    }
                    QueryNode::ExternalFilter { input, .. } => {
                        scratch.work_stack.push(WorkItem::FinishUnary(node_id));
                        scratch.work_stack.push(WorkItem::Visit(*input));
                    }
                    QueryNode::Filter { .. } => {
                        return Err(IndexError::UnsupportedFilterPredicate);
                    }
                }
            }
            WorkItem::ReduceChildren {
                node,
                next_child,
                accumulator,
            } => {
                let Some(child) = completed.take() else {
                    scratch.release_frame(accumulator);
                    completed = Some(scratch.acquire_frame());
                    continue;
                };
                let Some(query_node) = program.get(node) else {
                    scratch.release_frame(child);
                    scratch.release_frame(accumulator);
                    completed = Some(scratch.acquire_frame());
                    continue;
                };
                let (children, boost, kind) = match query_node {
                    QueryNode::Or { children, boost }
                    | QueryNode::TermExpansion {
                        children, boost, ..
                    } => (children, *boost, MergeKind::Union),
                    QueryNode::And { children, boost } => {
                        (children, *boost, MergeKind::Intersection)
                    }
                    _ => {
                        scratch.release_frame(child);
                        scratch.release_frame(accumulator);
                        completed = Some(scratch.acquire_frame());
                        continue;
                    }
                };
                let accumulator = if next_child == 1 {
                    let _ = copy_reusable_frame(&mut scratch.frame_pool, child, accumulator);
                    accumulator
                } else {
                    merge_reusable_frames(scratch, accumulator, child, kind)
                };
                scratch.release_frame(child);
                if let Some(next) = children.get(next_child).copied() {
                    scratch.work_stack.push(WorkItem::ReduceChildren {
                        node,
                        next_child: next_child + 1,
                        accumulator,
                    });
                    scratch.work_stack.push(WorkItem::Visit(next));
                } else {
                    apply_reusable_boost(&mut scratch.frame_pool[accumulator], boost);
                    completed = Some(accumulator);
                }
            }
            WorkItem::FinishUnary(node_id) => {
                let child = completed.take().unwrap_or_else(|| scratch.acquire_frame());
                match program.get(node_id) {
                    Some(QueryNode::Not { .. }) => {
                        completed = Some(complement_reusable_frame(index, scratch, child));
                    }
                    Some(QueryNode::ConstantScore { score, .. }) => {
                        scratch_constant_score(
                            &mut scratch.frame_pool[child].hits,
                            Score::try_from(*score).unwrap_or(Score::ZERO),
                        );
                        completed = Some(child);
                    }
                    Some(QueryNode::ExternalFilter { slot, .. }) => {
                        scratch_filter(&mut scratch.frame_pool[child].hits, |doc_id| {
                            filter.evaluate(*slot, &doc_id)
                        });
                        completed = Some(child);
                    }
                    _ => {
                        scratch.release_frame(child);
                        completed = Some(scratch.acquire_frame());
                    }
                }
            }
        }
    }
    Ok(completed.unwrap_or_else(|| scratch.acquire_frame()))
}

fn try_fill_bm25f_expansion_frame(
    index: &InMemoryIndex,
    node: &QueryNode,
    program: &QueryProgram,
    scorer: Bm25FScorer,
    scratch: &mut EvaluationScratch,
    stats: &mut ExecutionStats,
) -> Option<usize> {
    let QueryNode::TermExpansion {
        children,
        fields,
        boost: expansion_boost,
        field_weights,
    } = node
    else {
        return None;
    };

    scratch.terms.clear();
    let mut expected_text = None;
    let mut term_boost = None;
    for child in children {
        let QueryNode::Term { field, term, boost } = program.get(*child)? else {
            return None;
        };
        let entry = index.term_entries.get(term.as_u32() as usize)?;
        if entry.term_id != *term || entry.field_id != *field {
            return None;
        }
        if expected_text.is_some_and(|text: &str| text != entry.term.as_str())
            || term_boost.is_some_and(|value: f32| (value - *boost).abs() > f32::EPSILON)
        {
            return None;
        }
        expected_text.get_or_insert(entry.term.as_str());
        term_boost.get_or_insert(*boost);
        scratch.terms.push((*field, *term, *boost));
    }
    scratch.terms.sort_unstable_by_key(|(field, _, _)| *field);
    if scratch.terms.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return None;
    }

    scratch.fields.clear();
    let mut avg_doc_length = 0.0;
    for field in fields {
        let average = index.avg_field_doc_length(*field);
        let weight = field_weights.get(field).copied().unwrap_or(1.0);
        avg_doc_length += average;
        scratch.fields.push((*field, average, weight));
    }
    scratch.fields.sort_unstable_by_key(|(field, _, _)| *field);
    scratch.fields.dedup_by_key(|(field, _, _)| *field);

    scratch.doc_hits.clear();
    for (field, term, _) in &scratch.terms {
        let postings = index.postings.get(term)?;
        for posting in postings {
            stats.scored_postings = stats.scored_postings.saturating_add(1);
            scratch.doc_hits.push(DocFieldHit {
                doc_id: posting.doc_id,
                field: *field,
                term_frequency: posting.term_freq,
            });
        }
    }
    scratch
        .doc_hits
        .sort_unstable_by_key(|hit| (hit.doc_id, hit.field));
    let doc_frequency = u32::try_from(
        scratch
            .doc_hits
            .iter()
            .map(|hit| hit.doc_id)
            .fold((None, 0_usize), |(previous, count), doc_id| {
                (Some(doc_id), count + usize::from(previous != Some(doc_id)))
            })
            .1,
    )
    .unwrap_or(u32::MAX);
    let doc_count = index.document_count();
    let output = scratch.acquire_frame();
    let mut start = 0;
    while start < scratch.doc_hits.len() {
        let doc_id = scratch.doc_hits[start].doc_id;
        let mut end = start + 1;
        while end < scratch.doc_hits.len() && scratch.doc_hits[end].doc_id == doc_id {
            end += 1;
        }
        scratch.field_hits.clear();
        let mut hit_index = start;
        for (field, average, weight) in &scratch.fields {
            while hit_index < end && scratch.doc_hits[hit_index].field < *field {
                hit_index += 1;
            }
            let term_frequency = if hit_index < end && scratch.doc_hits[hit_index].field == *field {
                scratch.doc_hits[hit_index].term_frequency
            } else {
                0
            };
            scratch.field_hits.push(FieldHit {
                field: *field,
                term_frequency,
                field_length: index
                    .field_doc_lengths
                    .get(&(doc_id, *field))
                    .copied()
                    .unwrap_or_default(),
                avg_field_length: *average,
                weight: *weight,
            });
        }
        scratch.scoring_fields.clear();
        scratch
            .scoring_fields
            .extend(scratch.field_hits.iter().map(|hit| leit_score::FieldStats {
                field_id: hit.field,
                term_frequency: hit.term_frequency,
                field_length: hit.field_length,
                weight: hit.weight,
            }));
        let mut score = score_bm25f_fields(
            scorer,
            &scratch.scoring_fields,
            avg_doc_length,
            doc_count,
            doc_frequency,
            term_boost.unwrap_or(1.0),
        );
        if is_non_unit_boost(*expansion_boost) {
            score *= *expansion_boost;
        }
        scratch.frame_pool[output]
            .hits
            .push(ScoredHit::new(doc_id, score));
        start = end;
    }
    Some(output)
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
    pub(crate) analysis_schema_id: Option<AnalysisSchemaId>,
    pub(crate) analysis_fields: Vec<FieldId>,
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
        let analysis_schema_id = analyzers.schema_id();
        let analysis_fields = analyzers.registered_field_ids().collect();
        Self {
            analyzers,
            analysis_schema_id,
            analysis_fields,
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

    /// Return the analyzer-schema identity captured when this index was built.
    pub const fn analysis_schema_id(&self) -> Option<AnalysisSchemaId> {
        self.analysis_schema_id
    }

    pub(crate) fn analysis_field_ids(&self) -> &[FieldId] {
        &self.analysis_fields
    }

    /// Serialize the current index into a single validated segment buffer (Phase 2 DEC-05 format).
    pub fn to_segment_bytes(&self) -> Result<Vec<u8>, IndexError> {
        write_segment(self)
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

    /// Snapshot postings as primitive tuples for out-of-crate benchmarks.
    #[cfg(feature = "bench-internals")]
    #[doc(hidden)]
    pub fn benchmark_postings(&self) -> Vec<Vec<(u32, u32)>> {
        self.postings
            .values()
            .filter(|postings| !postings.is_empty())
            .map(|postings| {
                postings
                    .iter()
                    .map(|posting| (posting.doc_id, posting.term_freq))
                    .collect()
            })
            .collect()
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

    pub(crate) fn default_fields(&self) -> Vec<FieldId> {
        let fields: Vec<FieldId> = self.field_stats.values().map(|s| s.field_id).collect();
        if fields.is_empty() {
            self.field_names.values().copied().collect()
        } else {
            fields
        }
    }

    pub(crate) fn execute_reusable<S, F>(
        &self,
        plan: &ExecutionPlan,
        scoring: Option<SearchScorer>,
        filter: &F,
        scratch: &mut EvaluationScratch,
        collectors: &mut S,
        stats: &mut ExecutionStats,
        allow_pruning: bool,
    ) -> Result<(), IndexError>
    where
        S: Collector<u32> + ?Sized,
        F: FilterEvaluator<u32>,
    {
        if let Some(scorer) = scoring
            && allow_pruning
            && let Some(QueryNode::Term { field, term, boost }) =
                plan.program.get(plan.program.root())
        {
            self.execute_direct_term_reusable(
                *field, *term, *boost, scorer, scratch, collectors, stats,
            );
            return Ok(());
        }
        if let Some(scorer @ SearchScorer::Bm25(_)) = scoring
            && allow_pruning
            && let Some(QueryNode::TermExpansion {
                children, boost, ..
            }) = plan.program.get(plan.program.root())
            && children.len() == 1
            && let Some(QueryNode::Term {
                field,
                term,
                boost: term_boost,
            }) = plan.program.get(children[0])
        {
            self.execute_direct_term_reusable(
                *field,
                *term,
                *term_boost * *boost,
                scorer,
                scratch,
                collectors,
                stats,
            );
            return Ok(());
        }

        let frame = evaluate_reusable_occurrence(
            self,
            &plan.program,
            plan.program.root(),
            scoring,
            filter,
            scratch,
            stats,
        )?;
        for hit in &scratch.frame_pool[frame].hits {
            if scoring.is_some() {
                if allow_pruning && collectors.can_skip(hit.score) {
                    continue;
                }
                collectors.collect_scored(*hit);
            } else {
                collectors.collect_match(hit.id);
            }
            stats.collected_hits = stats.collected_hits.saturating_add(1);
        }
        scratch.release_frame(frame);
        Ok(())
    }

    fn execute_direct_term_reusable<S>(
        &self,
        field: FieldId,
        term: TermId,
        boost: f32,
        scoring: SearchScorer,
        scratch: &mut EvaluationScratch,
        collectors: &mut S,
        stats: &mut ExecutionStats,
    ) where
        S: Collector<u32> + ?Sized,
    {
        scratch.begin();
        let frame = scratch.acquire_frame();
        let Some(postings) = self.postings.get(&term) else {
            scratch.release_frame(frame);
            return;
        };
        let Some(blocks) = self.posting_blocks.get(&term) else {
            scratch.release_frame(frame);
            return;
        };
        let average = self.avg_field_doc_length(field);
        let doc_count = self.document_count();
        let doc_frequency = u32::try_from(postings.len()).unwrap_or(u32::MAX);
        for block in blocks {
            if boost >= 0.0
                && let Some(threshold) = collectors.min_competitive_score()
                && Self::block_upper_bound(
                    *block,
                    field,
                    boost,
                    scoring,
                    average,
                    doc_count,
                    doc_frequency,
                ) < threshold
            {
                stats.skipped_blocks = stats.skipped_blocks.saturating_add(1);
                continue;
            }
            scratch.frame_pool[frame].hits.clear();
            for posting in &postings[block.start..block.end] {
                stats.scored_postings = stats.scored_postings.saturating_add(1);
                scratch.frame_pool[frame].hits.push(ScoredHit::new(
                    posting.doc_id,
                    self.score_doc_tf(
                        posting.doc_id,
                        posting.term_freq,
                        field,
                        boost,
                        scoring,
                        average,
                        doc_count,
                        doc_frequency,
                    ),
                ));
            }
            for hit in &scratch.frame_pool[frame].hits {
                if collectors.can_skip(hit.score) {
                    continue;
                }
                collectors.collect_scored(*hit);
                stats.collected_hits = stats.collected_hits.saturating_add(1);
            }
        }
        scratch.release_frame(frame);
    }

    #[cfg(test)]
    #[expect(dead_code, reason = "retained only for legacy evaluator comparisons")]
    pub(crate) fn evaluate_plan<F: FilterEvaluator<u32>>(
        &self,
        plan: &ExecutionPlan,
        scorer: SearchScorer,
        filter: &F,
        stats: &mut ExecutionStats,
    ) -> Result<EvalResult, IndexError> {
        self.evaluate_plan_with_source(plan, scorer, filter, stats, CursorSource::default())
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "compressed cursor equivalence is retained as crate-private evidence"
        )
    )]
    pub(crate) fn evaluate_plan_with_source<F: FilterEvaluator<u32>>(
        &self,
        plan: &ExecutionPlan,
        scorer: SearchScorer,
        filter: &F,
        stats: &mut ExecutionStats,
        source: CursorSource<'_>,
    ) -> Result<EvalResult, IndexError> {
        self.evaluate_node_with_source(
            plan.program.root(),
            &plan.program,
            scorer,
            filter,
            stats,
            source,
        )
    }

    #[cfg(test)]
    #[expect(dead_code, reason = "retained only for legacy evaluator comparisons")]
    pub(crate) fn evaluate_matches<F: FilterEvaluator<u32>>(
        &self,
        plan: &ExecutionPlan,
        filter: &F,
    ) -> Result<BTreeSet<u32>, IndexError> {
        self.evaluate_matches_node(plan.program.root(), &plan.program, filter)
    }

    fn evaluate_node<F: FilterEvaluator<u32>>(
        &self,
        node_id: QueryNodeId,
        program: &QueryProgram,
        scoring: SearchScorer,
        filter: &F,
        stats: &mut ExecutionStats,
    ) -> Result<EvalResult, IndexError> {
        self.evaluate_node_with_source(
            node_id,
            program,
            scoring,
            filter,
            stats,
            CursorSource::default(),
        )
    }

    fn evaluate_node_with_source<F: FilterEvaluator<u32>>(
        &self,
        node_id: QueryNodeId,
        program: &QueryProgram,
        scoring: SearchScorer,
        filter: &F,
        stats: &mut ExecutionStats,
        source: CursorSource<'_>,
    ) -> Result<EvalResult, IndexError> {
        let Some(node) = program.get(node_id) else {
            return Ok(EvalResult::default());
        };

        match node {
            QueryNode::Term { field, term, boost } => {
                Ok(self.eval_term_with_source(*field, *term, *boost, scoring, stats, source))
            }
            QueryNode::TermExpansion {
                children,
                fields,
                boost,
                field_weights,
            } => {
                if let SearchScorer::Bm25F(_) = scoring
                    && let Some(mut result) = self.eval_bm25f_term_expansion(
                        children,
                        fields,
                        field_weights,
                        program,
                        scoring,
                        stats,
                    )
                {
                    if is_non_unit_boost(*boost) {
                        for score in result.scores.values_mut() {
                            MulAssign::mul_assign(score, *boost);
                        }
                    }
                    return Ok(result);
                }

                // Note: TermExpansion stays on InMemory path; compressed source wires only single-term (QueryNode::Term).
                self.eval_disjunction(children, *boost, program, scoring, filter, stats)
            }
            QueryNode::Or { children, boost } => {
                // Note: OR stays on InMemory; compressed source wires only single-term queries.
                self.eval_disjunction(children, *boost, program, scoring, filter, stats)
            }
            QueryNode::And { children, boost } => {
                let mut iter = children.iter();
                let Some(first) = iter.next() else {
                    return Ok(EvalResult::default());
                };
                let first_result = self
                    .evaluate_node_with_source(*first, program, scoring, filter, stats, source)?;
                let mut matches = first_result.matches.clone();
                let mut child_results = Vec::new();
                child_results.push(first_result);
                for child in iter {
                    let child_result = self.evaluate_node_with_source(
                        *child, program, scoring, filter, stats, source,
                    )?;
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
                let child_matches = self
                    .evaluate_node_with_source(*child, program, scoring, filter, stats, source)?
                    .matches;
                let mut matches = BTreeSet::new();
                for doc_id in &self.documents {
                    if !child_matches.contains(doc_id) {
                        matches.insert(*doc_id);
                    }
                }
                Ok(EvalResult::from_matches(matches))
            }
            QueryNode::ConstantScore { child, score } => {
                let mut result = self
                    .evaluate_node_with_source(*child, program, scoring, filter, stats, source)?;
                result.scores.clear();
                let safe_score = Score::try_from(*score).unwrap_or(Score::ZERO);
                for doc_id in &result.matches {
                    result.scores.insert(*doc_id, safe_score);
                }
                Ok(result)
            }
            QueryNode::ExternalFilter { input, slot } => {
                let mut result = self
                    .evaluate_node_with_source(*input, program, scoring, filter, stats, source)?;
                result
                    .matches
                    .retain(|doc_id| filter.evaluate(*slot, doc_id));
                result
                    .scores
                    .retain(|doc_id, _| result.matches.contains(doc_id));
                Ok(result)
            }
            QueryNode::Filter { .. } => Err(IndexError::UnsupportedFilterPredicate),
        }
    }

    fn eval_disjunction<F: FilterEvaluator<u32>>(
        &self,
        children: &[QueryNodeId],
        boost: f32,
        program: &QueryProgram,
        scoring: SearchScorer,
        filter: &F,
        stats: &mut ExecutionStats,
    ) -> Result<EvalResult, IndexError> {
        let mut matches = BTreeSet::new();
        let mut results = BTreeMap::new();
        for child in children {
            let child_result = self.evaluate_node(*child, program, scoring, filter, stats)?;
            matches.extend(child_result.matches);
            for (doc_id, score) in child_result.scores {
                let entry = results.entry(doc_id).or_insert(Score::ZERO);
                AddAssign::add_assign(entry, score);
            }
        }
        if is_non_unit_boost(boost) {
            for score in results.values_mut() {
                MulAssign::mul_assign(score, boost);
            }
        }
        Ok(EvalResult {
            matches,
            scores: results,
        })
    }

    fn eval_bm25f_term_expansion(
        &self,
        children: &[QueryNodeId],
        fields: &[FieldId],
        field_weights: &BTreeMap<FieldId, f32>,
        program: &QueryProgram,
        scoring: SearchScorer,
        stats: &mut ExecutionStats,
    ) -> Option<EvalResult> {
        let mut terms = Vec::with_capacity(children.len());
        let mut seen_fields = BTreeSet::new();
        let mut expected_text: Option<&str> = None;
        let mut expected_boost: Option<f32> = None;
        for child in children {
            let QueryNode::Term { field, term, boost } = program.get(*child)? else {
                return None;
            };
            if !seen_fields.insert(*field) {
                return None;
            }
            let term_entry = self.term_entries.get(term.as_u32() as usize)?;
            if term_entry.term_id != *term || term_entry.field_id != *field {
                return None;
            }
            match expected_text {
                Some(text) if text != term_entry.term.as_str() => return None,
                Some(_) => {}
                None => expected_text = Some(term_entry.term.as_str()),
            }
            match expected_boost {
                Some(value) if (value - *boost).abs() > f32::EPSILON => return None,
                Some(_) => {}
                None => expected_boost = Some(*boost),
            }
            terms.push((*field, *term));
        }

        let weight = |field: FieldId| -> f32 { field_weights.get(&field).copied().unwrap_or(1.0) };

        let mut aggregation_fields = Vec::with_capacity(fields.len());
        let mut avg_doc_length = 0.0_f32;
        for &field in fields {
            let avg_field_length = self.avg_field_doc_length(field);
            avg_doc_length += avg_field_length;
            aggregation_fields.push((field, avg_field_length));
        }

        let mut hits_by_doc = BTreeMap::<u32, BTreeMap<FieldId, FieldHit>>::new();
        for (field, term) in terms {
            let postings = self.postings.get(&term)?;
            let avg_field_length = self.avg_field_doc_length(field);
            for posting in postings {
                stats.scored_postings = stats.scored_postings.saturating_add(1);
                let field_length = self
                    .field_doc_lengths
                    .get(&(posting.doc_id, field))
                    .copied()
                    .unwrap_or_default();
                hits_by_doc.entry(posting.doc_id).or_default().insert(
                    field,
                    FieldHit {
                        field,
                        term_frequency: posting.term_freq,
                        field_length,
                        avg_field_length,
                        weight: weight(field),
                    },
                );
            }
        }

        let doc_count = self.document_count();
        let doc_frequency = u32::try_from(hits_by_doc.len()).unwrap_or(u32::MAX);
        let boost = expected_boost.unwrap_or(1.0);
        let mut scores = BTreeMap::new();
        for (doc_id, mut field_hits_by_field) in hits_by_doc {
            for (field, avg_field_length) in &aggregation_fields {
                field_hits_by_field
                    .entry(*field)
                    .or_insert_with(|| FieldHit {
                        field: *field,
                        term_frequency: 0,
                        field_length: self
                            .field_doc_lengths
                            .get(&(doc_id, *field))
                            .copied()
                            .unwrap_or_default(),
                        avg_field_length: *avg_field_length,
                        weight: weight(*field),
                    });
            }
            let field_hits: Vec<FieldHit> = field_hits_by_field.into_values().collect();
            let score = scoring.score_term_fields(
                &field_hits,
                avg_doc_length,
                doc_count,
                doc_frequency,
                boost,
            );
            scores.insert(doc_id, score);
        }
        Some(EvalResult::from_scores(scores))
    }

    #[cfg(test)]
    fn evaluate_matches_node<F: FilterEvaluator<u32>>(
        &self,
        node_id: QueryNodeId,
        program: &QueryProgram,
        filter: &F,
    ) -> Result<BTreeSet<u32>, IndexError> {
        let Some(node) = program.get(node_id) else {
            return Ok(BTreeSet::new());
        };

        match node {
            QueryNode::Term { term, .. } => {
                let mut matches = BTreeSet::new();
                if let Some(postings) = self.postings.get(term) {
                    for posting in postings {
                        matches.insert(posting.doc_id);
                    }
                }
                Ok(matches)
            }
            QueryNode::Or { children, .. } | QueryNode::TermExpansion { children, .. } => {
                let mut matches = BTreeSet::new();
                for child in children {
                    matches.extend(self.evaluate_matches_node(*child, program, filter)?);
                }
                Ok(matches)
            }
            QueryNode::And { children, .. } => {
                let mut iter = children.iter();
                let Some(first) = iter.next() else {
                    return Ok(BTreeSet::new());
                };
                let mut matches = self.evaluate_matches_node(*first, program, filter)?;
                for child in iter {
                    let child_matches = self.evaluate_matches_node(*child, program, filter)?;
                    matches.retain(|doc_id| child_matches.contains(doc_id));
                }
                Ok(matches)
            }
            QueryNode::Not { child } => {
                let child_matches = self.evaluate_matches_node(*child, program, filter)?;
                let mut matches = BTreeSet::new();
                for doc_id in &self.documents {
                    if !child_matches.contains(doc_id) {
                        matches.insert(*doc_id);
                    }
                }
                Ok(matches)
            }
            QueryNode::ConstantScore { child, .. } => {
                self.evaluate_matches_node(*child, program, filter)
            }
            QueryNode::ExternalFilter { input, slot } => {
                let mut matches = self.evaluate_matches_node(*input, program, filter)?;
                matches.retain(|doc_id| filter.evaluate(*slot, doc_id));
                Ok(matches)
            }
            QueryNode::Filter { .. } => Err(IndexError::UnsupportedFilterPredicate),
        }
    }

    /// Score a document given its term frequency. Shared scoring core driven by
    /// the generic `score_via_cursor` helper over any `TfCursor`.
    fn score_doc_tf(
        &self,
        doc_id: u32,
        term_freq: u32,
        field: FieldId,
        boost: f32,
        scoring: SearchScorer,
        avg_doc_length: f32,
        doc_count: u32,
        doc_frequency: u32,
    ) -> Score {
        let doc_length = self
            .field_doc_lengths
            .get(&(doc_id, field))
            .copied()
            .unwrap_or_default();
        let mut score = scoring.score_term(
            field,
            term_freq,
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

    /// Score postings via a generic cursor, invoking a sink for each scored document.
    ///
    /// This helper enables STORY-0001 cursor integration: both in-memory (via `MemPostingsCursor`)
    /// and compressed (via `CompressedCursor`, deferred to T3) paths route through the same
    /// generic scorer. The cursor drives traversal (`current_doc`, `current_tf`, `advance`);
    /// the sink receives `(doc_id, score)` pairs. The caller is responsible for stats updates.
    ///
    /// # Type Parameters
    /// - `C`: A cursor over postings, implementing `leit_postings::cursor::TfCursor` (which extends `DocCursor`).
    /// - `FnSink`: A callable that accepts `(u32, Score)` and processes the scored doc (e.g., insert into a map, collect).
    ///   The sink should increment `stats.scored_postings` if stat tracking is needed.
    fn score_via_cursor<C, FnSink>(
        &self,
        cursor: &mut C,
        field: FieldId,
        boost: f32,
        scoring: SearchScorer,
        avg_doc_length: f32,
        doc_count: u32,
        doc_frequency: u32,
        mut sink: FnSink,
    ) where
        C: TfCursor,
        FnSink: FnMut(u32, Score),
    {
        while let Some(doc_id) = cursor.current_doc() {
            let term_freq = cursor.current_tf();
            let score = self.score_doc_tf(
                doc_id,
                term_freq,
                field,
                boost,
                scoring,
                avg_doc_length,
                doc_count,
                doc_frequency,
            );
            sink(doc_id, score);
            if matches!(cursor.advance(), CursorStatus::Exhausted) {
                break;
            }
        }
    }

    fn eval_term_with_source(
        &self,
        field: FieldId,
        term: TermId,
        boost: f32,
        scoring: SearchScorer,
        stats: &mut ExecutionStats,
        cursor_source: CursorSource<'_>,
    ) -> EvalResult {
        let mut results = BTreeMap::new();
        let Some(postings) = self.postings.get(&term) else {
            return EvalResult::default();
        };

        let avg_doc_length = self.avg_field_doc_length(field);
        let doc_count = self.document_count();
        let doc_frequency = u32::try_from(postings.len()).unwrap_or(u32::MAX);

        match cursor_source {
            CursorSource::InMemory => {
                let mut cursor = MemPostingsCursor::new(postings);
                self.score_via_cursor(
                    &mut cursor,
                    field,
                    boost,
                    scoring,
                    avg_doc_length,
                    doc_count,
                    doc_frequency,
                    |doc_id, score| {
                        stats.scored_postings = stats.scored_postings.saturating_add(1);
                        results.insert(doc_id, score);
                    },
                );
            }
            CursorSource::Compressed {
                postings: prepared,
                scratch,
            } => {
                let Some(encoded_bytes) = prepared.get(&term) else {
                    return EvalResult::default();
                };
                let view = PostingsView::new(encoded_bytes, &[]);
                let mut scratch = scratch.borrow_mut();
                let mut cursor = match DefaultCursorFactory.open_doc_cursor(view, &mut scratch) {
                    Ok(c) => c,
                    Err(_) => return EvalResult::default(),
                };

                self.score_via_cursor(
                    &mut cursor,
                    field,
                    boost,
                    scoring,
                    avg_doc_length,
                    doc_count,
                    doc_frequency,
                    |doc_id, score| {
                        stats.scored_postings = stats.scored_postings.saturating_add(1);
                        results.insert(doc_id, score);
                    },
                );
            }
        }

        EvalResult::from_scores(results)
    }

    #[cfg(test)]
    #[expect(dead_code, reason = "retained only for legacy evaluator comparisons")]
    pub(crate) fn collect_result<S>(
        result: EvalResult,
        collectors: &mut S,
        stats: &mut ExecutionStats,
        allow_pruning: bool,
    ) where
        S: Collector<u32> + ?Sized,
    {
        for doc_id in result.matches {
            let score = result.scores.get(&doc_id).copied().unwrap_or(Score::ZERO);
            if allow_pruning && collectors.can_skip(score) {
                continue;
            }
            collectors.collect_scored(ScoredHit::new(doc_id, score));
            stats.collected_hits = stats.collected_hits.saturating_add(1);
        }
    }

    #[cfg(test)]
    pub(crate) fn collect_matches<S>(
        matches: BTreeSet<u32>,
        collectors: &mut S,
        stats: &mut ExecutionStats,
    ) where
        S: Collector<u32> + ?Sized,
    {
        for doc_id in matches {
            collectors.collect_match(doc_id);
            stats.collected_hits = stats.collected_hits.saturating_add(1);
        }
    }

    /// Try to execute the plan root via an optimized fast path.
    ///
    /// Returns `Ok(true)` if handled, `Ok(false)` to fall through to the
    /// general evaluator. The `filter` parameter is threaded to recursive
    /// calls (e.g. `ConstantScore` → `evaluate_node`) but is not consulted
    /// on leaf fast paths (`Term`) because those only fire when the root is
    /// a bare `Term` node with no `ExternalFilter` wrapping. Filter dispatch
    /// is node-mediated via `ExternalFilter` nodes in the general evaluator.
    #[cfg(test)]
    #[expect(dead_code, reason = "retained only for legacy evaluator comparisons")]
    pub(crate) fn try_execute_root<S, F>(
        &self,
        plan: &ExecutionPlan,
        scoring: SearchScorer,
        collectors: &mut S,
        stats: &mut ExecutionStats,
        allow_pruning: bool,
        filter: &F,
    ) -> Result<bool, IndexError>
    where
        S: Collector<u32> + ?Sized,
        F: FilterEvaluator<u32>,
    {
        let Some(node) = plan.program.get(plan.program.root()) else {
            return Ok(true);
        };
        match node {
            QueryNode::Term { field, term, boost } => {
                debug_assert!(
                    filter.slots().is_empty(),
                    "Term fast path fired with active filter slots; \
                     ensure plan() was called with the same filter as execute()"
                );
                self.collect_term(
                    *field,
                    *term,
                    *boost,
                    scoring,
                    collectors,
                    stats,
                    allow_pruning,
                );
                Ok(true)
            }
            QueryNode::TermExpansion {
                children, boost, ..
            } if matches!(scoring, SearchScorer::Bm25(_)) && children.len() == 1 => {
                let Some(QueryNode::Term {
                    field,
                    term,
                    boost: term_boost,
                }) = plan.program.get(children[0])
                else {
                    return Ok(false);
                };
                debug_assert!(
                    filter.slots().is_empty(),
                    "TermExpansion fast path fired with active filter slots; \
                     ensure plan() was called with the same filter as execute()"
                );
                self.collect_term(
                    *field,
                    *term,
                    *term_boost * *boost,
                    scoring,
                    collectors,
                    stats,
                    allow_pruning,
                );
                Ok(true)
            }
            QueryNode::ConstantScore { child, score } => {
                let mut result =
                    self.evaluate_node(*child, &plan.program, scoring, filter, stats)?;
                result.scores.clear();
                let score = Score::try_from(*score).unwrap_or(Score::ZERO);
                if allow_pruning && collectors.can_skip(score) {
                    return Ok(true);
                }
                for doc_id in result.matches {
                    collectors.collect_scored(ScoredHit::new(doc_id, score));
                    stats.collected_hits = stats.collected_hits.saturating_add(1);
                }
                Ok(true)
            }
            QueryNode::Filter { .. } | QueryNode::ExternalFilter { .. } => Ok(false),
            _ => Ok(false),
        }
    }

    /// Unscored variant of [`try_execute_root`](Self::try_execute_root).
    ///
    /// Same fast-path semantics: `filter` is threaded to recursive calls but
    /// not consulted on the bare `Term` leaf path.
    #[cfg(test)]
    #[expect(dead_code, reason = "retained only for legacy evaluator comparisons")]
    pub(crate) fn try_execute_root_unscored<S, F>(
        &self,
        plan: &ExecutionPlan,
        collectors: &mut S,
        stats: &mut ExecutionStats,
        filter: &F,
    ) -> Result<bool, IndexError>
    where
        S: Collector<u32> + ?Sized,
        F: FilterEvaluator<u32>,
    {
        let Some(node) = plan.program.get(plan.program.root()) else {
            return Ok(true);
        };
        match node {
            QueryNode::Term { term, .. } => {
                debug_assert!(
                    filter.slots().is_empty(),
                    "Term fast path fired with active filter slots; \
                     ensure plan() was called with the same filter as execute()"
                );
                self.collect_term_docs(*term, collectors, stats);
                Ok(true)
            }
            QueryNode::ConstantScore { child, .. } => {
                let matches = self.evaluate_matches_node(*child, &plan.program, filter)?;
                Self::collect_matches(matches, collectors, stats);
                Ok(true)
            }
            QueryNode::Filter { .. } | QueryNode::ExternalFilter { .. } => Ok(false),
            _ => Ok(false),
        }
    }

    /// Collect scored hits for a single term via the in-memory cursor, preserving
    /// per-block max-score pruning.
    ///
    /// This is the fast-path used by `try_execute_root`; it always traverses the
    /// uncompressed in-memory postings (the segment-backed compressed source is
    /// selected only on the `evaluate_plan_with_source` path via `eval_term_with_source`).
    #[cfg(test)]
    fn collect_term<S>(
        &self,
        field: FieldId,
        term: TermId,
        boost: f32,
        scoring: SearchScorer,
        collectors: &mut S,
        stats: &mut ExecutionStats,
        allow_pruning: bool,
    ) where
        S: Collector<u32> + ?Sized,
    {
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
            if allow_pruning
                && boost >= 0.0
                && let Some(threshold) = collectors.min_competitive_score()
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

            // Score the postings in this block via the cursor helper.
            let mut cursor = MemPostingsCursor::new(&postings[block.start..block.end]);
            self.score_via_cursor(
                &mut cursor,
                field,
                boost,
                scoring,
                avg_doc_length,
                doc_count,
                doc_frequency,
                |doc_id, score| {
                    stats.scored_postings = stats.scored_postings.saturating_add(1);
                    if allow_pruning && collectors.can_skip(score) {
                        return;
                    }
                    collectors.collect_scored(ScoredHit::new(doc_id, score));
                    stats.collected_hits = stats.collected_hits.saturating_add(1);
                },
            );
        }
    }

    #[cfg(test)]
    fn collect_term_docs<S>(&self, term: TermId, collectors: &mut S, stats: &mut ExecutionStats)
    where
        S: Collector<u32> + ?Sized,
    {
        let Some(postings) = self.postings.get(&term) else {
            return;
        };
        for posting in postings {
            collectors.collect_match(posting.doc_id);
            stats.collected_hits = stats.collected_hits.saturating_add(1);
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

impl PlanningIndex for InMemoryIndex {
    fn for_each_field(&self, f: &mut dyn FnMut(FieldId)) {
        if self.field_stats.is_empty() {
            for &field in self.field_names.values() {
                f(field);
            }
        } else {
            for stats in self.field_stats.values() {
                f(stats.field_id);
            }
        }
    }

    fn for_each_default_field(&self, f: &mut dyn FnMut(FieldId)) {
        for field in self.default_fields() {
            f(field);
        }
    }
}

impl ExecutableIndex for InMemoryIndex {
    fn document_count(&self) -> u32 {
        Self::document_count(self)
    }

    fn field_stats(&self, field: FieldId) -> Option<FieldStatsView> {
        self.field_stats.get(&field).map(|stats| FieldStatsView {
            field_id: stats.field_id,
            doc_count: stats.doc_count,
            total_terms: stats.total_terms,
        })
    }

    fn field_doc_length(&self, doc_id: u32, field: FieldId) -> u32 {
        self.field_doc_lengths
            .get(&(doc_id, field))
            .copied()
            .unwrap_or_default()
    }

    fn for_each_doc(&self, f: &mut dyn FnMut(u32)) {
        for &doc_id in &self.documents {
            f(doc_id);
        }
    }

    fn term_entry(&self, term: TermId) -> Option<TermEntryView<'_>> {
        self.term_entries
            .get(term.as_u32() as usize)
            .map(|entry| TermEntryView {
                field_id: entry.field_id,
                term_id: entry.term_id,
                term_text: entry.term.as_str(),
            })
    }

    fn postings(&self, term: TermId) -> Option<&[PostingEntry]> {
        self.postings.get(&term).map(Vec::as_slice)
    }

    fn for_each_posting_block(&self, term: TermId, f: &mut dyn FnMut(PostingBlockView)) {
        let Some(blocks) = self.posting_blocks.get(&term) else {
            return;
        };
        for block in blocks {
            f(PostingBlockView {
                start: block.start,
                end: block.end,
                max_term_freq: block.max_term_freq,
                min_doc_length: block.min_doc_length,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    use crate::builder::{InMemoryIndexBuilder, build_posting_blocks};
    use leit_text::{Analyzer, UnicodeNormalizer, WhitespaceTokenizer};

    fn execution_fixture() -> InMemoryIndex {
        let mut analyzers = FieldAnalyzers::new();
        analyzers.set(
            FieldId::new(1),
            Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
        );
        let mut builder = InMemoryIndexBuilder::new(analyzers);
        for (id, text) in [(1, "alpha"), (2, "beta"), (3, "beta gamma")] {
            builder
                .index_document(id, &[(FieldId::new(1), text)])
                .expect("fixture document should index");
        }
        builder.build_index()
    }

    fn term_node(index: &InMemoryIndex, term: &str) -> QueryNode {
        QueryNode::Term {
            field: FieldId::new(1),
            term: index
                .resolve_term(FieldId::new(1), term)
                .expect("fixture term should resolve"),
            boost: 1.0,
        }
    }

    fn evaluate_with_scratch(index: &InMemoryIndex, program: &QueryProgram) -> EvalResult {
        let mut scratch = EvaluationScratch::default();
        let mut stats = ExecutionStats::default();
        let frame = evaluate_reusable_occurrence(
            index,
            program,
            program.root(),
            Some(SearchScorer::bm25()),
            &leit_core::NoFilter,
            &mut scratch,
            &mut stats,
        )
        .expect("scratch traversal should evaluate valid program");
        let matches = scratch.frame_pool[frame]
            .hits
            .iter()
            .map(|hit| hit.id)
            .collect();
        let scores = scratch.frame_pool[frame]
            .hits
            .iter()
            .map(|hit| (hit.id, hit.score))
            .collect();
        EvalResult { matches, scores }
    }

    fn scoreless_intersection_program(
        index: &InMemoryIndex,
        children: [QueryNodeId; 2],
    ) -> QueryProgram {
        QueryProgram::new(
            vec![
                term_node(index, "alpha"),
                QueryNode::Not {
                    child: QueryNodeId::new(0),
                },
                term_node(index, "beta"),
                QueryNode::And {
                    children: children.into(),
                    boost: 1.0,
                },
            ],
            QueryNodeId::new(3),
            3,
        )
    }

    #[test]
    fn scratch_intersection_preserves_scores_from_either_operand() {
        let index = execution_fixture();
        let scoreless_first =
            scoreless_intersection_program(&index, [QueryNodeId::new(1), QueryNodeId::new(2)]);
        let scored_first =
            scoreless_intersection_program(&index, [QueryNodeId::new(2), QueryNodeId::new(1)]);

        let scoreless_first_result = evaluate_with_scratch(&index, &scoreless_first);
        let scored_first_result = evaluate_with_scratch(&index, &scored_first);

        assert_eq!(scoreless_first_result, scored_first_result);
        assert_eq!(scoreless_first_result.matches, BTreeSet::from([2, 3]));
        assert_eq!(scoreless_first_result.scores.len(), 2);
        let expected_two = SearchScorer::bm25().score_term(FieldId::new(1), 1, 1, 4.0 / 3.0, 3, 2);
        let expected_three =
            SearchScorer::bm25().score_term(FieldId::new(1), 1, 2, 4.0 / 3.0, 3, 2);
        assert_eq!(
            scoreless_first_result.scores[&2].as_f32().to_bits(),
            expected_two.as_f32().to_bits()
        );
        assert_eq!(
            scoreless_first_result.scores[&3].as_f32().to_bits(),
            expected_three.as_f32().to_bits()
        );
    }

    #[test]
    fn occurrence_stack_revisits_shared_children() {
        let index = execution_fixture();
        let program = QueryProgram::new(
            vec![
                term_node(&index, "beta"),
                term_node(&index, "gamma"),
                QueryNode::And {
                    children: vec![QueryNodeId::new(0), QueryNodeId::new(1)],
                    boost: 1.0,
                },
                QueryNode::Or {
                    children: vec![QueryNodeId::new(0), QueryNodeId::new(2)],
                    boost: 1.0,
                },
            ],
            QueryNodeId::new(3),
            3,
        );
        let mut scratch = EvaluationScratch::default();
        let mut stats = ExecutionStats::default();

        let frame = evaluate_reusable_occurrence(
            &index,
            &program,
            program.root(),
            Some(SearchScorer::bm25()),
            &leit_core::NoFilter,
            &mut scratch,
            &mut stats,
        )
        .expect("shared-child traversal should execute");

        assert_eq!(
            scratch.frame_pool[frame]
                .hits
                .iter()
                .map(|hit| hit.id)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([2, 3])
        );
        assert_eq!(stats.scored_postings, 5);
    }

    fn hit(id: u32, score: f32) -> ScoredHit<u32> {
        ScoredHit::new(id, Score::new(score))
    }

    fn merge_through_frame_pool(
        left: &[ScoredHit<u32>],
        right: &[ScoredHit<u32>],
        kind: MergeKind,
    ) -> Vec<ScoredHit<u32>> {
        let mut scratch = EvaluationScratch::default();
        scratch.begin();
        let left_frame = scratch.acquire_frame();
        let right_frame = scratch.acquire_frame();
        scratch.frame_pool[left_frame].hits.extend_from_slice(left);
        scratch.frame_pool[right_frame]
            .hits
            .extend_from_slice(right);
        let slot = match kind {
            MergeKind::Intersection => 1,
            MergeKind::Union | MergeKind::Difference => 0,
        };
        let reserved_output = scratch.spare_frames[slot];

        let output = merge_reusable_frames(&mut scratch, left_frame, right_frame, kind);

        assert_eq!(output, reserved_output);
        assert_eq!(scratch.spare_frames[slot], left_frame);
        scratch.frame_pool[output].hits.clone()
    }

    #[test]
    fn scratch_union_adds_scores_in_document_order() {
        let left = [hit(1, 1.0), hit(3, 2.0), hit(5, 5.0)];
        let right = [hit(2, 3.0), hit(3, 4.0), hit(4, 4.0)];

        let output = merge_through_frame_pool(&left, &right, MergeKind::Union);

        assert_eq!(
            output,
            vec![
                hit(1, 1.0),
                hit(2, 3.0),
                hit(3, 6.0),
                hit(4, 4.0),
                hit(5, 5.0)
            ]
        );
    }

    #[test]
    fn scratch_intersection_sums_available_scores() {
        let left = [hit(1, 0.0), hit(2, 2.0), hit(4, 0.0)];
        let right = [hit(1, 3.0), hit(3, 7.0), hit(4, 5.0)];
        let output = merge_through_frame_pool(&left, &right, MergeKind::Intersection);

        assert_eq!(output, vec![hit(1, 3.0), hit(4, 5.0)]);
    }

    #[test]
    fn scratch_difference_keeps_left_scores() {
        let left = [hit(1, 1.0), hit(2, 2.0), hit(4, 4.0)];
        let right = [hit(2, 9.0), hit(3, 9.0)];
        let output = merge_through_frame_pool(&left, &right, MergeKind::Difference);

        assert_eq!(output, vec![hit(1, 1.0), hit(4, 4.0)]);
    }

    #[test]
    fn scratch_merges_handle_empty_operands_and_tails() {
        let right = [hit(2, 2.0), hit(4, 4.0)];
        assert_eq!(
            merge_through_frame_pool(&[], &right, MergeKind::Union),
            right
        );
        assert_eq!(
            merge_through_frame_pool(&right, &[], MergeKind::Difference),
            right
        );
        assert!(merge_through_frame_pool(&[], &right, MergeKind::Intersection).is_empty());
    }

    #[test]
    fn scratch_constant_score_rewrites_every_match() {
        let mut hits = vec![hit(1, 1.0), hit(2, 2.0)];

        scratch_constant_score(&mut hits, Score::new(7.0));

        assert_eq!(hits, vec![hit(1, 7.0), hit(2, 7.0)]);
    }

    #[test]
    fn scratch_filter_retains_document_order() {
        let mut hits = vec![hit(1, 1.0), hit(2, 2.0), hit(3, 3.0)];

        scratch_filter(&mut hits, |doc_id| doc_id % 2 == 1);

        assert_eq!(hits, vec![hit(1, 1.0), hit(3, 3.0)]);
    }

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

    // ============================================================================
    // Prepared compressed term/conjunction paths preserve ranking equivalence
    // ============================================================================
    //
    // STORY-0088 AC-2 / STORY-0008 AC-1/AC-2/AC-3: Verify that compressed cursor
    // term paths (DeltaVarint, BlockDelta), plus conjunctions that thread that source,
    // produce identical top-k ranking to the in-memory uncompressed path. OR and
    // generic TermExpansion intentionally fall back to InMemory in this iteration.

    #[test]
    fn prepared_compressed_sources_preserve_ranking() {
        use leit_core::NoFilter;
        use leit_query::Planner;

        // Build a large deterministic test corpus: 300 docs across two fields.
        // This ensures BlockDelta (128-doc blocks) exercises multi-block decoding.
        let mut analyzers = FieldAnalyzers::new();
        let analyzer =
            Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new());
        analyzers.set(FieldId::new(1), analyzer);
        let analyzer2 =
            Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new());
        analyzers.set(FieldId::new(2), analyzer2);

        let mut builder = InMemoryIndexBuilder::new(analyzers);
        builder.register_field_alias(FieldId::new(1), "title");
        builder.register_field_alias(FieldId::new(2), "body");

        // Index 300 docs with varied term frequencies across both fields.
        // "programming" appears in all docs to exercise >128 postings in BlockDelta.
        for doc_id in 1..=300 {
            let title = if doc_id % 10 == 0 {
                "rust language"
            } else if doc_id % 5 == 0 {
                "rust systems"
            } else if doc_id % 3 == 0 {
                "rust guide"
            } else {
                "systems design"
            };

            let body = if doc_id % 15 == 0 {
                "programming language systems rust"
            } else if doc_id % 7 == 0 {
                "programming systems architecture"
            } else {
                "programming guide"
            };

            builder
                .index_document(doc_id, &[(FieldId::new(1), title), (FieldId::new(2), body)])
                .expect("doc should index");
        }

        let index = builder.build_index();

        // Verify "programming" has >128 postings (exercises multi-block BlockDelta).
        let field = FieldId::new(2);
        let Some(programming_term_id) = index.resolve_term(field, "programming") else {
            panic!("'programming' term should exist after indexing");
        };
        let programming_postings = index.postings.get(&programming_term_id).unwrap();
        assert!(
            programming_postings.len() > 128,
            "programming term should have >128 postings to test multi-block BlockDelta; got {}",
            programming_postings.len()
        );

        let prepare = |codec_id| {
            index
                .postings
                .iter()
                .map(|(term, postings)| {
                    let values: Vec<_> = postings
                        .iter()
                        .map(|posting| {
                            (
                                SegmentLocalDocId::new(posting.doc_id),
                                TermFreq::new(posting.term_freq),
                            )
                        })
                        .collect();
                    let encoded = match codec_id {
                        CodecId::DeltaVarint => DeltaVarintCodec.encode(&values),
                        CodecId::BlockDelta => BlockDeltaCodec.encode(&values),
                    };
                    (*term, encoded)
                })
                .collect::<BTreeMap<_, _>>()
        };
        let delta_postings = prepare(CodecId::DeltaVarint);
        let block_postings = prepare(CodecId::BlockDelta);
        let delta_scratch = RefCell::new(DecodeScratch::new());
        let block_scratch = RefCell::new(DecodeScratch::new());

        // Test cases: single-term, OR (two terms), AND (two terms), fielded query.
        // BM25F cross-field aggregation stays on the in-memory path.
        // OR is also an intentional fallback today; it is included here as a non-regression
        // check that the fallback preserves results under a compressed source request.
        let queries = alloc::vec![
            "programming",      // single-term: exercises Term node with source threading
            "rust OR systems",  // OR: verifies the documented InMemory fallback
            "rust AND systems", // AND: verifies conjunction threads source through children
            "title:rust",       // fielded: exercises Term node with source threading
        ];

        for query_text in queries {
            let mut plan_scratch = leit_query::PlannerScratch::new();
            let planner = Planner::new();
            let context = leit_query::PlanningContext::new(&index, &index)
                .with_default_fields(index.default_fields());
            let plan = planner
                .plan(query_text, &context, &mut plan_scratch)
                .expect("query should plan");

            let mut baseline_stats = ExecutionStats::default();
            let baseline_result = index
                .evaluate_plan_with_source(
                    &plan,
                    SearchScorer::bm25(),
                    &NoFilter,
                    &mut baseline_stats,
                    CursorSource::InMemory,
                )
                .expect("baseline query should execute");
            assert!(!baseline_result.matches.is_empty());
            let mut baseline_top_k: Vec<(u32, Score)> = baseline_result
                .scores
                .iter()
                .map(|(&doc_id, &score)| (doc_id, score))
                .collect();
            baseline_top_k.sort_by(|a, b| {
                b.1.partial_cmp(&a.1)
                    .unwrap_or(core::cmp::Ordering::Equal)
                    .then_with(|| a.0.cmp(&b.0))
            });

            for (source_name, prepared, scratch) in [
                ("DeltaVarint", &delta_postings, &delta_scratch),
                ("BlockDelta", &block_postings, &block_scratch),
            ] {
                let mut stats = ExecutionStats::default();
                let result = index
                    .evaluate_plan_with_source(
                        &plan,
                        SearchScorer::bm25(),
                        &NoFilter,
                        &mut stats,
                        CursorSource::Compressed {
                            postings: prepared,
                            scratch,
                        },
                    )
                    .expect("prepared query should execute");
                let mut top_k: Vec<(u32, Score)> = result
                    .scores
                    .iter()
                    .map(|(&doc_id, &score)| (doc_id, score))
                    .collect();
                top_k.sort_by(|a, b| {
                    b.1.partial_cmp(&a.1)
                        .unwrap_or(core::cmp::Ordering::Equal)
                        .then_with(|| a.0.cmp(&b.0))
                });
                assert_eq!(top_k, baseline_top_k, "{source_name}");
            }
        }
    }
}
