// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Behavioral guards for reusing query-execution state.

use leit_collect::TopKCollector;
use leit_core::{FieldId, QueryNodeId, Score};
use leit_index::{
    ExecutionStats, ExecutionWorkspace, InMemoryIndexBuilder, NoFilter, SearchScorer,
};
#[cfg(feature = "bench-internals")]
use leit_postings::codec::{BlockDeltaCodec, Codec, CodecId, DeltaVarintCodec};
#[cfg(feature = "bench-internals")]
use leit_postings::cursor::{CursorStatus, DocCursor, PostingsView, TfCursor};
use leit_query::{ExecutionPlan, FeatureSet, QueryNode, QueryProgram, TermDictionary};
use leit_score::{Bm25Scorer, ScoringStats};
use leit_text::{Analyzer, FieldAnalyzers, UnicodeNormalizer, WhitespaceTokenizer};

fn analyzers() -> FieldAnalyzers {
    let mut analyzers = FieldAnalyzers::new();
    analyzers.set(
        FieldId::new(1),
        Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
    );
    analyzers
}

#[test]
fn shared_child_preserves_visit_stats() {
    let mut builder = InMemoryIndexBuilder::new(analyzers());
    for (id, text) in [(1, "alpha"), (2, "alpha beta"), (3, "beta")] {
        builder
            .index_document(id, &[(FieldId::new(1), text)])
            .expect("fixture document should index");
    }
    let index = builder.build_index();
    let alpha = index
        .resolve_term(FieldId::new(1), "alpha")
        .expect("alpha should resolve");
    let beta = index
        .resolve_term(FieldId::new(1), "beta")
        .expect("beta should resolve");

    // Node 0 is intentionally shared: the root OR visits it directly, while
    // its AND child visits the same occurrence again.
    let program = QueryProgram::new(
        vec![
            QueryNode::Term {
                field: FieldId::new(1),
                term: alpha,
                boost: 1.0,
            },
            QueryNode::Term {
                field: FieldId::new(1),
                term: beta,
                boost: 1.0,
            },
            QueryNode::And {
                children: vec![QueryNodeId::new(0), QueryNodeId::new(1)],
                boost: 1.0,
            },
            QueryNode::Or {
                children: vec![QueryNodeId::new(0), QueryNodeId::new(2)],
                boost: 1.0,
            },
        ],
        QueryNodeId::new(3),
        3,
    );
    let plan = ExecutionPlan {
        program,
        selectivity: 1.0,
        cost: 1,
        required_features: FeatureSet::basic(),
    };

    let mut workspace = ExecutionWorkspace::new();
    let mut collector = TopKCollector::new(10);
    workspace
        .execute(
            &index,
            &plan,
            Some(SearchScorer::bm25()),
            &NoFilter,
            &mut collector,
        )
        .expect("shared-child plan should execute");
    let hits = collector.finish();

    let scorer = Bm25Scorer::new();
    let score = |doc_length| {
        scorer.score(&ScoringStats {
            term_frequency: 1,
            doc_length,
            avg_doc_length: 4.0 / 3.0,
            doc_count: 3,
            doc_frequency: 2,
            ..ScoringStats::new()
        })
    };
    let doc_one_score = score(1);
    let doc_two_term_score = score(2);
    let doc_two_score = doc_two_term_score + doc_two_term_score + doc_two_term_score;

    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].id, 2);
    assert_eq!(
        hits[0].score.as_f32().to_bits(),
        doc_two_score.as_f32().to_bits()
    );
    assert_eq!(hits[1].id, 1);
    assert_eq!(
        hits[1].score.as_f32().to_bits(),
        doc_one_score.as_f32().to_bits()
    );
    assert_eq!(
        workspace.last_stats(),
        ExecutionStats {
            scored_postings: 6,
            skipped_blocks: 0,
            collected_hits: 2,
        }
    );
    assert!(hits.iter().all(|hit| hit.score > Score::ZERO));
}

#[cfg(feature = "bench-internals")]
fn encoded_postings(count: u32) -> Vec<(leit_core::SegmentLocalDocId, leit_core::TermFreq)> {
    (0..count)
        .map(|index| {
            (
                leit_core::SegmentLocalDocId::new(index * 3 + 1),
                leit_core::TermFreq::new(index % 7 + 1),
            )
        })
        .collect()
}

#[cfg(feature = "bench-internals")]
fn traverse_prepared(
    workspace: &mut ExecutionWorkspace,
    encoded: &[u8],
) -> Vec<(leit_core::SegmentLocalDocId, leit_core::TermFreq)> {
    let mut cursor = workspace
        .decode_prepared_postings(PostingsView::new(encoded, &[]))
        .expect("prepared postings should decode");
    let mut decoded = Vec::new();
    while let Some(document) = cursor.current_doc() {
        decoded.push((
            leit_core::SegmentLocalDocId::new(document),
            leit_core::TermFreq::new(cursor.current_tf()),
        ));
        if cursor.advance() == CursorStatus::Exhausted {
            break;
        }
    }
    decoded
}

#[test]
#[cfg(feature = "bench-internals")]
fn compressed_decode_reuses_workspace_scratch() {
    let fitting = encoded_postings(48);

    for (label, codec_id) in [
        ("delta-varint", CodecId::DeltaVarint),
        ("block-delta", CodecId::BlockDelta),
    ] {
        let fitting_bytes = match codec_id {
            CodecId::DeltaVarint => DeltaVarintCodec.encode(&fitting),
            CodecId::BlockDelta => BlockDeltaCodec.encode(&fitting),
        };
        let mut workspace = ExecutionWorkspace::new();

        assert_eq!(
            traverse_prepared(&mut workspace, &fitting_bytes),
            fitting,
            "{label}"
        );
        let warmed = workspace.benchmark_decode_capacities();
        let larger_count = warmed
            .documents
            .max(warmed.term_frequencies)
            .checked_add(129)
            .expect("deterministic fixture size should fit usize");
        let larger = encoded_postings(
            u32::try_from(larger_count).expect("deterministic fixture size should fit u32"),
        );
        let larger_bytes = match codec_id {
            CodecId::DeltaVarint => DeltaVarintCodec.encode(&larger),
            CodecId::BlockDelta => BlockDeltaCodec.encode(&larger),
        };
        assert!(warmed.documents < larger.len(), "{label}");
        assert!(warmed.term_frequencies < larger.len(), "{label}");
        assert_eq!(
            traverse_prepared(&mut workspace, &fitting_bytes),
            fitting,
            "{label}"
        );
        assert_eq!(workspace.benchmark_decode_capacities(), warmed, "{label}");

        assert_eq!(
            traverse_prepared(&mut workspace, &larger_bytes),
            larger,
            "{label}"
        );
        let grown = workspace.benchmark_decode_capacities();
        assert!(grown.documents >= larger.len(), "{label}");
        assert!(grown.term_frequencies >= larger.len(), "{label}");
        assert!(grown.documents > warmed.documents, "{label}");
        assert!(grown.term_frequencies > warmed.term_frequencies, "{label}");

        assert_eq!(
            traverse_prepared(&mut workspace, &fitting_bytes),
            fitting,
            "{label}"
        );
        assert_eq!(workspace.benchmark_decode_capacities(), grown, "{label}");
    }
}
