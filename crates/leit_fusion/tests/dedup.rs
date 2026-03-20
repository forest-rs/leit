// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Per-list duplicate ID deduplication tests for `leit_fusion`.

use leit_fusion::{RankedResult, fuse_default};

#[test]
fn duplicate_ids_in_single_list_are_deduped_to_best_rank() {
    let list_with_dupes = vec![
        RankedResult::new("a", 1),
        RankedResult::new("b", 2),
        RankedResult::new("a", 3), // duplicate of "a"
    ];
    let clean_list = vec![RankedResult::new("b", 1), RankedResult::new("c", 2)];

    let fused = fuse_default(&[list_with_dupes, clean_list]);

    // "a" should only contribute ONE reciprocal-rank term from list 0 (rank 1),
    // not two terms (rank 1 + rank 3).
    let a_result = fused
        .iter()
        .find(|r| r.id == "a")
        .expect("a should be in results");
    let b_result = fused
        .iter()
        .find(|r| r.id == "b")
        .expect("b should be in results");

    // "a": 1/(60+1) from list 0 only = 1/61
    // "b": 1/(60+2) from list 0 + 1/(60+1) from list 1 = 1/62 + 1/61
    assert!(
        b_result.score > a_result.score,
        "b (in both lists) should outscore a (in one list only): b={}, a={}",
        b_result.score,
        a_result.score
    );
}

#[test]
fn all_duplicates_in_single_list_keeps_best_rank() {
    let list = vec![
        RankedResult::new("x", 5),
        RankedResult::new("x", 2),
        RankedResult::new("x", 8),
    ];

    let fused = fuse_default(&[list]);

    assert_eq!(fused.len(), 1);
    let expected = 1.0 / 62.0;
    let delta = (fused[0].score - expected).abs();
    assert!(delta < 1e-12, "expected {expected}, got {}", fused[0].score);
}
