// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Indexing-throughput benchmarks: build an `InMemoryIndex` from a deterministic
//! synthetic corpus at 1K and 10K document sizes.
//!
//! Run with `cargo bench -p leit_wind_tunnel_index`.

#![expect(
    missing_docs,
    reason = "criterion_group! generates an undocumented public `benches` fn"
)]

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use leit_core::FieldId;
use leit_index::InMemoryIndexBuilder;
use leit_text::{Analyzer, FieldAnalyzers, UnicodeNormalizer, WhitespaceTokenizer};
use leit_wind_tunnel::{CorpusGenerator, corpus::GeneratedDoc};

/// Fixed seed so every benchmark run indexes byte-identical corpora.
const SEED: u64 = 42;
/// `title` field, matching the corpus generator's field 1.
const TITLE: FieldId = FieldId::new(1);
/// `body` field, matching the corpus generator's field 2.
const BODY: FieldId = FieldId::new(2);

/// Build the field analyzers used for indexing (whitespace tokenizer + unicode
/// normalizer on both fields), matching the harness integration tests.
fn make_analyzers() -> FieldAnalyzers {
    let mut analyzers = FieldAnalyzers::new();
    analyzers.set(
        TITLE,
        Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
    );
    analyzers.set(
        BODY,
        Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
    );
    analyzers
}

/// One timed iteration: construct the builder, register field aliases, index
/// every document, and build the index. Corpus generation is excluded from the
/// timed region by passing a pre-generated slice.
fn build_index(corpus: &[GeneratedDoc]) {
    let mut builder = InMemoryIndexBuilder::new(make_analyzers());
    builder.register_field_alias(TITLE, "title");
    builder.register_field_alias(BODY, "body");

    for doc in corpus {
        builder
            .index_document(
                doc.id,
                &[(TITLE, doc.title.as_str()), (BODY, doc.body.as_str())],
            )
            .expect("indexing should succeed");
    }

    let index = builder.build_index();
    criterion::black_box(index);
}

fn bench_indexing(c: &mut Criterion) {
    let generator = CorpusGenerator::new(SEED);
    let mut group = c.benchmark_group("index_build");

    for (label, count) in [("1k", 1_000_u32), ("10k", 10_000_u32)] {
        let corpus = generator.generate(count);
        group.throughput(Throughput::Elements(u64::from(count)));
        group.bench_with_input(BenchmarkId::from_parameter(label), &corpus, |b, corpus| {
            b.iter(|| build_index(corpus));
        });
    }

    group.finish();
}

criterion_group!(benches, bench_indexing);
criterion_main!(benches);
