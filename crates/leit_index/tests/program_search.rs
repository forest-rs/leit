// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Typed query-program planning and search through `ExecutionWorkspace`.

use leit_collect::TopKCollector;
use leit_core::{FieldId, FilterSlotId};
use leit_index::{
    ExecutionWorkspace, FilterEvaluator, InMemoryIndex, InMemoryIndexBuilder, NoFilter,
    PlanOptions, QueryBuilder, SearchScorer,
};
use leit_query::{QueryError, QueryNode};
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

struct AllowFirstTwo;
impl AllowFirstTwo {
    const SLOTS: [FilterSlotId; 1] = [FilterSlotId::new(0)];
}
impl FilterEvaluator<u32> for AllowFirstTwo {
    fn evaluate(&self, _slot: FilterSlotId, id: &u32) -> bool {
        *id <= 2
    }

    fn slots(&self) -> &[FilterSlotId] {
        &Self::SLOTS
    }
}

fn build_weighted_index() -> InMemoryIndex {
    let mut analyzers = FieldAnalyzers::new();
    for field in [FieldId::new(1), FieldId::new(2)] {
        analyzers.set(
            field,
            Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
        );
    }
    let mut builder = InMemoryIndexBuilder::new(analyzers);
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
        .unwrap();
    builder
        .index_document(
            2,
            &[
                (FieldId::new(1), "memory safety"),
                (FieldId::new(2), "rust systems"),
            ],
        )
        .unwrap();
    builder
        .index_document(
            3,
            &[
                (FieldId::new(1), "rust filtered"),
                (FieldId::new(2), "systems"),
            ],
        )
        .unwrap();
    builder.build_index()
}

fn weighted_options(weights: &[(FieldId, f32)]) -> PlanOptions {
    PlanOptions::new().with_default_field_weights(weights.iter().copied().collect())
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
            PlanOptions::default(),
            &NoFilter,
        )
        .expect("textual search");

    let mut builder = QueryBuilder::new();
    let a = builder.term("rust");
    let b = builder.term("search");
    builder.or(vec![a, b]);
    let program = builder.build().expect("build program");

    let typed = workspace
        .search_program(
            &index,
            &program,
            10,
            SearchScorer::bm25(),
            PlanOptions::default(),
            &NoFilter,
        )
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
        .search_program(
            &index,
            &program,
            10,
            SearchScorer::bm25(),
            PlanOptions::default(),
            &NoFilter,
        )
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
        .search_program(
            &index,
            &program,
            10,
            SearchScorer::bm25(),
            PlanOptions::default(),
            &NoFilter,
        )
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
        .plan_program(&index, &program, PlanOptions::default(), &AcceptAll)
        .expect("plan with filter");

    assert!(
        matches!(
            plan.program.get(plan.program.root()),
            Some(QueryNode::ExternalFilter { .. })
        ),
        "filter slots must wrap the plan root, same as the textual path"
    );
}

#[test]
fn weighted_typed_search_ranks_and_filters_in_one_plan() {
    let index = build_weighted_index();
    let mut builder = QueryBuilder::new();
    builder.term("rust");
    let program = builder.build().expect("build program");
    let mut workspace = ExecutionWorkspace::new();
    let textual_options = weighted_options(&[(FieldId::new(1), 3.0), (FieldId::new(2), 1.0)]);
    let textual = workspace
        .search(
            &index,
            "rust",
            10,
            SearchScorer::bm25f(),
            textual_options,
            &AllowFirstTwo,
        )
        .expect("weighted textual search");
    let options = weighted_options(&[(FieldId::new(1), 3.0), (FieldId::new(2), 1.0)]);
    assert_eq!(options.default_field_weights().len(), 2);
    let plan = workspace
        .plan_program(&index, &program, options, &AllowFirstTwo)
        .expect("weighted typed plan");
    let mut collector = TopKCollector::new(10);
    workspace
        .execute(
            &index,
            &plan,
            Some(SearchScorer::bm25f()),
            &AllowFirstTwo,
            &mut collector,
        )
        .expect("weighted typed search");
    let hits = collector.finish();

    assert_eq!(
        hits.iter().map(|hit| hit.id).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(
        hits[0].id, 1,
        "title-heavy weighting should rank doc 1 first"
    );
    assert!(hits[0].score > hits[1].score);
    assert_eq!(textual, hits, "textual and typed weighted plans must agree");

    let body_heavy = weighted_options(&[(FieldId::new(1), 1.0), (FieldId::new(2), 3.0)]);
    let plan = workspace
        .plan_program(&index, &program, body_heavy, &AllowFirstTwo)
        .expect("body-heavy typed plan");
    let mut collector = TopKCollector::new(10);
    workspace
        .execute(
            &index,
            &plan,
            Some(SearchScorer::bm25f()),
            &AllowFirstTwo,
            &mut collector,
        )
        .expect("body-heavy typed search");
    let body_hits = collector.finish();
    assert_eq!(
        body_hits[0].id, 2,
        "body-heavy weighting should rank doc 2 first"
    );
    assert!(body_hits[0].score > body_hits[1].score);

    let reused_default = workspace
        .search_program(
            &index,
            &program,
            10,
            SearchScorer::bm25f(),
            PlanOptions::default(),
            &AllowFirstTwo,
        )
        .expect("default typed search after weighted plans");
    let fresh_default = ExecutionWorkspace::new()
        .search_program(
            &index,
            &program,
            10,
            SearchScorer::bm25f(),
            PlanOptions::default(),
            &AllowFirstTwo,
        )
        .expect("fresh default typed search");
    assert_eq!(
        reused_default, fresh_default,
        "plan options must not persist"
    );
}

#[test]
fn field_weights_are_scoped_to_multi_default_field_expansions() {
    let weighted_index = build_weighted_index();
    let explicit_program = leit_query::term_with_field("rust", "title");
    let mut workspace = ExecutionWorkspace::new();
    let explicit_default = workspace
        .search_program(
            &weighted_index,
            &explicit_program,
            10,
            SearchScorer::bm25f(),
            PlanOptions::default(),
            &NoFilter,
        )
        .expect("default explicit-field search");
    let explicit_weighted = workspace
        .search_program(
            &weighted_index,
            &explicit_program,
            10,
            SearchScorer::bm25f(),
            weighted_options(&[(FieldId::new(1), 0.0)]),
            &NoFilter,
        )
        .expect("weighted explicit-field search");
    assert_eq!(
        explicit_weighted, explicit_default,
        "explicitly fielded terms use unit weight"
    );

    let single_field_index = build_test_index();
    let single_default = workspace
        .search(
            &single_field_index,
            "rust",
            10,
            SearchScorer::bm25f(),
            PlanOptions::default(),
            &NoFilter,
        )
        .expect("single-default-field search");
    let single_weighted = workspace
        .search(
            &single_field_index,
            "rust",
            10,
            SearchScorer::bm25f(),
            weighted_options(&[(CONTENT, 0.0)]),
            &NoFilter,
        )
        .expect("weighted single-default-field search");
    assert_eq!(
        single_weighted, single_default,
        "single-default-field terms use unit weight"
    );
}

#[test]
fn weighted_typed_planning_rejects_invalid_weights() {
    let index = build_test_index();
    let mut builder = QueryBuilder::new();
    builder.term("rust");
    let program = builder.build().expect("build program");
    for invalid in [f32::INFINITY, f32::NEG_INFINITY, f32::NAN, -1.0] {
        let options = weighted_options(&[(CONTENT, invalid)]);
        let error = ExecutionWorkspace::new()
            .plan_program(&index, &program, options, &NoFilter)
            .expect_err("invalid field weights should be rejected");
        assert_eq!(
            error,
            leit_index::IndexError::Query(QueryError::InvalidFieldWeight { field: CONTENT })
        );
    }

    let zero_weight = weighted_options(&[(CONTENT, 0.0)]);
    ExecutionWorkspace::new()
        .plan_program(&index, &program, zero_weight, &NoFilter)
        .expect("zero field weights are valid");
}
