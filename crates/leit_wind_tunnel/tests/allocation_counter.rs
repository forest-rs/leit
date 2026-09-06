// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Contract tests for scoped allocation instrumentation.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;

use leit_core::{SegmentLocalDocId, TermFreq};
use leit_index::ExecutionWorkspace;
use leit_postings::codec::{BlockDeltaCodec, Codec, CodecId, DeltaVarintCodec};
use leit_postings::cursor::{CursorStatus, DocCursor, PostingsView, TfCursor};
use leit_wind_tunnel::allocation::{AllocationCounterError, AllocationSnapshot, CountingAllocator};

#[global_allocator]
static GLOBAL: CountingAllocator<System> = CountingAllocator::new(System);

static OTHER: CountingAllocator<System> = CountingAllocator::new(System);
static TEST_LOCK: Mutex<()> = Mutex::new(());
static WORKER_START: AtomicBool = AtomicBool::new(false);
static WORKER_DONE: AtomicBool = AtomicBool::new(false);
static WORKER_RESULT: AtomicU8 = AtomicU8::new(0);

fn serial_test() -> MutexGuard<'static, ()> {
    TEST_LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

fn wait_for_worker(done: &AtomicBool, worker: &JoinHandle<()>) -> bool {
    loop {
        if done.load(Ordering::Acquire) {
            return true;
        }
        if worker.is_finished() {
            return done.load(Ordering::Acquire);
        }
        std::hint::spin_loop();
    }
}

struct NullAllocator;

#[expect(
    unsafe_code,
    reason = "test/benchmark GlobalAlloc wrapper delegates to the inner allocator"
)]
// SAFETY: this allocator never owns a block; allocation always reports failure,
// and the tests never pass a pointer to its deallocator.
unsafe impl GlobalAlloc for NullAllocator {
    unsafe fn alloc(&self, _layout: Layout) -> *mut u8 {
        std::ptr::null_mut()
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

static NULL: CountingAllocator<NullAllocator> = CountingAllocator::new(NullAllocator);

struct FailingReallocator;

#[expect(
    unsafe_code,
    reason = "test/benchmark GlobalAlloc wrapper delegates to the inner allocator"
)]
// SAFETY: allocation and deallocation preserve `System`'s layout and ownership
// contracts; failed reallocation leaves the original block owned by the caller.
unsafe impl GlobalAlloc for FailingReallocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplied a valid `GlobalAlloc` layout unchanged.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: tests pass the live pointer and original layout from `System`.
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, _pointer: *mut u8, _layout: Layout, _new_size: usize) -> *mut u8 {
        std::ptr::null_mut()
    }
}

static FAILING_REALLOC: CountingAllocator<FailingReallocator> =
    CountingAllocator::new(FailingReallocator);

#[expect(
    unsafe_code,
    reason = "test/benchmark GlobalAlloc wrapper delegates to the inner allocator"
)]
mod direct_allocator {
    use super::{CountingAllocator, GlobalAlloc, Layout};

    pub(super) fn alloc<A: GlobalAlloc>(allocator: &CountingAllocator<A>, size: usize) -> *mut u8 {
        let layout = Layout::from_size_align(size, 1).expect("valid test layout");
        // SAFETY: the constructed nonzero layout satisfies `GlobalAlloc::alloc`.
        unsafe { allocator.alloc(layout) }
    }

    pub(super) fn dealloc<A: GlobalAlloc>(
        allocator: &CountingAllocator<A>,
        pointer: *mut u8,
        size: usize,
    ) {
        let layout = Layout::from_size_align(size, 1).expect("valid test layout");
        // SAFETY: callers pass a live pointer from this allocator and its layout.
        unsafe { allocator.dealloc(pointer, layout) }
    }

    pub(super) fn realloc<A: GlobalAlloc>(
        allocator: &CountingAllocator<A>,
        pointer: *mut u8,
        old_size: usize,
        new_size: usize,
    ) -> *mut u8 {
        let layout = Layout::from_size_align(old_size, 1).expect("valid test layout");
        // SAFETY: callers pass a live pointer from this allocator, its original
        // layout, and a nonzero replacement size.
        unsafe { allocator.realloc(pointer, layout, new_size) }
    }
}

#[test]
fn counting_is_disabled_between_leases_and_each_lease_resets_counters() {
    let _serial = serial_test();
    let outside = direct_allocator::alloc(&GLOBAL, 5);
    assert!(!outside.is_null());
    direct_allocator::dealloc(&GLOBAL, outside, 5);

    let lease = GLOBAL.try_start_counting().expect("first lease");
    let pointer = direct_allocator::alloc(&GLOBAL, 7);
    assert!(!pointer.is_null());
    direct_allocator::dealloc(&GLOBAL, pointer, 7);
    assert_eq!(
        lease.finish(),
        AllocationSnapshot {
            alloc_calls: 1,
            realloc_calls: 0,
            dealloc_calls: 1,
            allocated_bytes: 7,
            released_bytes: 7,
        }
    );

    let outside = direct_allocator::alloc(&GLOBAL, 11);
    assert!(!outside.is_null());
    direct_allocator::dealloc(&GLOBAL, outside, 11);
    assert_eq!(
        GLOBAL.try_start_counting().expect("reset lease").finish(),
        AllocationSnapshot::default()
    );
}

#[test]
fn successful_reallocation_counts_full_new_and_old_sizes() {
    let _serial = serial_test();
    let pointer = direct_allocator::alloc(&GLOBAL, 8);
    assert!(!pointer.is_null());

    let lease = GLOBAL.try_start_counting().expect("lease");
    let pointer = direct_allocator::realloc(&GLOBAL, pointer, 8, 16);
    assert!(!pointer.is_null());
    direct_allocator::dealloc(&GLOBAL, pointer, 16);

    assert_eq!(
        lease.finish(),
        AllocationSnapshot {
            alloc_calls: 0,
            realloc_calls: 1,
            dealloc_calls: 1,
            allocated_bytes: 16,
            released_bytes: 24,
        }
    );
}

#[test]
fn failed_allocations_do_not_change_counters() {
    let _serial = serial_test();
    let lease = NULL.try_start_counting().expect("lease");
    assert!(direct_allocator::alloc(&NULL, 8).is_null());
    assert_eq!(lease.finish(), AllocationSnapshot::default());
}

#[test]
fn failed_reallocation_retains_ownership_and_does_not_change_counters() {
    let _serial = serial_test();
    let pointer = direct_allocator::alloc(&FAILING_REALLOC, 8);
    assert!(!pointer.is_null());

    let lease = FAILING_REALLOC.try_start_counting().expect("lease");
    assert!(direct_allocator::realloc(&FAILING_REALLOC, pointer, 8, 16).is_null());
    assert_eq!(lease.finish(), AllocationSnapshot::default());

    direct_allocator::dealloc(&FAILING_REALLOC, pointer, 8);
}

#[test]
fn allocations_from_other_threads_are_excluded() {
    let _serial = serial_test();
    WORKER_START.store(false, Ordering::Relaxed);
    WORKER_DONE.store(false, Ordering::Relaxed);
    let worker = std::thread::spawn(|| {
        while !WORKER_START.load(Ordering::Acquire) {
            std::hint::spin_loop();
        }
        let pointer = direct_allocator::alloc(&GLOBAL, 19);
        assert!(!pointer.is_null());
        direct_allocator::dealloc(&GLOBAL, pointer, 19);
        WORKER_DONE.store(true, Ordering::Release);
    });

    let lease = GLOBAL.try_start_counting().expect("lease");
    WORKER_START.store(true, Ordering::Release);
    let completed = wait_for_worker(&WORKER_DONE, &worker);
    let snapshot = lease.finish();
    worker.join().expect("worker joined");
    assert!(completed, "worker exited without reporting completion");
    assert_eq!(snapshot, AllocationSnapshot::default());
}

#[test]
fn nested_and_concurrent_leases_are_rejected_process_wide() {
    let _serial = serial_test();
    WORKER_START.store(false, Ordering::Relaxed);
    WORKER_DONE.store(false, Ordering::Relaxed);
    WORKER_RESULT.store(0, Ordering::Relaxed);
    let worker = std::thread::spawn(|| {
        while !WORKER_START.load(Ordering::Acquire) {
            std::hint::spin_loop();
        }
        if OTHER.try_start_counting().unwrap_err() == AllocationCounterError::AlreadyActive {
            WORKER_RESULT.store(1, Ordering::Relaxed);
        }
        WORKER_DONE.store(true, Ordering::Release);
    });

    let lease = GLOBAL.try_start_counting().expect("lease");
    assert_eq!(
        GLOBAL.try_start_counting().unwrap_err(),
        AllocationCounterError::AlreadyActive
    );

    WORKER_START.store(true, Ordering::Release);
    let completed = wait_for_worker(&WORKER_DONE, &worker);
    let snapshot = lease.finish();
    worker.join().expect("worker joined");
    assert!(completed, "worker exited without reporting completion");
    assert_eq!(WORKER_RESULT.load(Ordering::Relaxed), 1);
    assert_eq!(snapshot, AllocationSnapshot::default());
}

#[test]
fn dropping_a_lease_during_unwind_restores_counting() {
    let _serial = serial_test();
    let result = std::panic::catch_unwind(|| {
        let _lease = GLOBAL.try_start_counting().expect("lease");
        panic!("exercise lease drop");
    });
    assert!(result.is_err());

    let ignored = direct_allocator::alloc(&GLOBAL, 23);
    assert!(!ignored.is_null());
    direct_allocator::dealloc(&GLOBAL, ignored, 23);

    assert_eq!(
        GLOBAL
            .try_start_counting()
            .expect("lease after unwind")
            .finish(),
        AllocationSnapshot::default()
    );
}

#[test]
fn finishing_a_lease_excludes_later_work() {
    let _serial = serial_test();
    assert_eq!(
        GLOBAL.try_start_counting().expect("lease").finish(),
        AllocationSnapshot::default()
    );
    let pointer = direct_allocator::alloc(&GLOBAL, 29);
    assert!(!pointer.is_null());
    direct_allocator::dealloc(&GLOBAL, pointer, 29);
    assert_eq!(
        GLOBAL.try_start_counting().expect("later lease").finish(),
        AllocationSnapshot::default()
    );
}

#[test]
fn panicking_worker_cannot_strand_the_owner_in_a_wait_loop() {
    let _serial = serial_test();
    WORKER_START.store(false, Ordering::Relaxed);
    WORKER_DONE.store(false, Ordering::Relaxed);
    let worker = std::thread::spawn(|| {
        while !WORKER_START.load(Ordering::Acquire) {
            std::hint::spin_loop();
        }
        panic!("intentional worker failure");
    });

    let lease = GLOBAL.try_start_counting().expect("lease");
    WORKER_START.store(true, Ordering::Release);
    assert!(!wait_for_worker(&WORKER_DONE, &worker));
    lease.finish();
    assert!(worker.join().is_err());
}

fn prepared_postings(count: u32) -> Vec<(SegmentLocalDocId, TermFreq)> {
    (0..count)
        .map(|index| {
            (
                SegmentLocalDocId::new(index * 3 + 1),
                TermFreq::new(index % 7 + 1),
            )
        })
        .collect()
}

fn traverse_prepared(workspace: &mut ExecutionWorkspace, view: PostingsView<'_>) -> (usize, u64) {
    let mut cursor = workspace
        .decode_prepared_postings(view)
        .expect("prepared postings should decode");
    let mut count = 0_usize;
    let mut checksum = 0_u64;
    while let Some(document) = cursor.current_doc() {
        count += 1;
        checksum += u64::from(document) + u64::from(cursor.current_tf());
        if cursor.advance() == CursorStatus::Exhausted {
            break;
        }
    }
    (count, checksum)
}

#[test]
fn decode_scratch_reuses_buffers_after_single_growth() {
    let _serial = serial_test();
    let fitting = prepared_postings(48);
    let expected_fitting = fitting
        .iter()
        .map(|(document, frequency)| u64::from(document.get()) + u64::from(frequency.get()))
        .sum::<u64>();
    for (label, codec_id) in [
        ("delta-varint", CodecId::DeltaVarint),
        ("block-delta", CodecId::BlockDelta),
    ] {
        let fitting_bytes = match codec_id {
            CodecId::DeltaVarint => DeltaVarintCodec.encode(&fitting),
            CodecId::BlockDelta => BlockDeltaCodec.encode(&fitting),
        };
        let fitting_view = PostingsView::new(&fitting_bytes, &[]);
        let mut workspace = ExecutionWorkspace::new();
        assert_eq!(
            traverse_prepared(&mut workspace, fitting_view),
            (fitting.len(), expected_fitting)
        );
        let warmed_capacities = workspace.benchmark_decode_capacities();
        let larger_count = warmed_capacities
            .documents
            .max(warmed_capacities.term_frequencies)
            .checked_add(129)
            .expect("deterministic fixture size should fit usize");
        let larger = prepared_postings(
            u32::try_from(larger_count).expect("deterministic fixture size should fit u32"),
        );
        let expected_larger = larger
            .iter()
            .map(|(document, frequency)| u64::from(document.get()) + u64::from(frequency.get()))
            .sum::<u64>();
        let larger_bytes = match codec_id {
            CodecId::DeltaVarint => DeltaVarintCodec.encode(&larger),
            CodecId::BlockDelta => BlockDeltaCodec.encode(&larger),
        };
        let larger_view = PostingsView::new(&larger_bytes, &[]);
        assert!(warmed_capacities.documents < larger.len(), "{label}");
        assert!(warmed_capacities.term_frequencies < larger.len(), "{label}");

        let growth_lease = GLOBAL.try_start_counting().expect("growth lease");
        let larger_observation = traverse_prepared(&mut workspace, larger_view);
        let growth = growth_lease.finish();
        let grown_capacities = workspace.benchmark_decode_capacities();

        assert_eq!(
            larger_observation,
            (larger.len(), expected_larger),
            "{label}"
        );
        assert_eq!(growth.alloc_calls, 0, "{label}: {growth:?}");
        assert!(growth.realloc_calls > 0, "{label}: {growth:?}");
        assert!(growth.realloc_calls <= 2, "{label}: {growth:?}");
        assert_eq!(growth.dealloc_calls, 0, "{label}: {growth:?}");
        assert!(grown_capacities.documents >= larger.len(), "{label}");
        assert!(grown_capacities.term_frequencies >= larger.len(), "{label}");
        assert!(
            grown_capacities.documents > warmed_capacities.documents,
            "{label}"
        );
        assert!(
            grown_capacities.term_frequencies > warmed_capacities.term_frequencies,
            "{label}"
        );

        let fitting_lease = GLOBAL.try_start_counting().expect("fitting lease");
        let fitting_observation = traverse_prepared(&mut workspace, fitting_view);
        let fitting_snapshot = fitting_lease.finish();
        let fitting_capacities = workspace.benchmark_decode_capacities();

        assert_eq!(
            fitting_observation,
            (fitting.len(), expected_fitting),
            "{label}"
        );
        assert_eq!(fitting_snapshot, AllocationSnapshot::default(), "{label}");
        assert_eq!(fitting_capacities, grown_capacities, "{label}");
        println!(
            "decode-scratch codec={label} growth={growth:?} capacities={grown_capacities:?} fitting={fitting_snapshot:?}"
        );
    }
}
