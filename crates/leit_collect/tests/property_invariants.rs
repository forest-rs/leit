// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Property-based invariant tests for `leit_collect`.

use leit_collect::{Collector, TopKCollector};
use leit_core::{Score, ScoredHit};
use proptest::collection::vec;
use proptest::prelude::*;

fn expected_top_k(mut hits: Vec<ScoredHit<u32>>, k: usize) -> Vec<ScoredHit<u32>> {
    hits.sort_by(|a, b| b.cmp(a));
    hits.truncate(k);
    hits
}

proptest! {
    #[test]
    fn topk_matches_full_sort_for_arbitrary_hits(
        entries in vec((any::<u32>(), -10_000_i16..10_000_i16), 0..64),
        k in 0_usize..32_usize,
    ) {
        let hits: Vec<_> = entries
            .into_iter()
            .map(|(id, raw_score)| ScoredHit::new(id, Score::new(f32::from(raw_score) / 100.0)))
            .collect();

        let mut collector = TopKCollector::new(k);
        collector.begin_query();
        for hit in &hits {
            collector.collect_scored(*hit);
        }

        let collected = collector.finish();
        let expected = expected_top_k(hits, k);

        prop_assert_eq!(collected, expected);
    }

    #[test]
    fn topk_min_score_is_monotonic_after_capacity_reached(
        entries in vec((any::<u32>(), -10_000_i16..10_000_i16), 1..64),
        k in 1_usize..16_usize,
    ) {
        let hits: Vec<_> = entries
            .into_iter()
            .map(|(id, raw_score)| ScoredHit::new(id, Score::new(f32::from(raw_score) / 100.0)))
            .collect();

        let mut collector = TopKCollector::new(k);
        collector.begin_query();
        let mut prior_min = Score::MIN;

        for hit in hits {
            collector.collect_scored(hit);
            if collector.len() >= k {
                let current_min = collector.min_score();
                prop_assert!(current_min >= prior_min);
                prior_min = current_min;
            }
        }
    }
}

#[test]
fn topk_replaces_with_better_tie_break_on_equal_score() {
    let mut collector = TopKCollector::<u32>::new(1);
    collector.begin_query();
    collector.collect_scored(ScoredHit::new(10, Score::new(1.0)));
    collector.collect_scored(ScoredHit::new(1, Score::new(1.0)));

    let hits = collector.finish();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0], ScoredHit::new(10, Score::new(1.0)));
}

#[test]
fn collector_reuse_clears_previous_query_state() {
    let mut collector = TopKCollector::<u32>::new(2);

    collector.begin_query();
    collector.collect_scored(ScoredHit::new(7, Score::new(0.7)));
    collector.collect_scored(ScoredHit::new(9, Score::new(0.9)));
    let first_hits = collector.finish();
    assert_eq!(
        first_hits,
        vec![
            ScoredHit::new(9, Score::new(0.9)),
            ScoredHit::new(7, Score::new(0.7))
        ]
    );

    collector.begin_query();
    assert!(collector.finish().is_empty());
    assert_eq!(collector.min_competitive_score(), None);

    collector.collect_scored(ScoredHit::new(3, Score::new(0.3)));
    let second_hits = collector.finish();
    assert_eq!(second_hits, vec![ScoredHit::new(3, Score::new(0.3))]);
}

#[test]
fn threshold_is_absent_until_topk_reaches_capacity() {
    let mut collector = TopKCollector::<u32>::new(2);

    collector.begin_query();
    assert_eq!(collector.min_competitive_score(), None);
    assert!(!collector.can_skip(Score::new(0.1)));

    collector.collect_scored(ScoredHit::new(1, Score::new(0.5)));
    assert_eq!(collector.min_competitive_score(), None);
    assert!(!collector.can_skip(Score::new(0.4)));

    collector.collect_scored(ScoredHit::new(2, Score::new(0.8)));
    assert_eq!(collector.min_competitive_score(), Some(Score::new(0.5)));
    assert!(!collector.can_skip(Score::new(0.5)));
    assert!(!collector.can_skip(Score::new(0.6)));
    assert!(collector.can_skip(Score::new(0.4)));
}
