// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Full-mode semantic validation for the v1 postings encoding discriminator.

use leit_core::FieldId;
use leit_index::{
    CodecId, InMemoryIndex, InMemoryIndexBuilder, SegmentError, SegmentView, ValidationMode,
    prepare_serialization,
};
use leit_text::{Analyzer, FieldAnalyzers, WhitespaceTokenizer};

const POSTINGS_TABLE_OFFSET_HEADER_FIELD: usize = 32;
const FOOTER_OFFSET_HEADER_FIELD: usize = 72;
const FIRST_TABLE_PAYLOAD_OFFSET: usize = 4;
const FIRST_TABLE_KIND_OFFSET: usize = 4 + 16;

fn fixture() -> InMemoryIndex {
    let mut analyzers = FieldAnalyzers::new();
    analyzers.set(FieldId::new(1), Analyzer::new(WhitespaceTokenizer::new()));
    let mut builder = InMemoryIndexBuilder::new(analyzers);
    builder
        .index_document(7, &[(FieldId::new(1), "alpha")])
        .expect("fixture should index");
    builder.build_index()
}

fn read_u64(bytes: &[u8], offset: usize) -> usize {
    usize::try_from(u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("header field should be complete"),
    ))
    .expect("fixture offset should fit usize")
}

fn crc32c(data: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0x82F6_3B78
            } else {
                crc >> 1
            };
        }
    }
    crc ^ u32::MAX
}

fn rewrite_checksum(bytes: &mut [u8]) {
    let footer_offset = read_u64(bytes, FOOTER_OFFSET_HEADER_FIELD);
    let checksum = crc32c(&bytes[..footer_offset]);
    bytes[footer_offset..footer_offset + 4].copy_from_slice(&checksum.to_le_bytes());
}

fn replace_first_kind(bytes: &mut [u8], kind: u32) {
    let table_offset = read_u64(bytes, POSTINGS_TABLE_OFFSET_HEADER_FIELD);
    let kind_offset = table_offset + FIRST_TABLE_KIND_OFFSET;
    bytes[kind_offset..kind_offset + 4].copy_from_slice(&kind.to_le_bytes());
    rewrite_checksum(bytes);
}

fn replace_first_payload_offset(bytes: &mut [u8], payload_offset: u64) {
    let table_offset = read_u64(bytes, POSTINGS_TABLE_OFFSET_HEADER_FIELD);
    let payload_offset_field = table_offset + FIRST_TABLE_PAYLOAD_OFFSET;
    bytes[payload_offset_field..payload_offset_field + 8]
        .copy_from_slice(&payload_offset.to_le_bytes());
    rewrite_checksum(bytes);
}

fn assert_cheap_modes_accept(bytes: &[u8]) {
    SegmentView::open_with_validation(bytes, ValidationMode::HeaderOnly)
        .expect("HeaderOnly must not perform postings semantic validation");
    SegmentView::open_with_validation(bytes, ValidationMode::Structural)
        .expect("Structural must not perform postings semantic validation");
}

#[test]
fn full_accepts_valid_legacy_delta_and_block_encodings() {
    let index = fixture();
    let legacy = index
        .to_segment_bytes()
        .expect("legacy raw serialization should succeed");
    let delta = prepare_serialization(&index, CodecId::DeltaVarint)
        .expect("delta serialization should prepare")
        .into_bytes();
    let block = prepare_serialization(&index, CodecId::BlockDelta)
        .expect("block serialization should prepare")
        .into_bytes();

    for (bytes, expected_kind, expected_marker) in [
        (&legacy, 0, None),
        (&delta, 1, Some(0)),
        (&block, 2, Some(1)),
    ] {
        assert_cheap_modes_accept(bytes);
        let view = SegmentView::open_with_validation(bytes, ValidationMode::Full)
            .expect("valid encoding should pass Full validation");
        let (payload_offset, payload_len, _, kind, _, _) = view
            .postings_table()
            .expect("table should open")
            .entry(0)
            .expect("fixture term should exist");
        assert_eq!(kind, expected_kind);
        let payload = view
            .postings_data()
            .expect("postings data should open")
            .range(payload_offset, payload_len)
            .expect("payload should be in bounds");
        assert_eq!(expected_marker, (kind != 0).then(|| payload[0]));
    }
}

#[test]
fn full_rejects_unknown_kind_after_checksum_is_recomputed() {
    let index = fixture();
    let mut bytes = prepare_serialization(&index, CodecId::DeltaVarint)
        .expect("delta serialization should prepare")
        .into_bytes();
    replace_first_kind(&mut bytes, 99);

    assert_cheap_modes_accept(&bytes);
    assert!(matches!(
        SegmentView::open_with_validation(&bytes, ValidationMode::Full),
        Err(SegmentError::UnknownPostingsEncoding {
            postings_index: 0,
            encoding_kind: 99,
        })
    ));
}

#[test]
fn full_rejects_kind_marker_mismatches_after_checksum_is_recomputed() {
    let index = fixture();
    for (codec, replacement_kind, expected_marker, found_marker) in [
        (CodecId::DeltaVarint, 2, 1, 0),
        (CodecId::BlockDelta, 1, 0, 1),
    ] {
        let mut bytes = prepare_serialization(&index, codec)
            .expect("serialization should prepare")
            .into_bytes();
        replace_first_kind(&mut bytes, replacement_kind);

        assert_cheap_modes_accept(&bytes);
        assert!(matches!(
            SegmentView::open_with_validation(&bytes, ValidationMode::Full),
            Err(SegmentError::PostingsEncodingMarkerMismatch {
                postings_index: 0,
                encoding_kind,
                expected,
                found: Some(found),
            }) if encoding_kind == replacement_kind
                && expected == expected_marker
                && found == found_marker
        ));
    }
}

#[test]
fn full_rejects_legacy_raw_payload_out_of_range_after_checksum_is_recomputed() {
    let index = fixture();
    let mut bytes = index
        .to_segment_bytes()
        .expect("legacy raw serialization should succeed");
    replace_first_payload_offset(&mut bytes, u64::MAX);

    assert_cheap_modes_accept(&bytes);
    assert!(matches!(
        SegmentView::open_with_validation(&bytes, ValidationMode::Full),
        Err(SegmentError::BadOffset {
            offset: u64::MAX,
            ..
        })
    ));
}
