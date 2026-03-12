//! Property tests for Unicode-bearing analysis and indexing paths.

use std::collections::BTreeSet;

use leit_core::FieldId;
use leit_index::{ExecutionWorkspace, InMemoryIndex, InMemoryIndexBuilder, SearchScorer};
use leit_text::{
    Analyzer, CaseMapping, FieldAnalyzers, Normalizer, UnicodeNormalizer, WhitespaceTokenizer,
};
use proptest::collection::vec;
use proptest::prelude::*;

const TOKEN_POOL: &[&str] = &[
    "Rust",
    "mañana",
    "naïve",
    "straße",
    "東京",
    "добро",
    "σπίτι",
    "café",
    "résumé",
    "emoji🙂",
    "über",
    "coöperate",
    "smörgås",
    "jalapeño",
];

const WHITESPACE_POOL: &[&str] = &[" ", "\n", "\t", "\u{00A0}", "\u{2003}"];

const fn default_normalizer() -> UnicodeNormalizer {
    UnicodeNormalizer::new()
}

const fn folded_normalizer() -> UnicodeNormalizer {
    UnicodeNormalizer::builder()
        .case_mapping(CaseMapping::Fold)
        .build()
}

const fn checked_sub_one(value: usize) -> usize {
    value
        .checked_sub(1)
        .expect("position must be greater than zero")
}

const fn checked_add_one(value: usize) -> usize {
    value
        .checked_add(1)
        .expect("index increment should not overflow")
}

#[allow(clippy::arithmetic_side_effects)]
const fn token_at(index: usize) -> &'static str {
    TOKEN_POOL[index % TOKEN_POOL.len()]
}

#[allow(clippy::arithmetic_side_effects)]
const fn whitespace_at(index: usize) -> &'static str {
    WHITESPACE_POOL[index % WHITESPACE_POOL.len()]
}

fn analyzer_registry() -> FieldAnalyzers {
    let mut analyzers = FieldAnalyzers::new();
    let analyzer = Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(default_normalizer());
    analyzers.set(FieldId::new(1), analyzer);
    analyzers
}

fn folded_analyzer_registry() -> FieldAnalyzers {
    let mut analyzers = FieldAnalyzers::new();
    let analyzer = Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(folded_normalizer());
    analyzers.set(FieldId::new(1), analyzer);
    analyzers
}

fn normalize_for_expectation(token: &str) -> String {
    default_normalizer().normalize(token)
}

fn search(index: &InMemoryIndex, query: &str) -> Vec<leit_core::ScoredHit<u32>> {
    let mut workspace = ExecutionWorkspace::new();
    workspace
        .search(index, query, 16, SearchScorer::bm25())
        .expect("search should succeed")
}

fn build_document(token_indexes: &[usize], separator_indexes: &[usize]) -> String {
    let mut document = String::new();
    for (position, token_index) in token_indexes.iter().copied().enumerate() {
        if position > 0 {
            let separator = whitespace_at(separator_indexes[checked_sub_one(position)]);
            document.push_str(separator);
        }
        document.push_str(token_at(token_index));
    }
    document
}

fn document_strategy() -> impl Strategy<Value = (Vec<usize>, Vec<usize>)> {
    vec(0usize..TOKEN_POOL.len(), 1..6).prop_flat_map(|token_indexes| {
        let separator_count = token_indexes.len().saturating_sub(1);
        let separators = vec(0usize..WHITESPACE_POOL.len(), separator_count);
        (Just(token_indexes), separators)
    })
}

proptest! {
    #[test]
    fn unicode_token_search_matches_expected_documents(
        documents in vec(document_strategy(), 1..5),
        query_index in 0usize..TOKEN_POOL.len(),
    ) {
        let mut builder = InMemoryIndexBuilder::new(analyzer_registry());
        let query = token_at(query_index);
        let normalized_query = normalize_for_expectation(query);

        let mut expected = BTreeSet::new();

        for (offset, (token_indexes, separators)) in documents.iter().enumerate() {
            let document = build_document(token_indexes, separators);
            let doc_id = u32::try_from(checked_add_one(offset))
                .expect("test document IDs should fit in u32");

            builder
                .index_document(doc_id, &[(FieldId::new(1), document.as_str())])
                .expect("generated document should index");

            if token_indexes.iter().any(|index| {
                normalize_for_expectation(token_at(*index)) == normalized_query
            }) {
                expected.insert(doc_id);
            }
        }

        let index = builder.build_index();
        let hits = search(&index, query);
        let actual: BTreeSet<_> = hits.into_iter().map(|hit| hit.id).collect();

        prop_assert_eq!(actual, expected);
    }
}

#[test]
fn unicode_search_matches_case_and_canonical_variants() {
    let mut builder = InMemoryIndexBuilder::new(analyzer_registry());

    builder
        .index_document(1, &[(FieldId::new(1), "CAFÉ")])
        .expect("composed uppercase document should index");
    builder
        .index_document(2, &[(FieldId::new(1), "cafe\u{301}")])
        .expect("decomposed lowercase document should index");
    builder
        .index_document(3, &[(FieldId::new(1), "ΣΠΊΤΙ")])
        .expect("greek uppercase document should index");

    let index = builder.build_index();

    let cafe_hits = search(&index, "café");
    let cafe_ids: BTreeSet<_> = cafe_hits.into_iter().map(|hit| hit.id).collect();
    assert_eq!(cafe_ids, BTreeSet::from([1, 2]));

    let greek_hits = search(&index, "σπίτι");
    let greek_ids: BTreeSet<_> = greek_hits.into_iter().map(|hit| hit.id).collect();
    assert_eq!(greek_ids, BTreeSet::from([3]));
}

#[test]
fn unicode_search_can_opt_into_case_fold_matching() {
    let mut builder = InMemoryIndexBuilder::new(folded_analyzer_registry());

    builder
        .index_document(1, &[(FieldId::new(1), "Straße")])
        .expect("sharp-s document should index");

    let index = builder.build_index();

    let hits = search(&index, "STRASSE");
    let actual: BTreeSet<_> = hits.into_iter().map(|hit| hit.id).collect();
    assert_eq!(actual, BTreeSet::from([1]));
}

#[test]
fn unicode_search_uses_context_sensitive_lowercase_behavior() {
    let mut builder = InMemoryIndexBuilder::new(analyzer_registry());

    builder
        .index_document(1, &[(FieldId::new(1), "ΟΣ")])
        .expect("greek sigma document should index");

    let index = builder.build_index();

    let hits = search(&index, "ος");
    let actual: BTreeSet<_> = hits.into_iter().map(|hit| hit.id).collect();
    assert_eq!(actual, BTreeSet::from([1]));
}
