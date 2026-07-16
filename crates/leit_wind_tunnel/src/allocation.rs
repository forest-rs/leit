// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Scoped allocation instrumentation for benchmarks.

#![expect(
    unsafe_code,
    reason = "test/benchmark GlobalAlloc wrapper delegates to the inner allocator"
)]

use std::alloc::{GlobalAlloc, Layout};
use std::cell::Cell;
use std::fmt;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
}

static LEASED: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
static FAIL_TLS_ENABLE: AtomicBool = AtomicBool::new(false);

#[derive(Debug)]
struct Counters {
    alloc_calls: AtomicU64,
    realloc_calls: AtomicU64,
    dealloc_calls: AtomicU64,
    allocated_bytes: AtomicU64,
    released_bytes: AtomicU64,
}

impl Counters {
    const fn new() -> Self {
        Self {
            alloc_calls: AtomicU64::new(0),
            realloc_calls: AtomicU64::new(0),
            dealloc_calls: AtomicU64::new(0),
            allocated_bytes: AtomicU64::new(0),
            released_bytes: AtomicU64::new(0),
        }
    }

    fn reset(&self) {
        self.alloc_calls.store(0, Ordering::Relaxed);
        self.realloc_calls.store(0, Ordering::Relaxed);
        self.dealloc_calls.store(0, Ordering::Relaxed);
        self.allocated_bytes.store(0, Ordering::Relaxed);
        self.released_bytes.store(0, Ordering::Relaxed);
    }

    fn snapshot(&self) -> AllocationSnapshot {
        AllocationSnapshot {
            alloc_calls: self.alloc_calls.load(Ordering::Relaxed),
            realloc_calls: self.realloc_calls.load(Ordering::Relaxed),
            dealloc_calls: self.dealloc_calls.load(Ordering::Relaxed),
            allocated_bytes: self.allocated_bytes.load(Ordering::Relaxed),
            released_bytes: self.released_bytes.load(Ordering::Relaxed),
        }
    }
}

/// An allocator wrapper whose counters can be enabled for one thread at a time.
pub struct CountingAllocator<A> {
    inner: A,
    counters: Counters,
}

impl<A> CountingAllocator<A> {
    /// Wraps `inner` without installing it as the process allocator.
    pub const fn new(inner: A) -> Self {
        Self {
            inner,
            counters: Counters::new(),
        }
    }
}

impl<A> fmt::Debug for CountingAllocator<A> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CountingAllocator")
            .field("counters", &self.counters)
            .finish_non_exhaustive()
    }
}

impl<A: GlobalAlloc> CountingAllocator<A> {
    /// Starts an exclusive counting window owned by the calling thread.
    ///
    /// # Errors
    ///
    /// Returns [`AllocationCounterError::AlreadyActive`] while any counting
    /// allocator has an active lease in this process.
    pub fn try_start_counting(
        &'static self,
    ) -> Result<AllocationLease<'static>, AllocationCounterError> {
        LEASED
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .map_err(|_| AllocationCounterError::AlreadyActive)?;

        self.counters.reset();
        if enable_for_current_thread().is_err() {
            LEASED.store(false, Ordering::Release);
            return Err(AllocationCounterError::ThreadLocalUnavailable);
        }

        Ok(AllocationLease {
            counters: &self.counters,
            active: true,
            not_send_or_sync: PhantomData,
        })
    }
}

/// A completed allocation-counting window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AllocationSnapshot {
    /// Successful allocation calls.
    pub alloc_calls: u64,
    /// Successful reallocation calls.
    pub realloc_calls: u64,
    /// Deallocation calls.
    pub dealloc_calls: u64,
    /// Bytes requested by successful allocations and reallocations.
    pub allocated_bytes: u64,
    /// Bytes released by deallocations and successful reallocations.
    pub released_bytes: u64,
}

/// Failure to acquire a scoped allocation counter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AllocationCounterError {
    /// Another allocator lease is active in this process.
    AlreadyActive,
    /// Thread-local state could not be enabled.
    ThreadLocalUnavailable,
}

impl fmt::Display for AllocationCounterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyActive => formatter.write_str("an allocation counter is already active"),
            Self::ThreadLocalUnavailable => {
                formatter.write_str("thread-local allocation counting is unavailable")
            }
        }
    }
}

impl std::error::Error for AllocationCounterError {}

/// Exclusive, thread-owned access to an allocation-counting window.
pub struct AllocationLease<'a> {
    counters: &'a Counters,
    active: bool,
    not_send_or_sync: PhantomData<Rc<()>>,
}

impl fmt::Debug for AllocationLease<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AllocationLease")
            .field("active", &self.active)
            .finish_non_exhaustive()
    }
}

impl AllocationLease<'_> {
    /// Ends counting and returns the completed snapshot.
    pub fn finish(mut self) -> AllocationSnapshot {
        disable_for_current_thread();
        let snapshot = self.counters.snapshot();
        LEASED.store(false, Ordering::Release);
        self.active = false;
        snapshot
    }
}

impl Drop for AllocationLease<'_> {
    fn drop(&mut self) {
        if self.active {
            disable_for_current_thread();
            LEASED.store(false, Ordering::Release);
            self.active = false;
        }
    }
}

fn enable_for_current_thread() -> Result<(), ()> {
    #[cfg(test)]
    if FAIL_TLS_ENABLE.swap(false, Ordering::Relaxed) {
        return Err(());
    }

    COUNTING
        .try_with(|counting| counting.set(true))
        .map_err(|_| ())
}

fn disable_for_current_thread() {
    let _ = COUNTING.try_with(|counting| counting.set(false));
}

fn is_counting_on_current_thread() -> bool {
    COUNTING.try_with(Cell::get).unwrap_or(false)
}

fn bytes(size: usize) -> u64 {
    u64::try_from(size).unwrap_or(u64::MAX)
}

// SAFETY: every operation delegates the unchanged pointer/layout contract to
// `inner`; successful calls only add atomic bookkeeping afterward.
unsafe impl<A: GlobalAlloc> GlobalAlloc for CountingAllocator<A> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `GlobalAlloc::alloc` gives `inner` the caller's valid layout.
        let pointer = unsafe { self.inner.alloc(layout) };
        if !pointer.is_null() && is_counting_on_current_thread() {
            self.counters.alloc_calls.fetch_add(1, Ordering::Relaxed);
            self.counters
                .allocated_bytes
                .fetch_add(bytes(layout.size()), Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: the caller guarantees `pointer` and `layout` belong to `inner`.
        unsafe { self.inner.dealloc(pointer, layout) };
        if is_counting_on_current_thread() {
            self.counters.dealloc_calls.fetch_add(1, Ordering::Relaxed);
            self.counters
                .released_bytes
                .fetch_add(bytes(layout.size()), Ordering::Relaxed);
        }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller guarantees the original allocation contract, and
        // `new_size` is forwarded unchanged to the owning allocator.
        let new_pointer = unsafe { self.inner.realloc(pointer, layout, new_size) };
        if !new_pointer.is_null() && is_counting_on_current_thread() {
            self.counters.realloc_calls.fetch_add(1, Ordering::Relaxed);
            self.counters
                .allocated_bytes
                .fetch_add(bytes(new_size), Ordering::Relaxed);
            self.counters
                .released_bytes
                .fetch_add(bytes(layout.size()), Ordering::Relaxed);
        }
        new_pointer
    }
}

#[cfg(test)]
mod tests {
    use std::alloc::System;

    use super::{AllocationCounterError, CountingAllocator, FAIL_TLS_ENABLE, Ordering};

    static ALLOCATOR: CountingAllocator<System> = CountingAllocator::new(System);

    #[test]
    fn failed_thread_local_enable_releases_process_lease() {
        FAIL_TLS_ENABLE.store(true, Ordering::Relaxed);
        assert_eq!(
            ALLOCATOR.try_start_counting().unwrap_err(),
            AllocationCounterError::ThreadLocalUnavailable
        );
        ALLOCATOR
            .try_start_counting()
            .expect("lease is released after failed enable")
            .finish();
    }
}
