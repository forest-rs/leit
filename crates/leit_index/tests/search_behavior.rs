//! Search behavior regressions for `leit-index`.

use std::collections::BTreeSet;

use leit_core::FieldId;
use leit_index::{ExecutionWorkspace, InMemoryIndex, InMemoryIndexBuilder, SearchScorer};
use leit_query::{Planner, PlannerScratch, PlanningContext, QueryError};
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
    workspace.search(index, query, limit, SearchScorer::bm25())
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
    assert_eq!(ids, BTreeSet::from([1, 2]));

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
fn lower_level_planner_still_reports_unknown_term() {
    let mut builder = InMemoryIndexBuilder::new(test_analyzers());
    builder
        .index_document(1, &[(FieldId::new(1), "alpha beta")])
        .expect("document should index");
    let index = builder.build_index();

    let planner = Planner::new();
    let mut scratch = PlannerScratch::new();
    let context = PlanningContext::new(&index, &index).with_default_field(FieldId::new(1));
    let error = planner
        .plan("missing", &context, &mut scratch)
        .expect_err("planner should still surface unknown terms");

    assert!(matches!(error, QueryError::UnknownTerm { .. }));
}
