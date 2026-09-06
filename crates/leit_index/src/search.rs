// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use leit_collect::{Collector, TopKCollector};
use leit_core::{FieldId, FilterEvaluator, Score, ScoredHit, ScratchSpace};
#[cfg(feature = "bench-internals")]
use leit_postings::codec::CodecError;
#[cfg(feature = "bench-internals")]
use leit_postings::cursor::{
    CompressedCursor, CursorFactory, DecodeScratch, DefaultCursorFactory, PostingsView,
};
use leit_query::{ExecutionPlan, Planner, PlannerScratch, PlanningContext, UserQueryProgram};
use leit_score::{Bm25FScorer, Bm25Scorer, FieldStats, ScoringStats};

use crate::error::IndexError;
use crate::index_surface::PlanningIndex;
use crate::memory::{EvaluationScratch, InMemoryIndex};

/// Reusable scratch buffers for high-level query execution.
#[derive(Clone, Debug, Default)]
pub struct ExecutionWorkspace {
    planner: PlannerScratch,
    default_fields: Vec<FieldId>,
    pub(crate) evaluation: EvaluationScratch,
    #[cfg(feature = "bench-internals")]
    decode: DecodeScratch,
    pub(crate) last_stats: ExecutionStats,
}

/// Observability counters for one query execution.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExecutionStats {
    /// Number of posting entries visited by scoring execution paths.
    ///
    /// Aggregate scorers may use a visited posting to build per-document field
    /// stats rather than scoring that posting independently.
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
    /// Multi-field BM25F lexical scoring.
    Bm25F(Bm25FScorer),
}

impl SearchScorer {
    /// Create a BM25 scorer selection with default parameters.
    pub const fn bm25() -> Self {
        Self::Bm25(Bm25Scorer::new())
    }

    /// Create a BM25F scorer selection with default parameters.
    pub const fn bm25f() -> Self {
        Self::Bm25F(Bm25FScorer::new())
    }

    pub(crate) fn score_term(
        self,
        field: FieldId,
        term_frequency: u32,
        doc_length: u32,
        avg_doc_length: f32,
        doc_count: u32,
        doc_frequency: u32,
    ) -> Score {
        match self {
            Self::Bm25(scorer) => score_bm25_term(
                scorer,
                term_frequency,
                doc_length,
                avg_doc_length,
                doc_count,
                doc_frequency,
                1.0,
            ),
            Self::Bm25F(scorer) => {
                let fields = [FieldStats {
                    field_id: field,
                    term_frequency,
                    field_length: doc_length,
                    weight: 1.0,
                }];
                score_bm25f_fields(
                    scorer,
                    &fields,
                    avg_doc_length,
                    doc_count,
                    doc_frequency,
                    1.0,
                )
            }
        }
    }

    pub(crate) fn score_term_fields(
        self,
        field_hits: &[FieldHit],
        avg_doc_length: f32,
        doc_count: u32,
        doc_frequency: u32,
        boost: f32,
    ) -> Score {
        let mut score = match self {
            Self::Bm25(scorer) => {
                score_bm25_fields(scorer, field_hits, doc_count, doc_frequency, 1.0)
            }
            Self::Bm25F(scorer) => {
                if field_hits.is_empty() {
                    return Score::ZERO;
                }
                let mut fields = Vec::with_capacity(field_hits.len());
                for hit in field_hits {
                    fields.push(FieldStats {
                        field_id: hit.field,
                        term_frequency: hit.term_frequency,
                        field_length: hit.field_length,
                        weight: hit.weight,
                    });
                }
                scorer.score(&fields, avg_doc_length, doc_count, doc_frequency)
            }
        };
        if (boost - 1.0).abs() > f32::EPSILON {
            score *= boost;
        }
        score
    }
}

pub(crate) fn score_bm25_term(
    scorer: Bm25Scorer,
    term_frequency: u32,
    doc_length: u32,
    avg_doc_length: f32,
    doc_count: u32,
    doc_frequency: u32,
    boost: f32,
) -> Score {
    let mut score = scorer.score(&ScoringStats {
        term_frequency,
        doc_length,
        avg_doc_length,
        doc_count,
        doc_frequency,
        ..ScoringStats::new()
    });
    if (boost - 1.0).abs() > f32::EPSILON {
        score *= boost;
    }
    score
}

pub(crate) fn score_bm25_fields(
    scorer: Bm25Scorer,
    field_hits: &[FieldHit],
    doc_count: u32,
    doc_frequency: u32,
    boost: f32,
) -> Score {
    let mut score = Score::ZERO;
    for hit in field_hits {
        score += scorer.score(&ScoringStats {
            term_frequency: hit.term_frequency,
            doc_length: hit.field_length,
            avg_doc_length: hit.avg_field_length,
            doc_count,
            doc_frequency,
            ..ScoringStats::new()
        });
    }
    if (boost - 1.0).abs() > f32::EPSILON {
        score *= boost;
    }
    score
}

pub(crate) fn score_bm25f_fields(
    scorer: Bm25FScorer,
    fields: &[FieldStats],
    avg_doc_length: f32,
    doc_count: u32,
    doc_frequency: u32,
    boost: f32,
) -> Score {
    let mut score = scorer.score(fields, avg_doc_length, doc_count, doc_frequency);
    if (boost - 1.0).abs() > f32::EPSILON {
        score *= boost;
    }
    score
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct FieldHit {
    pub(crate) field: FieldId,
    pub(crate) term_frequency: u32,
    pub(crate) field_length: u32,
    pub(crate) avg_field_length: f32,
    pub(crate) weight: f32,
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

    /// Return retained execution-buffer capacities for allocation evidence.
    #[cfg(feature = "bench-internals")]
    #[doc(hidden)]
    #[must_use]
    pub fn benchmark_scratch_capacities(&self) -> crate::memory::BenchmarkScratchCapacities {
        self.evaluation.benchmark_capacities()
    }

    /// Return retained compressed-decode capacities for allocation evidence.
    #[cfg(feature = "bench-internals")]
    #[doc(hidden)]
    #[must_use]
    pub fn benchmark_decode_capacities(&self) -> leit_postings::cursor::DecodeCapacities {
        self.decode.benchmark_capacities()
    }

    /// Decode an already encoded postings view into this workspace's retained buffers.
    ///
    /// Encoding and view construction belong outside a measured execution window.
    #[cfg(feature = "bench-internals")]
    #[doc(hidden)]
    pub fn decode_prepared_postings<'a>(
        &'a mut self,
        view: PostingsView<'a>,
    ) -> Result<CompressedCursor<'a>, CodecError> {
        DefaultCursorFactory.open_doc_cursor(view, &mut self.decode)
    }

    /// Plan a textual query for this index using reusable scratch state.
    ///
    /// The filter's [`slots()`](FilterEvaluator::slots) are used to wrap the
    /// plan with [`ExternalFilter`](leit_query::QueryNode::ExternalFilter) nodes.
    /// Pass [`NoFilter`](leit_core::NoFilter) for unfiltered queries.
    pub fn plan<I, F>(
        &mut self,
        index: &I,
        query: &str,
        filter: &F,
    ) -> Result<ExecutionPlan, IndexError>
    where
        I: PlanningIndex,
        F: FilterEvaluator<u32>,
    {
        self.clear();
        let planner = Planner::new();
        self.default_fields.clear();
        index.for_each_default_field(&mut |field| self.default_fields.push(field));
        let context =
            PlanningContext::new(index, index).with_default_fields(self.default_fields.clone());
        let mut plan = planner
            .plan(query, &context, &mut self.planner)
            .map_err(IndexError::Query)?;
        for slot in filter.slots() {
            plan.wrap_external_filter(*slot);
        }
        Ok(plan)
    }

    /// Plan a typed [`UserQueryProgram`] for this index using reusable
    /// scratch state.
    ///
    /// The typed counterpart of [`plan`](Self::plan): identical default-field
    /// setup and the same [`ExternalFilter`](leit_query::QueryNode::ExternalFilter)
    /// slot wrapping, but the query arrives as an AST instead of text, so no
    /// string parsing is involved. Invalid programs or missing default fields
    /// can still produce planning errors. Terms bypass query syntax parsing;
    /// lookup follows the index dictionary implementation. [`InMemoryIndex`]
    /// applies its field analyzer and requires exactly one resulting token.
    /// Phrases approximate AND of
    /// their terms without enforcing order, slop, or a common field; see
    /// [`Planner::plan_program`].
    pub fn plan_program<I, F>(
        &mut self,
        index: &I,
        program: &UserQueryProgram,
        filter: &F,
    ) -> Result<ExecutionPlan, IndexError>
    where
        I: PlanningIndex,
        F: FilterEvaluator<u32>,
    {
        self.clear();
        let planner = Planner::new();
        self.default_fields.clear();
        index.for_each_default_field(&mut |field| self.default_fields.push(field));
        let context =
            PlanningContext::new(index, index).with_default_fields(self.default_fields.clone());
        let mut plan = planner
            .plan_program(program, &context, &mut self.planner)
            .map_err(IndexError::Query)?;
        for slot in filter.slots() {
            plan.wrap_external_filter(*slot);
        }
        Ok(plan)
    }

    /// Plan a textual query with BM25F field-weight overrides.
    ///
    /// Fields absent from `field_weights` default to weight `1.0`. Invalid
    /// weights are rejected during planning.
    pub fn plan_with_field_weights<I, F>(
        &mut self,
        index: &I,
        query: &str,
        field_weights: BTreeMap<FieldId, f32>,
        filter: &F,
    ) -> Result<ExecutionPlan, IndexError>
    where
        I: PlanningIndex,
        F: FilterEvaluator<u32>,
    {
        self.clear();
        let planner = Planner::new();
        self.default_fields.clear();
        index.for_each_default_field(&mut |field| self.default_fields.push(field));
        let context = PlanningContext::new(index, index)
            .with_default_fields(self.default_fields.clone())
            .try_with_field_weights(field_weights)
            .map_err(IndexError::Query)?;
        let mut plan = planner
            .plan(query, &context, &mut self.planner)
            .map_err(IndexError::Query)?;
        for slot in filter.slots() {
            plan.wrap_external_filter(*slot);
        }
        Ok(plan)
    }

    /// Execute a planned query with an optional scorer, filter evaluator, and collectors.
    ///
    /// The `filter` evaluator is dispatched by [`ExternalFilter`](leit_query::QueryNode::ExternalFilter)
    /// nodes in the plan. It is **not** applied as a global post-filter — use
    /// [`plan`](Self::plan) with the same filter to ensure the plan contains the
    /// appropriate filter nodes.
    pub fn execute<S, F>(
        &mut self,
        index: &InMemoryIndex,
        plan: &ExecutionPlan,
        scorer: Option<SearchScorer>,
        filter: &F,
        collectors: &mut S,
    ) -> Result<(), IndexError>
    where
        S: Collector<u32> + ?Sized,
        F: FilterEvaluator<u32>,
    {
        self.last_stats = ExecutionStats::default();
        collectors.begin_query();
        let allow_pruning = !collectors.requires_exhaustive_matches();

        let scoring = if collectors.needs_scores() {
            Some(scorer.ok_or(IndexError::MissingScorer)?)
        } else {
            None
        };
        index.execute_reusable(
            plan,
            scoring,
            filter,
            &mut self.evaluation,
            collectors,
            &mut self.last_stats,
            allow_pruning,
        )
    }

    /// Plan and execute a textual query with an explicit scorer and filter.
    ///
    /// The filter's [`slots()`](FilterEvaluator::slots) are used to wrap the
    /// plan with [`ExternalFilter`](leit_query::QueryNode::ExternalFilter) nodes,
    /// and the evaluator is dispatched for each candidate during execution.
    /// Pass [`NoFilter`](leit_core::NoFilter) for unfiltered queries.
    pub fn search<F: FilterEvaluator<u32>>(
        &mut self,
        index: &InMemoryIndex,
        query: &str,
        limit: usize,
        scorer: SearchScorer,
        filter: &F,
    ) -> Result<Vec<ScoredHit<u32>>, IndexError> {
        let plan = self.plan(index, query, filter)?;
        let mut collector = TopKCollector::new(limit);
        self.execute(index, &plan, Some(scorer), filter, &mut collector)?;
        Ok(collector.finish())
    }

    /// Plan and execute a typed [`UserQueryProgram`] with an explicit scorer
    /// and filter.
    ///
    /// The typed counterpart of [`search`](Self::search): the filter's
    /// [`slots()`](FilterEvaluator::slots) wrap the plan with
    /// [`ExternalFilter`](leit_query::QueryNode::ExternalFilter) nodes. Pass
    /// [`NoFilter`](leit_core::NoFilter) for unfiltered queries.
    ///
    /// Terms bypass query syntax parsing. The index applies its field analyzer
    /// and resolves a term only when analysis produces exactly one token.
    /// Phrases approximate AND of their terms, without positional or same-field
    /// guarantees; see
    /// [`Planner::plan_program`].
    pub fn search_program<F: FilterEvaluator<u32>>(
        &mut self,
        index: &InMemoryIndex,
        program: &UserQueryProgram,
        limit: usize,
        scorer: SearchScorer,
        filter: &F,
    ) -> Result<Vec<ScoredHit<u32>>, IndexError> {
        let plan = self.plan_program(index, program, filter)?;
        let mut collector = TopKCollector::new(limit);
        self.execute(index, &plan, Some(scorer), filter, &mut collector)?;
        Ok(collector.finish())
    }

    /// Plan and execute a textual BM25F query with field-weight overrides.
    ///
    /// Fields absent from `field_weights` default to weight `1.0`.
    pub fn search_bm25f_with_field_weights<F: FilterEvaluator<u32>>(
        &mut self,
        index: &InMemoryIndex,
        query: &str,
        limit: usize,
        field_weights: BTreeMap<FieldId, f32>,
        filter: &F,
    ) -> Result<Vec<ScoredHit<u32>>, IndexError> {
        let plan = self.plan_with_field_weights(index, query, field_weights, filter)?;
        let mut collector = TopKCollector::new(limit);
        self.execute(
            index,
            &plan,
            Some(SearchScorer::bm25f()),
            filter,
            &mut collector,
        )?;
        Ok(collector.finish())
    }
}

impl ScratchSpace for ExecutionWorkspace {
    fn clear(&mut self) {
        self.planner.reset();
        self.default_fields.clear();
        self.last_stats = ExecutionStats::default();
    }
}

#[cfg(test)]
mod typed_boundary_tests {
    use super::*;
    use crate::InMemoryIndexBuilder;
    use alloc::vec;
    use leit_core::{FilterSlotId, NoFilter};
    use leit_query::QueryBuilder;
    use leit_text::{Analyzer, FieldAnalyzers, UnicodeNormalizer, WhitespaceTokenizer};

    fn index() -> InMemoryIndex {
        let field = FieldId::new(1);
        let mut analyzers = FieldAnalyzers::new();
        analyzers.set(field, Analyzer::new(WhitespaceTokenizer::new()));
        let mut builder = InMemoryIndexBuilder::new(analyzers);
        builder.register_field_alias(field, "body");
        builder
            .index_document(1, &[(field, "x:y (rust) OR")])
            .unwrap();
        builder.index_document(2, &[(field, "x:y")]).unwrap();
        builder.build_index()
    }

    struct OnlySecond;

    impl FilterEvaluator<u32> for OnlySecond {
        fn slots(&self) -> &[FilterSlotId] {
            const SLOTS: [FilterSlotId; 2] = [FilterSlotId::new(0), FilterSlotId::new(1)];
            &SLOTS
        }

        fn evaluate(&self, slot: FilterSlotId, id: &u32) -> bool {
            slot == FilterSlotId::new(0) || *id == 2
        }
    }

    #[test]
    fn typed_operator_like_terms_are_literal() {
        let index = index();
        let mut workspace = ExecutionWorkspace::new();
        for (text, count) in [("x:y", 2), ("(rust)", 1), ("OR", 1)] {
            let program = leit_query::term(text);
            let hits = workspace
                .search_program(&index, &program, 10, SearchScorer::bm25(), &NoFilter)
                .unwrap();
            assert_eq!(hits.len(), count, "literal term {text}");
        }
    }

    #[test]
    fn typed_terms_use_field_analysis() {
        let field = FieldId::new(1);
        let mut analyzers = FieldAnalyzers::new();
        analyzers.set(
            field,
            Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
        );
        let mut builder = InMemoryIndexBuilder::new(analyzers);
        builder.register_field_alias(field, "body");
        builder.index_document(1, &[(field, "café rust")]).unwrap();
        let index = builder.build_index();
        let mut workspace = ExecutionWorkspace::new();
        for fielded in [false, true] {
            for (text, count) in [("CAFE\u{301}", 1), ("RUST", 1), ("café rust", 0), ("", 0)] {
                let program = if fielded {
                    leit_query::term_with_field(text, "body")
                } else {
                    leit_query::term(text)
                };
                let hits = workspace
                    .search_program(&index, &program, 10, SearchScorer::bm25(), &NoFilter)
                    .unwrap();
                assert_eq!(hits.len(), count, "term {text:?}, fielded={fielded}");
                if count == 1 {
                    assert_eq!(hits[0].id, 1);
                }
            }
        }
    }

    #[test]
    fn typed_filters_reject_hits_and_workspace_recovers_after_error() {
        let index = index();
        let mut workspace = ExecutionWorkspace::new();
        let invalid = leit_query::term_with_field("x:y", "missing");
        assert!(workspace.plan_program(&index, &invalid, &NoFilter).is_err());

        for boolean in [false, true] {
            let mut builder = QueryBuilder::new();
            let term = builder.term("x:y");
            if boolean {
                let other = builder.term("OR");
                builder.or(vec![term, other]);
            }
            let program = builder.build().unwrap();
            for scorer in [SearchScorer::bm25(), SearchScorer::bm25f()] {
                let hits = workspace
                    .search_program(&index, &program, 10, scorer, &OnlySecond)
                    .unwrap();
                assert_eq!(hits.len(), 1);
                assert_eq!(hits[0].id, 2);
                let hits = workspace
                    .search_program(&index, &program, 10, scorer, &NoFilter)
                    .unwrap();
                assert_eq!(hits.len(), 2, "filter state must not leak between queries");
            }
        }
    }
}
