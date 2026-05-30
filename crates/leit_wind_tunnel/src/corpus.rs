// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Deterministic corpus generator using seeded hashing.

use crate::vocabulary::Vocabulary;

/// A generated document with multiple fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedDoc {
    /// Document identifier.
    pub id: u32,
    /// Title field content.
    pub title: String,
    /// Body field content.
    pub body: String,
}

/// Deterministic corpus generator using seeded hashing.
///
/// Uses deterministic hashing to derive all document content from `(seed, doc_id, field, position)`,
/// ensuring byte-identical reproduction from the same seed.
#[derive(Clone, Debug)]
pub struct CorpusGenerator {
    seed: u64,
    vocabulary: Vocabulary,
}

impl CorpusGenerator {
    /// Create a new corpus generator with the given seed.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            vocabulary: Vocabulary::new(),
        }
    }

    /// Generate a corpus of the given size.
    ///
    /// Returns a `Vec<GeneratedDoc>`, one for each document ID from 1 to count (inclusive).
    pub fn generate(&self, count: u32) -> Vec<GeneratedDoc> {
        (1..=count)
            .map(|doc_id| self.generate_document(doc_id))
            .collect()
    }

    fn generate_document(&self, doc_id: u32) -> GeneratedDoc {
        GeneratedDoc {
            id: doc_id,
            title: self.generate_field_text(doc_id, 1, 3, 8),
            body: self.generate_field_text(doc_id, 2, 20, 100),
        }
    }

    fn generate_field_text(
        &self,
        doc_id: u32,
        field_id: u32,
        min_tokens: usize,
        max_tokens: usize,
    ) -> String {
        // Compute token count using deterministic hash
        let count_hash = Self::hash_combine(self.seed, doc_id, field_id, 0);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "deterministic generation intentionally uses modulo arithmetic and casts"
        )]
        let token_count = min_tokens + (count_hash as usize % (max_tokens - min_tokens + 1));

        // Generate tokens
        let mut tokens = Vec::with_capacity(token_count);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "deterministic generation intentionally converts token count to u32 for loop"
        )]
        for position in 0..token_count as u32 {
            let term_hash = Self::hash_combine(self.seed, doc_id, field_id, position as u64);
            let normalized = (term_hash as f64) / (u64::MAX as f64);
            let term = self.vocabulary.term_for_hash(normalized);
            tokens.push(term.to_string());
        }

        tokens.join(" ")
    }

    /// Deterministic hash combining multiple values using `rapidhash::v1` versioned API.
    ///
    /// Concatenates (`seed`, `doc_id`, `field_id`, `position`) as little-endian bytes and
    /// passes to `rapidhash::v1::rapidhash_v1` for stable, cross-version output.
    fn hash_combine(seed: u64, doc_id: u32, field_id: u32, position: u64) -> u64 {
        let mut buffer = [0_u8; 32];
        buffer[0..8].copy_from_slice(&seed.to_le_bytes());
        buffer[8..12].copy_from_slice(&doc_id.to_le_bytes());
        buffer[12..16].copy_from_slice(&field_id.to_le_bytes());
        buffer[16..24].copy_from_slice(&position.to_le_bytes());
        rapidhash::v1::rapidhash_v1(&buffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_combines_using_rapidhash_v1_versioned_api() {
        // This test verifies that hash_combine produces the deterministic
        // output expected from rapidhash::v1::rapidhash_v1.
        // Known output: rapidhash_v1([0u8; 32]) = 9_904_199_578_037_325_623
        let result = CorpusGenerator::hash_combine(0, 0, 0, 0);
        assert_eq!(result, rapidhash::v1::rapidhash_v1(&[0_u8; 32]));
    }

    #[test]
    fn test_determinism_same_seed_produces_identical_docs() {
        let corpus_gen1 = CorpusGenerator::new(42);
        let corpus_gen2 = CorpusGenerator::new(42);

        let corpus1 = corpus_gen1.generate(100);
        let corpus2 = corpus_gen2.generate(100);

        assert_eq!(corpus1, corpus2);
    }

    #[test]
    fn test_determinism_different_seeds_differ() {
        let corpus_gen1 = CorpusGenerator::new(42);
        let corpus_gen2 = CorpusGenerator::new(43);

        let corpus1 = corpus_gen1.generate(10);
        let corpus2 = corpus_gen2.generate(10);

        assert_ne!(corpus1, corpus2);
    }

    #[test]
    fn test_doc_count_accuracy_1k() {
        let corpus_gen = CorpusGenerator::new(42);
        let corpus = corpus_gen.generate(1000);
        assert_eq!(corpus.len(), 1000);
        assert_eq!(corpus[0].id, 1);
        assert_eq!(corpus[999].id, 1000);
    }

    #[test]
    fn test_doc_count_accuracy_10k() {
        let corpus_gen = CorpusGenerator::new(42);
        let corpus = corpus_gen.generate(10_000);
        assert_eq!(corpus.len(), 10_000);
        assert_eq!(corpus[0].id, 1);
        assert_eq!(corpus[9_999].id, 10_000);
    }

    #[test]
    fn test_token_count_ranges() {
        let corpus_gen = CorpusGenerator::new(42);
        let corpus = corpus_gen.generate(100);

        for doc in corpus {
            let title_tokens = doc.title.split_whitespace().count();
            let body_tokens = doc.body.split_whitespace().count();

            assert!(
                (3..=8).contains(&title_tokens),
                "title token count {} out of range [3, 8]",
                title_tokens
            );
            assert!(
                (20..=100).contains(&body_tokens),
                "body token count {} out of range [20, 100]",
                body_tokens
            );
        }
    }
}
