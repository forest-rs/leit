// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Property-based boolean execution tests for `leit-index`.

use std::collections::BTreeSet;

use leit_collect::TopKCollector;
use leit_core::FieldId;
use leit_index::{ExecutionWorkspace, InMemoryIndex, InMemoryIndexBuilder, NoFilter, SearchScorer};
use leit_query::{ExecutionPlan, FeatureSet, QueryNode, QueryProgram, TermDictionary};
use leit_text::{Analyzer, FieldAnalyzers, UnicodeNormalizer, WhitespaceTokenizer};
use proptest::collection::vec;
use proptest::prelude::*;

fn test_analyzers() -> FieldAnalyzers {
    let mut analyzers = FieldAnalyzers::new();
    let analyzer =
        Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new());
    analyzers.set(FieldId::new(1), analyzer);
    analyzers
}

fn multi_field_analyzers() -> FieldAnalyzers {
    let mut analyzers = FieldAnalyzers::new();
    let analyzer =
        Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new());
    analyzers.set(FieldId::new(1), analyzer);
    let analyzer =
        Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new());
    analyzers.set(FieldId::new(2), analyzer);
    analyzers
}

fn doc_id_from_index(index: usize) -> u32 {
    u32::try_from(index.checked_add(1).expect("document index overflow"))
        .expect("test corpus should fit within u32 document IDs")
}

fn build_index(corpus: &[(bool, bool, bool)]) -> InMemoryIndex {
    let mut builder = InMemoryIndexBuilder::new(test_analyzers());
    for (offset, (alpha, beta, gamma)) in corpus.iter().copied().enumerate() {
        let mut terms = Vec::new();
        if alpha {
            terms.push("alpha");
        }
        if beta {
            terms.push("beta");
        }
        if gamma {
            terms.push("gamma");
        }
        let text = terms.join(" ");
        builder
            .index_document(
                doc_id_from_index(offset),
                &[(FieldId::new(1), text.as_str())],
            )
            .expect("document should index");
    }
    builder.build_index()
}

fn result_ids(index: &InMemoryIndex, query: &str, limit: usize) -> BTreeSet<u32> {
    let mut workspace = ExecutionWorkspace::new();
    workspace
        .search(index, query, limit, SearchScorer::bm25(), &NoFilter)
        .expect("query should search")
        .into_iter()
        .map(|hit| hit.id)
        .collect()
}

fn results(index: &InMemoryIndex, query: &str, limit: usize) -> Vec<leit_core::ScoredHit<u32>> {
    let mut workspace = ExecutionWorkspace::new();
    workspace
        .search(index, query, limit, SearchScorer::bm25(), &NoFilter)
        .expect("query should search")
}

fn build_mixed_scope_index(corpus: &[(bool, bool)]) -> InMemoryIndex {
    let mut builder = InMemoryIndexBuilder::new(multi_field_analyzers());
    builder.register_field_alias(FieldId::new(2), "title");
    for (offset, (default_beta, title_alpha)) in corpus.iter().copied().enumerate() {
        let default_text = if default_beta { "beta" } else { "" };
        let title_text = if title_alpha { "alpha" } else { "" };
        builder
            .index_document(
                doc_id_from_index(offset),
                &[
                    (FieldId::new(1), default_text),
                    (FieldId::new(2), title_text),
                ],
            )
            .expect("document should index");
    }
    builder.build_index()
}

fn execute_plan(
    index: &InMemoryIndex,
    plan: &ExecutionPlan,
    limit: usize,
) -> Vec<leit_core::ScoredHit<u32>> {
    let mut workspace = ExecutionWorkspace::new();
    let mut collector = TopKCollector::new(limit);
    workspace
        .execute(
            index,
            plan,
            Some(SearchScorer::bm25()),
            &NoFilter,
            &mut collector,
        )
        .expect("plan should execute");
    collector.finish()
}

#[test]
fn and_not_filters_without_adding_score() {
    let mut builder = InMemoryIndexBuilder::new(test_analyzers());
    builder
        .index_document(1, &[(FieldId::new(1), "alpha")])
        .expect("document should index");
    builder
        .index_document(2, &[(FieldId::new(1), "alpha beta")])
        .expect("document should index");
    let index = builder.build_index();

    let alpha_hits = results(&index, "alpha", 10);
    let filtered_hits = results(&index, "alpha AND NOT beta", 10);

    assert_eq!(filtered_hits.len(), 1);
    assert_eq!(filtered_hits[0].id, 1);
    assert_eq!(filtered_hits[0].score, alpha_hits[0].score);
}

#[test]
fn or_not_keeps_positive_scores_and_filter_matches_neutral() {
    let mut builder = InMemoryIndexBuilder::new(test_analyzers());
    builder
        .index_document(1, &[(FieldId::new(1), "alpha")])
        .expect("document should index");
    builder
        .index_document(2, &[(FieldId::new(1), "alpha beta")])
        .expect("document should index");
    builder
        .index_document(3, &[(FieldId::new(1), "gamma")])
        .expect("document should index");
    let index = builder.build_index();

    let hits = results(&index, "alpha OR NOT beta", 10);

    let by_id: std::collections::BTreeMap<_, _> =
        hits.into_iter().map(|hit| (hit.id, hit.score)).collect();
    assert_eq!(by_id.len(), 3);
    assert!(by_id[&1] > leit_core::Score::ZERO);
    assert!(by_id[&2] > leit_core::Score::ZERO);
    assert_eq!(by_id[&3], leit_core::Score::ZERO);
}

#[test]
fn bare_not_returns_neutral_scores() {
    let mut builder = InMemoryIndexBuilder::new(test_analyzers());
    builder
        .index_document(1, &[(FieldId::new(1), "alpha")])
        .expect("document should index");
    builder
        .index_document(2, &[(FieldId::new(1), "beta")])
        .expect("document should index");
    builder
        .index_document(3, &[(FieldId::new(1), "gamma")])
        .expect("document should index");
    let index = builder.build_index();

    let hits = results(&index, "NOT beta", 10);

    assert_eq!(
        hits.iter().map(|hit| hit.id).collect::<BTreeSet<_>>(),
        BTreeSet::from([1, 3])
    );
    assert!(hits.iter().all(|hit| hit.score == leit_core::Score::ZERO));
}

#[test]
fn constant_score_wraps_filter_only_not_query() {
    let mut builder = InMemoryIndexBuilder::new(test_analyzers());
    builder
        .index_document(1, &[(FieldId::new(1), "alpha")])
        .expect("document should index");
    builder
        .index_document(2, &[(FieldId::new(1), "beta")])
        .expect("document should index");
    builder
        .index_document(3, &[(FieldId::new(1), "gamma")])
        .expect("document should index");
    let index = builder.build_index();

    let program = QueryProgram::new(
        vec![
            QueryNode::Term {
                field: FieldId::new(1),
                term: index
                    .resolve_term(FieldId::new(1), "beta")
                    .expect("beta should resolve"),
                boost: 1.0,
            },
            QueryNode::Not {
                child: leit_core::QueryNodeId::new(0),
            },
            QueryNode::ConstantScore {
                child: leit_core::QueryNodeId::new(1),
                score: 7.5,
            },
        ],
        leit_core::QueryNodeId::new(2),
        3,
    );
    let plan = ExecutionPlan {
        program,
        selectivity: 1.0,
        cost: 1,
        required_features: FeatureSet::basic(),
    };

    let hits = execute_plan(&index, &plan, 10);

    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].score, leit_core::Score::new(7.5));
    assert_eq!(hits[1].score, leit_core::Score::new(7.5));
    assert_eq!(
        hits.iter().map(|hit| hit.id).collect::<BTreeSet<_>>(),
        BTreeSet::from([1, 3])
    );
}

#[test]
fn constant_score_wraps_mixed_scoring_and_filter_matches() {
    let mut builder = InMemoryIndexBuilder::new(test_analyzers());
    builder
        .index_document(1, &[(FieldId::new(1), "alpha")])
        .expect("document should index");
    builder
        .index_document(2, &[(FieldId::new(1), "alpha beta")])
        .expect("document should index");
    builder
        .index_document(3, &[(FieldId::new(1), "gamma")])
        .expect("document should index");
    let index = builder.build_index();

    let alpha = index
        .resolve_term(FieldId::new(1), "alpha")
        .expect("alpha should resolve");
    let beta = index
        .resolve_term(FieldId::new(1), "beta")
        .expect("beta should resolve");
    let program = QueryProgram::new(
        vec![
            QueryNode::Term {
                field: FieldId::new(1),
                term: alpha,
                boost: 1.0,
            },
            QueryNode::Term {
                field: FieldId::new(1),
                term: beta,
                boost: 1.0,
            },
            QueryNode::Not {
                child: leit_core::QueryNodeId::new(1),
            },
            QueryNode::Or {
                children: vec![
                    leit_core::QueryNodeId::new(0),
                    leit_core::QueryNodeId::new(2),
                ],
                boost: 1.0,
            },
            QueryNode::ConstantScore {
                child: leit_core::QueryNodeId::new(3),
                score: 2.25,
            },
        ],
        leit_core::QueryNodeId::new(4),
        3,
    );
    let plan = ExecutionPlan {
        program,
        selectivity: 1.0,
        cost: 1,
        required_features: FeatureSet::basic(),
    };

    let hits = execute_plan(&index, &plan, 10);

    assert_eq!(hits.len(), 3);
    assert!(
        hits.iter()
            .all(|hit| hit.score == leit_core::Score::new(2.25))
    );
    assert_eq!(
        hits.iter().map(|hit| hit.id).collect::<BTreeSet<_>>(),
        BTreeSet::from([1, 2, 3])
    );
}

proptest! {
    #[test]
    fn and_query_matches_set_intersection(corpus in vec((any::<bool>(), any::<bool>(), any::<bool>()), 1..24)) {
        let index = build_index(&corpus);
        let limit = corpus.len();

        let actual = result_ids(&index, "alpha AND beta", limit);
        let expected: BTreeSet<u32> = corpus
            .iter()
            .enumerate()
            .filter_map(|(idx, (alpha, beta, _))| {
                (*alpha && *beta).then_some(doc_id_from_index(idx))
            })
            .collect();

        prop_assert_eq!(actual, expected);
    }

    #[test]
    fn or_query_matches_set_union(corpus in vec((any::<bool>(), any::<bool>(), any::<bool>()), 1..24)) {
        let index = build_index(&corpus);
        let limit = corpus.len();

        let actual = result_ids(&index, "alpha OR beta", limit);
        let expected: BTreeSet<u32> = corpus
            .iter()
            .enumerate()
            .filter_map(|(idx, (alpha, beta, _))| {
                (*alpha || *beta).then_some(doc_id_from_index(idx))
            })
            .collect();

        prop_assert_eq!(actual, expected);
    }

    #[test]
    fn not_query_matches_complement(corpus in vec((any::<bool>(), any::<bool>(), any::<bool>()), 1..24)) {
        let index = build_index(&corpus);
        let limit = corpus.len();

        let actual = result_ids(&index, "NOT alpha", limit);
        let expected: BTreeSet<u32> = corpus
            .iter()
            .enumerate()
            .filter_map(|(idx, (alpha, _, _))| (!*alpha).then_some(doc_id_from_index(idx)))
            .collect();

        prop_assert_eq!(actual, expected);
    }

    #[test]
    fn reused_workspace_matches_fresh_workspace(
        corpus in vec((any::<bool>(), any::<bool>(), any::<bool>()), 1..24),
        query_index in 0_usize..4_usize,
    ) {
        let index = build_index(&corpus);
        let limit = corpus.len();
        let queries = ["alpha AND beta", "alpha OR gamma", "NOT beta", "alpha beta"];
        let query = queries[query_index];

        let mut fresh_workspace = ExecutionWorkspace::new();
        let fresh = fresh_workspace
            .search(&index, query, limit, SearchScorer::bm25(), &NoFilter)
            .expect("fresh workspace search should succeed");

        let mut workspace = ExecutionWorkspace::new();
        let reused = workspace
            .search(&index, query, limit, SearchScorer::bm25(), &NoFilter)
            .expect("reused workspace search should succeed");

        prop_assert_eq!(reused, fresh);
    }

    #[test]
    fn mixed_scope_and_matches_expected_ids(corpus in vec((any::<bool>(), any::<bool>()), 1..24)) {
        let index = build_mixed_scope_index(&corpus);
        let limit = corpus.len();

        let actual = result_ids(&index, "title:alpha AND beta", limit);
        let expected: BTreeSet<u32> = corpus
            .iter()
            .enumerate()
            .filter_map(|(idx, (default_beta, title_alpha))| {
                (*default_beta && *title_alpha).then_some(doc_id_from_index(idx))
            })
            .collect();

        prop_assert_eq!(actual, expected);
    }
}
