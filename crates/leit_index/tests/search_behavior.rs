// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Search behavior regressions for `leit-index`.

use std::collections::BTreeSet;

use leit_collect::{CountCollector, TopKCollector, collectors};
use leit_core::FieldId;
use leit_index::{
    ExecutionStats, ExecutionWorkspace, InMemoryIndex, InMemoryIndexBuilder, NoFilter, SearchScorer,
};
use leit_query::{Planner, PlannerScratch, PlanningContext, QueryError};
use leit_score::{Bm25FScorer, FieldStats};
use leit_text::{Analyzer, FieldAnalyzers, UnicodeNormalizer, WhitespaceTokenizer};

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

fn search(
    index: &InMemoryIndex,
    query: &str,
    limit: usize,
) -> Result<Vec<leit_core::ScoredHit<u32>>, leit_index::IndexError> {
    let mut workspace = ExecutionWorkspace::new();
    workspace.search(index, query, limit, SearchScorer::bm25(), &NoFilter)
}

fn search_with_stats(
    index: &InMemoryIndex,
    query: &str,
    limit: usize,
) -> Result<(Vec<leit_core::ScoredHit<u32>>, ExecutionStats), leit_index::IndexError> {
    let mut workspace = ExecutionWorkspace::new();
    let hits = workspace.search(index, query, limit, SearchScorer::bm25(), &NoFilter)?;
    Ok((hits, workspace.last_stats()))
}

#[test]
fn bare_multi_token_terms_require_all_tokens() {
    let mut builder = InMemoryIndexBuilder::new(test_analyzers());
    builder
        .index_document(1, &[(FieldId::new(1), "new york")])
        .expect("document should index");
    builder
        .index_document(2, &[(FieldId::new(1), "new jersey")])
        .expect("document should index");
    builder
        .index_document(3, &[(FieldId::new(1), "yorkshire")])
        .expect("document should index");
    let index = builder.build_index();

    let hits = search(&index, "new york", 5).expect("multi-token bare term should search");

    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, 1);
}

#[test]
fn repeated_field_values_merge_term_frequency_within_one_document() {
    let mut builder = InMemoryIndexBuilder::new(test_analyzers());
    builder
        .index_document(
            1,
            &[(FieldId::new(1), "rust rust"), (FieldId::new(1), "rust")],
        )
        .expect("document should index");
    builder
        .index_document(2, &[(FieldId::new(1), "rust")])
        .expect("document should index");
    let index = builder.build_index();

    let hits = search(&index, "rust", 5).expect("search should succeed");

    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].id, 1);
    assert_eq!(hits[1].id, 2);
    assert!(hits[0].score > hits[1].score);
}

#[test]
fn unqualified_search_uses_indexed_field_without_registered_alias() {
    let mut analyzers = FieldAnalyzers::new();
    let analyzer =
        Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new());
    analyzers.set(FieldId::new(3), analyzer);

    let mut builder = InMemoryIndexBuilder::new(analyzers);
    builder
        .index_document(1, &[(FieldId::new(3), "rust retrieval")])
        .expect("document should index");
    let index = builder.build_index();

    let hits = search(&index, "rust", 5).expect("search should succeed");

    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, 1);
}

#[test]
fn field_qualified_multi_token_terms_require_grouping() {
    let mut builder = InMemoryIndexBuilder::new(test_analyzers());
    builder.register_field_alias(FieldId::new(1), "title");
    builder
        .index_document(1, &[(FieldId::new(1), "new york")])
        .expect("document should index");
    let index = builder.build_index();

    let error = search(&index, "title:new york", 5)
        .expect_err("field-qualified multi-token terms should be rejected");

    assert!(matches!(
        error,
        leit_index::IndexError::Query(QueryError::ParseError)
    ));
}

#[test]
fn missing_terms_return_empty_results_for_high_level_search() {
    let mut builder = InMemoryIndexBuilder::new(test_analyzers());
    builder
        .index_document(1, &[(FieldId::new(1), "alpha beta")])
        .expect("document should index");
    let index = builder.build_index();

    let missing = search(&index, "missing", 10).expect("search should succeed");
    let missing_and_present =
        search(&index, "missing AND alpha", 10).expect("search should succeed");

    assert!(missing.is_empty());
    assert!(missing_and_present.is_empty());
}

#[test]
fn explicit_and_matches_implicit_whitespace_conjunction() {
    let mut builder = InMemoryIndexBuilder::new(test_analyzers());
    builder
        .index_document(1, &[(FieldId::new(1), "new york")])
        .expect("document should index");
    builder
        .index_document(2, &[(FieldId::new(1), "new jersey")])
        .expect("document should index");
    builder
        .index_document(3, &[(FieldId::new(1), "york city")])
        .expect("document should index");
    let index = builder.build_index();

    let implicit = search(&index, "new york", 10).expect("implicit conjunction should search");
    let explicit = search(&index, "new AND york", 10).expect("explicit conjunction should search");

    assert_eq!(implicit, explicit);
}

#[test]
fn explicit_or_returns_set_union() {
    let mut builder = InMemoryIndexBuilder::new(test_analyzers());
    builder
        .index_document(1, &[(FieldId::new(1), "new york")])
        .expect("document should index");
    builder
        .index_document(2, &[(FieldId::new(1), "new jersey")])
        .expect("document should index");
    builder
        .index_document(3, &[(FieldId::new(1), "york city")])
        .expect("document should index");
    let index = builder.build_index();

    let hits = search(&index, "new OR york", 10).expect("or query should search");

    let ids: Vec<_> = hits.into_iter().map(|hit| hit.id).collect();
    assert_eq!(ids.len(), 3);
    assert!(ids.contains(&1));
    assert!(ids.contains(&2));
    assert!(ids.contains(&3));
}

#[test]
fn field_qualified_and_mixed_scope_queries_are_stable() {
    let mut builder = InMemoryIndexBuilder::new(multi_field_analyzers());
    builder.register_field_alias(FieldId::new(2), "title");
    builder
        .index_document(
            1,
            &[(FieldId::new(1), "beta"), (FieldId::new(2), "alpha beta")],
        )
        .expect("document should index");
    builder
        .index_document(2, &[(FieldId::new(1), "beta"), (FieldId::new(2), "alpha")])
        .expect("document should index");
    builder
        .index_document(
            3,
            &[(FieldId::new(1), "gamma"), (FieldId::new(2), "alpha beta")],
        )
        .expect("document should index");
    let index = builder.build_index();

    let title_and_title =
        search(&index, "title:alpha AND title:beta", 10).expect("search should succeed");
    let mixed_and = search(&index, "title:alpha AND beta", 10).expect("search should succeed");
    let mixed_or = search(&index, "title:alpha OR beta", 10).expect("search should succeed");

    let ids: BTreeSet<_> = title_and_title.into_iter().map(|hit| hit.id).collect();
    assert_eq!(ids, BTreeSet::from([1, 3]));

    let ids: BTreeSet<_> = mixed_and.into_iter().map(|hit| hit.id).collect();
    assert_eq!(ids, BTreeSet::from([1, 2, 3]));

    let ids: Vec<_> = mixed_or.into_iter().map(|hit| hit.id).collect();
    assert_eq!(ids.len(), 3);
    assert!(ids.contains(&1));
    assert!(ids.contains(&2));
    assert!(ids.contains(&3));
}

#[test]
fn field_qualified_terms_use_field_local_bm25_stats() {
    let mut builder = InMemoryIndexBuilder::new(multi_field_analyzers());
    builder.register_field_alias(FieldId::new(2), "title");
    builder
        .index_document(
            1,
            &[
                (
                    FieldId::new(1),
                    "noise noise noise noise noise noise noise noise",
                ),
                (FieldId::new(2), "alpha"),
            ],
        )
        .expect("document should index");
    builder
        .index_document(2, &[(FieldId::new(1), ""), (FieldId::new(2), "alpha")])
        .expect("document should index");
    let index = builder.build_index();

    let title_hits = search(&index, "title:alpha", 10).expect("search should succeed");

    assert_eq!(title_hits.len(), 2);
    assert_eq!(title_hits[0].score, title_hits[1].score);
}

#[test]
fn term_search_skips_noncompetitive_blocks_once_threshold_rises() {
    let mut builder = InMemoryIndexBuilder::new(test_analyzers());
    builder
        .index_document(1, &[(FieldId::new(1), "alpha alpha alpha alpha")])
        .expect("document should index");
    builder
        .index_document(2, &[(FieldId::new(1), "alpha alpha alpha")])
        .expect("document should index");
    builder
        .index_document(
            3,
            &[(
                FieldId::new(1),
                "alpha noise noise noise noise noise noise noise noise noise",
            )],
        )
        .expect("document should index");
    builder
        .index_document(
            4,
            &[(
                FieldId::new(1),
                "alpha noise noise noise noise noise noise noise noise noise noise noise",
            )],
        )
        .expect("document should index");
    let index = builder.build_index();

    let (hits, stats) = search_with_stats(&index, "alpha", 1).expect("search should succeed");

    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, 1);
    assert!(stats.scored_postings < 4);
    assert!(stats.skipped_blocks > 0);
}

#[test]
fn bm25_single_child_default_field_expansion_uses_term_pruning() {
    let mut builder = InMemoryIndexBuilder::new(multi_field_analyzers());
    builder.register_field_alias(FieldId::new(1), "title");
    builder.register_field_alias(FieldId::new(2), "body");
    builder
        .index_document(
            1,
            &[
                (FieldId::new(1), "alpha alpha alpha alpha"),
                (FieldId::new(2), "body-only"),
            ],
        )
        .expect("document should index");
    builder
        .index_document(
            2,
            &[
                (FieldId::new(1), "alpha alpha alpha"),
                (FieldId::new(2), "body-only"),
            ],
        )
        .expect("document should index");
    builder
        .index_document(
            3,
            &[
                (
                    FieldId::new(1),
                    "alpha noise noise noise noise noise noise noise noise noise",
                ),
                (FieldId::new(2), "body-only"),
            ],
        )
        .expect("document should index");
    builder
        .index_document(
            4,
            &[
                (
                    FieldId::new(1),
                    "alpha noise noise noise noise noise noise noise noise noise noise noise",
                ),
                (FieldId::new(2), "body-only"),
            ],
        )
        .expect("document should index");
    let index = builder.build_index();

    let (hits, stats) = search_with_stats(&index, "alpha", 1).expect("search should succeed");

    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, 1);
    assert!(stats.scored_postings < 4);
    assert!(stats.skipped_blocks > 0);
}

#[test]
fn count_uses_unscored_execution_path() {
    let mut builder = InMemoryIndexBuilder::new(test_analyzers());
    builder
        .index_document(1, &[(FieldId::new(1), "alpha alpha alpha alpha")])
        .expect("document should index");
    builder
        .index_document(2, &[(FieldId::new(1), "alpha alpha alpha")])
        .expect("document should index");
    builder
        .index_document(
            3,
            &[(
                FieldId::new(1),
                "alpha noise noise noise noise noise noise noise noise noise",
            )],
        )
        .expect("document should index");
    let index = builder.build_index();

    let mut workspace = ExecutionWorkspace::new();
    let plan = workspace
        .plan(&index, "alpha", &NoFilter)
        .expect("plan should succeed");
    let mut counter = CountCollector::new();
    workspace
        .execute(&index, &plan, None, &NoFilter, &mut counter)
        .expect("count should succeed");
    let count = counter.finish();
    let stats = workspace.last_stats();

    assert_eq!(count, 3);
    assert_eq!(stats.scored_postings, 0);
    assert_eq!(stats.skipped_blocks, 0);
    assert_eq!(stats.collected_hits, 3);
}

#[test]
fn multi_collector_returns_topk_and_count_from_one_execution() {
    let mut builder = InMemoryIndexBuilder::new(test_analyzers());
    builder
        .index_document(1, &[(FieldId::new(1), "alpha alpha alpha alpha")])
        .expect("document should index");
    builder
        .index_document(2, &[(FieldId::new(1), "alpha alpha alpha")])
        .expect("document should index");
    builder
        .index_document(
            3,
            &[(
                FieldId::new(1),
                "alpha noise noise noise noise noise noise noise noise noise",
            )],
        )
        .expect("document should index");
    builder
        .index_document(
            4,
            &[(
                FieldId::new(1),
                "alpha noise noise noise noise noise noise noise noise noise noise noise",
            )],
        )
        .expect("document should index");
    let index = builder.build_index();

    let mut workspace = ExecutionWorkspace::new();
    let plan = workspace
        .plan(&index, "alpha", &NoFilter)
        .expect("plan should succeed");
    let mut top_k = TopKCollector::new(1);
    let mut count = CountCollector::new();
    let mut collectors = collectors([&mut top_k, &mut count]);
    workspace
        .execute(
            &index,
            &plan,
            Some(SearchScorer::bm25()),
            &NoFilter,
            &mut collectors,
        )
        .expect("multi-collector execution should succeed");
    let hits = top_k.finish();
    let count = count.finish();
    let stats = workspace.last_stats();

    assert_eq!(count, 4);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, 1);
    assert_eq!(stats.scored_postings, 4);
    assert_eq!(stats.skipped_blocks, 0);
    assert_eq!(stats.collected_hits, 4);
}

#[test]
fn multi_collector_uses_lowest_score_threshold_for_shared_pruning() {
    let mut builder = InMemoryIndexBuilder::new(test_analyzers());
    builder
        .index_document(1, &[(FieldId::new(1), "alpha alpha alpha alpha alpha")])
        .expect("document should index");
    builder
        .index_document(
            2,
            &[(
                FieldId::new(1),
                "alpha noise noise noise noise noise noise noise",
            )],
        )
        .expect("document should index");
    builder
        .index_document(
            3,
            &[(FieldId::new(1), "alpha alpha noise noise noise noise noise")],
        )
        .expect("document should index");
    builder
        .index_document(
            4,
            &[(
                FieldId::new(1),
                "alpha alpha alpha noise noise noise noise noise",
            )],
        )
        .expect("document should index");
    let index = builder.build_index();

    let mut workspace = ExecutionWorkspace::new();
    let plan = workspace
        .plan(&index, "alpha", &NoFilter)
        .expect("plan should succeed");

    let mut top1 = TopKCollector::new(1);
    let mut top3 = TopKCollector::new(3);
    let mut collectors = collectors([&mut top1, &mut top3]);

    workspace
        .execute(
            &index,
            &plan,
            Some(SearchScorer::bm25()),
            &NoFilter,
            &mut collectors,
        )
        .expect("execution should succeed");

    let top1_hits = top1.finish();
    let top3_hits = top3.finish();

    assert_eq!(top1_hits.len(), 1);
    assert_eq!(top3_hits.len(), 3);

    let top3_ids: BTreeSet<_> = top3_hits.into_iter().map(|hit| hit.id).collect();
    assert!(
        top3_ids.contains(&4),
        "later medium-scoring hit must not be pruned away",
    );
}

#[test]
fn score_aware_collectors_require_a_scorer() {
    let mut builder = InMemoryIndexBuilder::new(test_analyzers());
    builder
        .index_document(1, &[(FieldId::new(1), "alpha beta")])
        .expect("document should index");
    let index = builder.build_index();

    let mut workspace = ExecutionWorkspace::new();
    let plan = workspace
        .plan(&index, "alpha", &NoFilter)
        .expect("plan should succeed");
    let mut collector = TopKCollector::new(5);
    let error = workspace
        .execute(&index, &plan, None, &NoFilter, &mut collector)
        .expect_err("score-aware collector should fail without scorer");

    assert_eq!(error, leit_index::IndexError::MissingScorer);
}

#[test]
fn term_search_keeps_later_equal_score_tie_break_winner() {
    let mut builder = InMemoryIndexBuilder::new(test_analyzers());
    builder
        .index_document(1, &[(FieldId::new(1), "alpha")])
        .expect("document should index");
    builder
        .index_document(10, &[(FieldId::new(1), "alpha")])
        .expect("document should index");
    let index = builder.build_index();

    let (hits, stats) = search_with_stats(&index, "alpha", 1).expect("search should succeed");

    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, 10);
    assert_eq!(stats.skipped_blocks, 0);
}

#[test]
fn lower_level_planner_produces_empty_plan_for_unknown_term() {
    let mut builder = InMemoryIndexBuilder::new(test_analyzers());
    builder
        .index_document(1, &[(FieldId::new(1), "alpha beta")])
        .expect("document should index");
    let index = builder.build_index();

    let planner = Planner::new();
    let mut scratch = PlannerScratch::new();
    let context = PlanningContext::new(&index, &index).with_default_field(FieldId::new(1));
    let plan = planner
        .plan("missing", &context, &mut scratch)
        .expect("planner should produce an empty plan for unknown terms");

    // Execute the plan — it should produce zero results
    let mut workspace = ExecutionWorkspace::new();
    let mut collector = TopKCollector::new(10);
    workspace
        .execute(
            &index,
            &plan,
            Some(SearchScorer::bm25()),
            &NoFilter,
            &mut collector,
        )
        .expect("empty plan should execute");
    let results = collector.finish();
    assert!(results.is_empty(), "unknown term should match no documents");
}

#[test]
fn bare_query_matches_documents_in_any_indexed_field() {
    let mut builder = InMemoryIndexBuilder::new(multi_field_analyzers());
    builder.register_field_alias(FieldId::new(1), "title");
    builder.register_field_alias(FieldId::new(2), "body");
    builder
        .index_document(1, &[(FieldId::new(1), "rust programming")])
        .expect("document should index");
    builder
        .index_document(2, &[(FieldId::new(2), "rust retrieval")])
        .expect("document should index");
    let index = builder.build_index();

    let hits = search(&index, "rust", 10).expect("bare query should search across fields");

    let ids: BTreeSet<_> = hits.into_iter().map(|hit| hit.id).collect();
    assert_eq!(
        ids,
        BTreeSet::from([1, 2]),
        "bare 'rust' should match docs in both title and body fields"
    );
}

fn search_bm25f(
    index: &InMemoryIndex,
    query: &str,
    limit: usize,
) -> Result<Vec<leit_core::ScoredHit<u32>>, leit_index::IndexError> {
    let mut workspace = ExecutionWorkspace::new();
    workspace.search(index, query, limit, SearchScorer::bm25f(), &NoFilter)
}

fn search_bm25f_with_field_weights(
    index: &InMemoryIndex,
    query: &str,
    limit: usize,
    field_weights: std::collections::BTreeMap<FieldId, f32>,
) -> Result<Vec<leit_core::ScoredHit<u32>>, leit_index::IndexError> {
    let mut workspace = ExecutionWorkspace::new();
    workspace.search_bm25f_with_field_weights(index, query, limit, field_weights, &NoFilter)
}

fn search_bm25f_with_default_boost(
    index: &InMemoryIndex,
    query: &str,
    limit: usize,
    default_boost: f32,
) -> Result<Vec<leit_core::ScoredHit<u32>>, leit_index::IndexError> {
    let planner = Planner::new();
    let context = PlanningContext::new(index, index)
        .with_default_fields(vec![FieldId::new(1), FieldId::new(2)])
        .with_default_boost(default_boost);
    let mut scratch = PlannerScratch::new();
    let plan = planner
        .plan(query, &context, &mut scratch)
        .map_err(leit_index::IndexError::Query)?;
    let mut workspace = ExecutionWorkspace::new();
    let mut top_k = TopKCollector::new(limit);
    workspace.execute(
        index,
        &plan,
        Some(SearchScorer::bm25f()),
        &NoFilter,
        &mut top_k,
    )?;
    Ok(top_k.finish())
}

#[test]
fn bm25f_aggregates_field_stats_for_cross_field_matches() {
    let mut builder = InMemoryIndexBuilder::new(multi_field_analyzers());
    builder.register_field_alias(FieldId::new(1), "title");
    builder.register_field_alias(FieldId::new(2), "body");
    builder
        .index_document(
            1,
            &[
                (FieldId::new(1), "rust programming"),
                (FieldId::new(2), "rust is great"),
            ],
        )
        .expect("document should index");
    builder
        .index_document(
            2,
            &[
                (FieldId::new(1), "memory safety"),
                (FieldId::new(2), "ownership prevents bugs"),
            ],
        )
        .expect("document should index");
    let index = builder.build_index();

    let hits = search_bm25f(&index, "rust", 10).expect("bm25f search should succeed");

    assert_eq!(hits.len(), 1, "only doc 1 should match");
    assert_eq!(hits[0].id, 1);

    let expected = Bm25FScorer::new().score(
        &[
            FieldStats {
                field_id: FieldId::new(1),
                term_frequency: 1,
                field_length: 2,
                weight: 1.0,
            },
            FieldStats {
                field_id: FieldId::new(2),
                term_frequency: 1,
                field_length: 3,
                weight: 1.0,
            },
        ],
        5.0,
        2,
        1,
    );

    let delta = (hits[0].score.as_f32() - expected.as_f32()).abs();
    assert!(
        delta <= f32::EPSILON,
        "expected aggregated BM25F score {}, got {}",
        expected.as_f32(),
        hits[0].score.as_f32()
    );
}

#[test]
fn bm25f_uses_unique_document_frequency_across_fields() {
    let mut builder = InMemoryIndexBuilder::new(multi_field_analyzers());
    builder.register_field_alias(FieldId::new(1), "title");
    builder.register_field_alias(FieldId::new(2), "body");
    builder
        .index_document(
            1,
            &[
                (FieldId::new(1), "rust programming"),
                (FieldId::new(2), "rust systems"),
            ],
        )
        .expect("document should index");
    let index = builder.build_index();

    let hits = search_bm25f(&index, "rust", 10).expect("bm25f search should succeed");

    assert_eq!(hits.len(), 1, "document should match");
    assert!(
        hits[0].score > leit_core::Score::ZERO,
        "unique document frequency should not zero a valid two-field match"
    );
}

#[test]
fn bm25f_default_boost_multiplies_final_aggregate_score() {
    let mut builder = InMemoryIndexBuilder::new(multi_field_analyzers());
    builder.register_field_alias(FieldId::new(1), "title");
    builder.register_field_alias(FieldId::new(2), "body");
    builder
        .index_document(
            1,
            &[
                (FieldId::new(1), "rust programming"),
                (FieldId::new(2), "rust systems language"),
            ],
        )
        .expect("document should index");
    builder
        .index_document(
            2,
            &[
                (FieldId::new(1), "memory safety"),
                (FieldId::new(2), "ownership prevents bugs"),
            ],
        )
        .expect("document should index");
    let index = builder.build_index();

    let unboosted =
        search_bm25f_with_default_boost(&index, "rust", 10, 1.0).expect("search should succeed");
    let boosted =
        search_bm25f_with_default_boost(&index, "rust", 10, 2.0).expect("search should succeed");

    assert_eq!(unboosted.len(), 1);
    assert_eq!(boosted.len(), 1);
    assert_eq!(unboosted[0].id, boosted[0].id);
    let expected = unboosted[0].score.as_f32() * 2.0;
    let delta = (boosted[0].score.as_f32() - expected).abs();
    assert!(
        delta <= f32::EPSILON,
        "boost should multiply final score: expected {expected}, got {}",
        boosted[0].score.as_f32()
    );
}

#[test]
fn bm25f_duplicate_same_field_or_falls_back_to_generic_or_scoring() {
    let mut builder = InMemoryIndexBuilder::new(multi_field_analyzers());
    builder.register_field_alias(FieldId::new(1), "title");
    builder.register_field_alias(FieldId::new(2), "body");
    builder
        .index_document(
            1,
            &[
                (FieldId::new(1), "rust programming"),
                (FieldId::new(2), "systems language"),
            ],
        )
        .expect("document should index");
    let index = builder.build_index();

    let single = search_bm25f(&index, "title:rust", 10).expect("search should succeed");
    let duplicate =
        search_bm25f(&index, "title:rust OR title:rust", 10).expect("search should succeed");

    assert_eq!(single.len(), 1);
    assert_eq!(duplicate.len(), 1);
    assert_eq!(single[0].id, duplicate[0].id);
    let expected = single[0].score.as_f32() * 2.0;
    let delta = (duplicate[0].score.as_f32() - expected).abs();
    assert!(
        delta <= f32::EPSILON,
        "duplicate same-field OR should use generic OR summing: expected {expected}, got {}",
        duplicate[0].score.as_f32()
    );
}

#[test]
fn bm25f_explicit_cross_field_or_uses_generic_or_scoring() {
    let mut builder = InMemoryIndexBuilder::new(multi_field_analyzers());
    builder.register_field_alias(FieldId::new(1), "title");
    builder.register_field_alias(FieldId::new(2), "body");
    builder
        .index_document(
            1,
            &[
                (FieldId::new(1), "rust programming"),
                (FieldId::new(2), "rust systems language"),
            ],
        )
        .expect("document should index");
    builder
        .index_document(
            2,
            &[
                (FieldId::new(1), "memory safety"),
                (FieldId::new(2), "ownership prevents bugs"),
            ],
        )
        .expect("document should index");
    let index = builder.build_index();

    let title = search_bm25f(&index, "title:rust", 10).expect("search should succeed");
    let body = search_bm25f(&index, "body:rust", 10).expect("search should succeed");
    let explicit_or =
        search_bm25f(&index, "title:rust OR body:rust", 10).expect("search should succeed");

    assert_eq!(title.len(), 1);
    assert_eq!(body.len(), 1);
    assert_eq!(explicit_or.len(), 1);
    assert_eq!(title[0].id, explicit_or[0].id);
    assert_eq!(body[0].id, explicit_or[0].id);
    let expected = title[0].score.as_f32() + body[0].score.as_f32();
    let delta = (explicit_or[0].score.as_f32() - expected).abs();
    assert!(
        delta <= f32::EPSILON,
        "explicit cross-field OR should sum child scores: expected {expected}, got {}",
        explicit_or[0].score.as_f32()
    );
}

#[test]
fn bm25f_includes_non_hit_field_lengths_for_matching_documents() {
    let mut builder = InMemoryIndexBuilder::new(multi_field_analyzers());
    builder.register_field_alias(FieldId::new(1), "title");
    builder.register_field_alias(FieldId::new(2), "body");
    builder
        .index_document(1, &[(FieldId::new(1), "rust"), (FieldId::new(2), "short")])
        .expect("document should index");
    builder
        .index_document(
            2,
            &[
                (FieldId::new(1), "rust"),
                (
                    FieldId::new(2),
                    "long long long long long long long long long long",
                ),
            ],
        )
        .expect("document should index");
    builder
        .index_document(
            3,
            &[
                (FieldId::new(1), "memory"),
                (FieldId::new(2), "rust elsewhere"),
            ],
        )
        .expect("document should index");
    let index = builder.build_index();

    let hits = search_bm25f(&index, "rust", 10).expect("search should succeed");

    let doc1_score = hits
        .iter()
        .find(|hit| hit.id == 1)
        .expect("doc 1 should match")
        .score;
    let doc2_score = hits
        .iter()
        .find(|hit| hit.id == 2)
        .expect("doc 2 should match")
        .score;
    assert!(
        doc1_score > doc2_score,
        "non-hit field lengths should affect BM25F aggregate scores"
    );
}

#[test]
fn bm25f_includes_default_field_lengths_when_term_resolves_in_one_field() {
    let mut builder = InMemoryIndexBuilder::new(multi_field_analyzers());
    builder.register_field_alias(FieldId::new(1), "title");
    builder.register_field_alias(FieldId::new(2), "body");
    builder
        .index_document(1, &[(FieldId::new(1), "rust"), (FieldId::new(2), "short")])
        .expect("document should index");
    builder
        .index_document(
            2,
            &[
                (FieldId::new(1), "rust"),
                (
                    FieldId::new(2),
                    "long long long long long long long long long long",
                ),
            ],
        )
        .expect("document should index");
    let index = builder.build_index();

    let hits = search_bm25f(&index, "rust", 10).expect("search should succeed");

    let doc1_score = hits
        .iter()
        .find(|hit| hit.id == 1)
        .expect("doc 1 should match")
        .score;
    let doc2_score = hits
        .iter()
        .find(|hit| hit.id == 2)
        .expect("doc 2 should match")
        .score;
    assert!(
        doc1_score > doc2_score,
        "body lengths should affect BM25F even when rust resolves only in title"
    );
}

#[test]
fn bm25f_field_weights_match_scorer_output() {
    let mut builder = InMemoryIndexBuilder::new(multi_field_analyzers());
    builder.register_field_alias(FieldId::new(1), "title");
    builder.register_field_alias(FieldId::new(2), "body");
    builder
        .index_document(
            1,
            &[
                (FieldId::new(1), "rust programming"),
                (FieldId::new(2), "rust systems language"),
            ],
        )
        .expect("document should index");
    builder
        .index_document(
            2,
            &[
                (FieldId::new(1), "memory safety"),
                (FieldId::new(2), "ownership prevents bugs"),
            ],
        )
        .expect("document should index");
    let index = builder.build_index();

    let mut weights = std::collections::BTreeMap::new();
    weights.insert(FieldId::new(1), 2.0);
    weights.insert(FieldId::new(2), 0.5);
    let hits = search_bm25f_with_field_weights(&index, "rust", 10, weights)
        .expect("search should succeed");

    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, 1);

    let expected = Bm25FScorer::new().score(
        &[
            FieldStats {
                field_id: FieldId::new(1),
                term_frequency: 1,
                field_length: 2,
                weight: 2.0,
            },
            FieldStats {
                field_id: FieldId::new(2),
                term_frequency: 1,
                field_length: 3,
                weight: 0.5,
            },
        ],
        5.0, // avg_doc_length = avg(title) + avg(body) = 2.0 + 3.0
        2,   // doc_count
        1,   // doc_frequency (only doc 1 matches)
    );

    let delta = (hits[0].score.as_f32() - expected.as_f32()).abs();
    assert!(
        delta <= f32::EPSILON,
        "expected weighted BM25F score {}, got {}",
        expected.as_f32(),
        hits[0].score.as_f32()
    );
}

#[test]
fn bm25f_high_level_field_weights_reject_invalid_values() {
    let mut builder = InMemoryIndexBuilder::new(multi_field_analyzers());
    builder
        .index_document(1, &[(FieldId::new(1), "rust")])
        .expect("document should index");
    let index = builder.build_index();
    let mut weights = std::collections::BTreeMap::new();
    weights.insert(FieldId::new(1), f32::INFINITY);

    let error = search_bm25f_with_field_weights(&index, "rust", 10, weights)
        .expect_err("invalid field weights should be rejected");

    assert_eq!(
        error,
        leit_index::IndexError::Query(QueryError::InvalidFieldWeight {
            field: FieldId::new(1)
        })
    );
}

#[test]
fn bm25f_field_weights_affect_scoring() {
    let mut builder = InMemoryIndexBuilder::new(multi_field_analyzers());
    builder.register_field_alias(FieldId::new(1), "title");
    builder.register_field_alias(FieldId::new(2), "body");
    builder
        .index_document(
            1,
            &[
                (FieldId::new(1), "rust programming"),
                (FieldId::new(2), "systems language"),
            ],
        )
        .expect("document should index");
    builder
        .index_document(
            2,
            &[
                (FieldId::new(1), "memory safety"),
                (FieldId::new(2), "rust systems"),
            ],
        )
        .expect("document should index");
    let index = builder.build_index();

    let equal = search_bm25f(&index, "rust", 10).expect("search should succeed");
    let mut title_heavy = std::collections::BTreeMap::new();
    title_heavy.insert(FieldId::new(1), 3.0);
    title_heavy.insert(FieldId::new(2), 1.0);
    let weighted = search_bm25f_with_field_weights(&index, "rust", 10, title_heavy)
        .expect("search should succeed");

    assert_eq!(equal.len(), 2);
    assert_eq!(weighted.len(), 2);
    assert_eq!(
        weighted[0].id, 1,
        "title-heavy weighting should rank doc 1 first"
    );
    let equal_doc1 = equal.iter().find(|h| h.id == 1).unwrap().score;
    let weighted_doc1 = weighted.iter().find(|h| h.id == 1).unwrap().score;
    assert_ne!(
        equal_doc1, weighted_doc1,
        "field weights should change the aggregate score"
    );
}
