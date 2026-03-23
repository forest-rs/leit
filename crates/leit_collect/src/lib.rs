// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

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

/// Trait for collecting search results.
pub trait Collector<Id: EntityId> {
    /// Prepare the collector for a new query.
    fn begin_query(&mut self);

    /// Whether this collector needs scores to produce its output.
    fn needs_scores(&self) -> bool {
        false
    }

    /// Whether this collector must observe every matching document.
    fn requires_exhaustive_matches(&self) -> bool {
        true
    }

    /// Collect a matching document without a score.
    fn collect_doc(&mut self, doc: Id);

    /// Collect a scored hit.
    ///
    /// The default implementation drops the score and forwards to `collect_doc`.
    fn collect_scored(&mut self, hit: ScoredHit<Id>) {
        self.collect_doc(hit.id);
    }

    /// Return the current competitive threshold for this query, if any.
    fn min_competitive_score(&self) -> Option<Score> {
        None
    }

    /// Check if a hit with this exact score can be skipped.
    fn can_skip(&self, score: Score) -> bool {
        self.min_competitive_score()
            .is_some_and(|threshold| score < threshold)
    }
}

/// Build a grouped collector array without repeating the full trait-object type.
#[must_use]
pub fn collectors<Id: EntityId, const N: usize>(
    collectors: [&mut dyn Collector<Id>; N],
) -> [&mut dyn Collector<Id>; N] {
    collectors
}

impl<Id: EntityId> Collector<Id> for [&mut dyn Collector<Id>] {
    fn begin_query(&mut self) {
        for collector in self.iter_mut() {
            collector.begin_query();
        }
    }

    fn needs_scores(&self) -> bool {
        self.iter().any(|collector| collector.needs_scores())
    }

    fn requires_exhaustive_matches(&self) -> bool {
        self.iter()
            .any(|collector| collector.requires_exhaustive_matches())
    }

    fn collect_doc(&mut self, doc: Id) {
        for collector in self.iter_mut() {
            collector.collect_doc(doc);
        }
    }

    fn collect_scored(&mut self, hit: ScoredHit<Id>) {
        for collector in self.iter_mut() {
            collector.collect_scored(hit);
        }
    }

    fn min_competitive_score(&self) -> Option<Score> {
        aggregate_min_competitive_score(self)
    }
}

impl<Id: EntityId, const N: usize> Collector<Id> for [&mut dyn Collector<Id>; N] {
    fn begin_query(&mut self) {
        <[&mut dyn Collector<Id>] as Collector<Id>>::begin_query(&mut self[..]);
    }

    fn needs_scores(&self) -> bool {
        <[&mut dyn Collector<Id>] as Collector<Id>>::needs_scores(&self[..])
    }

    fn requires_exhaustive_matches(&self) -> bool {
        <[&mut dyn Collector<Id>] as Collector<Id>>::requires_exhaustive_matches(&self[..])
    }

    fn collect_doc(&mut self, doc: Id) {
        <[&mut dyn Collector<Id>] as Collector<Id>>::collect_doc(&mut self[..], doc);
    }

    fn collect_scored(&mut self, hit: ScoredHit<Id>) {
        <[&mut dyn Collector<Id>] as Collector<Id>>::collect_scored(&mut self[..], hit);
    }

    fn min_competitive_score(&self) -> Option<Score> {
        <[&mut dyn Collector<Id>] as Collector<Id>>::min_competitive_score(&self[..])
    }
}

fn aggregate_min_competitive_score<Id: EntityId>(
    collectors: &[&mut dyn Collector<Id>],
) -> Option<Score> {
    let mut threshold = None;
    for collector in collectors {
        if !collector.needs_scores() {
            continue;
        }

        let collector_threshold = collector.min_competitive_score()?;
        threshold = Some(match threshold {
            Some(current) if current <= collector_threshold => current,
            _ => collector_threshold,
        });
    }

    threshold
}

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

    /// Finalize the current query and return the hits in descending score order.
    pub fn finish(&mut self) -> Vec<ScoredHit<Id>> {
        let heap = mem::take(&mut self.heap);
        self.min_score = Score::MIN;
        Self::sorted_hits_from_heap(heap)
    }

    /// Number of retained hits for the current query.
    #[must_use]
    pub fn len(&self) -> usize {
        self.heap.len()
    }

    /// Check if the current query retained no hits.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    /// Check if a hit with this exact score can be skipped.
    #[must_use]
    pub fn can_skip(&self, score: Score) -> bool {
        <Self as Collector<Id>>::min_competitive_score(self)
            .is_some_and(|threshold| score < threshold)
    }
}

impl<Id: EntityId> Collector<Id> for TopKCollector<Id> {
    fn begin_query(&mut self) {
        self.heap.clear();
        self.min_score = Score::MIN;
    }

    fn needs_scores(&self) -> bool {
        true
    }

    fn requires_exhaustive_matches(&self) -> bool {
        false
    }

    fn collect_doc(&mut self, _doc: Id) {
        debug_assert!(
            false,
            "TopKCollector requires scored collection and cannot collect doc-only hits"
        );
    }

    fn collect_scored(&mut self, hit: ScoredHit<Id>) {
        if self.k == 0 {
            return;
        }
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

    fn min_competitive_score(&self) -> Option<Score> {
        if self.k == 0 {
            return Some(Score::MAX);
        }
        (self.heap.len() >= self.k).then_some(self.min_score)
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

    /// Check if the current query has collected no matches.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Finalize the current query and return the collected count.
    pub fn finish(&mut self) -> usize {
        let count = self.count;
        self.count = 0;
        count
    }
}

impl<Id: EntityId> Collector<Id> for CountCollector {
    fn begin_query(&mut self) {
        self.count = 0;
    }

    fn collect_doc(&mut self, _doc: Id) {
        self.count = self.count.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topk_basic() {
        let mut collector = TopKCollector::<u32>::new(3);
        <TopKCollector<u32> as Collector<u32>>::begin_query(&mut collector);

        <TopKCollector<u32> as Collector<u32>>::collect_scored(
            &mut collector,
            ScoredHit::new(1, Score::new(0.5)),
        );
        <TopKCollector<u32> as Collector<u32>>::collect_scored(
            &mut collector,
            ScoredHit::new(2, Score::new(0.8)),
        );
        <TopKCollector<u32> as Collector<u32>>::collect_scored(
            &mut collector,
            ScoredHit::new(3, Score::new(0.3)),
        );
        <TopKCollector<u32> as Collector<u32>>::collect_scored(
            &mut collector,
            ScoredHit::new(4, Score::new(0.9)),
        );
        <TopKCollector<u32> as Collector<u32>>::collect_scored(
            &mut collector,
            ScoredHit::new(5, Score::new(0.1)),
        );

        let hits = collector.finish();
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[0].id, 4); // score 0.9
        assert_eq!(hits[1].id, 2); // score 0.8
        assert_eq!(hits[2].id, 1); // score 0.5
    }

    #[test]
    fn test_can_skip() {
        let mut collector = TopKCollector::<u32>::new(2);
        <TopKCollector<u32> as Collector<u32>>::begin_query(&mut collector);

        // Not full yet
        assert_eq!(
            <TopKCollector<u32> as Collector<u32>>::min_competitive_score(&collector),
            None
        );

        <TopKCollector<u32> as Collector<u32>>::collect_scored(
            &mut collector,
            ScoredHit::new(1, Score::new(0.5)),
        );
        <TopKCollector<u32> as Collector<u32>>::collect_scored(
            &mut collector,
            ScoredHit::new(2, Score::new(0.8)),
        );

        // Now full, min score is 0.5
        assert_eq!(
            <TopKCollector<u32> as Collector<u32>>::min_competitive_score(&collector),
            Some(Score::new(0.5))
        );
    }

    #[test]
    fn test_count_collector() {
        let mut collector = CountCollector::new();
        <CountCollector as Collector<u32>>::begin_query(&mut collector);

        <CountCollector as Collector<u32>>::collect_doc(&mut collector, 1_u32);
        <CountCollector as Collector<u32>>::collect_doc(&mut collector, 2_u32);
        <CountCollector as Collector<u32>>::collect_doc(&mut collector, 3_u32);

        assert_eq!(collector.count(), 3);
        assert_eq!(collector.finish(), 3);
    }

    #[test]
    fn test_topk_keeps_higher_id_when_lowest_scores_tie() {
        let mut collector = TopKCollector::<u32>::new(2);
        <TopKCollector<u32> as Collector<u32>>::begin_query(&mut collector);

        <TopKCollector<u32> as Collector<u32>>::collect_scored(
            &mut collector,
            ScoredHit::new(0, Score::new(-84.97)),
        );
        <TopKCollector<u32> as Collector<u32>>::collect_scored(
            &mut collector,
            ScoredHit::new(0, Score::new(-0.47)),
        );
        <TopKCollector<u32> as Collector<u32>>::collect_scored(
            &mut collector,
            ScoredHit::new(1_134_700_433, Score::new(-84.97)),
        );

        let hits = collector.finish();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0], ScoredHit::new(0, Score::new(-0.47)));
        assert_eq!(hits[1], ScoredHit::new(1_134_700_433, Score::new(-84.97)));
    }
}
