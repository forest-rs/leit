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
        entries in vec((any::<u32>(), -10_000i16..10_000i16), 0..64),
        k in 0usize..32usize,
    ) {
        let hits: Vec<_> = entries
            .into_iter()
            .map(|(id, raw_score)| ScoredHit::new(id, Score::new(f32::from(raw_score) / 100.0)))
            .collect();

        let mut collector = TopKCollector::new(k);
        for hit in &hits {
            collector.collect(*hit);
        }

        let collected = collector.into_sorted_vec();
        let expected = expected_top_k(hits, k);

        prop_assert_eq!(collected, expected);
    }

    #[test]
    fn topk_min_score_is_monotonic_after_capacity_reached(
        entries in vec((any::<u32>(), -10_000i16..10_000i16), 1..64),
        k in 1usize..16usize,
    ) {
        let hits: Vec<_> = entries
            .into_iter()
            .map(|(id, raw_score)| ScoredHit::new(id, Score::new(f32::from(raw_score) / 100.0)))
            .collect();

        let mut collector = TopKCollector::new(k);
        let mut prior_min = Score::MIN;

        for hit in hits {
            collector.collect(hit);
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
    collector.collect(ScoredHit::new(10, Score::new(1.0)));
    collector.collect(ScoredHit::new(1, Score::new(1.0)));

    let hits = collector.into_sorted_vec();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0], ScoredHit::new(10, Score::new(1.0)));
}
