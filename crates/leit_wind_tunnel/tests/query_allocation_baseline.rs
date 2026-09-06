// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Measured allocation facts for retained-result query execution.

use std::alloc::System;

use leit_collect::TopKCollector;
use leit_core::{FieldId, ScoredHit};
use leit_index::{ExecutionWorkspace, InMemoryIndex, InMemoryIndexBuilder, NoFilter, SearchScorer};
use leit_query::ExecutionPlan;
use leit_text::{Analyzer, FieldAnalyzers, UnicodeNormalizer, WhitespaceTokenizer};
use leit_wind_tunnel::allocation::{AllocationSnapshot, CountingAllocator};
use leit_wind_tunnel::corpus::CorpusGenerator;
use leit_wind_tunnel::query_fixtures::QueryFixtures;

const RETAINED_QUERY_COUNT: usize = 32;
const TOP_K: usize = 10;
const CORPUS_DOCUMENT_COUNT: u32 = 100;

#[global_allocator]
static GLOBAL: CountingAllocator<System> = CountingAllocator::new(System);

struct PreparedQueryFixture {
    name: &'static str,
    index: InMemoryIndex,
    plan: ExecutionPlan,
    expected: Vec<ScoredHit<u32>>,
}

type RetainedFreshState = (ExecutionWorkspace, TopKCollector<u32>, Vec<ScoredHit<u32>>);

fn deterministic_query_fixture() -> PreparedQueryFixture {
    let corpus = CorpusGenerator::new(42).generate(CORPUS_DOCUMENT_COUNT);
    let title = FieldId::new(1);
    let body = FieldId::new(2);
    let mut analyzers = FieldAnalyzers::new();
    for field in [title, body] {
        analyzers.set(
            field,
            Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
        );
    }

    let mut builder = InMemoryIndexBuilder::new(analyzers);
    builder.register_field_alias(title, "title");
    builder.register_field_alias(body, "body");
    for document in corpus {
        builder
            .index_document(
                document.id,
                &[
                    (title, document.title.as_str()),
                    (body, document.body.as_str()),
                ],
            )
            .expect("deterministic fixture document should index");
    }
    let index = builder.build_index();
    let mut planning_workspace = ExecutionWorkspace::new();
    let query_fixture = QueryFixtures::multi_term_or();
    let plan = planning_workspace
        .plan(&index, query_fixture.text, &NoFilter)
        .expect("named query fixture should plan");

    let mut expected_workspace = ExecutionWorkspace::new();
    let mut expected_collector = TopKCollector::new(TOP_K);
    expected_workspace
        .execute(
            &index,
            &plan,
            Some(SearchScorer::bm25()),
            &NoFilter,
            &mut expected_collector,
        )
        .expect("named query fixture should execute");
    let mut expected = Vec::with_capacity(TOP_K);
    expected_collector.finish_into(&mut expected);

    PreparedQueryFixture {
        name: query_fixture.name,
        index,
        plan,
        expected,
    }
}

fn allocation_ops(snapshot: AllocationSnapshot) -> u64 {
    snapshot
        .alloc_calls
        .checked_add(snapshot.realloc_calls)
        .expect("allocation operation total should fit in u64")
}

fn measure_reused(
    fixture: &PreparedQueryFixture,
    execution_count: usize,
) -> (AllocationSnapshot, Vec<Vec<ScoredHit<u32>>>) {
    let mut workspace = ExecutionWorkspace::new();
    let mut collector = TopKCollector::new(TOP_K);
    let mut sinks = Vec::with_capacity(execution_count);
    sinks.resize_with(execution_count, || Vec::with_capacity(TOP_K));

    // Warm every reusable owner and output buffer before measurement.
    for sink in &mut sinks {
        workspace
            .execute(
                &fixture.index,
                &fixture.plan,
                Some(SearchScorer::bm25()),
                &NoFilter,
                &mut collector,
            )
            .expect("warm reused execution should succeed");
        collector.finish_into(sink);
        sink.clear();
    }

    let lease = match GLOBAL.try_start_counting() {
        Ok(lease) => lease,
        Err(error) => panic!("exclusive allocation lease should start: {error}"),
    };
    let mut execution_error = None;
    for sink in &mut sinks {
        if let Err(error) = workspace.execute(
            &fixture.index,
            &fixture.plan,
            Some(SearchScorer::bm25()),
            &NoFilter,
            &mut collector,
        ) {
            execution_error = Some(error);
            break;
        }
        collector.finish_into(sink);
    }
    let snapshot = lease.finish();
    if let Some(error) = execution_error {
        panic!("measured reused execution should succeed: {error}");
    }

    (snapshot, sinks)
}

fn measure_fresh(
    fixture: &PreparedQueryFixture,
    execution_count: usize,
) -> (AllocationSnapshot, Vec<RetainedFreshState>) {
    let mut retained = Vec::with_capacity(execution_count);
    let mut reserved_sinks = Vec::with_capacity(execution_count);
    reserved_sinks.resize_with(execution_count, || Vec::with_capacity(TOP_K));

    let lease = match GLOBAL.try_start_counting() {
        Ok(lease) => lease,
        Err(error) => panic!("exclusive allocation lease should start: {error}"),
    };
    let mut execution_error = None;
    // Pop keeps every unprocessed sink owned by `reserved_sinks` if execution
    // stops early; no iterator destructor can release them inside the lease.
    while let Some(mut sink) = reserved_sinks.pop() {
        let mut workspace = ExecutionWorkspace::new();
        let mut collector = TopKCollector::new(TOP_K);
        if let Err(error) = workspace.execute(
            &fixture.index,
            &fixture.plan,
            Some(SearchScorer::bm25()),
            &NoFilter,
            &mut collector,
        ) {
            execution_error = Some(error);
            retained.push((workspace, collector, sink));
            break;
        }
        collector.finish_into(&mut sink);
        retained.push((workspace, collector, sink));
    }
    let snapshot = lease.finish();
    if let Some(error) = execution_error {
        panic!("measured fresh execution should succeed: {error}");
    }

    (snapshot, retained)
}

fn report_snapshot(
    fixture: &PreparedQueryFixture,
    path: &str,
    execution_count: usize,
    snapshot: AllocationSnapshot,
) {
    println!(
        "query-allocation-baseline fixture={} corpus_docs={} path={} n={} alloc_calls={} \
         realloc_calls={} allocation_ops={} dealloc_calls={} allocated_bytes={} released_bytes={}",
        fixture.name,
        CORPUS_DOCUMENT_COUNT,
        path,
        execution_count,
        snapshot.alloc_calls,
        snapshot.realloc_calls,
        allocation_ops(snapshot),
        snapshot.dealloc_calls,
        snapshot.allocated_bytes,
        snapshot.released_bytes,
    );
}

#[test]
fn retained_results_show_constant_reused_allocation_work() {
    let fixture = deterministic_query_fixture();
    assert_eq!(
        fixture.expected.len(),
        TOP_K,
        "named fixture must fill top-k before allocation measurement"
    );

    let (reused_one, reused_one_sinks) = measure_reused(&fixture, 1);
    let (reused_many, reused_many_sinks) = measure_reused(&fixture, RETAINED_QUERY_COUNT);
    let (fresh_many, fresh_many_states) = measure_fresh(&fixture, RETAINED_QUERY_COUNT);

    assert_eq!(
        reused_one_sinks.as_slice(),
        std::slice::from_ref(&fixture.expected)
    );
    for sink in &reused_many_sinks {
        assert_eq!(sink, &fixture.expected, "reused retained-result parity");
    }
    for (_, _, sink) in &fresh_many_states {
        assert_eq!(sink, &fixture.expected, "fresh retained-result parity");
    }

    assert!(
        allocation_ops(reused_many) < allocation_ops(fresh_many),
        "reused execution should perform fewer allocation operations than fresh execution"
    );
    let reused_one_with_constant_tolerance = allocation_ops(reused_one)
        .checked_add(1)
        .expect("reused allocation tolerance should fit in u64");
    assert!(
        allocation_ops(reused_many) <= reused_one_with_constant_tolerance,
        "reused allocation work should remain constant while retaining more results"
    );

    report_snapshot(&fixture, "reused", reused_one_sinks.len(), reused_one);
    report_snapshot(&fixture, "reused", reused_many_sinks.len(), reused_many);
    report_snapshot(&fixture, "fresh", fresh_many_states.len(), fresh_many);
}
