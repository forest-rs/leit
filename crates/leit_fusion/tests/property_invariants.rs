//! Property-based invariant tests for `leit_fusion`.

use std::collections::BTreeSet;

use leit_fusion::{RankedResult, fuse};
use proptest::collection::vec;
use proptest::prelude::*;

fn materialize_ranked_lists(input: &[Vec<(u8, usize)>]) -> Vec<Vec<RankedResult>> {
    input
        .iter()
        .map(|list| {
            list.iter()
                .map(|(id, rank)| RankedResult::new(format!("doc-{id}"), *rank))
                .collect()
        })
        .collect()
}

proptest! {
    #[test]
    fn fused_results_have_contiguous_ranks_and_unique_ids(
        lists in vec(vec((any::<u8>(), 1usize..50usize), 0..20), 0..6)
    ) {
        let ranked_lists = materialize_ranked_lists(&lists);
        let fused = fuse(&ranked_lists, None);

        for (index, result) in fused.iter().enumerate() {
            prop_assert_eq!(result.rank, index.checked_add(1).expect("rank overflow"));
        }

        let unique_ids: BTreeSet<_> = fused.iter().map(|result| result.id.clone()).collect();
        prop_assert_eq!(unique_ids.len(), fused.len());
    }

    #[test]
    fn fusion_is_deterministic_for_identical_inputs(
        lists in vec(vec((any::<u8>(), 1usize..50usize), 0..20), 0..6)
    ) {
        let ranked_lists = materialize_ranked_lists(&lists);
        let fused_once = fuse(&ranked_lists, None);
        let fused_twice = fuse(&ranked_lists, None);

        prop_assert_eq!(fused_once, fused_twice);
    }

    #[test]
    fn fusion_is_invariant_under_input_list_reordering(
        lists in vec(vec((any::<u8>(), 1usize..50usize), 0..20), 0..6)
    ) {
        let ranked_lists = materialize_ranked_lists(&lists);
        let mut reversed_lists = ranked_lists.clone();
        reversed_lists.reverse();

        let fused = fuse(&ranked_lists, None);
        let reversed = fuse(&reversed_lists, None);

        prop_assert_eq!(fused, reversed);
    }

    #[test]
    fn adding_unique_document_does_not_change_other_scores(
        lists in vec(vec((1u8..32u8, 1usize..50usize), 0..20), 0..6),
        extra_rank in 1usize..50usize,
    ) {
        let ranked_lists = materialize_ranked_lists(&lists);
        let fused_before = fuse(&ranked_lists, None);

        let mut extended_lists = ranked_lists;
        if extended_lists.is_empty() {
            extended_lists.push(Vec::new());
        }
        extended_lists[0].push(RankedResult::new("doc-255", extra_rank));

        let fused_after = fuse(&extended_lists, None);

        for before in fused_before {
            let after = fused_after
                .iter()
                .find(|result| result.id == before.id)
                .expect("existing result should still be present");
            prop_assert_eq!(after.score.to_bits(), before.score.to_bits());
        }
    }
}
