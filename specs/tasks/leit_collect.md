# Specification: leit_collect Crate

## 1. Overview and Purpose

The `leit_collect` crate provides collection primitives for top-k retrieval and result aggregation in the Leif information retrieval system. It implements efficient top-k collection using min-heap semantics, grouping operations for faceted search, and count aggregation for analytics.

**Key Goals:**
- Efficient top-k collection with O(n log k) complexity
- No-std compatibility with alloc support
- WAND-style skip threshold optimization for early termination
- Flexible grouping and counting operations
- Generic over entity types through EntityId

## 2. Dependencies

### Internal Dependencies
- `leit_core`
  - Types: `Hit`, `Score` (f32 wrapper), `EntityId`
  - Traits: Entity identification and scoring

### External Dependencies (no_std compatible)
- `alloc` - For heap-allocated collections (Vec, BinaryHeap)
- `core` - Core Rust constructs

### Optional Dependencies
- `serde` - For serialization support (feature-gated)
- `proptest` - For property-based testing (dev dependency)

## 3. Target Platform

**Target:** `no_std + alloc`

- Must compile with `#![no_std]`
- Use `extern crate alloc` for heap allocations
- Avoid any std-only APIs (e.g., HashMap, HashSet, Rc, Arc)
- Prefer array-based structures and simple Vec operations
- Document any alloc-specific APIs

## 4. Public API Specification

### 4.1 Core Trait: Collector

```rust
/// Core trait for collecting and scoring hits.
pub trait Collector<Id: EntityId> {
    /// Type returned from collection
    type Output;

    /// Collect a hit with its score
    fn collect(&mut self, hit: Hit<Id>, score: Score) -> Result<(), CollectError>;

    /// Finalize collection and return results
    fn finalize(self) -> Self::Output;

    /// Reset collector state for reuse
    fn reset(&mut self);

    /// Check if a hit can be skipped based on score threshold
    fn can_skip(&self, score: Score) -> bool {
        false
    }
}

/// Errors during collection
#[derive(Debug, Clone, PartialEq)]
pub enum CollectError {
    CapacityExceeded,
    InvalidScore,
    CollectorFull,
}
```

### 4.2 Core Type: Collected

```rust
/// Result of collection with hit and final score
#[derive(Debug, Clone, PartialEq)]
pub struct Collected<Id: EntityId> {
    pub hit: Hit<Id>,
    pub score: Score,
}

impl<Id: EntityId> Collected<Id> {
    pub fn new(hit: Hit<Id>, score: Score) -> Self;
    pub fn with_score(mut self, score: Score) -> Self;
}
```

### 4.3 Top-K Collector

```rust
/// Efficient top-k collector using min-heap semantics
#[derive(Debug, Clone)]
pub struct TopKCollector<Id: EntityId> {
    k: usize,
    heap: BinaryHeap<Reverse<Collected<Id>>>,
    skip_threshold: Score,
    count: usize,
}

impl<Id: EntityId> TopKCollector<Id> {
    /// Create a new top-k collector
    pub fn new(k: usize) -> Self;

    /// Create with pre-allocated capacity
    pub fn with_capacity(k: usize, capacity: usize) -> Self;

    /// Get current k (top-k size)
    pub fn k(&self) -> usize;

    /// Get number of collected items
    pub fn len(&self) -> usize;

    /// Check if collector is empty
    pub fn is_empty(&self) -> bool;

    /// Get current skip threshold (minimum score in top-k)
    pub fn skip_threshold(&self) -> Score;

    /// Check if full (has k items)
    pub fn is_full(&self) -> bool;
}

impl<Id: EntityId> Collector<Id> for TopKCollector<Id> {
    type Output = Vec<Collected<Id>>;

    fn collect(&mut self, hit: Hit<Id>, score: Score) -> Result<(), CollectError>;
    fn finalize(self) -> Self::Output;
    fn reset(&mut self);

    /// Override can_skip for WAND optimization
    fn can_skip(&self, score: Score) -> bool {
        !self.is_empty() && score < self.skip_threshold()
    }
}
```

### 4.4 Grouping Collector

```rust
/// Collect hits grouped by a key
pub trait GroupKey<Id: EntityId>: Eq + Hash + Clone {
    fn extract_key(hit: &Hit<Id>) -> Self;
}

/// Collector for grouped results
#[derive(Debug, Clone)]
pub struct GroupingCollector<Id: EntityId, K: GroupKey<Id>> {
    groups: BTreeMap<K, TopKCollector<Id>>,
    group_k: usize,
    skip_threshold: Score,
}

impl<Id: EntityId, K: GroupKey<Id>> GroupingCollector<Id, K> {
    pub fn new(group_k: usize) -> Self;
    pub fn with_capacity(group_k: usize, capacity: usize) -> Self;
    pub fn group_count(&self) -> usize;
    pub fn total_count(&self) -> usize;
    pub fn get_group(&self, key: &K) -> Option<&Vec<Collected<Id>>>;
}

impl<Id: EntityId, K: GroupKey<Id>> Collector<Id> for GroupingCollector<Id, K> {
    type Output = BTreeMap<K, Vec<Collected<Id>>>;

    fn collect(&mut self, hit: Hit<Id>, score: Score) -> Result<(), CollectError>;
    fn finalize(self) -> Self::Output;
    fn reset(&mut self);
}
```

**Note:** Uses `BTreeMap` instead of `HashMap` for no_std compatibility. For production use, consider `hashbrown` crate if HashMap performance is critical.

### 4.5 Count Collector

```rust
/// Simple collector for counting hits above a threshold
#[derive(Debug, Clone)]
pub struct CountCollector<Id: EntityId> {
    count: usize,
    threshold: Score,
    max_count: Option<usize>,
}

impl<Id: EntityId> CountCollector<Id> {
    pub fn new(threshold: Score) -> Self;
    pub fn with_max_count(threshold: Score, max_count: usize) -> Self;
    pub fn count(&self) -> usize;
    pub fn threshold(&self) -> Score;
    pub fn is_complete(&self) -> bool;
}

impl<Id: EntityId> Collector<Id> for CountCollector<Id> {
    type Output = usize;

    fn collect(&mut self, hit: Hit<Id>, score: Score) -> Result<(), CollectError>;
    fn finalize(self) -> Self::Output;
    fn reset(&mut self);

    fn can_skip(&self, score: Score) -> bool {
        score < self.threshold()
    }
}
```

### 4.6 Helper Types

```rust
/// Reverse wrapper for min-heap behavior with BinaryHeap
pub struct Reverse<T>(pub T);

impl<T: PartialEq> PartialEq for Reverse<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq(&other.0)
    }
}

impl<T: Eq> Eq for Reverse<T> {}

impl<T: PartialOrd> PartialOrd for Reverse<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        other.0.partial_cmp(&self.0)
    }
}

impl<T: Ord> Ord for Reverse<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other.0.cmp(&self.0)
    }
}
```

## 5. Min-Heap Implementation Details

### 5.1 Data Structure

```rust
// Internal representation
heap: BinaryHeap<Reverse<Collected<Id>>>
```

- Uses `alloc::collections::BinaryHeap` (max-heap)
- Wraps in `Reverse` for min-heap behavior
- Maintains smallest element at top for O(1) threshold access
- Supports O(log k) insert and O(log k) pop

### 5.2 Insert Algorithm

```rust
fn collect(&mut self, hit: Hit<Id>, score: Score) -> Result<(), CollectError> {
    let item = Collected { hit, score };

    if self.heap.len() < self.k {
        // Not full: insert and update threshold
        self.heap.push(Reverse(item));
        self.update_threshold();
        Ok(())
    } else if score > self.skip_threshold {
        // Full but beats threshold: replace min
        self.heap.pop();
        self.heap.push(Reverse(item));
        self.update_threshold();
        Ok(())
    } else {
        // Below threshold: reject
        Err(CollectError::CollectorFull)
    }
}
```

### 5.3 Threshold Update

```rust
fn update_threshold(&mut self) {
    self.skip_threshold = self.heap
        .peek()
        .map(|r| r.0.score)
        .unwrap_or(Score::ZERO);
}
```

### 5.4 Finalize (Sort Descending)

```rust
fn finalize(mut self) -> Vec<Collected<Id>> {
    let mut items: Vec<_> = self.heap.into_iter().map(|r| r.0).collect();
    items.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    items
}
```

## 6. Skip Threshold Optimization

### 6.1 Purpose

Enables WAND-style dynamic index pruning by providing a score threshold that collectors can use to skip posting list entries that cannot make the top-k.

### 6.2 Threshold Calculation

```rust
pub fn skip_threshold(&self) -> Score {
    self.skip_threshold
}
```

- Returns the minimum score in current top-k
- Returns `Score::ZERO` if fewer than k items collected
- Updated after every successful insert

### 6.3 Integration with WAND

```rust
// In posting list iterator
let threshold = collector.skip_threshold();
while let Some(entry) = self.next_entry() {
    if entry.upper_bound_score < threshold {
        // WAND skip: entire list cannot beat threshold
        return None;
    }
    if entry.score >= threshold {
        collector.collect(entry.hit, entry.score)?;
    }
}
```

### 6.4 Skip Decision

```rust
fn can_skip(&self, score: Score) -> bool {
    !self.is_empty() && score < self.skip_threshold
}
```

- Returns `true` if score cannot make top-k
- Used by iterators to skip work early
- Critical for query performance

## 7. Acceptance Criteria

### 7.1 Core Functionality
- [ ] TopKCollector correctly maintains top-k items by score
- [ ] Threshold updates correctly after each insert
- [ ] `finalize()` returns results sorted in descending order
- [ ] `can_skip()` correctly identifies skippable scores
- [ ] Collector handles edge cases: k=0, k=1, empty input

### 7.2 no_std Compatibility
- [ ] Compiles with `#![no_std]`
- [ ] No std dependencies in lib.rs
- [ ] Uses `alloc` for heap allocations only
- [ ] Works with `extern crate alloc`

### 7.3 API Completeness
- [ ] All public types and traits documented
- [ ] Example usage in docs
- [ ] `GroupKey` trait with common implementations
- [ ] Error types cover all failure modes

### 7.4 Performance
- [ ] O(n log k) complexity for n inserts
- [ ] No unnecessary allocations
- [ ] Efficient threshold access (O(1))
- [ ] Memory usage bounded by k

### 7.5 Integration
- [ ] Compatible with `leit_core::Hit` and `Score`
- [ ] Works with generic `EntityId` types
- [ ] GroupingCollector uses no_std compatible map
- [ ] CountCollector supports early termination

### 7.6 Testing
- [ ] Unit tests for all public methods
- [ ] Property-based tests with proptest
- [ ] Edge case coverage (empty, full, overflow)
- [ ] Benchmark suite for performance validation

## 8. Test Plan

### 8.1 Unit Tests

**TopKCollector Tests:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topk_basic() {
        let mut collector = TopKCollector::<u32>::new(3);
        collector.collect(Hit::new(1), Score::new(0.5)).unwrap();
        collector.collect(Hit::new(2), Score::new(0.8)).unwrap();
        collector.collect(Hit::new(3), Score::new(0.3)).unwrap();
        collector.collect(Hit::new(4), Score::new(0.6)).unwrap();

        let results = collector.finalize();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].score.value(), 0.8);
        assert_eq!(results[1].score.value(), 0.6);
        assert_eq!(results[2].score.value(), 0.5);
    }

    #[test]
    fn test_skip_threshold() {
        let mut collector = TopKCollector::<u32>::new(3);
        collector.collect(Hit::new(1), Score::new(0.5)).unwrap();
        collector.collect(Hit::new(2), Score::new(0.8)).unwrap();
        collector.collect(Hit::new(3), Score::new(0.3)).unwrap();

        // Threshold should be 0.3 (minimum)
        assert!(collector.can_skip(Score::new(0.2)));
        assert!(!collector.can_skip(Score::new(0.4)));
    }

    #[test]
    fn test_empty_collector() {
        let collector = TopKCollector::<u32>::new(3);
        assert_eq!(collector.len(), 0);
        assert!(collector.is_empty());
        assert!(!collector.is_full());
    }

    #[test]
    fn test_reset() {
        let mut collector = TopKCollector::<u32>::new(3);
        collector.collect(Hit::new(1), Score::new(0.5)).unwrap();
        collector.reset();
        assert_eq!(collector.len(), 0);
        assert!(collector.is_empty());
    }
}
```

**GroupingCollector Tests:**
```rust
#[test]
fn test_grouping() {
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    struct Category(String);

    impl GroupKey<u32> for Category {
        fn extract_key(hit: &Hit<u32>) -> Self {
            // Mock implementation
            Category(String::from("default"))
        }
    }

    let mut collector = GroupingCollector::<u32, Category>::new(2);
    // Test grouping logic
}

#[test]
fn test_multiple_groups() {
    // Verify items are correctly grouped by key
    // Verify each group maintains top-k independently
}
```

**CountCollector Tests:**
```rust
#[test]
fn test_count_threshold() {
    let mut collector = CountCollector::<u32>::new(Score::new(0.5));
    collector.collect(Hit::new(1), Score::new(0.8)).unwrap();
    collector.collect(Hit::new(2), Score::new(0.3)).unwrap(); // Skipped
    collector.collect(Hit::new(3), Score::new(0.6)).unwrap();

    assert_eq!(collector.finalize(), 2);
}

#[test]
fn test_max_count() {
    let mut collector = CountCollector::<u32>::with_max_count(Score::new(0.5), 2);
    collector.collect(Hit::new(1), Score::new(0.8)).unwrap();
    collector.collect(Hit::new(2), Score::new(0.6)).unwrap();
    collector.collect(Hit::new(3), Score::new(0.7)).unwrap();

    assert!(collector.is_complete());
}
```

### 8.2 Property-Based Tests

```rust
#[cfg(test)]
mod proptests {
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_topk_preserves_highest(
            scores in prop::collection::vec(0.0f32..1.0, 0..100),
            k in 1usize..20
        ) {
            let mut collector = TopKCollector::<u32>::new(k);
            for (i, &score) in scores.iter().enumerate() {
                collector.collect(Hit::new(i as u32), Score::new(score)).ok();
            }
            let results = collector.finalize();

            let mut sorted_scores = scores.clone();
            sorted_scores.sort_by(|a, b| b.partial_cmp(a).unwrap());
            let expected: Vec<_> = sorted_scores.into_iter().take(k).collect();

            prop_assert_eq!(results.len(), expected.len().min(k));
            for (result, expected_score) in results.iter().zip(expected.iter()) {
                prop_assert_eq!(result.score.value(), *expected_score);
            }
        }
    }
}
```

### 8.3 Benchmark Tests

```rust
#[cfg(test)]
mod benchmarks {
    use super::*;
    use std::time::Instant;

    #[test]
    #[ignore] // Run manually
    fn benchmark_insert_100k_k10() {
        let mut collector = TopKCollector::<u32>::new(10);
        let start = Instant::now();

        for i in 0..100_000 {
            let score = (i as f32) / 100_000.0;
            collector.collect(Hit::new(i), Score::new(score)).ok();
        }

        let duration = start.elapsed();
        println!("100k inserts (k=10): {:?}", duration);
        assert!(duration.as_millis() < 100); // Should be fast
    }

    #[test]
    #[ignore]
    fn benchmark_skip_threshold() {
        // Measure overhead of threshold checks
    }
}
```

### 8.4 Edge Case Tests

```rust
#[test]
fn test_k_zero() {
    let mut collector = TopKCollector::<u32>::new(0);
    let result = collector.collect(Hit::new(1), Score::new(0.5));
    assert!(matches!(result, Err(CollectError::CapacityExceeded)));
}

#[test]
fn test_nan_score_handling() {
    let mut collector = TopKCollector::<u32>::new(3);
    let result = collector.collect(Hit::new(1), Score::new(f32::NAN));
    assert!(matches!(result, Err(CollectError::InvalidScore)));
}

#[test]
fn test_duplicate_scores() {
    // Verify stable ordering for ties
}

#[test]
fn test_large_k() {
    // Test with k = 10000
}
```

## 9. Verification Commands

### 9.1 Build Verification

```bash
# Verify no_std compilation
cargo build --target x86_64-unknown-linux-gnu --no-default-features

# Check with alloc feature
cargo build --features alloc

# Full build with all features
cargo build --all-features

# Verify documentation builds
cargo doc --no-deps --open
```

### 9.2 Test Execution

```bash
# Run all tests
cargo test --package leit_collect

# Run with alloc feature
cargo test --package leit_collect --features alloc

# Run property tests
cargo test --package leit_collect --features proptest

# Run ignored benchmarks
cargo test --package leit_collect -- --ignored

# Run with no_std target
cargo test --package leit_collect --target x86_64-unknown-linux-gnu --no-default-features
```

### 9.3 Linting and Formatting

```bash
# Format check
cargo fmt --package leit_collect -- --check

# Clippy lints
cargo clippy --package leit_collect -- -D warnings

# Check documentation coverage
cargo doc --package leit_collect --no-deps && echo "Docs built successfully"
```

### 9.4 API Verification

```bash
# Verify public API items
cargo doc --package leit_collect --no-deps --open

# Check trait implementations
cargo test --package leit_collect -- --list | grep trait

# Verify type signatures
cargo test --package leit_collect --doc
```

### 9.5 Integration Checks

```bash
# Verify leit_core dependency
cargo tree --package leit_collect --depth 1

# Check for std dependencies
cargo tree --package leit_collect | grep std

# Verify alloc usage
cargo build --package leit_collect --no-default-features 2>&1 | grep -i alloc
```

## 10. Implementation Phases

### Phase 1: Core Infrastructure
- Implement `Collector` trait and `CollectError`
- Implement `Collected<Id>` type
- Implement `Reverse` wrapper for min-heap
- Basic unit tests

### Phase 2: TopKCollector
- Implement `TopKCollector` with min-heap logic
- Implement threshold tracking and `can_skip()`
- Implement `finalize()` with correct sorting
- Comprehensive unit and property tests

### Phase 3: Specialized Collectors
- Implement `CountCollector` with threshold logic
- Implement `GroupingCollector` with `GroupKey` trait
- Tests for specialized collectors

### Phase 4: Optimization and Integration
- Benchmark suite
- WAND integration examples
- Documentation and examples

### Phase 5: Verification
- Full no_std compatibility check
- Integration with leit_core types
- Final review and cleanup
