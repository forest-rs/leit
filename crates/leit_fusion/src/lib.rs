// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

#![no_std]

//! RRF (Reciprocal Rank Fusion) implementation for combining multiple search result rankings.
//!
//! This module provides a simple, effective algorithm for fusing ranked lists from
//! different search sources or relevance scoring methods.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::cmp::Ordering;

/// Default constant k parameter for RRF scoring.
/// This constant prevents rankings from dominating too heavily.
const DEFAULT_K: f64 = 60.0;

/// Fusion strategy parameters.
#[derive(Debug, Clone)]
pub struct FusionConfig {
    /// The RRF constant k (default: 60.0)
    /// Higher values give more weight to lower-ranked items
    pub k: f64,
}

impl Default for FusionConfig {
    fn default() -> Self {
        Self { k: DEFAULT_K }
    }
}

impl FusionConfig {
    /// Create a new fusion config with a custom k parameter.
    pub fn new(k: f64) -> Self {
        assert!(
            k.is_finite() && k > 0.0,
            "rrf k must be finite and positive"
        );
        Self { k }
    }

    /// Create a config with the default k parameter.
    pub fn default_config() -> Self {
        Self::default()
    }
}

/// Represents a search result from a single source.
#[derive(Debug, Clone, PartialEq)]
pub struct RankedResult {
    /// The unique identifier for this result (e.g., document ID, file path)
    pub id: String,
    /// The rank in the original source (1-indexed)
    pub rank: usize,
}

impl RankedResult {
    /// Create a new ranked result.
    pub fn new(id: impl Into<String>, rank: usize) -> Self {
        assert!(rank > 0, "rank must be at least 1");
        Self {
            id: id.into(),
            rank,
        }
    }
}

/// Represents the fused result with its combined score.
#[derive(Debug, Clone, PartialEq)]
#[must_use]
pub struct FusedResult {
    /// The unique identifier for this result
    pub id: String,
    /// The combined RRF score
    pub score: f64,
    /// The final rank after fusion (1-indexed)
    pub rank: usize,
}

/// Fuses multiple ranked lists using Reciprocal Rank Fusion (RRF).
///
/// # Algorithm
///
/// For each item that appears in any of the ranked lists, RRF calculates:
///
/// ```text
/// score(item) = Σ 1 / (k + rank(item))
/// ```
///
/// Where:
/// - `k` is a constant (default: 60.0)
/// - `rank(item)` is the position of the item in a single list (1-indexed)
/// - The sum is taken over all lists where the item appears
///
/// Items are then re-ranked by their combined score in descending order.
///
/// # Arguments
///
/// * `ranked_lists` - A slice of ranked result lists to fuse
/// * `config` - Optional fusion configuration (uses default if None)
///
/// # Returns
///
/// A vector of fused results sorted by score (highest first).
///
/// # Example
///
/// ```
/// use leit_fusion::{fuse, RankedResult, FusionConfig};
///
/// // Two search engines return different rankings
/// let engine1 = vec![
///     RankedResult::new("doc1", 1),
///     RankedResult::new("doc2", 2),
///     RankedResult::new("doc3", 3),
/// ];
///
/// let engine2 = vec![
///     RankedResult::new("doc2", 1),
///     RankedResult::new("doc4", 2),
///     RankedResult::new("doc1", 3),
/// ];
///
/// let fused = fuse(&[engine1, engine2], None);
///
/// // doc2 appears at rank 1 in engine2 and rank 2 in engine1, so it scores highly
/// assert_eq!(fused[0].id, "doc2");
/// ```
pub fn fuse(ranked_lists: &[Vec<RankedResult>], config: Option<FusionConfig>) -> Vec<FusedResult> {
    let config = config.unwrap_or_default();
    let mut ranks_by_id: BTreeMap<String, Vec<usize>> = BTreeMap::new();

    // Collect ranks first so floating-point accumulation order is deterministic.
    for list in ranked_lists {
        for result in list {
            ranks_by_id
                .entry(result.id.clone())
                .or_default()
                .push(result.rank);
        }
    }

    let mut scores: BTreeMap<String, (f64, usize)> = BTreeMap::new();
    for (id, mut ranks) in ranks_by_id {
        ranks.sort_unstable();
        let min_rank = ranks.first().copied().unwrap_or(usize::MAX);
        let score = ranks
            .into_iter()
            .fold(0.0, |acc, rank| acc + 1.0 / (config.k + usize_to_f64(rank)));
        scores.insert(id, (score, min_rank));
    }

    // Convert to results and sort by score (descending), then min_rank (ascending), then id (descending for determinism)
    let mut fused_results: Vec<(String, f64, usize)> = scores
        .into_iter()
        .map(|(id, (score, min_rank))| (id, score, min_rank))
        .collect();

    fused_results.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.2.cmp(&b.2))
            .then_with(|| b.0.cmp(&a.0))
    });

    // Convert to FusedResult and assign final ranks
    let mut results: Vec<FusedResult> = fused_results
        .into_iter()
        .map(|(id, score, _min_rank)| FusedResult {
            id,
            score,
            rank: 0, // Will be set after sorting
        })
        .collect();

    // Assign final ranks
    for (i, result) in results.iter_mut().enumerate() {
        result.rank = i.checked_add(1).expect("rank overflow");
    }

    results
}

/// Convenience function to fuse ranked lists with default configuration.
///
/// # Arguments
///
/// * `ranked_lists` - A slice of ranked result lists to fuse
///
/// # Returns
///
/// A vector of fused results sorted by score (highest first).
pub fn fuse_default(ranked_lists: &[Vec<RankedResult>]) -> Vec<FusedResult> {
    fuse(ranked_lists, None)
}

const fn usize_to_f64(value: usize) -> f64 {
    value as f64
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    #[test]
    fn test_basic_fusion() {
        let list1 = vec![
            RankedResult::new("a", 1),
            RankedResult::new("b", 2),
            RankedResult::new("c", 3),
        ];
        let list2 = vec![
            RankedResult::new("b", 1),
            RankedResult::new("a", 2),
            RankedResult::new("d", 3),
        ];

        let fused = fuse_default(&[list1, list2]);

        // 'b' appears at rank 1 in list2 and rank 2 in list1 - highest score
        // 'a' appears at rank 1 in list1 and rank 2 in list2 - second highest
        // 'c' appears only at rank 3 in list1
        // 'd' appears only at rank 3 in list2
        assert_eq!(fused[0].id, "b");
        assert_eq!(fused[1].id, "a");
        assert_eq!(fused.len(), 4);
    }

    #[test]
    fn test_custom_k() {
        let list1 = vec![RankedResult::new("a", 1)];
        let list2 = vec![RankedResult::new("a", 10)];

        // With a very small positive k, ranks dominate more strongly.
        let config_low_k = FusionConfig::new(1.0);
        let fused_low = fuse(&[list1.clone(), list2.clone()], Some(config_low_k));

        // With k=100, ranks matter less
        let config_high_k = FusionConfig::new(100.0);
        let fused_high = fuse(&[list1, list2], Some(config_high_k));

        // Both should have 'a' as the only result
        assert_eq!(fused_low.len(), 1);
        assert_eq!(fused_high.len(), 1);
        assert_eq!(fused_low[0].id, "a");
        assert_eq!(fused_high[0].id, "a");
    }

    #[test]
    fn test_empty_lists() {
        let fused = fuse_default(&[]);
        assert_eq!(fused.len(), 0);
    }

    #[test]
    fn test_single_list() {
        let list = vec![
            RankedResult::new("a", 1),
            RankedResult::new("b", 2),
            RankedResult::new("c", 3),
        ];

        let fused = fuse_default(&[list]);

        assert_eq!(fused.len(), 3);
        assert_eq!(fused[0].id, "a");
        assert_eq!(fused[0].rank, 1);
        assert_eq!(fused[1].id, "b");
        assert_eq!(fused[1].rank, 2);
        assert_eq!(fused[2].id, "c");
        assert_eq!(fused[2].rank, 3);
    }

    #[test]
    fn test_rank_assignment() {
        let list1 = vec![RankedResult::new("a", 1), RankedResult::new("b", 2)];
        let list2 = vec![RankedResult::new("c", 1)];

        let fused = fuse_default(&[list1, list2]);

        // Check that ranks are properly assigned (1-indexed)
        let ranks: Vec<usize> = fused.iter().map(|r| r.rank).collect();
        assert_eq!(ranks, vec![1, 2, 3]);
    }

    #[test]
    fn test_no_overlap() {
        let list1 = vec![RankedResult::new("a", 1), RankedResult::new("b", 2)];
        let list2 = vec![RankedResult::new("c", 1), RankedResult::new("d", 2)];

        let fused = fuse_default(&[list1, list2]);

        // No overlap, so items from each list are ranked by their position
        assert_eq!(fused.len(), 4);
    }

    #[test]
    fn test_fusion_config_default() {
        let config = FusionConfig::default();
        assert_f64_eq(config.k, DEFAULT_K);
    }

    #[test]
    fn test_fusion_config_new() {
        let config = FusionConfig::new(42.0);
        assert_f64_eq(config.k, 42.0);
    }

    #[test]
    #[should_panic(expected = "rank must be at least 1")]
    fn test_ranked_result_rejects_zero_rank() {
        let _ = RankedResult::new("bad", 0);
    }

    #[test]
    #[should_panic(expected = "rrf k must be finite and positive")]
    fn test_fusion_config_rejects_zero_k() {
        let _ = FusionConfig::new(0.0);
    }

    #[test]
    #[should_panic(expected = "rrf k must be finite and positive")]
    fn test_fusion_config_rejects_negative_k() {
        let _ = FusionConfig::new(-1.0);
    }

    fn assert_f64_eq(actual: f64, expected: f64) {
        let delta = (actual - expected).abs();
        assert!(delta <= f64::EPSILON, "expected {expected}, got {actual}");
    }
}
