// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::SegmentView;

/// Execution-oriented adapter over a borrowed [`SegmentView`].
///
/// `SegmentView` remains the canonical raw storage/view type for serialized
/// segment bytes. `SegmentIndex` is the place where execution-facing trait
/// implementations can live once the segment format carries the full planner and
/// scorer metadata needed by the shared index surface.
///
/// Today this type is a thin wrapper only. The current segment format does not
/// yet serialize enough information to implement the full `PlanningIndex` /
/// `ExecutableIndex` traits without rebuilding in-memory state.
#[derive(Clone, Copy, Debug)]
pub struct SegmentIndex<'a> {
    view: SegmentView<'a>,
}

impl<'a> SegmentIndex<'a> {
    /// Wrap a borrowed segment view as an execution-facing segment index.
    pub const fn new(view: SegmentView<'a>) -> Self {
        Self { view }
    }

    /// Access the underlying borrowed segment view.
    pub const fn view(&self) -> SegmentView<'a> {
        self.view
    }
}
