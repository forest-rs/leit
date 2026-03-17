// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Segment round-trip and validation tests for `leit-index`.

use leit_core::FieldId;
use leit_index::{InMemoryIndexBuilder, SectionKind, SegmentError, SegmentView};
use leit_text::{Analyzer, FieldAnalyzers, UnicodeNormalizer, WhitespaceTokenizer};
use proptest::prelude::*;

fn test_analyzers() -> FieldAnalyzers {
    let mut analyzers = FieldAnalyzers::new();
    let analyzer =
        Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new());
    analyzers.set(FieldId::new(1), analyzer);
    analyzers
}

#[test]
fn roundtrip_segment_view_preserves_basic_counts() {
    let mut builder = InMemoryIndexBuilder::new(test_analyzers());
    builder
        .index_document(1, &[(FieldId::new(1), "Rust Retrieval")])
        .expect("document 1 should index");
    builder
        .index_document(2, &[(FieldId::new(1), "Rust Systems")])
        .expect("document 2 should index");
    let index = builder.build_index();

    let bytes = index
        .to_segment_bytes()
        .expect("segment export should work");
    let view = SegmentView::open(&bytes).expect("segment should reopen");

    assert_eq!(view.document_count(), 2);
    assert_eq!(view.field_count(), 1);
    assert!(view.term_count() >= 3);
    assert!(view.has_section(SectionKind::TermDictionary));
    assert!(view.has_section(SectionKind::FieldMetadata));
    assert!(view.has_section(SectionKind::PostingsMetadata));
    assert!(view.has_section(SectionKind::PostingsPayload));
}

#[test]
fn segment_view_rejects_invalid_magic() {
    let err = SegmentView::open(&[0_u8; 24]).expect_err("bad magic should fail");
    assert!(matches!(err, SegmentError::InvalidMagic));
}

#[test]
fn segment_view_rejects_truncated_header() {
    let err = SegmentView::open(b"short").expect_err("short header should fail");
    assert!(matches!(err, SegmentError::TruncatedHeader));
}

#[test]
fn segment_view_rejects_sections_inside_header_or_directory() {
    let mut builder = InMemoryIndexBuilder::new(test_analyzers());
    builder
        .index_document(1, &[(FieldId::new(1), "Rust Retrieval")])
        .expect("document should index");
    let index = builder.build_index();

    let mut bytes = index
        .to_segment_bytes()
        .expect("segment export should work");
    bytes[28..32].copy_from_slice(&0_u32.to_le_bytes());

    let err = SegmentView::open(&bytes).expect_err("section offset inside header must fail");
    assert!(matches!(
        err,
        SegmentError::OutOfBoundsSection(SectionKind::TermDictionary)
    ));
}

#[test]
fn builder_failure_does_not_poison_document_id() {
    let mut builder = InMemoryIndexBuilder::new(test_analyzers());

    let err = builder
        .index_document(7, &[(FieldId::new(2), "missing analyzer")])
        .expect_err("unknown field analyzer should fail");
    assert!(
        matches!(err, leit_index::IndexError::MissingAnalyzer(field) if field == FieldId::new(2))
    );

    builder
        .index_document(7, &[(FieldId::new(1), "retry succeeds")])
        .expect("retrying the same document id after a failed add should work");
}

proptest! {
    #[test]
    fn segment_view_rejects_corrupted_directory_offsets(offset in 0_u32..72_u32) {
        let mut builder = InMemoryIndexBuilder::new(test_analyzers());
        builder
            .index_document(1, &[(FieldId::new(1), "Rust Retrieval")])
            .expect("document should index");
        let index = builder.build_index();

        let mut bytes = index.to_segment_bytes().expect("segment export should work");
        bytes[28..32].copy_from_slice(&offset.to_le_bytes());

        let result = SegmentView::open(&bytes);
        prop_assert!(matches!(
            result,
            Err(SegmentError::OutOfBoundsSection(SectionKind::TermDictionary))
        ));
    }
}
