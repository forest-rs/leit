use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;
use core::ops::{AddAssign, MulAssign};

use leit_collect::{Collector, TopKCollector};
use leit_core::{FieldId, QueryNodeId, Score, ScoredHit, ScratchSpace, TermId};
use leit_query::{
    ExecutionPlan, FieldRegistry, Planner, PlannerScratch, PlanningContext, QueryNode,
    QueryProgram, TermDictionary,
};
use leit_score::{Bm25Scorer, ScoringStats};
use leit_text::FieldAnalyzers;

use crate::codec::encode_segment;
use crate::error::IndexError;

const SEARCH_MISSING_TERM_ID: TermId = TermId::new(u32::MAX);
const DEFAULT_POSTINGS_BLOCK_SIZE: usize = 2;

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
struct PostingBlock {
    start: usize,
    end: usize,
    end_doc: u32,
    max_term_freq: u32,
    min_doc_length: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct EvalResult {
    matches: BTreeSet<u32>,
    scores: BTreeMap<u32, Score>,
}

impl EvalResult {
    fn from_scores(scores: BTreeMap<u32, Score>) -> Self {
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

#[derive(Debug)]
struct BuildState {
    documents: BTreeSet<u32>,
    terms_to_ids: BTreeMap<(FieldId, String), TermId>,
    term_entries: Vec<TermEntry>,
    postings: BTreeMap<TermId, Vec<PostingEntry>>,
    field_stats: BTreeMap<FieldId, FieldMetadata>,
    field_names: BTreeMap<String, FieldId>,
    field_doc_lengths: BTreeMap<(u32, FieldId), u32>,
    next_term_id: u32,
}

#[derive(Clone, Copy, Debug)]
struct BlockConfig {
    postings_block_size: usize,
}

impl Default for BlockConfig {
    fn default() -> Self {
        Self {
            postings_block_size: DEFAULT_POSTINGS_BLOCK_SIZE,
        }
    }
}

impl BuildState {
    const fn new() -> Self {
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

fn is_non_unit_boost(boost: f32) -> bool {
    (boost - 1.0).abs() > f32::EPSILON
}

#[allow(clippy::cast_precision_loss)]
const fn u32_to_f32(value: u32) -> f32 {
    value as f32
}

/// Reusable scratch buffers for high-level query execution.
#[derive(Clone, Debug, Default)]
pub struct ExecutionWorkspace {
    planner: PlannerScratch,
    last_stats: ExecutionStats,
}

/// Observability counters for one query execution.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExecutionStats {
    /// Number of postings individually scored during execution.
    pub scored_postings: usize,
    /// Number of postings blocks skipped by threshold pruning on the current
    /// direct root-term execution path.
    pub skipped_blocks: usize,
    /// Number of hits submitted to the collector.
    pub collected_hits: usize,
}

/// Explicit scorer selection for Phase 1 search execution.
#[derive(Clone, Copy, Debug)]
pub enum SearchScorer {
    /// Standard BM25 lexical scoring.
    Bm25(Bm25Scorer),
}

impl SearchScorer {
    /// Create a BM25 scorer selection with default parameters.
    pub const fn bm25() -> Self {
        Self::Bm25(Bm25Scorer::new())
    }

    fn score_term(
        self,
        term_frequency: u32,
        doc_length: u32,
        avg_doc_length: f32,
        doc_count: u32,
        doc_frequency: u32,
    ) -> Score {
        match self {
            Self::Bm25(scorer) => scorer.score(&ScoringStats {
                term_frequency,
                doc_length,
                avg_doc_length,
                doc_count,
                doc_frequency,
            }),
        }
    }
}

impl ExecutionWorkspace {
    /// Create an empty execution workspace.
    pub fn new() -> Self {
        Self::default()
    }

    /// Return stats for the most recent execution.
    ///
    /// `skipped_blocks` is currently populated only by the direct root-term
    /// execution path.
    #[must_use]
    pub const fn last_stats(&self) -> ExecutionStats {
        self.last_stats
    }

    /// Plan a textual query for this index using reusable scratch state.
    pub fn plan(
        &mut self,
        index: &InMemoryIndex,
        query: &str,
    ) -> Result<ExecutionPlan, IndexError> {
        self.clear();
        let planner = Planner::new();
        let default_field = index.default_field();
        let dictionary = SearchDictionary { index };
        let context = PlanningContext::new(&dictionary, index).with_default_field(default_field);
        planner
            .plan(query, &context, &mut self.planner)
            .map_err(IndexError::Query)
    }

    /// Execute a planned query with an explicit scorer and collector.
    pub fn execute<C: Collector<u32>>(
        &mut self,
        index: &InMemoryIndex,
        plan: &ExecutionPlan,
        scorer: SearchScorer,
        collector: &mut C,
    ) -> Result<C::Output, IndexError> {
        self.last_stats = ExecutionStats::default();
        collector.begin_query();
        if !index.try_execute_root(plan, scorer, collector, &mut self.last_stats)? {
            let result = index.evaluate_plan(plan, scorer, &mut self.last_stats)?;
            InMemoryIndex::collect_result(result, collector, &mut self.last_stats);
        }
        Ok(collector.finish())
    }

    /// Plan and execute a textual query with an explicit scorer.
    pub fn search(
        &mut self,
        index: &InMemoryIndex,
        query: &str,
        limit: usize,
        scorer: SearchScorer,
    ) -> Result<Vec<ScoredHit<u32>>, IndexError> {
        let plan = self.plan(index, query)?;
        let mut collector = TopKCollector::new(limit);
        self.execute(index, &plan, scorer, &mut collector)
    }
}

impl ScratchSpace for ExecutionWorkspace {
    fn clear(&mut self) {
        self.planner.reset();
        self.last_stats = ExecutionStats::default();
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
                postings_block_size: DEFAULT_POSTINGS_BLOCK_SIZE,
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
        InMemoryIndex {
            analyzers: self.analyzers,
            documents: self.state.documents,
            terms_to_ids: self.state.terms_to_ids,
            term_entries: self.state.term_entries,
            postings: self.state.postings,
            posting_blocks,
            field_stats: self.state.field_stats,
            field_names: self.state.field_names,
            field_doc_lengths: self.state.field_doc_lengths,
        }
    }
}

/// An immutable searchable in-memory Phase 1 index.
#[derive(Debug)]
pub struct InMemoryIndex {
    analyzers: FieldAnalyzers,
    documents: BTreeSet<u32>,
    terms_to_ids: BTreeMap<(FieldId, String), TermId>,
    term_entries: Vec<TermEntry>,
    postings: BTreeMap<TermId, Vec<PostingEntry>>,
    posting_blocks: BTreeMap<TermId, Vec<PostingBlock>>,
    field_stats: BTreeMap<FieldId, FieldMetadata>,
    field_names: BTreeMap<String, FieldId>,
    field_doc_lengths: BTreeMap<(u32, FieldId), u32>,
}

impl InMemoryIndex {
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

    fn default_field(&self) -> FieldId {
        self.field_stats
            .values()
            .map(|stats| stats.field_id)
            .min()
            .or_else(|| self.field_names.values().min().copied())
            .unwrap_or(FieldId::new(0))
    }

    fn evaluate_plan(
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
                for doc_id in &result.matches {
                    result.scores.insert(*doc_id, Score::new(*score));
                }
                Ok(result)
            }
        }
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
        let doc_count = self
            .field_stats
            .get(&field)
            .map_or(0, |stats| stats.doc_count);
        let doc_frequency = u32::try_from(postings.len()).unwrap_or(u32::MAX);

        for posting in postings {
            stats.scored_postings = stats.scored_postings.saturating_add(1);
            let doc_length = self
                .field_doc_lengths
                .get(&(posting.doc_id, field))
                .copied()
                .unwrap_or_default();
            let mut score = scoring.score_term(
                posting.term_freq,
                doc_length,
                avg_doc_length,
                doc_count,
                doc_frequency,
            );
            if is_non_unit_boost(boost) {
                MulAssign::mul_assign(&mut score, boost);
            }
            results.insert(posting.doc_id, score);
        }

        EvalResult::from_scores(results)
    }

    fn collect_result<C: Collector<u32>>(
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

    fn try_execute_root<C: Collector<u32>>(
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
                let score = Score::new(*score);
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
        let doc_count = self
            .field_stats
            .get(&field)
            .map_or(0, |field_stats| field_stats.doc_count);
        let doc_frequency = u32::try_from(postings.len()).unwrap_or(u32::MAX);

        for block in blocks {
            if let Some(threshold) = collector.threshold() {
                let bound = Self::block_upper_bound(
                    *block,
                    boost,
                    scoring,
                    avg_doc_length,
                    doc_count,
                    doc_frequency,
                );
                if bound <= threshold {
                    stats.skipped_blocks = stats.skipped_blocks.saturating_add(1);
                    continue;
                }
            }

            for posting in &postings[block.start..block.end] {
                stats.scored_postings = stats.scored_postings.saturating_add(1);
                let doc_length = self
                    .field_doc_lengths
                    .get(&(posting.doc_id, field))
                    .copied()
                    .unwrap_or_default();
                let mut score = scoring.score_term(
                    posting.term_freq,
                    doc_length,
                    avg_doc_length,
                    doc_count,
                    doc_frequency,
                );
                if is_non_unit_boost(boost) {
                    MulAssign::mul_assign(&mut score, boost);
                }
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
        boost: f32,
        scoring: SearchScorer,
        avg_doc_length: f32,
        doc_count: u32,
        doc_frequency: u32,
    ) -> Score {
        let mut bound = scoring.score_term(
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

fn build_posting_blocks(
    term_entries: &[TermEntry],
    postings: &BTreeMap<TermId, Vec<PostingEntry>>,
    field_doc_lengths: &BTreeMap<(u32, FieldId), u32>,
    postings_block_size: usize,
) -> BTreeMap<TermId, Vec<PostingBlock>> {
    let mut blocks = BTreeMap::new();
    let block_size = postings_block_size.max(1);
    for (&term_id, term_postings) in postings {
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

struct SearchDictionary<'a> {
    index: &'a InMemoryIndex,
}

impl TermDictionary for SearchDictionary<'_> {
    fn resolve_term(&self, field: FieldId, term: &str) -> Option<TermId> {
        let analyzer = self.index.analyzers.get(field)?;
        let analyzed_tokens = analyzer.analyze(term);
        if analyzed_tokens.len() != 1 {
            return None;
        }
        let normalized = analyzed_tokens[0].1.as_str();
        Some(
            self.index
                .terms_to_ids
                .get(&(field, normalized.into()))
                .copied()
                .unwrap_or(SEARCH_MISSING_TERM_ID),
        )
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
