// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Contract tests for deterministic merge-candidate selection.

use leit_index::{SegmentSummary, select_merge_candidates};

fn summary(ordinal: usize, level: u32, size_bytes: u64) -> SegmentSummary {
    SegmentSummary {
        ordinal,
        level,
        size_bytes,
    }
}

#[test]
fn selects_lowest_level_with_at_least_two_segments() {
    let summaries = [
        summary(0, 0, 100),
        summary(1, 1, 20),
        summary(2, 1, 10),
        summary(3, 2, 1),
        summary(4, 2, 2),
    ];

    assert_eq!(select_merge_candidates(&summaries), Some(vec![2, 1]));
}

#[test]
fn orders_equal_sizes_by_ordinal() {
    let summaries = [
        summary(9, 3, 10),
        summary(2, 3, 10),
        summary(7, 3, 10),
        summary(1, 3, 10),
    ];

    assert_eq!(select_merge_candidates(&summaries), Some(vec![1, 2, 7, 9]));
}

#[test]
fn limits_fan_in_to_four_smallest_segments() {
    let summaries = [
        summary(0, 4, 50),
        summary(1, 4, 10),
        summary(2, 4, 40),
        summary(3, 4, 20),
        summary(4, 4, 30),
    ];

    assert_eq!(select_merge_candidates(&summaries), Some(vec![1, 3, 4, 2]));
}

#[test]
fn requires_at_least_two_segments_at_one_level() {
    assert_eq!(select_merge_candidates(&[]), None);
    assert_eq!(select_merge_candidates(&[summary(0, 0, 1)]), None);
    assert_eq!(
        select_merge_candidates(&[summary(0, 0, 1), summary(1, 1, 1)]),
        None
    );
}

#[test]
fn repeated_selection_is_deterministic() {
    let summaries = [
        summary(8, 2, 20),
        summary(3, 2, 10),
        summary(5, 2, 10),
        summary(1, 2, 30),
    ];
    let expected = Some(vec![3, 5, 8, 1]);

    for _ in 0..16 {
        assert_eq!(select_merge_candidates(&summaries), expected);
    }
}
