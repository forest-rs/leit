// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Reusable top-k result-buffer ordering and capacity tests.

#![cfg(feature = "bench-internals")]

use leit_collect::{Collector, TopKCollector};
use leit_core::{Score, ScoredHit};

fn candidates() -> [ScoredHit<u32>; 7] {
    [
        ScoredHit::new(11, Score::new(0.5)),
        ScoredHit::new(4, Score::new(0.9)),
        ScoredHit::new(8, Score::new(0.9)),
        ScoredHit::new(3, Score::new(0.7)),
        ScoredHit::new(12, Score::new(0.2)),
        ScoredHit::new(6, Score::new(0.7)),
        ScoredHit::new(1, Score::new(0.1)),
    ]
}

fn collect_candidates(collector: &mut TopKCollector<u32>) {
    collector.begin_query();
    for hit in candidates() {
        collector.collect_scored(hit);
    }
}

fn expected_hits() -> Vec<ScoredHit<u32>> {
    vec![
        ScoredHit::new(8, Score::new(0.9)),
        ScoredHit::new(4, Score::new(0.9)),
        ScoredHit::new(6, Score::new(0.7)),
        ScoredHit::new(3, Score::new(0.7)),
    ]
}

#[test]
fn reusable_finish_preserves_existing_result_order_including_score_ties() {
    let mut allocating_collector = TopKCollector::new(4);
    let mut reusable_collector = TopKCollector::new(4);
    collect_candidates(&mut allocating_collector);
    collect_candidates(&mut reusable_collector);

    let expected = allocating_collector.finish();
    assert_eq!(expected, expected_hits());
    let mut actual = Vec::with_capacity(8);
    actual.push(ScoredHit::new(99, Score::new(99.0)));
    let capacity = actual.capacity();
    reusable_collector.finish_into(&mut actual);

    assert_eq!(actual, expected_hits());
    assert_eq!(actual, expected);
    assert_eq!(actual.len(), 4);
    assert_eq!(actual.capacity(), capacity);
}

#[test]
fn repeated_reusable_finishes_retain_heap_and_result_capacity() {
    let mut collector = TopKCollector::new(4);
    let mut output = Vec::with_capacity(8);
    output.push(ScoredHit::new(99, Score::new(99.0)));
    let expected = expected_hits();

    collect_candidates(&mut collector);
    assert_eq!(collector.len(), 4);
    let heap_capacity = collector.benchmark_heap_capacity();
    let output_capacity = output.capacity();
    assert!(heap_capacity > 0);
    assert!(output_capacity > 0);
    collector.finish_into(&mut output);

    assert_eq!(output, expected);
    assert_eq!(collector.min_score(), Score::MIN);
    assert_eq!(collector.benchmark_heap_capacity(), heap_capacity);
    assert_eq!(output.capacity(), output_capacity);

    collect_candidates(&mut collector);
    assert_eq!(collector.len(), 4);
    assert_eq!(collector.benchmark_heap_capacity(), heap_capacity);
    output.push(ScoredHit::new(100, Score::new(100.0)));
    collector.finish_into(&mut output);

    assert_eq!(output, expected);
    assert_eq!(collector.min_score(), Score::MIN);
    assert_eq!(collector.benchmark_heap_capacity(), heap_capacity);
    assert_eq!(output.capacity(), output_capacity);
}

#[test]
fn empty_query_finish_replaces_prior_results() {
    let mut collector = TopKCollector::<u32>::new(4);
    let mut output = Vec::with_capacity(4);
    output.push(ScoredHit::new(99, Score::new(99.0)));
    let capacity = output.capacity();

    collector.begin_query();
    collector.finish_into(&mut output);

    assert!(output.is_empty());
    assert_eq!(output.capacity(), capacity);
    assert_eq!(collector.min_score(), Score::MIN);
}

#[test]
fn zero_limit_finish_replaces_prior_results() {
    let mut collector = TopKCollector::<u32>::new(0);
    let mut output = Vec::with_capacity(4);
    output.push(ScoredHit::new(99, Score::new(99.0)));
    let capacity = output.capacity();

    collector.begin_query();
    for hit in candidates() {
        collector.collect_scored(hit);
    }
    collector.finish_into(&mut output);

    assert!(output.is_empty());
    assert_eq!(output.capacity(), capacity);
    assert_eq!(collector.min_score(), Score::MIN);
}
