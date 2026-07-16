// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Contract coverage for block metadata rebuilt during prepared serialization.

use leit_core::{FieldId, SegmentLocalDocId, TermFreq};
use leit_index::{
    CodecId, ExecutableIndex, InMemoryIndex, InMemoryIndexBuilder, SegmentView,
    prepare_serialization,
};
use leit_postings::codec::{BlockDeltaCodec, Codec, DeltaVarintCodec};
use leit_query::TermDictionary;
use leit_text::{
    AnalysisSchemaId, Analyzer, FieldAnalyzers, UnicodeNormalizer, WhitespaceTokenizer,
};

fn fixture() -> InMemoryIndex {
    let field = FieldId::new(1);
    let mut analyzers = FieldAnalyzers::with_schema_id(
        AnalysisSchemaId::new(7_076).expect("fixture schema ID is nonzero"),
    );
    analyzers.set(
        field,
        Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
    );
    let mut builder = InMemoryIndexBuilder::new(analyzers);
    builder.register_field_alias(field, "body");

    for doc_id in 0..129 {
        let mut text = String::from("multi");
        if doc_id < 128 {
            text.push_str(" exact");
        }
        if doc_id == 0 || doc_id == 128 {
            text.push_str(" short");
        }
        if doc_id == 17 {
            text.push_str(" exact exact");
        }
        if doc_id == 128 {
            text.push_str(" multi multi multi");
        }
        builder
            .index_document(doc_id, &[(field, &text)])
            .expect("fixture document should index");
    }
    builder.build_index()
}

fn decode(codec_id: CodecId, payload: &[u8]) -> (Vec<SegmentLocalDocId>, Vec<TermFreq>) {
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
    (docs, tfs)
}

#[test]
fn prepared_sidecar_describes_real_codec_blocks_and_offsets() {
    let index = fixture();

    for codec_id in [CodecId::DeltaVarint, CodecId::BlockDelta] {
        let bytes = prepare_serialization(&index, codec_id)
            .expect("fixture should serialize")
            .into_bytes();
        let view = SegmentView::open(&bytes).expect("prepared segment should open");
        let lexicon = view.lexicon().expect("lexicon should open");
        let table = view.postings_table().expect("postings table should open");
        let data = view.postings_data().expect("postings data should open");
        let metadata = view.block_meta().expect("block metadata should open");
        let mut described_blocks = 0_u32;

        for term_index in 0..table.len() {
            let (term, _) = lexicon.entry(term_index).expect("term should decode");
            let term_text = core::str::from_utf8(term).expect("fixture term should be UTF-8");
            let term_id = index
                .resolve_term(FieldId::new(1), term_text)
                .expect("fixture term should resolve");
            let expected =
                ExecutableIndex::postings(&index, term_id).expect("fixture postings should exist");
            let (offset, len, doc_freq, _, first_block, block_count) =
                table.entry(term_index).expect("table entry should decode");
            let payload = data.range(offset, len).expect("term payload should exist");
            assert_eq!(doc_freq as usize, expected.len());

            let expected_block_count = match codec_id {
                CodecId::DeltaVarint => u32::from(!expected.is_empty()),
                CodecId::BlockDelta => u32::try_from(expected.len())
                    .expect("fixture postings count fits u32")
                    .div_ceil(128),
            };
            assert_eq!(block_count, expected_block_count, "term {term_text}");

            for block_index in 0..block_count {
                let (end_doc, max_tf, decode_offset) = metadata
                    .entry(first_block + block_index)
                    .expect("metadata entry should decode");
                let (start, end) = match codec_id {
                    CodecId::DeltaVarint => (0, len),
                    CodecId::BlockDelta => {
                        assert!(decode_offset > 0, "offset skips the term codec marker");
                        let next = if block_index + 1 == block_count {
                            len
                        } else {
                            metadata
                                .entry(first_block + block_index + 1)
                                .expect("next metadata entry should decode")
                                .2
                        };
                        (decode_offset, next)
                    }
                };
                assert_eq!(decode_offset, start, "term {term_text} block {block_index}");

                let independently_decodable = match codec_id {
                    CodecId::DeltaVarint => payload.to_vec(),
                    CodecId::BlockDelta => {
                        let mut block = vec![codec_id.to_u8()];
                        block.extend_from_slice(&payload[start as usize..end as usize]);
                        block
                    }
                };
                let (docs, tfs) = decode(codec_id, &independently_decodable);
                let expected_start = match codec_id {
                    CodecId::DeltaVarint => 0,
                    CodecId::BlockDelta => block_index as usize * 128,
                };
                let expected_end = match codec_id {
                    CodecId::DeltaVarint => expected.len(),
                    CodecId::BlockDelta => core::cmp::min(expected_start + 128, expected.len()),
                };
                let expected_block = &expected[expected_start..expected_end];
                assert_eq!(docs.len(), expected_block.len());
                assert_eq!(
                    end_doc,
                    expected_block.last().expect("block is nonempty").doc_id()
                );
                assert_eq!(
                    max_tf,
                    expected_block
                        .iter()
                        .map(|posting| posting.term_freq())
                        .max()
                        .expect("block is nonempty")
                );
                assert_eq!(
                    docs.iter().map(|doc| doc.get()).collect::<Vec<_>>(),
                    expected_block
                        .iter()
                        .map(|posting| posting.doc_id())
                        .collect::<Vec<_>>()
                );
                assert_eq!(
                    tfs.iter().map(|tf| tf.get()).collect::<Vec<_>>(),
                    expected_block
                        .iter()
                        .map(|posting| posting.term_freq())
                        .collect::<Vec<_>>()
                );
            }
            described_blocks += block_count;
        }

        assert_eq!(metadata.len(), described_blocks);
    }
}

#[test]
fn empty_index_has_no_block_metadata() {
    let index = InMemoryIndexBuilder::new(FieldAnalyzers::with_schema_id(
        AnalysisSchemaId::new(7_076).expect("fixture schema ID is nonzero"),
    ))
    .build_index();

    for codec_id in [CodecId::DeltaVarint, CodecId::BlockDelta] {
        let bytes = prepare_serialization(&index, codec_id)
            .expect("empty index should serialize")
            .into_bytes();
        let view = SegmentView::open(&bytes).expect("empty segment should open");
        assert!(
            view.block_meta()
                .expect("block metadata should open")
                .is_empty()
        );
    }
}
