// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Statistics parity for the benchmark-only reference execution index.

#![cfg(feature = "bench-internals")]

use leit_core::FieldId;
use leit_index::{ExecutableIndex, InMemoryIndexBuilder, ReferenceExecutionIndex};
use leit_text::{Analyzer, FieldAnalyzers, UnicodeNormalizer, WhitespaceTokenizer};

const TITLE: FieldId = FieldId::new(1);
const BODY: FieldId = FieldId::new(2);

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

fn optimized_statistics(index: &impl ExecutableIndex) -> (u32, Vec<(u32, u32, u32)>) {
    let mut fields = Vec::new();
    for field in [TITLE, BODY] {
        if let Some(stats) = index.field_stats(field) {
            fields.push((stats.field_id.as_u32(), stats.doc_count, stats.total_terms));
        }
    }
    (index.document_count(), fields)
}

#[test]
fn reference_statistics_match_the_optimized_index() -> Result<(), leit_index::IndexError> {
    let aliases = [(TITLE, "title"), (BODY, "body")];
    let first = [(TITLE, "Rust systems"), (BODY, "safe fast rust")];
    let second = [(TITLE, "Search"), (BODY, "fast search")];
    let documents = [(11, first.as_slice()), (29, second.as_slice())];

    let mut builder = InMemoryIndexBuilder::new(make_analyzers());
    for &(field, alias) in &aliases {
        builder.register_field_alias(field, alias);
    }
    for &(doc_id, fields) in &documents {
        builder.index_document(doc_id, fields)?;
    }
    let optimized = builder.build_index();

    let reference =
        ReferenceExecutionIndex::from_documents(make_analyzers(), &aliases, &documents)?;
    assert_eq!(
        reference.statistics_snapshot(),
        optimized_statistics(&optimized)
    );
    Ok(())
}
