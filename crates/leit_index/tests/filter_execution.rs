// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Integration tests for filter execution in `leit-index`.

use leit_collect::TopKCollector;
use leit_core::{FieldId, FilterSlotId, QueryNodeId, Score, TermId};
use leit_index::{
    ExecutionWorkspace, FilterEvaluator, InMemoryIndex, InMemoryIndexBuilder, NoFilter,
    SearchScorer,
};
use leit_query::{
    ExecutionPlan, FeatureSet, FilterPredicate, FilterValue, QueryNode, QueryProgram,
    TermDictionary,
};
use leit_text::{Analyzer, FieldAnalyzers, UnicodeNormalizer, WhitespaceTokenizer};

fn build_test_index() -> InMemoryIndex {
    let title = FieldId::new(0);
    let mut analyzers = FieldAnalyzers::new();
    analyzers.set(
        title,
        Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
    );
    let mut builder = InMemoryIndexBuilder::new(analyzers);
    builder.register_field_alias(title, "title");
    builder
        .index_document(1, &[(title, "rust search engine")])
        .unwrap();
    builder
        .index_document(2, &[(title, "rust programming")])
        .unwrap();
    builder
        .index_document(3, &[(title, "search algorithms")])
        .unwrap();
    builder.build_index()
}

struct AcceptAll;
impl AcceptAll {
    const SLOTS: [FilterSlotId; 1] = [FilterSlotId::new(0)];
}
impl FilterEvaluator<u32> for AcceptAll {
    fn evaluate(&self, _slot: FilterSlotId, _id: &u32) -> bool {
        true
    }

    fn slots(&self) -> &[FilterSlotId] {
        &Self::SLOTS
    }
}

struct RejectAll;
impl RejectAll {
    const SLOTS: [FilterSlotId; 1] = [FilterSlotId::new(0)];
}
impl FilterEvaluator<u32> for RejectAll {
    fn evaluate(&self, _slot: FilterSlotId, _id: &u32) -> bool {
        false
    }

    fn slots(&self) -> &[FilterSlotId] {
        &Self::SLOTS
    }
}

struct AcceptOnly(Vec<u32>);
impl AcceptOnly {
    const SLOTS: [FilterSlotId; 1] = [FilterSlotId::new(0)];
}
impl FilterEvaluator<u32> for AcceptOnly {
    fn evaluate(&self, _slot: FilterSlotId, id: &u32) -> bool {
        self.0.contains(id)
    }

    fn slots(&self) -> &[FilterSlotId] {
        &Self::SLOTS
    }
}

struct MultiSlotFilter;
impl MultiSlotFilter {
    const SLOTS: [FilterSlotId; 2] = [FilterSlotId::new(0), FilterSlotId::new(1)];
}
impl FilterEvaluator<u32> for MultiSlotFilter {
    fn evaluate(&self, slot: FilterSlotId, id: &u32) -> bool {
        match slot.as_u32() {
            0 => [1, 2].contains(id),
            1 => [2, 3].contains(id),
            _ => false,
        }
    }

    fn slots(&self) -> &[FilterSlotId] {
        &Self::SLOTS
    }
}

#[test]
fn accept_all_matches_unfiltered() {
    let index = build_test_index();
    let mut ws_no_filter = ExecutionWorkspace::new();
    let mut ws_accept_all = ExecutionWorkspace::new();

    let no_filter_hits = ws_no_filter
        .search(&index, "rust", 10, SearchScorer::bm25(), &NoFilter)
        .expect("NoFilter search should succeed");
    let accept_all_hits = ws_accept_all
        .search(&index, "rust", 10, SearchScorer::bm25(), &AcceptAll)
        .expect("AcceptAll search should succeed");

    assert_eq!(no_filter_hits, accept_all_hits);
}

#[test]
fn reject_all_returns_empty() {
    let index = build_test_index();

    let filter = RejectAll;
    let mut workspace = ExecutionWorkspace::new();
    let plan = workspace
        .plan(&index, "rust", &filter)
        .expect("plan should succeed");
    let mut collector = TopKCollector::new(10);
    workspace
        .execute(
            &index,
            &plan,
            Some(SearchScorer::bm25()),
            &filter,
            &mut collector,
        )
        .expect("execution should succeed");
    let hits = collector.finish();

    assert!(hits.is_empty(), "RejectAll filter should yield no results");
}

#[test]
fn selective_filter_keeps_matching_docs() {
    let index = build_test_index();

    let filter = AcceptOnly(vec![1]);
    let mut workspace = ExecutionWorkspace::new();
    let plan = workspace
        .plan(&index, "rust", &filter)
        .expect("plan should succeed");
    let mut collector = TopKCollector::new(10);
    workspace
        .execute(
            &index,
            &plan,
            Some(SearchScorer::bm25()),
            &filter,
            &mut collector,
        )
        .expect("execution should succeed");
    let hits = collector.finish();

    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, 1);
    assert!(hits[0].score > Score::ZERO);
}

#[test]
fn plan_filtered_chains_external_filters() {
    let index = build_test_index();

    let filter = MultiSlotFilter;
    let mut workspace = ExecutionWorkspace::new();
    // Two filter slots: slot 0 accepts {1,2}, slot 1 accepts {2,3}.
    // Only doc 2 matches "rust" AND passes both filters.
    let plan = workspace
        .plan(&index, "rust", &filter)
        .expect("plan should succeed");
    let mut collector = TopKCollector::new(10);
    workspace
        .execute(
            &index,
            &plan,
            Some(SearchScorer::bm25()),
            &filter,
            &mut collector,
        )
        .expect("execution should succeed");
    let hits = collector.finish();

    assert_eq!(hits.len(), 1, "only doc 2 should survive both filter slots");
    assert_eq!(hits[0].id, 2);
}

#[test]
fn structured_filter_returns_error() {
    let index = build_test_index();
    let title = FieldId::new(0);

    let term_id: TermId = index
        .resolve_term(title, "rust")
        .expect("rust should resolve");

    let program = QueryProgram::new(
        vec![
            QueryNode::Term {
                field: title,
                term: term_id,
                boost: 1.0,
            },
            QueryNode::Filter {
                input: QueryNodeId::new(0),
                predicate: FilterPredicate::Eq {
                    field: title,
                    value: FilterValue::U64(42),
                },
            },
        ],
        QueryNodeId::new(1),
        3,
    );
    let plan = ExecutionPlan {
        program,
        selectivity: 1.0,
        cost: 2,
        required_features: FeatureSet::basic(),
    };

    let mut workspace = ExecutionWorkspace::new();
    let mut collector = TopKCollector::new(10);
    let result = workspace.execute(
        &index,
        &plan,
        Some(SearchScorer::bm25()),
        &NoFilter,
        &mut collector,
    );

    assert!(
        matches!(
            result,
            Err(leit_index::IndexError::UnsupportedFilterPredicate)
        ),
        "structured Filter node should return UnsupportedFilterPredicate error, got: {result:?}"
    );
}
