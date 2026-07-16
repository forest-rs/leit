// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Measured allocation facts for the two indexing phases.

use std::alloc::System;

use leit_core::FieldId;
use leit_index::{ExecutableIndex, InMemoryIndex, InMemoryIndexBuilder};
use leit_text::{Analyzer, FieldAnalyzers, UnicodeNormalizer, WhitespaceTokenizer};
use leit_wind_tunnel::allocation::{AllocationSnapshot, CountingAllocator};
use leit_wind_tunnel::corpus::{CorpusGenerator, GeneratedDoc};

const FIXTURE_NAME: &str = "index-100";
const DOCUMENT_COUNT: u32 = 100;
const TITLE: FieldId = FieldId::new(1);
const BODY: FieldId = FieldId::new(2);

#[global_allocator]
static GLOBAL: CountingAllocator<System> = CountingAllocator::new(System);

fn deterministic_index_fixture() -> (Vec<GeneratedDoc>, FieldAnalyzers) {
    let corpus = CorpusGenerator::new(42).generate(DOCUMENT_COUNT);
    let mut analyzers = FieldAnalyzers::new();
    for field in [TITLE, BODY] {
        analyzers.set(
            field,
            Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
        );
    }
    (corpus, analyzers)
}

fn start_counting() -> leit_wind_tunnel::allocation::AllocationLease<'static> {
    match GLOBAL.try_start_counting() {
        Ok(lease) => lease,
        Err(error) => panic!("exclusive allocation lease should start: {error}"),
    }
}

fn assert_nonempty_phase(phase: &str, snapshot: AllocationSnapshot) {
    assert!(
        snapshot.alloc_calls > 0 || snapshot.realloc_calls > 0,
        "{phase} must perform allocation work"
    );
    assert!(
        snapshot.allocated_bytes > 0,
        "{phase} must request allocated bytes"
    );
}

fn report(phase: &str, snapshot: AllocationSnapshot) {
    println!(
        "allocation-baseline fixture={FIXTURE_NAME} phase={phase} alloc_calls={} \
         realloc_calls={} dealloc_calls={} allocated_bytes={} released_bytes={}",
        snapshot.alloc_calls,
        snapshot.realloc_calls,
        snapshot.dealloc_calls,
        snapshot.allocated_bytes,
        snapshot.released_bytes,
    );
}

#[test]
fn insertion_and_finalization_have_separate_allocation_snapshots() {
    let (corpus, analyzers) = deterministic_index_fixture();
    let mut builder = InMemoryIndexBuilder::new(analyzers);
    builder.register_field_alias(TITLE, "title");
    builder.register_field_alias(BODY, "body");

    let mut insertion_error = None;
    let insertion_lease = start_counting();
    for document in &corpus {
        if let Err(error) = builder.index_document(
            document.id,
            &[
                (TITLE, document.title.as_str()),
                (BODY, document.body.as_str()),
            ],
        ) {
            insertion_error = Some(error);
            break;
        }
    }
    let insertion = insertion_lease.finish();

    if let Some(error) = insertion_error {
        panic!("deterministic fixture insertion should succeed: {error}");
    }

    let finalization_lease = start_counting();
    let finished_index: InMemoryIndex = builder.build_index();
    let finalization = finalization_lease.finish();

    assert_eq!(
        finished_index.document_count(),
        DOCUMENT_COUNT,
        "finished index must retain every fixture document"
    );
    assert_nonempty_phase("insertion", insertion);
    assert_nonempty_phase("finalization", finalization);
    report("insertion", insertion);
    report("finalization", finalization);

    // Keep the finished index, corpus, and analyzers' derived state alive through
    // both snapshots, all assertions, and report formatting.
    std::hint::black_box((&finished_index, &corpus));
}
