// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Whole-build allocation scaling observations at 1K, 10K, and 100K documents.
//!
//! Run with `cargo bench -p leit_wind_tunnel_index --bench index_allocations`.

use std::alloc::System;

use leit_core::FieldId;
use leit_index::{ExecutableIndex, InMemoryIndexBuilder};
use leit_text::{Analyzer, FieldAnalyzers, UnicodeNormalizer, WhitespaceTokenizer};
use leit_wind_tunnel::{CorpusGenerator, allocation::CountingAllocator};

const SEED: u64 = 42;
const TITLE: FieldId = FieldId::new(1);
const BODY: FieldId = FieldId::new(2);

#[global_allocator]
static GLOBAL: CountingAllocator<System> = CountingAllocator::new(System);

fn make_analyzers() -> FieldAnalyzers {
    let mut analyzers = FieldAnalyzers::new();
    for field in [TITLE, BODY] {
        analyzers.set(
            field,
            Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
        );
    }
    analyzers
}

fn main() {
    for document_count in [1_000_u32, 10_000, 100_000] {
        let corpus = CorpusGenerator::new(SEED).generate(document_count);
        let lease = GLOBAL.try_start_counting().expect("allocation lease");
        let mut builder = InMemoryIndexBuilder::new(make_analyzers());
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
        let index = builder.build_index();
        let snapshot = lease.finish();
        assert_eq!(
            index.document_count(),
            document_count,
            "finished index must retain every generated document"
        );

        let documents = f64::from(document_count);
        println!(
            "index-allocation-scale documents={document_count} alloc_calls={} realloc_calls={} \
             dealloc_calls={} allocated_bytes={} released_bytes={} outstanding_bytes={} \
             peak_outstanding_bytes={} allocated_bytes_per_document={:.2} \
             outstanding_bytes_per_document={:.2} peak_bytes_per_document={:.2}",
            snapshot.alloc_calls,
            snapshot.realloc_calls,
            snapshot.dealloc_calls,
            snapshot.allocated_bytes,
            snapshot.released_bytes,
            snapshot.outstanding_bytes,
            snapshot.peak_outstanding_bytes,
            snapshot.allocated_bytes as f64 / documents,
            snapshot.outstanding_bytes as f64 / documents,
            snapshot.peak_outstanding_bytes as f64 / documents,
        );

        std::hint::black_box(index);
    }
}
