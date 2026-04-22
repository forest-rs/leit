// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::vec::Vec;

use leit_collect::{Collector, TopKCollector};
use leit_core::{FieldId, FilterEvaluator, Score, ScoredHit, ScratchSpace};
use leit_query::{ExecutionPlan, Planner, PlannerScratch, PlanningContext};
use leit_score::{Bm25FScorer, Bm25Scorer, FieldStats, Scorer, ScoringStats};

use crate::error::IndexError;
use crate::memory::InMemoryIndex;

/// Reusable scratch buffers for high-level query execution.
#[derive(Clone, Debug, Default)]
pub struct ExecutionWorkspace {
    planner: PlannerScratch,
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
            Self::Bm25(scorer) => scorer.score(&ScoringStats {
                term_frequency,
                doc_length,
                avg_doc_length,
                doc_count,
                doc_frequency,
                ..ScoringStats::new()
            }),
            Self::Bm25F(scorer) => {
                let stats = ScoringStats {
                    term_frequency,
                    doc_length,
                    avg_doc_length,
                    doc_count,
                    doc_frequency,
                    field_stats: alloc::vec![FieldStats {
                        field_id: field,
                        term_frequency,
                        field_length: doc_length,
                        weight: 1.0,
                    }],
                };
                Scorer::score(&scorer, &stats).unwrap_or(Score::ZERO)
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
                let mut score = Score::ZERO;
                for hit in field_hits {
                    let field_score = scorer.score(&ScoringStats {
                        term_frequency: hit.term_frequency,
                        doc_length: hit.field_length,
                        avg_doc_length: hit.avg_field_length,
                        doc_count,
                        doc_frequency,
                        ..ScoringStats::new()
                    });
                    score += field_score;
                }
                score
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

    /// Plan a textual query for this index using reusable scratch state.
    ///
    /// The filter's [`slots()`](FilterEvaluator::slots) are used to wrap the
    /// plan with [`ExternalFilter`](leit_query::QueryNode::ExternalFilter) nodes.
    /// Pass [`NoFilter`](leit_core::NoFilter) for unfiltered queries.
    pub fn plan<F: FilterEvaluator<u32>>(
        &mut self,
        index: &InMemoryIndex,
        query: &str,
        filter: &F,
    ) -> Result<ExecutionPlan, IndexError> {
        self.clear();
        let planner = Planner::new();
        let default_fields = index.default_fields();
        let context = PlanningContext::new(index, index).with_default_fields(default_fields);
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
        // Pruning is only safe when every active collector is non-exhaustive.
        // If any collector needs all matches, shared execution must visit them all.
        let allow_pruning = !collectors.requires_exhaustive_matches();

        if collectors.needs_scores() {
            let scorer = scorer.ok_or(IndexError::MissingScorer)?;
            if !index.try_execute_root(
                plan,
                scorer,
                collectors,
                &mut self.last_stats,
                allow_pruning,
                filter,
            )? {
                let result = index.evaluate_plan(plan, scorer, filter, &mut self.last_stats)?;
                InMemoryIndex::collect_result(
                    result,
                    collectors,
                    &mut self.last_stats,
                    allow_pruning,
                );
            }
        } else if !index.try_execute_root_unscored(
            plan,
            collectors,
            &mut self.last_stats,
            filter,
        )? {
            let matches = index.evaluate_matches(plan, filter)?;
            InMemoryIndex::collect_matches(matches, collectors, &mut self.last_stats);
        }
        Ok(())
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
}

impl ScratchSpace for ExecutionWorkspace {
    fn clear(&mut self) {
        self.planner.reset();
        self.last_stats = ExecutionStats::default();
    }
}
