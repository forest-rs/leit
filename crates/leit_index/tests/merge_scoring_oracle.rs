// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Assembled logical-merge scoring oracle (SCENARIO-0008 / logical SCENARIO-0027).

use std::collections::{BTreeMap, BTreeSet};

use leit_core::{FieldId, ScoredHit, TermId};
use leit_index::{
    ExecutableIndex, ExecutionWorkspace, InMemoryIndex, InMemoryIndexBuilder, NoFilter,
    SearchScorer, prepare_merge,
};
use leit_query::TermDictionary;
use leit_text::{
    AnalysisSchemaId, Analyzer, FieldAnalyzers, UnicodeNormalizer, WhitespaceTokenizer,
};

const TITLE: FieldId = FieldId::new(1);
const BODY: FieldId = FieldId::new(2);
const SCHEMA: u64 = 66;

#[derive(Clone, Debug)]
struct Document {
    id: u32,
    title: String,
    body: String,
}

fn analyzers() -> FieldAnalyzers {
    let schema = AnalysisSchemaId::new(SCHEMA).expect("fixture schema is nonzero");
    let mut analyzers = FieldAnalyzers::with_schema_id(schema);
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

fn build_index(documents: &[Document]) -> InMemoryIndex {
    let mut builder = InMemoryIndexBuilder::new(analyzers());
    builder.register_field_alias(TITLE, "title");
    builder.register_field_alias(BODY, "body");
    for document in documents {
        builder
            .index_document(
                document.id,
                &[
                    (TITLE, document.title.as_str()),
                    (BODY, document.body.as_str()),
                ],
            )
            .expect("fixture document indexes");
    }
    builder.build_index()
}

fn fixtures() -> Vec<Vec<Document>> {
    let empty = Vec::new();
    let overlapping = vec![
        Document {
            id: 90,
            title: "Common Rust".into(),
            body: "common common overlap-only".into(),
        },
        Document {
            id: 10,
            title: "Rare".into(),
            body: "common short".into(),
        },
    ];
    let disjoint = vec![Document {
        id: 10,
        title: "Common Search".into(),
        body: "disjoint-only common".into(),
    }];
    let multi_block = (0..130)
        .rev()
        .map(|id| Document {
            id: id * 3 + 7,
            title: if id % 11 == 0 {
                "common boosted".into()
            } else {
                "archive".into()
            },
            body: if id % 17 == 0 {
                "common common tail-rare".into()
            } else {
                "common tail".into()
            },
        })
        .collect();
    vec![empty, overlapping, disjoint, multi_block]
}

fn independent_document_remaps(sources: &[Vec<Document>]) -> Vec<Vec<(u32, u32)>> {
    let mut next_global = 0_u32;
    sources
        .iter()
        .map(|documents| {
            let local_ids: BTreeSet<_> = documents.iter().map(|document| document.id).collect();
            local_ids
                .into_iter()
                .map(|local| {
                    let global = next_global;
                    next_global = next_global.checked_add(1).expect("fixture IDs fit u32");
                    (local, global)
                })
                .collect()
        })
        .collect()
}

fn normalized_vocabulary(sources: &[Vec<Document>]) -> BTreeSet<(FieldId, String)> {
    let analyzers = analyzers();
    let mut vocabulary = BTreeSet::new();
    for document in sources.iter().flatten() {
        for (field, text) in [
            (TITLE, document.title.as_str()),
            (BODY, document.body.as_str()),
        ] {
            let analyzer = analyzers.get(field).expect("fixture field has analyzer");
            vocabulary.extend(
                analyzer
                    .analyze(text)
                    .into_iter()
                    .map(|(_token, normalized)| (field, normalized)),
            );
        }
    }
    vocabulary
}

fn indexed_vocabulary(index: &InMemoryIndex) -> BTreeSet<(FieldId, String)> {
    let mut vocabulary = BTreeSet::new();
    let mut raw_term = 0_u32;
    while let Some(entry) = index.term_entry(TermId::new(raw_term)) {
        vocabulary.insert((entry.field_id, entry.term_text.to_owned()));
        raw_term = raw_term.checked_add(1).expect("fixture term IDs fit u32");
    }
    vocabulary
}

fn assert_hits_bitwise_eq(actual: &[ScoredHit<u32>], expected: &[ScoredHit<u32>]) {
    assert_eq!(actual.len(), expected.len(), "hit count differs");
    for (actual, expected) in actual.iter().zip(expected) {
        assert_eq!(actual.id, expected.id, "ranked document differs");
        assert_eq!(
            actual.score.as_f32().to_bits(),
            expected.score.as_f32().to_bits(),
            "score differs for document {}: {} != {}",
            actual.id,
            actual.score,
            expected.score
        );
    }
}

fn search(
    index: &InMemoryIndex,
    query: &str,
    limit: usize,
    scorer: SearchScorer,
) -> Vec<ScoredHit<u32>> {
    ExecutionWorkspace::new()
        .search(index, query, limit, scorer, &NoFilter)
        .expect("oracle query executes")
}

#[test]
fn logical_merge_matches_fresh_global_corpus_for_postings_bm25_and_bm25f() {
    let source_documents = fixtures();
    let independent_remaps = independent_document_remaps(&source_documents);
    let canonical_terms = normalized_vocabulary(&source_documents);
    let sources: Vec<_> = source_documents
        .iter()
        .map(|documents| build_index(documents))
        .collect();
    let source_union_vocabulary: BTreeSet<_> =
        sources.iter().flat_map(indexed_vocabulary).collect();
    assert_eq!(source_union_vocabulary, canonical_terms);
    let source_postings: Vec<_> = sources
        .iter()
        .map(|source| {
            canonical_terms
                .iter()
                .filter_map(|(field, text)| {
                    let term = source.resolve_term(*field, text)?;
                    let postings = source.postings(term)?;
                    Some(((*field, text.clone()), postings.to_vec()))
                })
                .collect::<BTreeMap<_, _>>()
        })
        .collect();

    let prepared = prepare_merge(sources, analyzers()).expect("logical merge prepares");
    for (source, expected) in independent_remaps.iter().enumerate() {
        assert_eq!(prepared.document_remap(source), Some(expected.as_slice()));
    }
    let mut expected_postings = BTreeMap::<_, Vec<_>>::new();
    for (source, postings_by_term) in source_postings.iter().enumerate() {
        let remap = &independent_remaps[source];
        for (canonical, postings) in postings_by_term {
            let expected = expected_postings.entry(canonical.clone()).or_default();
            for posting in postings {
                let merged_document = remap
                    .binary_search_by_key(&posting.doc_id(), |&(source, _)| source)
                    .map(|position| remap[position].1)
                    .expect("source posting document has a remap");
                expected.push((merged_document, posting.term_freq()));
            }
        }
    }
    for postings in expected_postings.values_mut() {
        postings.sort_unstable();
    }

    let merged = prepared.execute();
    for (source, expected) in independent_remaps.iter().enumerate() {
        assert_eq!(merged.document_remap(source), Some(expected.as_slice()));
    }
    assert_eq!(indexed_vocabulary(merged.index()), canonical_terms);
    for (field, text) in &canonical_terms {
        let term = merged
            .index()
            .resolve_term(*field, text)
            .expect("union term exists in merged index");
        let actual: Vec<_> = merged
            .index()
            .postings(term)
            .expect("union term has postings")
            .iter()
            .map(|posting| (posting.doc_id(), posting.term_freq()))
            .collect();
        assert_eq!(actual, expected_postings[&(*field, text.clone())]);
    }
    let body_common = merged
        .index()
        .resolve_term(BODY, "common")
        .expect("body/common exists");
    assert!(
        merged
            .index()
            .postings(body_common)
            .expect("postings exist")
            .len()
            > 128,
        "fixture must exercise a multi-block postings list"
    );

    let mut fresh_documents = Vec::new();
    for (source, documents) in source_documents.iter().enumerate() {
        let remap = &independent_remaps[source];
        for document in documents {
            let mut global = document.clone();
            global.id = remap
                .binary_search_by_key(&document.id, |&(source, _)| source)
                .map(|position| remap[position].1)
                .expect("source document has a remap");
            fresh_documents.push(global);
        }
    }
    let oracle = build_index(&fresh_documents);

    for field in [TITLE, BODY] {
        let merged_stats = merged
            .index()
            .field_stats(field)
            .expect("merged field has statistics");
        let oracle_stats = oracle
            .field_stats(field)
            .expect("fresh field has statistics");
        assert_eq!(merged_stats.doc_count, oracle_stats.doc_count);
        assert_eq!(merged_stats.total_terms, oracle_stats.total_terms);
    }
    for (field, text) in &canonical_terms {
        let merged_term = merged
            .index()
            .resolve_term(*field, text)
            .expect("merged oracle term exists");
        let oracle_term = oracle
            .resolve_term(*field, text)
            .expect("fresh oracle term exists");
        let merged_postings = merged
            .index()
            .postings(merged_term)
            .expect("merged oracle term has postings");
        let oracle_postings = oracle
            .postings(oracle_term)
            .expect("fresh oracle term has postings");
        assert_eq!(
            merged_postings.len(),
            oracle_postings.len(),
            "document frequency differs for field {} term {text}",
            field.as_u32()
        );
        assert_eq!(
            merged_postings
                .iter()
                .map(|posting| u64::from(posting.term_freq()))
                .sum::<u64>(),
            oracle_postings
                .iter()
                .map(|posting| u64::from(posting.term_freq()))
                .sum::<u64>(),
            "collection frequency differs for field {} term {text}",
            field.as_u32()
        );
    }

    for &(query, bm25f) in &[
        ("body:common", false),
        ("body:tail-rare OR body:overlap-only", false),
        ("common", true),
        ("common OR rare", true),
    ] {
        for limit in [1, 5, 256] {
            let scorer = if bm25f {
                SearchScorer::bm25f()
            } else {
                SearchScorer::bm25()
            };
            let actual = search(merged.index(), query, limit, scorer);
            let expected = search(
                &oracle,
                query,
                limit,
                if bm25f {
                    SearchScorer::bm25f()
                } else {
                    SearchScorer::bm25()
                },
            );
            assert!(!expected.is_empty(), "oracle query {query:?} must match");
            assert_hits_bitwise_eq(&actual, &expected);

            let repeated = search(
                merged.index(),
                query,
                limit,
                if bm25f {
                    SearchScorer::bm25f()
                } else {
                    SearchScorer::bm25()
                },
            );
            assert_hits_bitwise_eq(&repeated, &actual);
        }
    }
}
