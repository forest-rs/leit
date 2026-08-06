// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Typed query-program planning and search through `ExecutionWorkspace`.

use leit_core::{FieldId, FilterSlotId};
use leit_index::{
    ExecutionWorkspace, FilterEvaluator, InMemoryIndex, InMemoryIndexBuilder, NoFilter,
    QueryBuilder, SearchScorer,
};
use leit_query::QueryNode;
use leit_text::{Analyzer, FieldAnalyzers, UnicodeNormalizer, WhitespaceTokenizer};

const CONTENT: FieldId = FieldId::new(1);

fn build_test_index() -> InMemoryIndex {
    let mut analyzers = FieldAnalyzers::new();
    analyzers.set(
        CONTENT,
        Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
    );
    let mut builder = InMemoryIndexBuilder::new(analyzers);
    builder.register_field_alias(CONTENT, "content");
    builder
        .index_document(1, &[(CONTENT, "rust search engine")])
        .unwrap();
    builder
        .index_document(2, &[(CONTENT, "rust programming")])
        .unwrap();
    builder
        .index_document(3, &[(CONTENT, "search algorithms")])
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

#[test]
fn search_program_matches_textual_search_for_equivalent_query() {
    let index = build_test_index();
    let mut workspace = ExecutionWorkspace::new();

    let textual = workspace
        .search(
            &index,
            "rust OR search",
            10,
            SearchScorer::bm25(),
            &NoFilter,
        )
        .expect("textual search");

    let mut builder = QueryBuilder::new();
    let a = builder.term("rust");
    let b = builder.term("search");
    builder.or(vec![a, b]);
    let program = builder.build().expect("build program");

    let typed = workspace
        .search_program(&index, &program, 10, SearchScorer::bm25(), &NoFilter)
        .expect("typed search");

    assert_eq!(textual, typed, "typed and textual pipelines must agree");
    assert_eq!(typed.len(), 3);
}

#[test]
fn search_program_boolean_and_narrows_hits() {
    let index = build_test_index();
    let mut workspace = ExecutionWorkspace::new();

    let mut builder = QueryBuilder::new();
    let a = builder.term("rust");
    let b = builder.term("search");
    builder.and(vec![a, b]);
    let program = builder.build().expect("build program");

    let hits = workspace
        .search_program(&index, &program, 10, SearchScorer::bm25(), &NoFilter)
        .expect("typed AND search");

    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, 1);
}

#[test]
fn search_program_field_qualified_term() {
    let index = build_test_index();
    let mut workspace = ExecutionWorkspace::new();

    let mut builder = QueryBuilder::new();
    builder.term_with_field("rust", "content");
    let program = builder.build().expect("build program");

    let hits = workspace
        .search_program(&index, &program, 10, SearchScorer::bm25(), &NoFilter)
        .expect("typed fielded search");

    assert_eq!(hits.len(), 2);
}

#[test]
fn plan_program_wraps_filter_slots() {
    let index = build_test_index();
    let mut workspace = ExecutionWorkspace::new();

    let mut builder = QueryBuilder::new();
    builder.term("rust");
    let program = builder.build().expect("build program");

    let plan = workspace
        .plan_program(&index, &program, &AcceptAll)
        .expect("plan with filter");

    assert!(
        matches!(
            plan.program.get(plan.program.root()),
            Some(QueryNode::ExternalFilter { .. })
        ),
        "filter slots must wrap the plan root, same as the textual path"
    );
}
