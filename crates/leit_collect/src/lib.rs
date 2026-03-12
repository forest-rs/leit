//! Result collection for Leit retrieval system.
//!
//! This crate provides collectors for gathering search results,
//! including top-k collection with threshold reporting for pruning-aware executors.

#![no_std]

extern crate alloc;

use alloc::collections::BinaryHeap;
use alloc::vec::Vec;
use core::cmp::Ordering;
use core::mem;
use leit_core::{EntityId, Score, ScoredHit};

// ============================================================================
// Collector Trait
// ============================================================================

/// Trait for collecting search results.
pub trait Collector<Id: EntityId> {
    /// Final output produced for one query.
    type Output;

    /// Prepare the collector for a new query.
    fn begin_query(&mut self);

    /// Collect a hit.
    fn collect(&mut self, hit: ScoredHit<Id>);

    /// Return the current competitive threshold for this query, if any.
    ///
    /// Execution may skip a candidate or block when it has an exact score or a
    /// sound upper bound that is strictly less than this value.
    ///
    /// Equal scores remain competitive because collectors may apply an
    /// additional deterministic tie-break after comparing scores.
    fn threshold(&self) -> Option<Score>;

    /// Finalize the current query and return its output.
    fn finish(&mut self) -> Self::Output;

    /// Number of hits collected.
    fn len(&self) -> usize;

    /// Check if a candidate with this exact score can be skipped.
    fn can_skip(&self, score: Score) -> bool {
        self.threshold().is_some_and(|threshold| score < threshold)
    }

    /// Check if no hits have been collected.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// ============================================================================
// TopKCollector
// ============================================================================

/// A collector that maintains the top-k hits by score.
///
/// Uses a min-heap internally, so the smallest score is at the top.
/// This allows efficient threshold maintenance for pruning-aware executors.
#[derive(Debug)]
pub struct TopKCollector<Id: EntityId> {
    heap: BinaryHeap<ReverseHit<Id>>,
    k: usize,
    min_score: Score,
}

impl<Id: EntityId> TopKCollector<Id> {
    /// Create a new top-k collector.
    #[must_use]
    pub const fn new(k: usize) -> Self {
        Self {
            heap: BinaryHeap::new(),
            k,
            min_score: Score::MIN,
        }
    }

    /// Get the current minimum score in the top-k.
    /// Returns `Score::MIN` if fewer than k hits have been collected.
    #[must_use]
    pub const fn min_score(&self) -> Score {
        self.min_score
    }

    /// Finalize the collection and return the hits in descending score order.
    #[must_use]
    pub fn into_sorted_vec(self) -> Vec<ScoredHit<Id>> {
        Self::sorted_hits_from_heap(self.heap)
    }
}

impl<Id: EntityId> Collector<Id> for TopKCollector<Id> {
    type Output = Vec<ScoredHit<Id>>;

    fn begin_query(&mut self) {
        self.heap.clear();
        self.min_score = Score::MIN;
    }

    fn collect(&mut self, hit: ScoredHit<Id>) {
        if self.heap.len() < self.k {
            // Not full yet, always add
            self.heap.push(ReverseHit(hit));
            self.update_min_score();
        } else if let Some(top) = self.heap.peek() {
            // Full: only add if the hit outranks the current minimum.
            if hit > top.0 {
                self.heap.pop();
                self.heap.push(ReverseHit(hit));
                self.update_min_score();
            }
        }
    }

    fn threshold(&self) -> Option<Score> {
        (self.heap.len() >= self.k).then_some(self.min_score)
    }

    fn finish(&mut self) -> Self::Output {
        let heap = mem::take(&mut self.heap);
        self.min_score = Score::MIN;
        Self::sorted_hits_from_heap(heap)
    }

    fn len(&self) -> usize {
        self.heap.len()
    }
}

impl<Id: EntityId> TopKCollector<Id> {
    fn sorted_hits_from_heap(heap: BinaryHeap<ReverseHit<Id>>) -> Vec<ScoredHit<Id>> {
        let mut hits: Vec<_> = heap.into_iter().map(|rh| rh.0).collect();
        hits.sort_by(|a, b| b.cmp(a));
        hits
    }

    fn update_min_score(&mut self) {
        if let Some(top) = self.heap.peek() {
            self.min_score = top.0.score;
        }
    }
}

/// A wrapper to reverse the ordering for min-heap behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ReverseHit<Id: EntityId>(ScoredHit<Id>);

impl<Id: EntityId> PartialOrd for ReverseHit<Id> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<Id: EntityId> Ord for ReverseHit<Id> {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering: larger scores compare as "smaller" so they stay at bottom of heap
        // This gives us min-heap behavior where smallest score is at top
        other
            .0
            .score
            .partial_cmp(&self.0.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| other.0.id.cmp(&self.0.id))
    }
}

// ============================================================================
// CountCollector
// ============================================================================

/// A simple collector that just counts hits.
#[derive(Clone, Copy, Debug, Default)]
pub struct CountCollector {
    count: usize,
}

impl CountCollector {
    /// Create a new count collector.
    #[must_use]
    pub const fn new() -> Self {
        Self { count: 0 }
    }

    /// Get the count of hits.
    #[must_use]
    pub const fn count(&self) -> usize {
        self.count
    }
}

impl<Id: EntityId> Collector<Id> for CountCollector {
    type Output = usize;

    fn begin_query(&mut self) {
        self.count = 0;
    }

    fn collect(&mut self, _hit: ScoredHit<Id>) {
        self.count = self.count.saturating_add(1);
    }

    fn threshold(&self) -> Option<Score> {
        None
    }

    fn finish(&mut self) -> Self::Output {
        let count = self.count;
        self.count = 0;
        count
    }

    fn len(&self) -> usize {
        self.count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topk_basic() {
        let mut collector = TopKCollector::<u32>::new(3);
        collector.begin_query();

        collector.collect(ScoredHit::new(1, Score::new(0.5)));
        collector.collect(ScoredHit::new(2, Score::new(0.8)));
        collector.collect(ScoredHit::new(3, Score::new(0.3)));
        collector.collect(ScoredHit::new(4, Score::new(0.9)));
        collector.collect(ScoredHit::new(5, Score::new(0.1)));

        let hits = collector.finish();
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[0].id, 4); // score 0.9
        assert_eq!(hits[1].id, 2); // score 0.8
        assert_eq!(hits[2].id, 1); // score 0.5
    }

    #[test]
    fn test_can_skip() {
        let mut collector = TopKCollector::<u32>::new(2);
        collector.begin_query();

        // Not full yet
        assert!(!collector.can_skip(Score::ZERO));

        collector.collect(ScoredHit::new(1, Score::new(0.5)));
        collector.collect(ScoredHit::new(2, Score::new(0.8)));

        // Now full, min score is 0.5
        assert!(collector.can_skip(Score::new(0.3)));
        assert!(!collector.can_skip(Score::new(0.5)));
        assert!(!collector.can_skip(Score::new(0.6)));
    }

    #[test]
    fn test_count_collector() {
        let mut collector = CountCollector::new();
        <CountCollector as Collector<u32>>::begin_query(&mut collector);

        collector.collect(ScoredHit::new(1u32, Score::ONE));
        collector.collect(ScoredHit::new(2u32, Score::ONE));
        collector.collect(ScoredHit::new(3u32, Score::ONE));

        assert_eq!(collector.count(), 3);
        assert_eq!(
            <CountCollector as Collector<u32>>::finish(&mut collector),
            3
        );
    }

    #[test]
    fn test_topk_keeps_higher_id_when_lowest_scores_tie() {
        let mut collector = TopKCollector::<u32>::new(2);
        collector.begin_query();

        collector.collect(ScoredHit::new(0, Score::new(-84.97)));
        collector.collect(ScoredHit::new(0, Score::new(-0.47)));
        collector.collect(ScoredHit::new(1_134_700_433, Score::new(-84.97)));

        let hits = collector.finish();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0], ScoredHit::new(0, Score::new(-0.47)));
        assert_eq!(hits[1], ScoredHit::new(1_134_700_433, Score::new(-84.97)));
    }
}
