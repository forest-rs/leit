// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Result parity for the benchmark-only reference execution index.

#![cfg(feature = "bench-internals")]

use leit_collect::TopKCollector;
use leit_core::{FieldId, FilterSlotId, QueryNodeId, ScoredHit};
use leit_index::{
    ExecutionWorkspace, FilterEvaluator, InMemoryIndex, InMemoryIndexBuilder, NoFilter,
    ReferenceExecutionIndex, SearchScorer,
};
use leit_query::{ExecutionPlan, FeatureSet, QueryNode, QueryProgram, TermDictionary};
use leit_text::{Analyzer, FieldAnalyzers, UnicodeNormalizer, WhitespaceTokenizer};

const TITLE: FieldId = FieldId::new(1);
const BODY: FieldId = FieldId::new(2);

fn make_analyzers() -> FieldAnalyzers {
    let mut analyzers = FieldAnalyzers::new();
    for field in [TITLE, BODY] {
        analyzers.set(
            field,
            Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
        );
    }
    analyzers
}

fn indexes() -> Result<(InMemoryIndex, ReferenceExecutionIndex), leit_index::IndexError> {
    let aliases = [(TITLE, "title"), (BODY, "body")];
    let d1 = [(TITLE, "Rust systems"), (BODY, "safe fast rust")];
    let d2 = [(TITLE, "Search rust"), (BODY, "fast search")];
    let d3 = [(TITLE, "Systems"), (BODY, "rust rust search")];
    let documents = [
        (11, d1.as_slice()),
        (29, d2.as_slice()),
        (47, d3.as_slice()),
    ];
    let mut builder = InMemoryIndexBuilder::new(make_analyzers());
    for &(field, name) in &aliases {
        builder.register_field_alias(field, name);
    }
    for &(doc_id, fields) in &documents {
        builder.index_document(doc_id, fields)?;
    }
    Ok((
        builder.build_index(),
        ReferenceExecutionIndex::from_documents(make_analyzers(), &aliases, &documents)?,
    ))
}

struct RejectDocument29;

impl FilterEvaluator<u32> for RejectDocument29 {
    fn evaluate(&self, _slot: FilterSlotId, id: &u32) -> bool {
        *id != 29
    }

    fn slots(&self) -> &[FilterSlotId] {
        const SLOTS: &[FilterSlotId] = &[FilterSlotId::new(3)];
        SLOTS
    }
}

fn assert_golden_score_bits(hits: &[ScoredHit<u32>], expected: &[(u32, u32)]) {
    let actual: Vec<_> = hits
        .iter()
        .map(|hit| (hit.id, hit.score.as_f32().to_bits()))
        .collect();
    assert_eq!(actual, expected, "fixture score bits changed");
}

#[test]
fn fielded_bm25_term_scores_match() -> Result<(), leit_index::IndexError> {
    let (optimized, reference) = indexes()?;
    let mut workspace = ExecutionWorkspace::new();
    let plan = workspace.plan(&optimized, "title:rust", &NoFilter)?;
    for limit in [1, 16] {
        let mut collector = TopKCollector::new(limit);
        workspace.execute(
            &optimized,
            &plan,
            Some(SearchScorer::bm25()),
            &NoFilter,
            &mut collector,
        )?;
        let expected = collector.finish();
        let actual = reference.execute_snapshot(&plan, SearchScorer::bm25(), &NoFilter, limit)?;
        assert_eq!(actual, expected);
        if limit == 16 {
            assert_golden_score_bits(&actual, &[(29, 0x3ede_712b), (11, 0x3ede_712b)]);
        }
    }
    Ok(())
}

#[test]
fn fielded_or_combines_term_scores() -> Result<(), leit_index::IndexError> {
    let (optimized, reference) = indexes()?;
    let mut workspace = ExecutionWorkspace::new();
    let plan = workspace.plan(&optimized, "title:rust OR body:search", &NoFilter)?;
    for limit in [1, 16] {
        let mut collector = TopKCollector::new(limit);
        workspace.execute(
            &optimized,
            &plan,
            Some(SearchScorer::bm25()),
            &NoFilter,
            &mut collector,
        )?;
        let actual = reference.execute_snapshot(&plan, SearchScorer::bm25(), &NoFilter, limit)?;
        assert_eq!(actual, collector.finish());
        if limit == 16 {
            assert_golden_score_bits(
                &actual,
                &[(29, 0x3f75_3fda), (47, 0x3ee4_ef59), (11, 0x3ede_712b)],
            );
        }
    }
    Ok(())
}

#[test]
fn fielded_term_rejects_document_29() -> Result<(), leit_index::IndexError> {
    let (optimized, reference) = indexes()?;
    let mut workspace = ExecutionWorkspace::new();
    let plan = workspace.plan(&optimized, "title:rust", &RejectDocument29)?;
    for limit in [1, 16] {
        let mut collector = TopKCollector::new(limit);
        workspace.execute(
            &optimized,
            &plan,
            Some(SearchScorer::bm25()),
            &RejectDocument29,
            &mut collector,
        )?;
        assert_eq!(
            reference.execute_snapshot(&plan, SearchScorer::bm25(), &RejectDocument29, limit)?,
            collector.finish()
        );
    }
    Ok(())
}

#[test]
fn unfielded_bm25f_combines_field_hits() -> Result<(), leit_index::IndexError> {
    let (optimized, reference) = indexes()?;
    let mut workspace = ExecutionWorkspace::new();
    let plan = workspace.plan(&optimized, "rust", &NoFilter)?;
    for limit in [1, 16] {
        let mut collector = TopKCollector::new(limit);
        workspace.execute(
            &optimized,
            &plan,
            Some(SearchScorer::bm25f()),
            &NoFilter,
            &mut collector,
        )?;
        let actual = reference.execute_snapshot(&plan, SearchScorer::bm25f(), &NoFilter, limit)?;
        assert_eq!(actual, collector.finish());
        if limit == 16 {
            assert_golden_score_bits(
                &actual,
                &[(47, 0x3e40_2b77), (11, 0x3e34_36eb), (29, 0x3e0d_2dcb)],
            );
        }
    }
    Ok(())
}

#[test]
fn fielded_bm25f_term_scores_match() -> Result<(), leit_index::IndexError> {
    let (optimized, reference) = indexes()?;
    let mut workspace = ExecutionWorkspace::new();
    let plan = workspace.plan(&optimized, "title:rust", &NoFilter)?;
    for limit in [1, 16] {
        let mut collector = TopKCollector::new(limit);
        workspace.execute(
            &optimized,
            &plan,
            Some(SearchScorer::bm25f()),
            &NoFilter,
            &mut collector,
        )?;
        assert_eq!(
            reference.execute_snapshot(&plan, SearchScorer::bm25f(), &NoFilter, limit)?,
            collector.finish()
        );
    }
    Ok(())
}

#[test]
fn fielded_bm25f_or_combines_term_scores() -> Result<(), leit_index::IndexError> {
    let (optimized, reference) = indexes()?;
    let mut workspace = ExecutionWorkspace::new();
    let plan = workspace.plan(&optimized, "title:rust OR body:search", &NoFilter)?;
    for limit in [1, 16] {
        let mut collector = TopKCollector::new(limit);
        workspace.execute(
            &optimized,
            &plan,
            Some(SearchScorer::bm25f()),
            &NoFilter,
            &mut collector,
        )?;
        assert_eq!(
            reference.execute_snapshot(&plan, SearchScorer::bm25f(), &NoFilter, limit)?,
            collector.finish()
        );
    }
    Ok(())
}

#[test]
fn constant_score_overrides_term_scores() -> Result<(), leit_index::IndexError> {
    let (optimized, reference) = indexes()?;
    let term = optimized.resolve_term(TITLE, "rust").expect("fixture term");
    let plan = ExecutionPlan {
        program: QueryProgram::new(
            vec![
                QueryNode::Term {
                    field: TITLE,
                    term,
                    boost: 1.0,
                },
                QueryNode::ConstantScore {
                    child: QueryNodeId::new(0),
                    score: 4.25,
                },
            ],
            QueryNodeId::new(1),
            2,
        ),
        selectivity: 1.0,
        cost: 2,
        required_features: FeatureSet::basic(),
    };
    let mut workspace = ExecutionWorkspace::new();
    for limit in [1, 16] {
        let mut collector = TopKCollector::new(limit);
        workspace.execute(
            &optimized,
            &plan,
            Some(SearchScorer::bm25()),
            &NoFilter,
            &mut collector,
        )?;
        let actual = reference.execute_snapshot(&plan, SearchScorer::bm25(), &NoFilter, limit)?;
        assert_eq!(actual, collector.finish());
        if limit == 16 {
            assert_golden_score_bits(&actual, &[(29, 0x4088_0000), (11, 0x4088_0000)]);
        }
    }
    Ok(())
}

#[test]
fn conjunction_excludes_negated_matches() -> Result<(), leit_index::IndexError> {
    let (optimized, reference) = indexes()?;
    let mut workspace = ExecutionWorkspace::new();
    let plan = workspace.plan(&optimized, "title:rust AND NOT body:search", &NoFilter)?;
    for limit in [1, 16] {
        let mut collector = TopKCollector::new(limit);
        workspace.execute(
            &optimized,
            &plan,
            Some(SearchScorer::bm25()),
            &NoFilter,
            &mut collector,
        )?;
        assert_eq!(
            reference.execute_snapshot(&plan, SearchScorer::bm25(), &NoFilter, limit)?,
            collector.finish()
        );
    }
    Ok(())
}
