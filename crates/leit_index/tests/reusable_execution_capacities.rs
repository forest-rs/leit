// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Capacity evidence for reusable query-execution state.

#![cfg(feature = "bench-internals")]

use leit_collect::TopKCollector;
use leit_core::{FieldId, QueryNodeId};
use leit_index::{ExecutionWorkspace, InMemoryIndex, InMemoryIndexBuilder, NoFilter, SearchScorer};
use leit_query::{ExecutionPlan, FeatureSet, QueryNode, QueryProgram, TermDictionary};
use leit_text::{Analyzer, FieldAnalyzers, UnicodeNormalizer, WhitespaceTokenizer};

fn fixture() -> InMemoryIndex {
    let mut analyzers = FieldAnalyzers::new();
    for field in [FieldId::new(1), FieldId::new(2)] {
        analyzers.set(
            field,
            Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
        );
    }
    let mut builder = InMemoryIndexBuilder::new(analyzers);
    for (id, title, body) in [
        (1, "alpha shared", "shared"),
        (2, "alpha beta", "shared beta"),
        (3, "beta shared", "alpha shared"),
    ] {
        builder
            .index_document(id, &[(FieldId::new(1), title), (FieldId::new(2), body)])
            .expect("fixture document should index");
    }
    builder.build_index()
}

fn composite_plan(index: &InMemoryIndex) -> ExecutionPlan {
    let alpha = index
        .resolve_term(FieldId::new(1), "alpha")
        .expect("alpha should resolve");
    let beta = index
        .resolve_term(FieldId::new(1), "beta")
        .expect("beta should resolve");
    ExecutionPlan {
        program: QueryProgram::new(
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
                QueryNode::Or {
                    children: vec![QueryNodeId::new(0), QueryNodeId::new(1)],
                    boost: 1.0,
                },
                QueryNode::And {
                    children: vec![QueryNodeId::new(2), QueryNodeId::new(0)],
                    boost: 1.0,
                },
            ],
            QueryNodeId::new(3),
            3,
        ),
        selectivity: 1.0,
        cost: 1,
        required_features: FeatureSet::basic(),
    }
}

fn exercise_all_buffers(index: &InMemoryIndex, workspace: &mut ExecutionWorkspace) {
    let mut collector = TopKCollector::new(8);
    workspace
        .execute(
            index,
            &composite_plan(index),
            Some(SearchScorer::bm25()),
            &NoFilter,
            &mut collector,
        )
        .expect("composite plan should execute");

    let plan = workspace
        .plan(index, "shared", &NoFilter)
        .expect("cross-field term should plan");
    workspace
        .execute(
            index,
            &plan,
            Some(SearchScorer::bm25f()),
            &NoFilter,
            &mut collector,
        )
        .expect("BM25F plan should execute");
}

#[test]
fn used_execution_capacities_are_nonzero_and_stable() {
    let index = fixture();
    let mut workspace = ExecutionWorkspace::new();
    exercise_all_buffers(&index, &mut workspace);
    let warmed = workspace.benchmark_scratch_capacities();

    assert!(warmed.work_stack > 0);
    assert!(warmed.frame_pool > 0);
    assert!(warmed.free_frames > 0);
    assert!(!warmed.frame_hits.is_empty());
    assert!(warmed.frame_hits.iter().all(|capacity| *capacity > 0));
    assert!(warmed.terms > 0);
    assert!(warmed.fields > 0);
    assert!(warmed.doc_hits > 0);
    assert!(warmed.field_hits > 0);
    assert!(warmed.scoring_fields > 0);
    assert!(warmed.union_spare_hits > 0);
    assert!(warmed.intersection_spare_hits > 0);

    exercise_all_buffers(&index, &mut workspace);
    assert_eq!(workspace.benchmark_scratch_capacities(), warmed);
}
