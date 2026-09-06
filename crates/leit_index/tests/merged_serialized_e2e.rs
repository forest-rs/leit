// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Composed logical-merge and serialized-segment contract (SCENARIO-0027).

use std::collections::{BTreeMap, BTreeSet};

use leit_core::{FieldId, ScoredHit, TermId};
use leit_index::{
    CodecId, ExecutableIndex, ExecutionWorkspace, InMemoryIndex, InMemoryIndexBuilder, NoFilter,
    SearchScorer, SegmentView, ValidationMode, prepare_merge, prepare_serialization,
};
use leit_postings::codec::{BlockDeltaCodec, Codec, DeltaVarintCodec};
use leit_text::{
    AnalysisSchemaId, Analyzer, FieldAnalyzers, UnicodeNormalizer, WhitespaceTokenizer,
};

const TITLE: FieldId = FieldId::new(1);
const BODY: FieldId = FieldId::new(2);
type CanonicalTerm = (FieldId, String);
type LogicalPosting = (u32, u32);
type PostingsByTerm = BTreeMap<CanonicalTerm, Vec<LogicalPosting>>;

#[derive(Clone, Debug)]
struct Document {
    local_id: u32,
    title: String,
    body: String,
}

fn analyzers() -> FieldAnalyzers {
    let mut analyzers = FieldAnalyzers::with_schema_id(
        AnalysisSchemaId::new(2_027).expect("fixture schema ID is nonzero"),
    );
    for field in [TITLE, BODY] {
        analyzers.set(
            field,
            Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
        );
    }
    analyzers
}

fn build_index(documents: &[Document]) -> InMemoryIndex {
    let mut builder = InMemoryIndexBuilder::new(analyzers());
    builder.register_field_alias(TITLE, "title");
    builder.register_field_alias(BODY, "body");
    for document in documents {
        builder
            .index_document(
                document.local_id,
                &[
                    (TITLE, document.title.as_str()),
                    (BODY, document.body.as_str()),
                ],
            )
            .expect("fixture document should index");
    }
    builder.build_index()
}

fn n_source_fixture() -> Vec<Vec<Document>> {
    let mut bulk = Vec::new();
    for ordinal in (0..129).rev() {
        let mut body = String::from("common partial");
        if ordinal < 128 {
            body.push_str(" exact");
        }
        if ordinal % 17 == 0 {
            body.push_str(" boosted boosted");
        }
        bulk.push(Document {
            local_id: ordinal * 7 + 3,
            title: if ordinal % 11 == 0 {
                "common archive".into()
            } else {
                "archive".into()
            },
            body,
        });
    }
    vec![
        Vec::new(),
        vec![
            Document {
                local_id: 90,
                title: "Common Rust".into(),
                body: "common overlap-only".into(),
            },
            Document {
                local_id: 10,
                title: "Rare".into(),
                body: "common short".into(),
            },
        ],
        vec![Document {
            local_id: 10,
            title: "Common Search".into(),
            body: "disjoint-only common".into(),
        }],
        bulk,
    ]
}

fn independent_remaps(sources: &[Vec<Document>]) -> Vec<Vec<(u32, u32)>> {
    let mut next = 0_u32;
    sources
        .iter()
        .map(|documents| {
            let local_ids: BTreeSet<_> =
                documents.iter().map(|document| document.local_id).collect();
            local_ids
                .into_iter()
                .map(|local| {
                    let global = next;
                    next = next
                        .checked_add(1)
                        .expect("fixture document count fits u32");
                    (local, global)
                })
                .collect()
        })
        .collect()
}

fn decode_serialized_postings(
    index: &InMemoryIndex,
    codec: CodecId,
    expected_canonical_terms: usize,
) {
    let bytes = prepare_serialization(index, codec)
        .expect("merged index should prepare for serialization")
        .into_bytes();
    let view = SegmentView::open_with_validation(&bytes, ValidationMode::Full)
        .expect("prepared segment should Full-open");
    let lexicon = view.lexicon().expect("serialized lexicon should open");
    let table = view
        .postings_table()
        .expect("serialized postings table should open");
    let data = view
        .postings_data()
        .expect("serialized postings data should open");
    let expected_canonical_terms =
        u32::try_from(expected_canonical_terms).expect("fixture term count fits u32");
    assert_eq!(
        table.len(),
        expected_canonical_terms,
        "postings table must contain every independently derived canonical term"
    );
    assert_eq!(
        lexicon.len(),
        expected_canonical_terms,
        "lexicon must contain every independently derived canonical term"
    );

    for term_index in 0..table.len() {
        let (term_text, postings_index) = lexicon
            .entry(term_index)
            .expect("serialized lexicon entry should decode");
        assert_eq!(
            postings_index, term_index,
            "lexicon index must be canonical"
        );
        assert_eq!(
            index
                .term_entry(TermId::new(term_index))
                .expect("logical term should exist")
                .term_text
                .as_bytes(),
            term_text,
            "serialized term text must match the logical term"
        );
        let (offset, len, doc_freq, kind, _, _) = table
            .entry(postings_index)
            .expect("serialized postings entry should decode");
        assert_eq!(
            kind,
            match codec {
                CodecId::DeltaVarint => 1,
                CodecId::BlockDelta => 2,
            },
            "postings kind must identify the selected codec"
        );
        let payload = data
            .range(offset, len)
            .expect("serialized postings payload should be in bounds");
        let mut docs = Vec::new();
        let mut term_freqs = Vec::new();
        match codec {
            CodecId::DeltaVarint => DeltaVarintCodec
                .decode(payload, &mut docs, &mut term_freqs)
                .expect("DeltaVarint payload should decode"),
            CodecId::BlockDelta => BlockDeltaCodec
                .decode(payload, &mut docs, &mut term_freqs)
                .expect("BlockDelta payload should decode"),
        }
        let actual: Vec<_> = docs
            .into_iter()
            .zip(term_freqs)
            .map(|(doc, term_freq)| (doc.get(), term_freq.get()))
            .collect();
        let expected: Vec<_> = index
            .postings(TermId::new(term_index))
            .expect("logical postings should exist")
            .iter()
            .map(|posting| (posting.doc_id(), posting.term_freq()))
            .collect();
        assert_eq!(actual.len(), doc_freq as usize, "doc frequency must match");
        assert_eq!(
            actual, expected,
            "serialized postings must match logical postings"
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
        .expect("fixture query should execute")
}

fn assert_hits_exact(actual: &[ScoredHit<u32>], expected: &[ScoredHit<u32>]) {
    assert_eq!(actual.len(), expected.len(), "hit counts must match");
    for (actual, expected) in actual.iter().zip(expected) {
        assert_eq!(actual.id, expected.id, "ranked document must match");
        assert_eq!(
            actual.score.as_f32().to_bits(),
            expected.score.as_f32().to_bits(),
            "score bits must match"
        );
    }
}

#[test]
fn merged_segment_composes_remaps_codecs_full_validation_and_fresh_scoring_oracle() {
    let n_sources = n_source_fixture();
    let singleton = vec![n_sources[1].clone()];
    let fixture_sets = vec![Vec::new(), singleton, n_sources];

    for source_documents in fixture_sets {
        let remaps = independent_remaps(&source_documents);
        let sources: Vec<_> = source_documents
            .iter()
            .map(|documents| build_index(documents))
            .collect();
        let source_postings: Vec<PostingsByTerm> = sources
            .iter()
            .map(|source| {
                let mut postings = BTreeMap::new();
                let mut term_index = 0_u32;
                while let Some(entry) = source.term_entry(TermId::new(term_index)) {
                    postings.insert(
                        (entry.field_id, entry.term_text.to_owned()),
                        source
                            .postings(TermId::new(term_index))
                            .expect("source term should have postings")
                            .iter()
                            .map(|posting| (posting.doc_id(), posting.term_freq()))
                            .collect(),
                    );
                    term_index += 1;
                }
                postings
            })
            .collect();
        let prepared = prepare_merge(sources, analyzers()).expect("fixture merge should prepare");
        for (source, expected) in remaps.iter().enumerate() {
            assert_eq!(prepared.document_remap(source), Some(expected.as_slice()));
        }
        let merged = prepared.execute();

        let mut expected_union = BTreeMap::<(FieldId, String), Vec<(u32, u32)>>::new();
        for (source, source_terms) in source_postings.iter().enumerate() {
            for (term, postings) in source_terms {
                let target = expected_union.entry(term.clone()).or_default();
                for &(local_doc, term_freq) in postings {
                    let position = remaps[source]
                        .binary_search_by_key(&local_doc, |&(local, _)| local)
                        .expect("source posting should have a document remap");
                    target.push((remaps[source][position].1, term_freq));
                }
            }
        }
        for expected in expected_union.values_mut() {
            expected.sort_unstable();
        }
        let mut actual_merged = PostingsByTerm::new();
        let mut term_index = 0_u32;
        while let Some(entry) = merged.index().term_entry(TermId::new(term_index)) {
            let postings = merged
                .index()
                .postings(TermId::new(term_index))
                .expect("merged term should have postings")
                .iter()
                .map(|posting| (posting.doc_id(), posting.term_freq()))
                .collect();
            assert!(
                actual_merged
                    .insert((entry.field_id, entry.term_text.to_owned()), postings)
                    .is_none(),
                "merged canonical terms must be unique"
            );
            term_index += 1;
        }
        assert_eq!(
            actual_merged, expected_union,
            "merged field-qualified vocabulary and postings must equal the source union"
        );
        if !expected_union.is_empty() {
            assert!(
                expected_union.contains_key(&(TITLE, "common".to_owned())),
                "fixture must retain title/common as a field-qualified term"
            );
            assert!(
                expected_union.contains_key(&(BODY, "common".to_owned())),
                "fixture must retain body/common independently from title/common"
            );
        }
        if expected_union.contains_key(&(BODY, "partial".to_owned())) {
            assert_eq!(
                expected_union[&(BODY, "exact".to_owned())].len(),
                128,
                "body/exact must exercise one full codec block"
            );
            assert_eq!(
                expected_union[&(BODY, "partial".to_owned())].len(),
                129,
                "body/partial must exercise a partial final codec block"
            );
            assert_eq!(
                expected_union[&(BODY, "short".to_owned())].len(),
                1,
                "body/short must remain a genuinely short postings list"
            );
        }

        for codec in [CodecId::DeltaVarint, CodecId::BlockDelta] {
            decode_serialized_postings(merged.index(), codec, expected_union.len());
        }

        let mut oracle_documents = Vec::new();
        for (source, documents) in source_documents.iter().enumerate() {
            for document in documents {
                let position = remaps[source]
                    .binary_search_by_key(&document.local_id, |&(local, _)| local)
                    .expect("source document should have a remap");
                let mut remapped = document.clone();
                remapped.local_id = remaps[source][position].1;
                oracle_documents.push(remapped);
            }
        }
        let oracle = build_index(&oracle_documents);
        if oracle_documents.is_empty() {
            continue;
        }
        for (query, bm25f) in [
            ("body:common", false),
            ("body:exact OR body:overlap-only", false),
            ("common", true),
            ("common OR rare", true),
        ] {
            for limit in [1, 5, 512] {
                let actual = search(
                    merged.index(),
                    query,
                    limit,
                    if bm25f {
                        SearchScorer::bm25f()
                    } else {
                        SearchScorer::bm25()
                    },
                );
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
                assert_hits_exact(&actual, &expected);
            }
        }
    }
}
