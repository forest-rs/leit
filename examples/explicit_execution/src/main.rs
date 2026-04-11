// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Explicit planning and execution example for the Leit stack.

use leit_collect::{CountCollector, TopKCollector, collectors};
use leit_core::{FieldId, ScoredHit};
use leit_index::{
    ExecutionStats, ExecutionWorkspace, InMemoryIndexBuilder, NoFilter, SearchScorer,
};
use leit_text::{Analyzer, FieldAnalyzers, UnicodeNormalizer, WhitespaceTokenizer};

const TITLE: FieldId = FieldId::new(1);
const BODY: FieldId = FieldId::new(2);

fn main() -> Result<(), Box<dyn core::error::Error>> {
    let index = build_index()?;
    let query = "title:rust OR retrieval";

    let mut workspace = ExecutionWorkspace::new();

    // Planning is a separate step, so callers can inspect or reuse the query
    // program before deciding how to execute it.
    let plan = workspace.plan(&index, query, &NoFilter)?;
    println!("query: {query}");
    println!("plan:");
    println!("  nodes: {}", plan.program.node_count());
    println!("  max_depth: {}", plan.program.max_depth());
    println!("  root: {}", plan.program.root().as_u32());
    println!("  cost: {}", plan.cost);
    println!("  selectivity: {:.3}", plan.selectivity);
    println!("  required_features: {:?}", plan.required_features);
    println!();

    // One planned query can feed multiple collectors in one execution.
    let mut top_k = TopKCollector::new(2);
    let mut count = CountCollector::new();
    let mut collectors = collectors([&mut top_k, &mut count]);
    workspace.execute(
        &index,
        &plan,
        Some(SearchScorer::bm25()),
        &NoFilter,
        &mut collectors,
    )?;
    let hits = top_k.finish();
    let count = count.finish();
    println!("top-k + count collectors:");
    print_hits(&hits);
    println!("  matches: {count}");
    print_stats(workspace.last_stats());

    // Reuse the plan when only the total count matters and avoid scoring entirely.
    let mut count = CountCollector::new();
    workspace.execute(&index, &plan, None, &NoFilter, &mut count)?;
    let count = count.finish();
    println!("count-only execution:");
    println!("  matches: {count}");
    print_stats(workspace.last_stats());

    Ok(())
}

fn build_index() -> Result<leit_index::InMemoryIndex, leit_index::IndexError> {
    // Use the same per-field analyzers the planner will apply again at query
    // time so indexing and lookup stay aligned.
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

    Ok(builder.build_index())
}

fn print_hits(hits: &[ScoredHit<u32>]) {
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

fn print_stats(stats: ExecutionStats) {
    println!("  stats:");
    println!("    scored_postings: {}", stats.scored_postings);
    println!("    skipped_blocks: {}", stats.skipped_blocks);
    println!("    collected_hits: {}", stats.collected_hits);
    println!();
}
