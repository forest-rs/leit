// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Scoring algorithms for Leit retrieval system.
//!
//! This crate provides scoring functions for ranking search results,
//! including BM25 and BM25F (multi-field BM25).

#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

use alloc::vec::Vec;

#[cfg_attr(
    feature = "std",
    expect(
        unused_imports,
        reason = "core_maths provides f32 math methods in no_std"
    )
)]
use core_maths::CoreFloat as _;

use leit_core::Score;

// ============================================================================
// Scorer Trait
// ============================================================================

/// Trait for scoring documents.
///
/// This trait is designed to be generic enough to support:
/// - Traditional IR scoring (BM25, TF-IDF)
/// - Neural/ML-based scoring (embeddings, transformers)
/// - Hybrid scoring (combining multiple signals)
pub trait Scorer {
    /// Compute a score for the given statistics.
    /// Returns None if scoring is not possible with the given data.
    fn score(&self, stats: &ScoringStats) -> Option<Score>;

    /// Get the name of this scorer for debugging/logging.
    fn name(&self) -> &'static str;

    /// Check if this scorer needs term vectors (positions).
    fn needs_positions(&self) -> bool {
        false
    }

    /// Check if this scorer needs field-level statistics.
    fn needs_field_stats(&self) -> bool {
        false
    }
}

// ============================================================================
// CombinedScorer
// ============================================================================

/// Combines exactly two scorers with configurable weights.
/// For more scorers, nest them: `CombinedScorer<CombinedScorer<A, B>, C>`
#[derive(Clone, Copy, Debug)]
pub struct CombinedScorer<A: Scorer, B: Scorer> {
    first: A,
    first_weight: f32,
    second: B,
    second_weight: f32,
}

impl<A: Scorer, B: Scorer> CombinedScorer<A, B> {
    /// Create a new combined scorer.
    pub const fn new(first: A, first_weight: f32, second: B, second_weight: f32) -> Self {
        Self {
            first,
            first_weight,
            second,
            second_weight,
        }
    }
}

impl<A: Scorer, B: Scorer> Scorer for CombinedScorer<A, B> {
    fn score(&self, stats: &ScoringStats) -> Option<Score> {
        let s1 = self.first.score(stats);
        let s2 = self.second.score(stats);

        match (s1, s2) {
            (Some(a), Some(b)) => Some(a * self.first_weight + b * self.second_weight),
            (Some(a), None) => Some(a * self.first_weight),
            (None, Some(b)) => Some(b * self.second_weight),
            (None, None) => None,
        }
    }

    fn name(&self) -> &'static str {
        "combined"
    }
}

// ============================================================================
// Scoring Stats
// ============================================================================

/// Statistics needed for scoring a document.
///
/// For single-field scorers like BM25, the top-level `term_frequency` and
/// `doc_length` fields are sufficient. For multi-field scorers like BM25F,
/// per-field breakdowns are carried in `field_stats`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScoringStats {
    /// Term frequency in this document.
    pub term_frequency: u32,
    /// Length of this document (in tokens).
    pub doc_length: u32,
    /// Average document length in the collection.
    pub avg_doc_length: f32,
    /// Total number of documents in the collection.
    pub doc_count: u32,
    /// Number of documents containing this term (document frequency).
    pub doc_frequency: u32,
    /// Per-field statistics for multi-field scorers.
    pub field_stats: Vec<FieldStats>,
}

impl ScoringStats {
    /// Create new scoring stats.
    pub const fn new() -> Self {
        Self {
            term_frequency: 0,
            doc_length: 0,
            avg_doc_length: 0.0,
            doc_count: 0,
            doc_frequency: 0,
            field_stats: Vec::new(),
        }
    }
}

// ============================================================================
// BM25
// ============================================================================

/// BM25 scoring parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bm25Params {
    /// Term frequency saturation parameter. Default: 1.2
    pub k1: f32,
    /// Document length normalization parameter. Default: 0.75
    pub b: f32,
}

impl Default for Bm25Params {
    fn default() -> Self {
        Self { k1: 1.2, b: 0.75 }
    }
}

impl Bm25Params {
    /// Create new BM25 parameters with default values.
    pub const fn new() -> Self {
        Self { k1: 1.2, b: 0.75 }
    }

    /// Set k1 parameter.
    ///
    /// # Panics
    ///
    /// Panics if `k1` is negative, NaN, or infinite.
    #[must_use = "this returns a new Bm25Params with the modified k1 value"]
    pub const fn with_k1(mut self, k1: f32) -> Self {
        assert!(k1.is_finite() && k1 >= 0.0, "k1 must be finite and >= 0.0");
        self.k1 = k1;
        self
    }

    /// Set b parameter.
    ///
    /// # Panics
    ///
    /// Panics if `b` is outside the range `[0.0, 1.0]` or non-finite.
    #[must_use = "this returns a new Bm25Params with the modified b value"]
    pub const fn with_b(mut self, b: f32) -> Self {
        assert!(
            b.is_finite() && b >= 0.0 && b <= 1.0,
            "b must be finite and in [0.0, 1.0]"
        );
        self.b = b;
        self
    }
}

/// BM25 scorer.
#[derive(Clone, Copy, Debug)]
pub struct Bm25Scorer {
    params: Bm25Params,
}

impl Bm25Scorer {
    /// Create a new BM25 scorer with default parameters.
    pub const fn new() -> Self {
        Self {
            params: Bm25Params::new(),
        }
    }

    /// Create a BM25 scorer with custom parameters.
    pub const fn with_params(params: Bm25Params) -> Self {
        Self { params }
    }

    /// Get the parameters.
    pub const fn params(&self) -> &Bm25Params {
        &self.params
    }
}

impl Bm25Scorer {
    /// Compute the BM25 score for a term in a document.
    ///
    /// BM25 formula:
    /// `score(D, Q) = Σ IDF(qi) * (f(qi, D) * (k1 + 1)) / (f(qi, D) + k1 * (1 - b + b * |D| / avgdl))`
    ///
    /// where:
    /// - `f(qi, D)` = term frequency of `qi` in `D`
    /// - `|D|` = length of document `D`
    /// - `avgdl` = average document length
    /// - `IDF(qi)` = `log((N - n(qi) + 0.5) / (n(qi) + 0.5) + 1)`
    /// - `N` = total number of documents
    /// - `n(qi)` = number of documents containing `qi`
    pub fn score(&self, stats: &ScoringStats) -> Score {
        if stats.term_frequency == 0
            || stats.doc_count == 0
            || stats.doc_frequency == 0
            || stats.doc_frequency > stats.doc_count
            || !stats.avg_doc_length.is_finite()
            || stats.avg_doc_length <= 0.0
        {
            return Score::ZERO;
        }

        let tf = stats.term_frequency as f32;
        let doc_len = stats.doc_length as f32;
        let avg_dl = stats.avg_doc_length;
        let n = stats.doc_count as f32;
        let df = stats.doc_frequency as f32;

        // IDF calculation using BM25+ variant (add 1 to avoid negative IDF)
        let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();

        // Document length normalization factor
        let dl_norm = 1.0 - self.params.b + self.params.b * (doc_len / avg_dl);

        // TF saturation
        let tf_sat = (tf * (self.params.k1 + 1.0)) / (tf + self.params.k1 * dl_norm);

        let score = idf * tf_sat;
        Score::from_arithmetic_result(score)
    }
}

impl Default for Bm25Scorer {
    fn default() -> Self {
        Self::new()
    }
}

impl Scorer for Bm25Scorer {
    fn score(&self, stats: &ScoringStats) -> Option<Score> {
        let score = self.score(stats);
        if score == Score::ZERO {
            None
        } else {
            Some(score)
        }
    }

    fn name(&self) -> &'static str {
        "bm25"
    }

    fn needs_field_stats(&self) -> bool {
        false
    }
}

// ============================================================================
// BM25F (Multi-field)
// ============================================================================

/// Field-specific scoring information.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FieldStats {
    /// Field identifier.
    pub field_id: leit_core::FieldId,
    /// Term frequency in this field.
    pub term_frequency: u32,
    /// Length of this field.
    pub field_length: u32,
    /// Weight for this field.
    pub weight: f32,
}

/// BM25F scorer for multi-field documents.
#[derive(Clone, Copy, Debug)]
pub struct Bm25FScorer {
    params: Bm25Params,
}

impl Bm25FScorer {
    /// Create a new BM25F scorer with default parameters.
    pub const fn new() -> Self {
        Self {
            params: Bm25Params::new(),
        }
    }

    /// Create a BM25F scorer with custom parameters.
    pub const fn with_params(params: Bm25Params) -> Self {
        Self { params }
    }
}

impl Bm25FScorer {
    /// Compute the BM25F score across multiple fields.
    ///
    /// BM25F extends BM25 to handle multi-field documents by:
    /// 1. Computing a weighted term frequency across fields
    /// 2. Using the sum of weighted field lengths as document length
    /// 3. Applying BM25 to the aggregated values
    pub fn score(
        &self,
        fields: &[FieldStats],
        avg_doc_length: f32,
        doc_count: u32,
        doc_frequency: u32,
    ) -> Score {
        if doc_count == 0
            || doc_frequency == 0
            || doc_frequency > doc_count
            || !avg_doc_length.is_finite()
            || avg_doc_length <= 0.0
        {
            return Score::ZERO;
        }

        // Aggregate weighted TF and document length
        let mut weighted_tf = 0.0_f32;
        let mut weighted_doc_len = 0.0_f32;

        for field in fields {
            let tf = field.term_frequency as f32;
            let fl = field.field_length as f32;
            weighted_tf += tf * field.weight;
            weighted_doc_len += fl * field.weight;
        }

        if weighted_tf == 0.0 {
            return Score::ZERO;
        }

        let n = doc_count as f32;
        let df = doc_frequency as f32;

        // IDF
        let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();

        // Document length normalization
        let dl_norm = 1.0 - self.params.b + self.params.b * (weighted_doc_len / avg_doc_length);

        // TF saturation
        let tf_sat =
            (weighted_tf * (self.params.k1 + 1.0)) / (weighted_tf + self.params.k1 * dl_norm);

        let score = idf * tf_sat;
        Score::from_arithmetic_result(score)
    }
}

impl Default for Bm25FScorer {
    fn default() -> Self {
        Self::new()
    }
}

impl Scorer for Bm25FScorer {
    fn score(&self, stats: &ScoringStats) -> Option<Score> {
        if stats.field_stats.is_empty() {
            return None;
        }
        let score = self.score(
            &stats.field_stats,
            stats.avg_doc_length,
            stats.doc_count,
            stats.doc_frequency,
        );
        if score == Score::ZERO {
            None
        } else {
            Some(score)
        }
    }

    fn name(&self) -> &'static str {
        "bm25f"
    }

    fn needs_field_stats(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bm25_basic() {
        let scorer = Bm25Scorer::new();
        let stats = ScoringStats {
            term_frequency: 2,
            doc_length: 100,
            avg_doc_length: 150.0,
            doc_count: 1000,
            doc_frequency: 50,
            ..ScoringStats::new()
        };

        let score = scorer.score(&stats);
        assert!(score > Score::ZERO);
    }

    #[test]
    fn test_bm25_zero_tf() {
        let scorer = Bm25Scorer::new();
        let stats = ScoringStats {
            term_frequency: 0,
            doc_length: 100,
            avg_doc_length: 150.0,
            doc_count: 1000,
            doc_frequency: 50,
            ..ScoringStats::new()
        };

        let score = scorer.score(&stats);
        assert_eq!(score, Score::ZERO);
    }

    #[test]
    fn test_bm25f_basic() {
        let scorer = Bm25FScorer::new();
        let fields = [
            FieldStats {
                field_id: leit_core::FieldId::new(0),
                term_frequency: 2,
                field_length: 50,
                weight: 2.0,
            },
            FieldStats {
                field_id: leit_core::FieldId::new(1),
                term_frequency: 1,
                field_length: 100,
                weight: 1.0,
            },
        ];

        let score = scorer.score(&fields, 150.0, 1000, 50);
        assert!(score > Score::ZERO);
    }

    #[test]
    fn test_bm25_large_scores_are_not_clamped() {
        let scorer = Bm25Scorer::new();
        let stats = ScoringStats {
            term_frequency: 20,
            doc_length: 50,
            avg_doc_length: 100.0,
            doc_count: 10_000,
            doc_frequency: 1,
            ..ScoringStats::new()
        };

        let score = scorer.score(&stats);
        assert!(score.as_f32() > 1.0_f32);
    }

    #[test]
    fn test_bm25_zero_avg_doc_length_returns_zero() {
        let scorer = Bm25Scorer::new();
        let stats = ScoringStats {
            term_frequency: 3,
            doc_length: 100,
            avg_doc_length: 0.0,
            doc_count: 1000,
            doc_frequency: 50,
            ..ScoringStats::new()
        };

        let score = scorer.score(&stats);
        assert_eq!(score, Score::ZERO);
    }

    #[test]
    fn test_bm25f_zero_avg_doc_length_returns_zero() {
        let scorer = Bm25FScorer::new();
        let fields = [FieldStats {
            field_id: leit_core::FieldId::new(0),
            term_frequency: 2,
            field_length: 50,
            weight: 1.0,
        }];

        let score = scorer.score(&fields, 0.0, 1000, 50);
        assert_eq!(score, Score::ZERO);
    }
}
