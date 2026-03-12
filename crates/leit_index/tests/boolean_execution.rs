//! Property-based boolean execution tests for `leit-index`.

use std::collections::BTreeSet;

use leit_core::FieldId;
use leit_index::{ExecutionWorkspace, InMemoryIndex, InMemoryIndexBuilder, SearchScorer};
use leit_text::{Analyzer, FieldAnalyzers, UnicodeNormalizer, WhitespaceTokenizer};
use proptest::collection::vec;
use proptest::prelude::*;

fn test_analyzers() -> FieldAnalyzers {
    let mut analyzers = FieldAnalyzers::new();
    let analyzer =
        Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new());
    analyzers.set(FieldId::new(1), analyzer);
    analyzers
}

fn multi_field_analyzers() -> FieldAnalyzers {
    let mut analyzers = FieldAnalyzers::new();
    let analyzer =
        Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new());
    analyzers.set(FieldId::new(1), analyzer);
    let analyzer =
        Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new());
    analyzers.set(FieldId::new(2), analyzer);
    analyzers
}

fn doc_id_from_index(index: usize) -> u32 {
    u32::try_from(index.checked_add(1).expect("document index overflow"))
        .expect("test corpus should fit within u32 document IDs")
}

fn build_index(corpus: &[(bool, bool, bool)]) -> InMemoryIndex {
    let mut builder = InMemoryIndexBuilder::new(test_analyzers());
    for (offset, (alpha, beta, gamma)) in corpus.iter().copied().enumerate() {
        let mut terms = Vec::new();
        if alpha {
            terms.push("alpha");
        }
        if beta {
            terms.push("beta");
        }
        if gamma {
            terms.push("gamma");
        }
        let text = terms.join(" ");
        builder
            .index_document(
                doc_id_from_index(offset),
                &[(FieldId::new(1), text.as_str())],
            )
            .expect("document should index");
    }
    builder.build_index()
}

fn result_ids(index: &InMemoryIndex, query: &str, limit: usize) -> BTreeSet<u32> {
    let mut workspace = ExecutionWorkspace::new();
    workspace
        .search(index, query, limit, SearchScorer::bm25())
        .expect("query should search")
        .into_iter()
        .map(|hit| hit.id)
        .collect()
}

fn build_mixed_scope_index(corpus: &[(bool, bool)]) -> InMemoryIndex {
    let mut builder = InMemoryIndexBuilder::new(multi_field_analyzers());
    builder.register_field_alias(FieldId::new(2), "title");
    for (offset, (default_beta, title_alpha)) in corpus.iter().copied().enumerate() {
        let default_text = if default_beta { "beta" } else { "" };
        let title_text = if title_alpha { "alpha" } else { "" };
        builder
            .index_document(
                doc_id_from_index(offset),
                &[
                    (FieldId::new(1), default_text),
                    (FieldId::new(2), title_text),
                ],
            )
            .expect("document should index");
    }
    builder.build_index()
}

proptest! {
    #[test]
    fn and_query_matches_set_intersection(corpus in vec((any::<bool>(), any::<bool>(), any::<bool>()), 1..24)) {
        let index = build_index(&corpus);
        let limit = corpus.len();

        let actual = result_ids(&index, "alpha AND beta", limit);
        let expected: BTreeSet<u32> = corpus
            .iter()
            .enumerate()
            .filter_map(|(idx, (alpha, beta, _))| {
                (*alpha && *beta).then_some(doc_id_from_index(idx))
            })
            .collect();

        prop_assert_eq!(actual, expected);
    }

    #[test]
    fn or_query_matches_set_union(corpus in vec((any::<bool>(), any::<bool>(), any::<bool>()), 1..24)) {
        let index = build_index(&corpus);
        let limit = corpus.len();

        let actual = result_ids(&index, "alpha OR beta", limit);
        let expected: BTreeSet<u32> = corpus
            .iter()
            .enumerate()
            .filter_map(|(idx, (alpha, beta, _))| {
                (*alpha || *beta).then_some(doc_id_from_index(idx))
            })
            .collect();

        prop_assert_eq!(actual, expected);
    }

    #[test]
    fn not_query_matches_complement(corpus in vec((any::<bool>(), any::<bool>(), any::<bool>()), 1..24)) {
        let index = build_index(&corpus);
        let limit = corpus.len();

        let actual = result_ids(&index, "NOT alpha", limit);
        let expected: BTreeSet<u32> = corpus
            .iter()
            .enumerate()
            .filter_map(|(idx, (alpha, _, _))| (!*alpha).then_some(doc_id_from_index(idx)))
            .collect();

        prop_assert_eq!(actual, expected);
    }

    #[test]
    fn reused_workspace_matches_fresh_workspace(
        corpus in vec((any::<bool>(), any::<bool>(), any::<bool>()), 1..24),
        query_index in 0usize..4usize,
    ) {
        let index = build_index(&corpus);
        let limit = corpus.len();
        let queries = ["alpha AND beta", "alpha OR gamma", "NOT beta", "alpha beta"];
        let query = queries[query_index];

        let mut fresh_workspace = ExecutionWorkspace::new();
        let fresh = fresh_workspace
            .search(&index, query, limit, SearchScorer::bm25())
            .expect("fresh workspace search should succeed");

        let mut workspace = ExecutionWorkspace::new();
        let reused = workspace
            .search(&index, query, limit, SearchScorer::bm25())
            .expect("reused workspace search should succeed");

        prop_assert_eq!(reused, fresh);
    }

    #[test]
    fn mixed_scope_and_matches_expected_ids(corpus in vec((any::<bool>(), any::<bool>()), 1..24)) {
        let index = build_mixed_scope_index(&corpus);
        let limit = corpus.len();

        let actual = result_ids(&index, "title:alpha AND beta", limit);
        let expected: BTreeSet<u32> = corpus
            .iter()
            .enumerate()
            .filter_map(|(idx, (default_beta, title_alpha))| {
                (*default_beta && *title_alpha).then_some(doc_id_from_index(idx))
            })
            .collect();

        prop_assert_eq!(actual, expected);
    }
}
