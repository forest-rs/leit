// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Query fixtures drawn from frequent vocabulary terms.

/// A query fixture with label and text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryFixture {
    /// Fixture name/label.
    pub name: &'static str,
    /// Query text in `leit_query` syntax.
    pub text: &'static str,
}

/// Pre-built query fixtures for benchmarking.
///
/// All fixtures are drawn from the frequent end of the Zipfian vocabulary
/// to guarantee hits when executed against a generated corpus.
/// Fixtures use the confirmed `leit_query` syntax:
/// - Single-term: `"term"` (unfielded)
/// - Multi-term OR: `"term1 OR term2 OR term3"` (space-separated OR operator)
/// - Multi-term AND: `"term1 AND term2"` (space-separated AND operator)
/// - Fielded: `"field:term"` (field name colon term, no spaces)
/// - Cross-field: unfielded term expands via default fields in `ExecutionWorkspace`
#[derive(Clone, Debug)]
pub struct QueryFixtures;

impl QueryFixtures {
    /// Single-term fixture: `"the"`
    pub fn single_term() -> QueryFixture {
        QueryFixture {
            name: "single_term",
            text: "the",
        }
    }

    /// Multi-term OR fixture (3 terms): `"the OR and OR to"`
    pub fn multi_term_or() -> QueryFixture {
        QueryFixture {
            name: "multi_term_or",
            text: "the OR and OR to",
        }
    }

    /// Multi-term AND fixture (2 terms): `"the AND of"`
    pub fn multi_term_and() -> QueryFixture {
        QueryFixture {
            name: "multi_term_and",
            text: "the AND of",
        }
    }

    /// Fielded query fixture (title field): `"title:the"`
    pub fn fielded_title() -> QueryFixture {
        QueryFixture {
            name: "fielded_title",
            text: "title:the",
        }
    }

    /// BM25F cross-field fixture (unfielded term): `"in"`
    pub fn cross_field() -> QueryFixture {
        QueryFixture {
            name: "cross_field",
            text: "in",
        }
    }

    /// All fixtures.
    pub fn all() -> &'static [QueryFixture] {
        &[
            QueryFixture {
                name: "single_term",
                text: "the",
            },
            QueryFixture {
                name: "multi_term_or",
                text: "the OR and OR to",
            },
            QueryFixture {
                name: "multi_term_and",
                text: "the AND of",
            },
            QueryFixture {
                name: "fielded_title",
                text: "title:the",
            },
            QueryFixture {
                name: "cross_field",
                text: "in",
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixture_queries_non_empty() {
        for fixture in QueryFixtures::all() {
            assert!(
                !fixture.text.is_empty(),
                "fixture {} has empty text",
                fixture.name
            );
            assert!(!fixture.name.is_empty(), "fixture has empty name");
        }
    }

    #[test]
    fn test_fixture_syntax_validity() {
        let single = QueryFixtures::single_term();
        assert_eq!(single.text, "the");

        let multi_or = QueryFixtures::multi_term_or();
        assert!(multi_or.text.contains(" OR "));

        let multi_and = QueryFixtures::multi_term_and();
        assert!(multi_and.text.contains(" AND "));

        let fielded = QueryFixtures::fielded_title();
        assert!(fielded.text.contains(":"));

        let cross = QueryFixtures::cross_field();
        assert!(!cross.text.contains(" "));
        assert!(!cross.text.contains(":"));
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::corpus::CorpusGenerator;
    use leit_core::FieldId;
    use leit_index::{ExecutionWorkspace, InMemoryIndexBuilder, NoFilter, SearchScorer};
    use leit_text::{Analyzer, FieldAnalyzers, UnicodeNormalizer, WhitespaceTokenizer};

    #[test]
    fn test_single_term_produces_hits() {
        let corpus_gen = CorpusGenerator::new(42);
        let corpus = corpus_gen.generate(100);

        let mut analyzers = FieldAnalyzers::new();
        analyzers.set(
            FieldId::new(1),
            Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
        );
        analyzers.set(
            FieldId::new(2),
            Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
        );

        let mut builder = InMemoryIndexBuilder::new(analyzers);
        builder.register_field_alias(FieldId::new(1), "title");
        builder.register_field_alias(FieldId::new(2), "body");

        for doc in &corpus {
            builder
                .index_document(
                    doc.id,
                    &[
                        (FieldId::new(1), doc.title.as_str()),
                        (FieldId::new(2), doc.body.as_str()),
                    ],
                )
                .expect("indexing should succeed");
        }
        let index = builder.build_index();

        let mut workspace = ExecutionWorkspace::new();
        let fixture = QueryFixtures::single_term();
        let hits = workspace
            .search(&index, fixture.text, 10, SearchScorer::bm25(), &NoFilter)
            .expect("search should succeed");

        assert!(!hits.is_empty(), "single_term fixture should produce hits");
    }

    #[test]
    fn test_or_query_produces_hits() {
        let corpus_gen = CorpusGenerator::new(42);
        let corpus = corpus_gen.generate(100);

        let mut analyzers = FieldAnalyzers::new();
        analyzers.set(
            FieldId::new(1),
            Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
        );
        analyzers.set(
            FieldId::new(2),
            Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
        );

        let mut builder = InMemoryIndexBuilder::new(analyzers);
        builder.register_field_alias(FieldId::new(1), "title");
        builder.register_field_alias(FieldId::new(2), "body");

        for doc in &corpus {
            builder
                .index_document(
                    doc.id,
                    &[
                        (FieldId::new(1), doc.title.as_str()),
                        (FieldId::new(2), doc.body.as_str()),
                    ],
                )
                .expect("indexing should succeed");
        }
        let index = builder.build_index();

        let mut workspace = ExecutionWorkspace::new();
        let fixture = QueryFixtures::multi_term_or();
        let hits = workspace
            .search(&index, fixture.text, 10, SearchScorer::bm25(), &NoFilter)
            .expect("search should succeed");

        assert!(
            !hits.is_empty(),
            "multi_term_or fixture should produce hits"
        );
    }

    #[test]
    fn test_and_query_produces_hits() {
        let corpus_gen = CorpusGenerator::new(42);
        let corpus = corpus_gen.generate(100);

        let mut analyzers = FieldAnalyzers::new();
        analyzers.set(
            FieldId::new(1),
            Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
        );
        analyzers.set(
            FieldId::new(2),
            Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
        );

        let mut builder = InMemoryIndexBuilder::new(analyzers);
        builder.register_field_alias(FieldId::new(1), "title");
        builder.register_field_alias(FieldId::new(2), "body");

        for doc in &corpus {
            builder
                .index_document(
                    doc.id,
                    &[
                        (FieldId::new(1), doc.title.as_str()),
                        (FieldId::new(2), doc.body.as_str()),
                    ],
                )
                .expect("indexing should succeed");
        }
        let index = builder.build_index();

        let mut workspace = ExecutionWorkspace::new();
        let fixture = QueryFixtures::multi_term_and();
        let hits = workspace
            .search(&index, fixture.text, 10, SearchScorer::bm25(), &NoFilter)
            .expect("search should succeed");

        assert!(
            !hits.is_empty(),
            "multi_term_and fixture should produce hits"
        );
    }

    #[test]
    fn test_fielded_query_produces_hits() {
        let corpus_gen = CorpusGenerator::new(42);
        let corpus = corpus_gen.generate(100);

        let mut analyzers = FieldAnalyzers::new();
        analyzers.set(
            FieldId::new(1),
            Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
        );
        analyzers.set(
            FieldId::new(2),
            Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
        );

        let mut builder = InMemoryIndexBuilder::new(analyzers);
        builder.register_field_alias(FieldId::new(1), "title");
        builder.register_field_alias(FieldId::new(2), "body");

        for doc in &corpus {
            builder
                .index_document(
                    doc.id,
                    &[
                        (FieldId::new(1), doc.title.as_str()),
                        (FieldId::new(2), doc.body.as_str()),
                    ],
                )
                .expect("indexing should succeed");
        }
        let index = builder.build_index();

        let mut workspace = ExecutionWorkspace::new();
        let fixture = QueryFixtures::fielded_title();
        let hits = workspace
            .search(&index, fixture.text, 10, SearchScorer::bm25(), &NoFilter)
            .expect("search should succeed");

        assert!(
            !hits.is_empty(),
            "fielded_title fixture should produce hits"
        );
    }

    #[test]
    fn test_cross_field_query_produces_hits() {
        let corpus_gen = CorpusGenerator::new(42);
        let corpus = corpus_gen.generate(100);

        let mut analyzers = FieldAnalyzers::new();
        analyzers.set(
            FieldId::new(1),
            Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
        );
        analyzers.set(
            FieldId::new(2),
            Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
        );

        let mut builder = InMemoryIndexBuilder::new(analyzers);
        builder.register_field_alias(FieldId::new(1), "title");
        builder.register_field_alias(FieldId::new(2), "body");

        for doc in &corpus {
            builder
                .index_document(
                    doc.id,
                    &[
                        (FieldId::new(1), doc.title.as_str()),
                        (FieldId::new(2), doc.body.as_str()),
                    ],
                )
                .expect("indexing should succeed");
        }
        let index = builder.build_index();

        let mut workspace = ExecutionWorkspace::new();
        let fixture = QueryFixtures::cross_field();
        let hits = workspace
            .search(&index, fixture.text, 10, SearchScorer::bm25(), &NoFilter)
            .expect("search should succeed");

        assert!(!hits.is_empty(), "cross_field fixture should produce hits");
    }
}
