// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::vec::Vec;

use leit_collect::{CollectorSink, TopKCollector};
use leit_core::{FieldId, Score, ScoredHit, ScratchSpace};
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
        let default_fields = index.default_fields();
        let context = PlanningContext::new(index, index).with_default_fields(default_fields);
        planner
            .plan(query, &context, &mut self.planner)
            .map_err(IndexError::Query)
    }

    /// Execute a planned query with an optional scorer and collectors.
    pub fn execute<S>(
        &mut self,
        index: &InMemoryIndex,
        plan: &ExecutionPlan,
        scorer: Option<SearchScorer>,
        collectors: &mut S,
    ) -> Result<(), IndexError>
    where
        S: CollectorSink<u32> + ?Sized,
    {
        self.last_stats = ExecutionStats::default();
        collectors.begin_query();
        let allow_pruning = !collectors.requires_exhaustive_matches();

        if collectors.needs_scores() {
            let scorer = scorer.ok_or(IndexError::MissingScorer)?;
            if !index.try_execute_root(
                plan,
                scorer,
                collectors,
                &mut self.last_stats,
                allow_pruning,
            )? {
                let result = index.evaluate_plan(plan, scorer, &mut self.last_stats)?;
                InMemoryIndex::collect_result(
                    result,
                    collectors,
                    &mut self.last_stats,
                    allow_pruning,
                );
            }
        } else if !index.try_execute_root_unscored(plan, collectors, &mut self.last_stats)? {
            let matches = index.evaluate_matches(plan)?;
            InMemoryIndex::collect_matches(matches, collectors, &mut self.last_stats);
        }
        Ok(())
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
        self.execute(index, &plan, Some(scorer), &mut collector)?;
        Ok(collector.finish())
    }
}

impl ScratchSpace for ExecutionWorkspace {
    fn clear(&mut self) {
        self.planner.reset();
        self.last_stats = ExecutionStats::default();
    }
}
