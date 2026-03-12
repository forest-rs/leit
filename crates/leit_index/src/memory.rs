use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;
use core::ops::{AddAssign, MulAssign};

use leit_collect::{Collector, TopKCollector};
use leit_core::{FieldId, QueryNodeId, Score, ScoredHit, ScratchSpace, TermId};
use leit_query::{
    ExecutionPlan, FieldRegistry, QueryNode, QueryProgram, Planner, PlannerScratch,
    PlanningContext, TermDictionary,
};
use leit_score::{Bm25Scorer, ScoringStats};
use leit_text::FieldAnalyzers;

use crate::codec::encode_segment;
use crate::error::IndexError;

const SEARCH_MISSING_TERM_ID: TermId = TermId::new(u32::MAX);

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

#[derive(Clone, Debug, Default, PartialEq)]
struct EvalResult {
    scores: BTreeMap<u32, Score>,
}

impl EvalResult {
    const fn from_scores(scores: BTreeMap<u32, Score>) -> Self {
        Self { scores }
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
    doc_lengths: BTreeMap<u32, u32>,
    next_term_id: u32,
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
            doc_lengths: BTreeMap::new(),
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

#[allow(clippy::cast_precision_loss)]
const fn usize_to_f32(value: usize) -> f32 {
    value as f32
}

/// Reusable scratch buffers for high-level query execution.
#[derive(Clone, Debug, Default)]
pub struct ExecutionWorkspace {
    planner: PlannerScratch,
}

impl ExecutionWorkspace {
    /// Create an empty execution workspace.
    pub fn new() -> Self {
        Self::default()
    }
}

impl ScratchSpace for ExecutionWorkspace {
    fn clear(&mut self) {
        self.planner.reset();
    }
}

/// Mutable Phase 1 index builder.
#[derive(Debug)]
pub struct InMemoryIndexBuilder {
    analyzers: FieldAnalyzers,
    state: BuildState,
}

impl InMemoryIndexBuilder {
    /// Create an empty builder using the supplied per-field analyzers.
    pub const fn new(analyzers: FieldAnalyzers) -> Self {
        Self {
            analyzers,
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

        let mut document_length = 0_u32;
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
                document_length = document_length
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
        self.state.doc_lengths.insert(doc_id, document_length);

        for (field_id, (frequencies, field_token_count)) in pending_fields {
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
        InMemoryIndex {
            analyzers: self.analyzers,
            documents: self.state.documents,
            terms_to_ids: self.state.terms_to_ids,
            term_entries: self.state.term_entries,
            postings: self.state.postings,
            field_stats: self.state.field_stats,
            field_names: self.state.field_names,
            doc_lengths: self.state.doc_lengths,
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
    field_stats: BTreeMap<FieldId, FieldMetadata>,
    field_names: BTreeMap<String, FieldId>,
    doc_lengths: BTreeMap<u32, u32>,
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

    fn avg_doc_length(&self) -> f32 {
        if self.doc_lengths.is_empty() {
            return 0.0;
        }
        let total = u32_to_f32(self.doc_lengths.values().copied().sum::<u32>());
        total / usize_to_f32(self.doc_lengths.len())
    }

    /// Search the index and return the highest-scoring hits.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<ScoredHit<u32>>, IndexError> {
        let mut workspace = ExecutionWorkspace::new();
        self.search_with_workspace(query, limit, &mut workspace)
    }

    /// Search using a reusable execution workspace.
    pub fn search_with_workspace(
        &self,
        query: &str,
        limit: usize,
        workspace: &mut ExecutionWorkspace,
    ) -> Result<Vec<ScoredHit<u32>>, IndexError> {
        workspace.clear();
        let planner = Planner::new();
        let default_field = self
            .field_stats
            .values()
            .map(|stats| stats.field_id)
            .min()
            .or_else(|| self.field_names.values().min().copied())
            .unwrap_or(FieldId::new(0));
        let dictionary = SearchDictionary { index: self };
        let context = PlanningContext::new(&dictionary, self).with_default_field(default_field);
        let plan = planner
            .plan(query, &context, &mut workspace.planner)
            .map_err(IndexError::Query)?;

        let result = self.evaluate_plan(&plan)?;
        let mut collector = TopKCollector::new(limit);
        collector.begin_query();
        Self::collect_result(result, &mut collector);
        Ok(collector.finish())
    }

    fn evaluate_plan(&self, plan: &ExecutionPlan) -> Result<EvalResult, IndexError> {
        self.evaluate_node(plan.program.root(), &plan.program)
    }

    fn evaluate_node(
        &self,
        node_id: QueryNodeId,
        program: &QueryProgram,
    ) -> Result<EvalResult, IndexError> {
        let Some(node) = program.get(node_id) else {
            return Ok(EvalResult::default());
        };

        match node {
            QueryNode::Term { term, boost, .. } => Ok(self.eval_term(*term, *boost)),
            QueryNode::Or { children, boost } => {
                let mut scores = BTreeMap::new();
                for child in children {
                    let child_result = self.evaluate_node(*child, program)?;
                    for (doc_id, score) in child_result.scores {
                        let entry = scores.entry(doc_id).or_insert(Score::ZERO);
                        AddAssign::add_assign(entry, score);
                    }
                }
                if is_non_unit_boost(*boost) {
                    for score in scores.values_mut() {
                        MulAssign::mul_assign(score, *boost);
                    }
                }
                Ok(EvalResult::from_scores(scores))
            }
            QueryNode::And { children, boost } => {
                let mut iter = children.iter();
                let Some(first) = iter.next() else {
                    return Ok(EvalResult::default());
                };
                let mut scores = self.evaluate_node(*first, program)?.scores;
                for child in iter {
                    let child_scores = self.evaluate_node(*child, program)?.scores;
                    scores.retain(|doc_id, score| {
                        child_scores.get(doc_id).is_some_and(|child_score| {
                            AddAssign::add_assign(score, *child_score);
                            true
                        })
                    });
                }
                if is_non_unit_boost(*boost) {
                    for score in scores.values_mut() {
                        MulAssign::mul_assign(score, *boost);
                    }
                }
                Ok(EvalResult::from_scores(scores))
            }
            QueryNode::Not { child } => {
                let child_scores = self.evaluate_node(*child, program)?.scores;
                let mut scores = BTreeMap::new();
                for doc_id in &self.documents {
                    if !child_scores.contains_key(doc_id) {
                        scores.insert(*doc_id, Score::ONE);
                    }
                }
                Ok(EvalResult::from_scores(scores))
            }
            QueryNode::ConstantScore { child, score } => {
                let mut result = self.evaluate_node(*child, program)?;
                for value in result.scores.values_mut() {
                    MulAssign::mul_assign(value, *score);
                }
                Ok(result)
            }
        }
    }

    fn eval_term(&self, term: TermId, boost: f32) -> EvalResult {
        let mut scores = BTreeMap::new();
        let Some(postings) = self.postings.get(&term) else {
            return EvalResult::default();
        };

        let bm25 = Bm25Scorer::new();
        let avg_doc_length = self.avg_doc_length();
        let doc_count = self.document_count();
        let doc_frequency = u32::try_from(postings.len()).unwrap_or(u32::MAX);

        for posting in postings {
            let doc_length = self
                .doc_lengths
                .get(&posting.doc_id)
                .copied()
                .unwrap_or_default();
            let stats = ScoringStats {
                term_frequency: posting.term_freq,
                doc_length,
                avg_doc_length,
                doc_count,
                doc_frequency,
            };
            let mut score = bm25.score(&stats);
            if is_non_unit_boost(boost) {
                MulAssign::mul_assign(&mut score, boost);
            }
            scores.insert(posting.doc_id, score);
        }

        EvalResult::from_scores(scores)
    }

    fn collect_result<C: Collector<u32>>(result: EvalResult, collector: &mut C) {
        for (doc_id, score) in result.scores {
            if collector.can_skip(score) {
                continue;
            }
            collector.collect(ScoredHit::new(doc_id, score));
        }
    }
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
