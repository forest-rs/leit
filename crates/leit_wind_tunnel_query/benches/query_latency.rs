// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Query-latency benchmarks across all five execution paths (single-term,
//! multi-term OR, multi-term AND, BM25F cross-field, fielded) at 1K and 10K
//! corpus sizes.
//!
//! The index is built once per corpus size outside the timed region; each timed
//! iteration runs `workspace.search(...)` reusing a single `ExecutionWorkspace`
//! to match realistic, amortized-allocation usage.
//!
//! Run with `cargo bench -p leit_wind_tunnel_query`.

#![expect(
    missing_docs,
    reason = "criterion_group! generates an undocumented public `benches` fn"
)]

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use leit_core::FieldId;
use leit_index::{ExecutionWorkspace, InMemoryIndex, InMemoryIndexBuilder, NoFilter, SearchScorer};
use leit_text::{Analyzer, FieldAnalyzers, UnicodeNormalizer, WhitespaceTokenizer};
use leit_wind_tunnel::{CorpusGenerator, QueryFixtures};

/// Fixed seed so every benchmark run queries byte-identical corpora.
const SEED: u64 = 42;
/// Top-k retrieval depth used for every query benchmark.
const LIMIT: usize = 10;
/// `title` field, matching the corpus generator's field 1.
const TITLE: FieldId = FieldId::new(1);
/// `body` field, matching the corpus generator's field 2.
const BODY: FieldId = FieldId::new(2);

/// Build an `InMemoryIndex` from a generated corpus, once, outside any timed
/// region.
fn build_index(corpus_size: u32) -> InMemoryIndex {
    let corpus = CorpusGenerator::new(SEED).generate(corpus_size);

    let mut analyzers = FieldAnalyzers::new();
    analyzers.set(
        TITLE,
        Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
    );
    analyzers.set(
        BODY,
        Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
    );

    let mut builder = InMemoryIndexBuilder::new(analyzers);
    builder.register_field_alias(TITLE, "title");
    builder.register_field_alias(BODY, "body");

    for doc in &corpus {
        builder
            .index_document(
                doc.id,
                &[(TITLE, doc.title.as_str()), (BODY, doc.body.as_str())],
            )
            .expect("indexing should succeed");
    }

    builder.build_index()
}

/// One execution path under benchmark: its Criterion group name, the query text
/// in `leit_query` syntax, and the scorer it exercises.
struct ExecutionPath {
    name: &'static str,
    query: &'static str,
    scorer: SearchScorer,
}

/// The five execution paths, mirroring the wind-tunnel design's benchmark matrix.
/// BM25 is used for single-term, multi-term, and fielded paths; BM25F for the
/// cross-field aggregation path.
fn execution_paths() -> [ExecutionPath; 5] {
    [
        ExecutionPath {
            name: "single_term",
            query: QueryFixtures::single_term().text,
            scorer: SearchScorer::bm25(),
        },
        ExecutionPath {
            name: "multi_term_or",
            query: QueryFixtures::multi_term_or().text,
            scorer: SearchScorer::bm25(),
        },
        ExecutionPath {
            name: "multi_term_and",
            query: QueryFixtures::multi_term_and().text,
            scorer: SearchScorer::bm25(),
        },
        ExecutionPath {
            name: "bm25f_cross_field",
            query: QueryFixtures::cross_field().text,
            scorer: SearchScorer::bm25f(),
        },
        ExecutionPath {
            name: "fielded",
            query: QueryFixtures::fielded_title().text,
            scorer: SearchScorer::bm25(),
        },
    ]
}

fn bench_queries(c: &mut Criterion) {
    // Build each corpus-size index once, before any timed region.
    let indexes = [("1k", build_index(1_000)), ("10k", build_index(10_000))];

    for path in execution_paths() {
        let mut group = c.benchmark_group(path.name);
        for (label, index) in &indexes {
            group.bench_with_input(BenchmarkId::from_parameter(label), index, |b, index| {
                // ExecutionWorkspace is created once and reused across every
                // timed iteration (amortized allocation), matching real usage.
                let mut workspace = ExecutionWorkspace::new();
                b.iter(|| {
                    let hits = workspace
                        .search(index, path.query, LIMIT, path.scorer, &NoFilter)
                        .expect("search should succeed");
                    criterion::black_box(hits)
                });
            });
        }
        group.finish();
    }
}

criterion_group!(benches, bench_queries);
criterion_main!(benches);
