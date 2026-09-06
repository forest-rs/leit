// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::vec::Vec;

/// The policy inputs for one immutable segment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SegmentSummary {
    /// Stable source identity returned when the segment is selected.
    pub ordinal: usize,
    /// Size tier used to group merge candidates.
    pub level: u32,
    /// Encoded size used to prefer smaller candidates within a tier.
    pub size_bytes: u64,
}

/// Selects two to four candidates from the lowest eligible size tier.
#[must_use]
pub fn select_merge_candidates(summaries: &[SegmentSummary]) -> Option<Vec<usize>> {
    let eligible_level = summaries
        .iter()
        .map(|summary| summary.level)
        .filter(|level| {
            summaries
                .iter()
                .filter(|summary| summary.level == *level)
                .take(2)
                .count()
                == 2
        })
        .min()?;

    let mut candidates: Vec<_> = summaries
        .iter()
        .filter(|summary| summary.level == eligible_level)
        .map(|summary| (summary.size_bytes, summary.ordinal))
        .collect();
    candidates.sort_unstable();
    candidates.truncate(4);

    Some(candidates.into_iter().map(|(_, ordinal)| ordinal).collect())
}
