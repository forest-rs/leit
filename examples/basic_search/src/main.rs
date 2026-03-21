// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Minimal end-to-end example for the Leit stack.

use leit_core::{FieldId, ScoredHit};
use leit_index::{ExecutionWorkspace, InMemoryIndexBuilder, SearchScorer};
use leit_text::{Analyzer, CaseMapping, FieldAnalyzers, UnicodeNormalizer, WhitespaceTokenizer};

const TITLE: FieldId = FieldId::new(1);
const BODY: FieldId = FieldId::new(2);

fn main() -> Result<(), Box<dyn core::error::Error>> {
    // Configure title and body separately so the example can show both the
    // default Unicode normalization path and opt-in case folding.
    let mut analyzers = FieldAnalyzers::new();
    analyzers.set(
        TITLE,
        Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
    );
    analyzers.set(
        BODY,
        Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(
            UnicodeNormalizer::builder()
                .case_mapping(CaseMapping::Fold)
                .build(),
        ),
    );

    let mut builder = InMemoryIndexBuilder::new(analyzers);
    builder.register_field_alias(TITLE, "title");
    builder.register_field_alias(BODY, "body");

    builder.index_document(
        1,
        &[
            (TITLE, "Rust Retrieval"),
            (BODY, "Rust retrieval systems use inverted indices"),
        ],
    )?;
    builder.index_document(
        2,
        &[
            (TITLE, "Memory Safety"),
            (BODY, "Rust memory safety relies on ownership"),
        ],
    )?;
    builder.index_document(
        3,
        &[
            (TITLE, "Search Engines"),
            (BODY, "Search engines rank retrieval results with bm25"),
        ],
    )?;
    builder.index_document(
        4,
        &[
            (TITLE, "CAFE\u{301} Notes"),
            (BODY, "Canonical Unicode forms should still match"),
        ],
    )?;
    builder.index_document(
        5,
        &[
            (TITLE, "Normalization Notes"),
            (
                BODY,
                "STRASSE systems benefit from Unicode-aware normalization",
            ),
        ],
    )?;

    let index = builder.build_index();

    let mut workspace = ExecutionWorkspace::new();
    // Ordinary lexical query over a mix of explicit and bare terms.
    print_hits(
        "title:rust OR retrieval",
        workspace.search(&index, "title:rust OR retrieval", 5, SearchScorer::bm25())?,
    );
    // The indexed title uses a decomposed uppercase spelling (`CAFE\u{301}`),
    // while the query uses the composed lowercase form.
    print_hits(
        "café",
        workspace.search(&index, "café", 5, SearchScorer::bm25())?,
    );
    // The indexed body contains uppercase `STRASSE`, while the query uses
    // lowercase `straße`. This field opts into full Unicode case folding.
    print_hits(
        "straße",
        workspace.search(&index, "straße", 5, SearchScorer::bm25())?,
    );

    Ok(())
}

fn print_hits(query: &str, hits: Vec<ScoredHit<u32>>) {
    println!("query: {query}");
    println!("hits:");
    for (rank, hit) in hits.iter().enumerate() {
        println!(
            "  {}. doc={} score={:.4}",
            rank + 1,
            hit.id,
            hit.score.as_f32(),
        );
    }
    println!();
}
