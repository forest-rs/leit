// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Contract tests for exact, codec-selected prepared segment serialization.

use leit_core::{FieldId, SegmentLocalDocId, TermFreq, TermId};
use leit_index::{
    CodecId, ExecutableIndex, InMemoryIndexBuilder, MergedIndex, SegmentView, prepare_merge,
    prepare_serialization,
};
use leit_postings::codec::{BlockDeltaCodec, Codec, DeltaVarintCodec};
use leit_text::{
    AnalysisSchemaId, Analyzer, FieldAnalyzers, UnicodeNormalizer, WhitespaceTokenizer,
};

fn analyzers() -> FieldAnalyzers {
    let mut analyzers = FieldAnalyzers::with_schema_id(
        AnalysisSchemaId::new(7_075).expect("fixture schema ID is nonzero"),
    );
    analyzers.set(
        FieldId::new(1),
        Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
    );
    analyzers
}

fn source(source_ordinal: usize) -> leit_index::InMemoryIndex {
    let mut builder = InMemoryIndexBuilder::new(analyzers());
    builder.register_field_alias(FieldId::new(1), "body");
    for doc_id in 0..80 {
        let text = if source_ordinal == 0 && doc_id == 0 {
            "common rare"
        } else {
            "common"
        };
        builder
            .index_document(doc_id, &[(FieldId::new(1), text)])
            .expect("fixture should index");
    }
    builder.build_index()
}

fn merged_index() -> MergedIndex {
    prepare_merge(vec![source(0), source(1)], analyzers())
        .expect("matching explicit schemas should prepare")
        .execute()
}

fn expected_postings(
    merged: &MergedIndex,
    term_idx: u32,
    term_text: &[u8],
) -> Vec<(SegmentLocalDocId, TermFreq)> {
    let term_id = TermId::new(term_idx);
    let entry = merged
        .index()
        .term_entry(term_id)
        .expect("serialized term must exist in merged lexicon");
    assert_eq!(
        entry.term_text.as_bytes(),
        term_text,
        "serialized lexicon term must match merged term metadata"
    );
    merged
        .index()
        .postings(term_id)
        .expect("serialized term must have merged postings")
        .iter()
        .map(|posting| {
            (
                SegmentLocalDocId::new(posting.doc_id()),
                TermFreq::new(posting.term_freq()),
            )
        })
        .collect()
}

#[test]
fn prepared_codec_matrix_round_trips_exact_postings() {
    let merged = merged_index();
    let index = merged.index();
    let mut saw_short_term = false;
    let mut saw_multi_block_term = false;

    for (codec_id, table_kind) in [(CodecId::DeltaVarint, 1), (CodecId::BlockDelta, 2)] {
        let bytes = prepare_serialization(index, codec_id)
            .expect("serialization should prepare")
            .into_bytes();
        let view = SegmentView::open(&bytes).expect("prepared bytes should open");
        let table = view.postings_table().expect("postings table should open");
        let data = view.postings_data().expect("postings data should open");
        let lexicon = view.lexicon().expect("lexicon should open");

        assert_eq!(table.len(), 2);
        for term_idx in 0..table.len() {
            let (offset, len, doc_freq, kind, _, _) =
                table.entry(term_idx).expect("table entry should decode");
            assert_eq!(kind, table_kind);
            let payload = data.range(offset, len).expect("payload range should exist");
            assert_eq!(payload[0], codec_id.to_u8());

            let mut docs = Vec::new();
            let mut tfs = Vec::new();
            match codec_id {
                CodecId::DeltaVarint => DeltaVarintCodec
                    .decode(payload, &mut docs, &mut tfs)
                    .expect("delta payload should decode"),
                CodecId::BlockDelta => BlockDeltaCodec
                    .decode(payload, &mut docs, &mut tfs)
                    .expect("block payload should decode"),
            }
            assert_eq!(docs.len(), doc_freq as usize);
            assert_eq!(docs.len(), tfs.len());
            let (term, postings_table_index) = lexicon
                .entry(term_idx)
                .expect("lexicon entry should decode");
            assert_eq!(postings_table_index, term_idx);
            let actual: Vec<_> = docs.into_iter().zip(tfs).collect();
            let expected = expected_postings(&merged, term_idx, term);
            saw_short_term |= expected.len() < 128;
            saw_multi_block_term |= expected.len() > 128;
            assert_eq!(actual, expected);
        }
    }

    assert!(
        saw_short_term,
        "fixture must exercise a short postings list"
    );
    assert!(
        saw_multi_block_term,
        "fixture must exercise a multi-block postings list"
    );
    assert_eq!(
        index.analysis_schema_id().map(AnalysisSchemaId::get),
        Some(7_075)
    );
}

#[test]
fn legacy_writer_remains_kind_zero_raw_without_codec_marker() {
    let merged = merged_index();
    let bytes = merged
        .index()
        .to_segment_bytes()
        .expect("legacy serialization should work");
    let view = SegmentView::open(&bytes).expect("legacy bytes should open");
    let table = view.postings_table().expect("postings table should open");
    let data = view.postings_data().expect("postings data should open");
    let lexicon = view.lexicon().expect("lexicon should open");

    for term_idx in 0..table.len() {
        let (offset, len, doc_freq, kind, _, _) =
            table.entry(term_idx).expect("table entry should decode");
        assert_eq!(kind, 0);
        assert_eq!(len, doc_freq * 8);
        let payload = data.range(offset, len).expect("payload range should exist");
        let actual: Vec<_> = payload
            .chunks_exact(8)
            .map(|raw| {
                let doc = u32::from_le_bytes(raw[0..4].try_into().expect("doc bytes"));
                let tf = u32::from_le_bytes(raw[4..8].try_into().expect("tf bytes"));
                (SegmentLocalDocId::new(doc), TermFreq::new(tf))
            })
            .collect();
        let (term, _) = lexicon
            .entry(term_idx)
            .expect("lexicon entry should decode");
        assert_eq!(actual, expected_postings(&merged, term_idx, term));
    }
}
