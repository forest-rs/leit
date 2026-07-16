// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Warmed allocation evidence for reusable query execution.

use std::alloc::System;
use std::collections::BTreeMap;

use leit_collect::{CountCollector, TopKCollector};
use leit_core::{FieldId, FilterSlotId, QueryNodeId, ScoredHit};
use leit_index::{
    ExecutionWorkspace, FilterEvaluator, InMemoryIndex, InMemoryIndexBuilder, SearchScorer,
};
use leit_query::{ExecutionPlan, FeatureSet, QueryNode, QueryProgram, TermDictionary};
use leit_text::{Analyzer, FieldAnalyzers, UnicodeNormalizer, WhitespaceTokenizer};
use leit_wind_tunnel::allocation::{AllocationSnapshot, CountingAllocator};

#[global_allocator]
static GLOBAL: CountingAllocator<System> = CountingAllocator::new(System);

struct OddDocumentFilter;

impl FilterEvaluator<u32> for OddDocumentFilter {
    fn evaluate(&self, _slot: FilterSlotId, id: &u32) -> bool {
        id % 2 == 1
    }

    fn slots(&self) -> &[FilterSlotId] {
        const SLOTS: &[FilterSlotId] = &[FilterSlotId::new(0)];
        SLOTS
    }
}

#[derive(Clone)]
struct ExecutionCase {
    label: &'static str,
    plan: ExecutionPlan,
    scorer: Option<SearchScorer>,
}

fn fixture() -> InMemoryIndex {
    let mut analyzers = FieldAnalyzers::new();
    for field in [FieldId::new(1), FieldId::new(2)] {
        analyzers.set(
            field,
            Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
        );
    }
    let mut builder = InMemoryIndexBuilder::new(analyzers);
    let documents = [
        (1, "alpha beta", "alpha"),
        (2, "alpha", "beta"),
        (3, "beta gamma", "alpha beta"),
        (4, "alpha gamma", "gamma"),
        (5, "beta", "alpha gamma"),
        (6, "alpha beta gamma", "beta"),
    ];
    for (id, title, body) in documents {
        builder
            .index_document(id, &[(FieldId::new(1), title), (FieldId::new(2), body)])
            .expect("fixture document should index");
    }
    builder.build_index()
}

fn plan(nodes: Vec<QueryNode>, root: u32) -> ExecutionPlan {
    ExecutionPlan {
        program: QueryProgram::new(nodes, QueryNodeId::new(root), 4),
        selectivity: 1.0,
        cost: 1,
        required_features: FeatureSet::basic(),
    }
}

fn term(field: FieldId, id: leit_core::TermId) -> QueryNode {
    QueryNode::Term {
        field,
        term: id,
        boost: 1.0,
    }
}

fn execution_cases(index: &InMemoryIndex) -> Vec<ExecutionCase> {
    let title = FieldId::new(1);
    let body = FieldId::new(2);
    let alpha_title = index.resolve_term(title, "alpha").expect("alpha title");
    let beta_title = index.resolve_term(title, "beta").expect("beta title");
    let alpha_body = index.resolve_term(body, "alpha").expect("alpha body");
    let external = QueryNode::ExternalFilter {
        input: QueryNodeId::new(0),
        slot: FilterSlotId::new(0),
    };

    vec![
        ExecutionCase {
            label: "scored-direct-bm25",
            plan: plan(vec![term(title, alpha_title)], 0),
            scorer: Some(SearchScorer::bm25()),
        },
        ExecutionCase {
            label: "scored-direct-bm25f",
            plan: plan(vec![term(title, alpha_title)], 0),
            scorer: Some(SearchScorer::bm25f()),
        },
        ExecutionCase {
            label: "scored-bm25f-fallback",
            plan: plan(
                vec![
                    term(title, alpha_title),
                    term(body, alpha_body),
                    QueryNode::TermExpansion {
                        children: vec![QueryNodeId::new(0), QueryNodeId::new(1)],
                        fields: vec![title, body],
                        boost: 1.0,
                        field_weights: BTreeMap::new(),
                    },
                ],
                2,
            ),
            scorer: Some(SearchScorer::bm25f()),
        },
        ExecutionCase {
            label: "scored-or",
            plan: plan(
                vec![
                    term(title, alpha_title),
                    term(title, beta_title),
                    QueryNode::Or {
                        children: vec![QueryNodeId::new(0), QueryNodeId::new(1)],
                        boost: 1.0,
                    },
                ],
                2,
            ),
            scorer: Some(SearchScorer::bm25()),
        },
        ExecutionCase {
            label: "scored-and",
            plan: plan(
                vec![
                    term(title, alpha_title),
                    term(title, beta_title),
                    QueryNode::And {
                        children: vec![QueryNodeId::new(0), QueryNodeId::new(1)],
                        boost: 1.0,
                    },
                ],
                2,
            ),
            scorer: Some(SearchScorer::bm25()),
        },
        ExecutionCase {
            label: "scored-not",
            plan: plan(
                vec![
                    term(title, alpha_title),
                    QueryNode::Not {
                        child: QueryNodeId::new(0),
                    },
                ],
                1,
            ),
            scorer: Some(SearchScorer::bm25()),
        },
        ExecutionCase {
            label: "scored-constant",
            plan: plan(
                vec![
                    term(title, alpha_title),
                    QueryNode::ConstantScore {
                        child: QueryNodeId::new(0),
                        score: 3.0,
                    },
                ],
                1,
            ),
            scorer: Some(SearchScorer::bm25()),
        },
        ExecutionCase {
            label: "scored-external-filter",
            plan: plan(vec![term(title, alpha_title), external], 1),
            scorer: Some(SearchScorer::bm25()),
        },
        ExecutionCase {
            label: "unscored-direct",
            plan: plan(vec![term(title, alpha_title)], 0),
            scorer: None,
        },
        ExecutionCase {
            label: "unscored-constant",
            plan: plan(
                vec![
                    term(title, alpha_title),
                    QueryNode::ConstantScore {
                        child: QueryNodeId::new(0),
                        score: 3.0,
                    },
                ],
                1,
            ),
            scorer: None,
        },
        ExecutionCase {
            label: "unscored-fallback",
            plan: plan(
                vec![
                    term(title, alpha_title),
                    term(title, beta_title),
                    QueryNode::Or {
                        children: vec![QueryNodeId::new(0), QueryNodeId::new(1)],
                        boost: 1.0,
                    },
                ],
                2,
            ),
            scorer: None,
        },
    ]
}

fn fresh_scored(index: &InMemoryIndex, case: &ExecutionCase) -> Vec<ScoredHit<u32>> {
    let mut workspace = ExecutionWorkspace::new();
    let mut collector = TopKCollector::new(16);
    workspace
        .execute(
            index,
            &case.plan,
            case.scorer,
            &OddDocumentFilter,
            &mut collector,
        )
        .expect("fresh scored execution");
    collector.finish()
}

fn fresh_count(index: &InMemoryIndex, case: &ExecutionCase) -> usize {
    let mut workspace = ExecutionWorkspace::new();
    let mut collector = CountCollector::new();
    workspace
        .execute(index, &case.plan, None, &OddDocumentFilter, &mut collector)
        .expect("fresh unscored execution");
    collector.finish()
}

#[test]
fn warmed_query_execution_performs_no_allocations() {
    let index = fixture();
    for case in execution_cases(&index) {
        let mut workspace = ExecutionWorkspace::new();
        if case.scorer.is_some() {
            let expected = fresh_scored(&index, &case);
            let mut collector = TopKCollector::new(16);
            let mut sink = Vec::with_capacity(16);
            for _ in 0..3 {
                workspace
                    .execute(
                        &index,
                        &case.plan,
                        case.scorer,
                        &OddDocumentFilter,
                        &mut collector,
                    )
                    .expect("warm scored execution");
                collector.finish_into(&mut sink);
            }
            let capacities = workspace.benchmark_scratch_capacities();
            let lease = GLOBAL.try_start_counting().expect("allocation lease");
            let execution = workspace.execute(
                &index,
                &case.plan,
                case.scorer,
                &OddDocumentFilter,
                &mut collector,
            );
            collector.finish_into(&mut sink);
            let snapshot = lease.finish();
            let retained = sink.clone();
            let after = workspace.benchmark_scratch_capacities();
            assert!(execution.is_ok(), "{} execution failed", case.label);
            assert_eq!(retained, expected, "{} result parity", case.label);
            assert_eq!(after, capacities, "{} capacity stability", case.label);
            assert_eq!(snapshot, AllocationSnapshot::default(), "{}", case.label);
        } else {
            let expected = fresh_count(&index, &case);
            let mut collector = CountCollector::new();
            for _ in 0..3 {
                workspace
                    .execute(&index, &case.plan, None, &OddDocumentFilter, &mut collector)
                    .expect("warm unscored execution");
            }
            let capacities = workspace.benchmark_scratch_capacities();
            let lease = GLOBAL.try_start_counting().expect("allocation lease");
            let execution =
                workspace.execute(&index, &case.plan, None, &OddDocumentFilter, &mut collector);
            let snapshot = lease.finish();
            let retained = collector.finish();
            let after = workspace.benchmark_scratch_capacities();
            assert!(execution.is_ok(), "{} execution failed", case.label);
            assert_eq!(retained, expected, "{} result parity", case.label);
            assert_eq!(after, capacities, "{} capacity stability", case.label);
            assert_eq!(snapshot, AllocationSnapshot::default(), "{}", case.label);
        }
    }
}
