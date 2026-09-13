// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Planning, end-to-end search, and warmed-execution allocation observations.
//!
//! Run with `cargo bench -p leit_wind_tunnel_query --bench query_allocations`.

use std::alloc::System;
use std::collections::BTreeMap;

use leit_collect::TopKCollector;
use leit_core::{FieldId, ScoredHit};
use leit_index::{
    ExecutionStats, ExecutionWorkspace, InMemoryIndex, InMemoryIndexBuilder, NoFilter, PlanOptions,
    QueryBuilder, SearchScorer, UserQueryProgram,
};
use leit_text::{Analyzer, FieldAnalyzers, UnicodeNormalizer, WhitespaceTokenizer};
use leit_wind_tunnel::{
    CorpusGenerator,
    allocation::{AllocationSnapshot, CountingAllocator},
};

const SEED: u64 = 42;
const TOP_K: usize = 10;
const TITLE: FieldId = FieldId::new(1);
const BODY: FieldId = FieldId::new(2);
const TERMS: [&str; 8] = ["the", "be", "to", "of", "and", "a", "in", "that"];
const PLANNING_TERMS: [&str; 32] = [
    "the", "be", "to", "of", "and", "a", "in", "that", "have", "i", "it", "for", "not", "on",
    "with", "he", "as", "you", "do", "at", "this", "but", "his", "by", "from", "they", "we", "say",
    "her", "she", "or", "an",
];

#[global_allocator]
static GLOBAL: CountingAllocator<System> = CountingAllocator::new(System);

fn build_index(document_count: u32) -> InMemoryIndex {
    let corpus = CorpusGenerator::new(SEED).generate(document_count);
    let mut analyzers = FieldAnalyzers::new();
    for field in [TITLE, BODY] {
        analyzers.set(
            field,
            Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
        );
    }
    let mut builder = InMemoryIndexBuilder::new(analyzers);
    builder.register_field_alias(TITLE, "title");
    builder.register_field_alias(BODY, "body");
    for document in &corpus {
        builder
            .index_document(
                document.id,
                &[
                    (TITLE, document.title.as_str()),
                    (BODY, document.body.as_str()),
                ],
            )
            .expect("deterministic document should index");
    }
    builder.build_index()
}

fn typed_program(terms: &[&str]) -> UserQueryProgram {
    let mut builder = QueryBuilder::new();
    let children = terms.iter().map(|term| builder.term(*term)).collect();
    builder.or(children);
    builder.build().expect("typed query should build")
}

fn weighted_options() -> PlanOptions {
    PlanOptions::new().with_default_field_weights(BTreeMap::from([(TITLE, 2.0), (BODY, 0.5)]))
}

fn options_with_weight_count(weight_count: u32) -> PlanOptions {
    PlanOptions::new().with_default_field_weights(
        (1..=weight_count)
            .map(|field| (FieldId::new(field), 1.0))
            .collect(),
    )
}

fn report(document_count: u32, path: &str, snapshot: AllocationSnapshot) {
    println!(
        "query-allocation-scale documents={document_count} path={path} alloc_calls={} \
         realloc_calls={} dealloc_calls={} allocated_bytes={} released_bytes={} \
         outstanding_bytes={} peak_outstanding_bytes={}",
        snapshot.alloc_calls,
        snapshot.realloc_calls,
        snapshot.dealloc_calls,
        snapshot.allocated_bytes,
        snapshot.released_bytes,
        snapshot.outstanding_bytes,
        snapshot.peak_outstanding_bytes,
    );
}

fn report_work(document_count: u32, path: &str, stats: ExecutionStats) {
    println!(
        "query-work-scale documents={document_count} path={path} scored_postings={} \
         skipped_blocks={} collected_hits={}",
        stats.scored_postings, stats.skipped_blocks, stats.collected_hits,
    );
}

fn measure_planning(index: &InMemoryIndex, document_count: u32, program: &UserQueryProgram) {
    let mut workspace = ExecutionWorkspace::new();
    let _ = workspace
        .plan(
            index,
            "the OR be OR to OR of OR and OR a OR in OR that",
            PlanOptions::default(),
            &NoFilter,
        )
        .expect("text query should warm");
    let lease = GLOBAL.try_start_counting().expect("text planning lease");
    let text_plan = workspace
        .plan(
            index,
            "the OR be OR to OR of OR and OR a OR in OR that",
            PlanOptions::default(),
            &NoFilter,
        )
        .expect("text query should plan");
    let text_snapshot = lease.finish();
    report(document_count, "plan_text_8_terms", text_snapshot);
    std::hint::black_box(text_plan);

    let _ = workspace
        .plan_program(index, program, weighted_options(), &NoFilter)
        .expect("weighted typed query should warm");
    let lease = GLOBAL.try_start_counting().expect("typed planning lease");
    let options = weighted_options();
    let typed_plan = workspace
        .plan_program(index, program, options, &NoFilter)
        .expect("weighted typed query should plan");
    let typed_snapshot = lease.finish();
    report(
        document_count,
        "plan_typed_weighted_8_terms",
        typed_snapshot,
    );
    std::hint::black_box(typed_plan);
}

fn measure_planning_shape(index: &InMemoryIndex, term_count: usize, weight_count: u32) {
    let program = typed_program(&PLANNING_TERMS[..term_count]);
    let mut workspace = ExecutionWorkspace::new();
    let _ = workspace
        .plan_program(
            index,
            &program,
            options_with_weight_count(weight_count),
            &NoFilter,
        )
        .expect("weighted typed query should warm");
    let lease = GLOBAL.try_start_counting().expect("planning-shape lease");
    let options = options_with_weight_count(weight_count);
    let plan = workspace
        .plan_program(index, &program, options, &NoFilter)
        .expect("weighted typed query should plan");
    let snapshot = lease.finish();
    println!(
        "query-allocation-shape terms={term_count} weights={weight_count} alloc_calls={} \
         realloc_calls={} dealloc_calls={} allocated_bytes={} released_bytes={} \
         outstanding_bytes={} peak_outstanding_bytes={}",
        snapshot.alloc_calls,
        snapshot.realloc_calls,
        snapshot.dealloc_calls,
        snapshot.allocated_bytes,
        snapshot.released_bytes,
        snapshot.outstanding_bytes,
        snapshot.peak_outstanding_bytes,
    );
    std::hint::black_box(plan);
}

fn measure_execution(index: &InMemoryIndex, document_count: u32, program: &UserQueryProgram) {
    let mut workspace = ExecutionWorkspace::new();
    let plan = workspace
        .plan_program(index, program, weighted_options(), &NoFilter)
        .expect("weighted typed query should plan");
    let mut collector = TopKCollector::new(TOP_K);
    let mut sink = Vec::<ScoredHit<u32>>::with_capacity(TOP_K);
    workspace
        .execute(
            index,
            &plan,
            Some(SearchScorer::bm25f()),
            &NoFilter,
            &mut collector,
        )
        .expect("weighted execution should warm");
    collector.finish_into(&mut sink);
    sink.clear();

    let lease = GLOBAL.try_start_counting().expect("execution lease");
    workspace
        .execute(
            index,
            &plan,
            Some(SearchScorer::bm25f()),
            &NoFilter,
            &mut collector,
        )
        .expect("weighted execution should succeed");
    collector.finish_into(&mut sink);
    let snapshot = lease.finish();
    assert_eq!(
        snapshot,
        AllocationSnapshot::default(),
        "warmed execution allocated"
    );
    report(document_count, "execute_typed_weighted_warmed", snapshot);
    report_work(
        document_count,
        "execute_typed_weighted_warmed",
        workspace.last_stats(),
    );
    std::hint::black_box(sink);
}

fn measure_end_to_end(index: &InMemoryIndex, document_count: u32, program: &UserQueryProgram) {
    let mut workspace = ExecutionWorkspace::new();
    for _ in 0..3 {
        let _ = workspace
            .search(
                index,
                "the",
                TOP_K,
                SearchScorer::bm25(),
                PlanOptions::default(),
                &NoFilter,
            )
            .expect("text search should warm");
    }
    let lease = GLOBAL.try_start_counting().expect("text search lease");
    let text_hits = workspace
        .search(
            index,
            "the",
            TOP_K,
            SearchScorer::bm25(),
            PlanOptions::default(),
            &NoFilter,
        )
        .expect("text search should succeed");
    let text_snapshot = lease.finish();
    report(document_count, "search_text_warmed", text_snapshot);
    report_work(document_count, "search_text_warmed", workspace.last_stats());
    std::hint::black_box(text_hits);

    for _ in 0..3 {
        let _ = workspace
            .search_program(
                index,
                program,
                TOP_K,
                SearchScorer::bm25f(),
                weighted_options(),
                &NoFilter,
            )
            .expect("weighted typed search should warm");
    }
    let lease = GLOBAL.try_start_counting().expect("typed search lease");
    let options = weighted_options();
    let typed_hits = workspace
        .search_program(
            index,
            program,
            TOP_K,
            SearchScorer::bm25f(),
            options,
            &NoFilter,
        )
        .expect("weighted typed search should succeed");
    let typed_snapshot = lease.finish();
    report(
        document_count,
        "search_typed_weighted_warmed",
        typed_snapshot,
    );
    report_work(
        document_count,
        "search_typed_weighted_warmed",
        workspace.last_stats(),
    );
    std::hint::black_box(typed_hits);
}

fn main() {
    let program = typed_program(&TERMS);
    for document_count in [1_000_u32, 10_000, 100_000] {
        let index = build_index(document_count);
        measure_planning(&index, document_count, &program);
        measure_execution(&index, document_count, &program);
        measure_end_to_end(&index, document_count, &program);
        if document_count == 100_000 {
            for (term_count, weight_count) in [(1, 2), (8, 2), (32, 2), (32, 8), (32, 32)] {
                measure_planning_shape(&index, term_count, weight_count);
            }
        }
    }
}
