// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Codec comparison benchmarks: encode/decode latency and compressed size.
//!
//! This benchmark indexes a deterministic wind-tunnel corpus and extracts all postings,
//! then measures and reports:
//! - Encode time per codec
//! - Decode time per codec
//! - Compressed size ratio vs 8-byte uncompressed baseline
//!
//! Run with `cargo bench -p leit_wind_tunnel_index --bench codec_compare`.

#![expect(
    missing_docs,
    reason = "criterion_group! generates an undocumented public `benches` fn"
)]

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use leit_core::{FieldId, SegmentLocalDocId, TermFreq};
use leit_index::InMemoryIndexBuilder;
use leit_postings::codec::{BlockDeltaCodec, Codec, DeltaVarintCodec};
use leit_text::{Analyzer, FieldAnalyzers, UnicodeNormalizer, WhitespaceTokenizer};
use leit_wind_tunnel::{CorpusGenerator, corpus::GeneratedDoc};

/// Fixed seed so every benchmark run indexes byte-identical corpora.
const SEED: u64 = 42;
/// `title` field, matching the corpus generator's field 1.
const TITLE: FieldId = FieldId::new(1);
/// `body` field, matching the corpus generator's field 2.
const BODY: FieldId = FieldId::new(2);

/// Postings list type: `Vec` of (`SegmentLocalDocId`, `TermFreq`) tuples per term.
type PostingsList = Vec<Vec<(SegmentLocalDocId, TermFreq)>>;

struct CorpusCase {
    label: &'static str,
    doc_count: u32,
    postings: PostingsList,
}

impl CorpusCase {
    fn total_postings(&self) -> usize {
        self.postings.iter().map(Vec::len).sum()
    }

    fn max_postings_len(&self) -> usize {
        self.postings.iter().map(Vec::len).max().unwrap_or(0)
    }
}

/// Build the field analyzers used for indexing (whitespace tokenizer + unicode
/// normalizer on both fields), matching the wind-tunnel integration tests.
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

/// Index a corpus and extract all postings as `Vec<(doc_id, term_freq)>` tuples.
fn extract_postings(corpus: &[GeneratedDoc]) -> PostingsList {
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

    // Extract all postings as (SegmentLocalDocId, TermFreq) tuples.
    // Postings are already doc-sorted from the index.
    index
        .benchmark_postings()
        .into_iter()
        .map(|posting_list| {
            posting_list
                .into_iter()
                .map(|(doc_id, term_freq)| {
                    (SegmentLocalDocId::new(doc_id), TermFreq::new(term_freq))
                })
                .collect()
        })
        .collect()
}

fn validate_decodes<C: Codec>(codec: &C, encoded: &[Vec<u8>], postings: &PostingsList) {
    let max_len = postings.iter().map(Vec::len).max().unwrap_or(0);
    let mut docs = Vec::with_capacity(max_len);
    let mut tfs = Vec::with_capacity(max_len);

    for (bytes, expected) in encoded.iter().zip(postings) {
        codec
            .decode(bytes, &mut docs, &mut tfs)
            .expect("decode should succeed");
        assert_eq!(docs.len(), expected.len(), "doc count mismatch");
        assert_eq!(tfs.len(), expected.len(), "tf count mismatch");
        for (index, (doc_id, tf)) in expected.iter().enumerate() {
            assert_eq!(docs[index], *doc_id, "doc mismatch at index {index}");
            assert_eq!(tfs[index], *tf, "tf mismatch at index {index}");
        }
    }
}

/// Measure compression ratio and emit a summary table.
fn report_compression_detailed(
    corpus_size: u32,
    total_postings: usize,
    codec_sizes: &[(&str, usize)],
) {
    // Baseline: 8 bytes per posting (u32 doc_id + u32 term_freq).
    let uncompressed_baseline = total_postings * 8;

    eprintln!(
        "\n=== Codec Compression Summary (corpus: {} docs, {} total postings) ===",
        corpus_size, total_postings
    );
    eprintln!("Baseline (uncompressed): 8 bytes per posting");
    eprintln!("  Total uncompressed: {} bytes", uncompressed_baseline);

    for (name, bytes) in codec_sizes {
        let ratio = if uncompressed_baseline > 0 {
            (*bytes as f64) / (uncompressed_baseline as f64) * 100.0
        } else {
            0.0
        };
        let avg_bytes_per_posting = if total_postings > 0 {
            *bytes as f64 / total_postings as f64
        } else {
            0.0
        };
        eprintln!(
            "  {}: {} bytes ({:.1}% of baseline, {:.2} bytes/posting)",
            name, bytes, ratio, avg_bytes_per_posting
        );
    }
}

fn bench_codec_encode_decode(c: &mut Criterion) {
    let generator = CorpusGenerator::new(SEED);

    // Prepare corpora and postings once outside the benchmark loop.
    let corpora = [
        ("1k", 1_000_u32, generator.generate(1_000)),
        ("10k", 10_000_u32, generator.generate(10_000)),
    ];

    let all_postings: Vec<CorpusCase> = corpora
        .iter()
        .map(|(label, doc_count, corpus)| CorpusCase {
            label,
            doc_count: *doc_count,
            postings: extract_postings(corpus),
        })
        .collect();

    // Report compression sizes upfront.
    for corpus in &all_postings {
        let total_postings = corpus.total_postings();
        let mut sizes = Vec::new();

        // Encode all postings with each codec and measure total size.
        let delta_varint_codec = DeltaVarintCodec;
        let block_delta_codec = BlockDeltaCodec;

        let mut dv_total_size = 0;
        let mut bd_total_size = 0;

        for postings in &corpus.postings {
            let dv_encoded = delta_varint_codec.encode(postings);
            dv_total_size += dv_encoded.len();

            let bd_encoded = block_delta_codec.encode(postings);
            bd_total_size += bd_encoded.len();
        }

        sizes.push(("DeltaVarint", dv_total_size));
        sizes.push(("BlockDelta", bd_total_size));

        report_compression_detailed(corpus.doc_count, total_postings, &sizes);
    }

    let mut group = c.benchmark_group("codec_encode");
    for corpus in &all_postings {
        let delta_varint_codec = DeltaVarintCodec;
        let block_delta_codec = BlockDeltaCodec;
        group.throughput(Throughput::Elements(corpus.total_postings() as u64));

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("deltavarint/{}", corpus.label)),
            &corpus.doc_count,
            |b, _| {
                b.iter(|| {
                    let mut total_bytes = 0;
                    for postings in &corpus.postings {
                        let encoded = delta_varint_codec.encode(postings);
                        total_bytes += encoded.len();
                    }
                    criterion::black_box(total_bytes);
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("blockdelta/{}", corpus.label)),
            &corpus.doc_count,
            |b, _| {
                b.iter(|| {
                    let mut total_bytes = 0;
                    for postings in &corpus.postings {
                        let encoded = block_delta_codec.encode(postings);
                        total_bytes += encoded.len();
                    }
                    criterion::black_box(total_bytes);
                });
            },
        );
    }
    group.finish();

    let mut group = c.benchmark_group("codec_decode");
    for corpus in &all_postings {
        let delta_varint_codec = DeltaVarintCodec;
        let block_delta_codec = BlockDeltaCodec;

        // Pre-encode for decode benchmarks.
        let dv_encoded: Vec<Vec<u8>> = corpus
            .postings
            .iter()
            .map(|postings| delta_varint_codec.encode(postings))
            .collect();

        let bd_encoded: Vec<Vec<u8>> = corpus
            .postings
            .iter()
            .map(|postings| block_delta_codec.encode(postings))
            .collect();

        validate_decodes(&delta_varint_codec, &dv_encoded, &corpus.postings);
        validate_decodes(&block_delta_codec, &bd_encoded, &corpus.postings);
        group.throughput(Throughput::Elements(corpus.total_postings() as u64));

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("deltavarint/{}", corpus.label)),
            &corpus.doc_count,
            |b, _| {
                let max_len = corpus.max_postings_len();
                let mut docs = Vec::with_capacity(max_len);
                let mut tfs = Vec::with_capacity(max_len);
                b.iter(|| {
                    let mut decoded_count = 0;
                    for encoded in &dv_encoded {
                        delta_varint_codec
                            .decode(encoded, &mut docs, &mut tfs)
                            .expect("decode should succeed");
                        decoded_count += docs.len();
                    }
                    criterion::black_box(decoded_count);
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("blockdelta/{}", corpus.label)),
            &corpus.doc_count,
            |b, _| {
                let max_len = corpus.max_postings_len();
                let mut docs = Vec::with_capacity(max_len);
                let mut tfs = Vec::with_capacity(max_len);
                b.iter(|| {
                    let mut decoded_count = 0;
                    for encoded in &bd_encoded {
                        block_delta_codec
                            .decode(encoded, &mut docs, &mut tfs)
                            .expect("decode should succeed");
                        decoded_count += docs.len();
                    }
                    criterion::black_box(decoded_count);
                });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_codec_encode_decode);
criterion_main!(benches);
