# leit_fusion Crate Specification

## 1. Overview and Purpose

The `leit_fusion` crate provides result fusion algorithms for combining and ranking search results from multiple sources in the Leif search engine library. It implements Reciprocal Rank Fusion (RRF) and score normalization techniques to merge heterogeneous result sets.

### Purpose
- Combine ranked result lists from multiple query sources
- Normalize scores across different scoring algorithms
- Implement Reciprocal Rank Fusion for robust result merging
- Support customizable fusion strategies

### Design Goals
- **no_std + alloc**: Suitable for embedded and WASM targets
- **Algorithm correctness**: Well-tested fusion implementations with reference verification
- **Flexibility**: Pluggable normalization and fusion strategies
- **Performance**: Efficient combination of result sets

## 2. Dependencies

### Required
- `leit_core`: Access to `Hit<Id>`, `Score`, and `EntityId` trait

### External Dependencies
- `alloc`: For dynamic collections (Vec, HashMap)

### Excluded
- No `std` dependency (no_std + alloc only)
- No external algorithm crates (implementations are self-contained)

## 3. Target Environment

- **Target:** `no_std` + `alloc`
- **Rust Edition:** 2021
- **Minimum Supported Rust Version (MSRV):** 1.70.0

### no_std + alloc Configuration

```toml
[dependencies]
alloc = "1"

[features]
default = ["std"]
std = ["alloc"]
```

### Conditional Compilation

- Core fusion algorithms must work without `std`
- Use `alloc` for `Vec`, `HashMap`, and other collections
- `std` feature enables additional error conversions and convenience methods
- All type definitions must be `#[cfg_attr(not(feature = "std"), no_std)]` compatible

## 4. Public API Specification

### 4.1 Fusion<Id> Trait

Core trait for result fusion strategies.

```rust
/// Trait for fusing multiple ranked result lists into a single ranked list
pub trait Fusion<Id>
where
    Id: EntityId,
{
    /// Fuse multiple result lists into a single ranked list
    /// 
    /// # Arguments
    /// * `results` - Slice of result lists to fuse, each pre-sorted by score descending
    /// 
    /// # Returns
    /// A single fused result list, sorted by fused score descending
    /// 
    /// # Behavior
    /// - Each input list should be sorted by score descending (highest first)
    /// - Results appearing in multiple lists are combined according to the fusion strategy
    /// - Results unique to a single list are included with their original score
    /// - The output list is truncated to a reasonable size (e.g., top 1000)
    fn fuse(&self, results: &[Vec<Hit<Id>>]) -> Vec<Hit<Id>>;
    
    /// Fuse with a limit on the number of results returned
    /// 
    /// # Arguments
    /// * `results` - Slice of result lists to fuse
    /// * `limit` - Maximum number of results to return
    /// 
    /// # Returns
    /// Top N fused results, sorted by score descending
    fn fuse_with_limit(&self, results: &[Vec<Hit<Id>>], limit: usize) -> Vec<Hit<Id>> {
        let mut fused = self.fuse(results);
        fused.truncate(limit);
        fused
    }
}
```

### 4.2 RrfFusion

Reciprocal Rank Fusion implementation.

```rust
/// Reciprocal Rank Fusion (RRF) for combining ranked result lists
/// 
/// RRF is a robust, score-agnostic fusion method that combines rankings
/// rather than raw scores, making it ideal for merging heterogeneous result sets.
/// 
/// # Formula
/// For each entity, the RRF score is:
/// ```text
/// score = Σ (k / (k + rank_i))
/// ```
/// where `rank_i` is the position (1-indexed) of the entity in result list i,
/// and `k` is a smoothing parameter (default 60).
#[derive(Debug, Clone)]
pub struct RrfFusion {
    /// Smoothing parameter controlling rank contribution
    /// Higher k gives more weight to lower-ranked results
    /// Default: 60
    /// Typical range: [1, 100]
    k: f32,
}

impl RrfFusion {
    /// Create RRF fusion with default k=60
    pub fn new() -> Self;
    
    /// Create RRF fusion with custom k parameter
    /// 
    /// # Arguments
    /// * `k` - Smoothing parameter (must be > 0)
    /// 
    /// # Panics
    /// Panics if k <= 0
    pub fn with_k(k: f32) -> Self;
    
    /// Get the current k parameter
    pub fn k(&self) -> f32;
    
    /// Set a new k parameter
    /// 
    /// # Arguments
    /// * `k` - New smoothing parameter (must be > 0)
    /// 
    /// # Panics
    /// Panics if k <= 0
    pub fn set_k(&mut self, k: f32);
}

impl Default for RrfFusion {
    fn default() -> Self {
        Self::new()
    }
}

impl<Id: EntityId> Fusion<Id> for RrfFusion {
    fn fuse(&self, results: &[Vec<Hit<Id>>]) -> Vec<Hit<Id>>;
}
```

### 4.3 ScoreNormalizer

Score normalization strategies for making scores comparable.

```rust
/// Strategy for normalizing scores to a common range
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NormalizeStrategy {
    /// Min-max normalization to [0, 1]
    /// Formula: (score - min) / (max - min)
    MinMax,
    
    /// Z-score normalization (standardization)
    /// Formula: (score - mean) / std_dev
    ZScore,
}

/// Score normalizer for making heterogeneous scores comparable
#[derive(Debug, Clone)]
pub struct ScoreNormalizer {
    /// Normalization strategy
    strategy: NormalizeStrategy,
}

impl ScoreNormalizer {
    /// Create a score normalizer with the given strategy
    pub fn new(strategy: NormalizeStrategy) -> Self;
    
    /// Create a min-max normalizer
    pub fn min_max() -> Self {
        Self::new(NormalizeStrategy::MinMax)
    }
    
    /// Create a z-score normalizer
    pub fn z_score() -> Self {
        Self::new(NormalizeStrategy::ZScore)
    }
    
    /// Normalize a single score using precomputed statistics
    /// 
    /// # Arguments
    /// * `score` - Score to normalize
    /// * `stats` - Precomputed statistics for the score distribution
    /// 
    /// # Returns
    /// Normalized score
    pub fn normalize(&self, score: Score, stats: &ScoreStats) -> Score;
    
    /// Normalize a slice of scores in-place
    /// 
    /// # Arguments
    /// * `scores` - Slice of scores to normalize (modified in place)
    pub fn normalize_slice(&self, scores: &mut [Score]) {
        let stats = ScoreStats::from_slice(scores);
        for score in scores.iter_mut() {
            *score = self.normalize(*score, &stats);
        }
    }
    
    /// Get the current strategy
    pub fn strategy(&self) -> NormalizeStrategy;
}

impl Default for ScoreNormalizer {
    fn default() -> Self {
        Self::min_max()
    }
}
```

### 4.4 Score Statistics

Statistics for score normalization.

```rust
/// Statistical summary of a score distribution
#[derive(Debug, Clone, Copy)]
pub struct ScoreStats {
    /// Minimum score
    pub min: f32,
    /// Maximum score
    pub max: f32,
    /// Mean score
    pub mean: f32,
    /// Standard deviation
    pub std_dev: f32,
    /// Number of samples
    pub count: usize,
}

impl ScoreStats {
    /// Compute statistics from a slice of scores
    pub fn from_slice(scores: &[Score]) -> Self;
    
    /// Compute statistics from an iterator of scores
    pub fn from_iter<'a>(scores: impl Iterator<Item = &'a Score>) -> Self;
    
    /// Check if the distribution is degenerate (all values equal)
    pub fn is_constant(&self) -> bool {
        self.min == self.max
    }
    
    /// Range (max - min)
    pub fn range(&self) -> f32 {
        self.max - self.min
    }
}
```

### 4.5 WeightedFusion

Score-based fusion with configurable weights.

```rust
/// Weighted score fusion for combining normalized scores
/// 
/// Combines scores from multiple result lists using configurable weights.
/// Scores should be normalized before fusion for best results.
#[derive(Debug, Clone)]
pub struct WeightedFusion {
    /// Weights for each result list (sum should be 1.0)
    weights: Vec<f32>,
}

impl WeightedFusion {
    /// Create weighted fusion with equal weights
    /// 
    /// # Arguments
    /// * `num_lists` - Number of result lists to fuse
    pub fn equal_weights(num_lists: usize) -> Self;
    
    /// Create weighted fusion with custom weights
    /// 
    /// # Arguments
    /// * `weights` - Weight for each result list (must have num_lists elements)
    /// 
    /// # Panics
    /// Panics if weights is empty or contains negative values
    pub fn with_weights(weights: Vec<f32>) -> Self;
    
    /// Get the weights
    pub fn weights(&self) -> &[f32];
    
    /// Normalize weights to sum to 1.0
    fn normalize_weights(&mut self);
}

impl<Id: EntityId> Fusion<Id> for WeightedFusion {
    fn fuse(&self, results: &[Vec<Hit<Id>>]) -> Vec<Hit<Id>>;
}
```

### 4.6 CombFusion

CombSUM fusion strategy (sum of normalized scores).

```rust
/// CombSUM fusion: sum of normalized scores
/// 
/// CombSUM is a simple but effective fusion method that sums
/// the normalized scores of each entity across all result lists.
#[derive(Debug, Clone, Copy, Default)]
pub struct CombSumFusion;

impl CombSumFusion {
    pub fn new() -> Self;
}

impl<Id: EntityId> Fusion<Id> for CombSumFusion {
    fn fuse(&self, results: &[Vec<Hit<Id>>]) -> Vec<Hit<Id>>;
}
```

### 4.7 CombMNZFusion

CombMNZ fusion strategy (sum of scores × number of lists containing entity).

```rust
/// CombMNZ fusion: sum of scores multiplied by occurrence count
/// 
/// CombMNZ rewards entities that appear in multiple result lists
/// by multiplying the score sum by the number of lists containing the entity.
#[derive(Debug, Clone, Copy, Default)]
pub struct CombMNZFusion;

impl CombMNZFusion {
    pub fn new() -> Self;
}

impl<Id: EntityId> Fusion<Id> for CombMNZFusion {
    fn fuse(&self, results: &[Vec<Hit<Id>>]) -> Vec<Hit<Id>>;
}
```

## 5. Reciprocal Rank Fusion Algorithm

### 5.1 RRF Formula

Reciprocal Rank Fusion combines rankings rather than raw scores:

```text
rrf_score(entity) = Σ (k / (k + rank_i(entity)))
```

Where:
- **k** = smoothing parameter (default 60)
- **rank_i(entity)** = 1-indexed rank of entity in result list i
- The sum is over all result lists where the entity appears

### 5.2 Implementation Details

#### RRF Score Calculation

```rust
fn calculate_rrf_score<Id: EntityId>(
    entity: Id,
    results: &[Vec<Hit<Id>>],
    k: f32,
) -> Score {
    let mut score = 0.0;
    
    for list in results {
        if let Some(rank) = list.iter().position(|hit| hit.id == entity) {
            // Ranks are 1-indexed
            let rank = rank + 1;
            score += k / (k + rank as f32);
        }
    }
    
    Score::new(score)
}
```

#### Edge Cases

- **Empty input lists**: Return empty result list
- **Entity not in any list**: Score = 0.0 (not included in output)
- **Single list**: Returns list resorted by RRF score (preserves order)
- **Duplicate entities within a list**: Use first occurrence (lowest rank)
- **k parameter**: Must be > 0, panics otherwise

#### Performance Considerations

- Use HashMap to accumulate scores: `Id -> (score, rank)`
- For each list, iterate and update entity scores
- Sort final results by score descending
- Truncate to top N results (e.g., 1000) to avoid massive outputs

### 5.3 RRF Algorithm Steps

1. **Initialize**: Create empty HashMap for entity scores
2. **Process each list**:
   - For each entity at position i (0-indexed):
   - Compute contribution: `k / (k + i + 1)`
   - Add to entity's accumulated score
3. **Collect results**: Convert HashMap to Vec<Hit<Id>>
4. **Sort**: Sort by score descending
5. **Truncate**: Limit to top N results (e.g., 1000)

## 6. Score Normalization Algorithms

### 6.1 Min-Max Normalization

Scales scores to [0, 1] range:

```text
normalized = (score - min) / (max - min)
```

**Implementation**:

```rust
fn min_max_normalize(score: f32, stats: &ScoreStats) -> f32 {
    if stats.is_constant() {
        return 0.5; // All scores equal, return middle value
    }
    (score - stats.min) / stats.range()
}
```

**Edge Cases**:
- **Constant distribution**: Return 0.5 (middle of range)
- **Single value**: Return 0.0 or 1.0 depending on distribution

### 6.2 Z-Score Normalization

Standardizes scores to mean=0, std_dev=1:

```text
normalized = (score - mean) / std_dev
```

**Implementation**:

```rust
fn z_score_normalize(score: f32, stats: &ScoreStats) -> f32 {
    if stats.std_dev == 0.0 {
        return 0.0; // All scores equal, return zero
    }
    (score - stats.mean) / stats.std_dev
}
```

**Edge Cases**:
- **Zero std_dev**: Return 0.0 (all scores equal to mean)
- **Single value**: Return 0.0

### 6.3 Statistics Computation

```rust
impl ScoreStats {
    pub fn from_slice(scores: &[Score]) -> Self {
        let count = scores.len();
        if count == 0 {
            return Self {
                min: 0.0,
                max: 0.0,
                mean: 0.0,
                std_dev: 0.0,
                count: 0,
            };
        }
        
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        let mut sum = 0.0;
        
        for score in scores {
            let s = score.into_f32();
            min = min.min(s);
            max = max.max(s);
            sum += s;
        }
        
        let mean = sum / count as f32;
        
        // Compute variance
        let mut variance_sum = 0.0;
        for score in scores {
            let diff = score.into_f32() - mean;
            variance_sum += diff * diff;
        }
        let variance = variance_sum / count as f32;
        let std_dev = variance.sqrt();
        
        Self {
            min,
            max,
            mean,
            std_dev,
            count,
        }
    }
}
```

## 7. Acceptance Criteria Checklist

### 7.1 Functional Requirements

- [ ] RRF fusion correctly implements the RRF formula
- [ ] RRF handles empty result lists gracefully
- [ ] RRF handles single result list (preserves ranking)
- [ ] RRF k parameter is validated (must be > 0)
- [ ] Score normalizer handles empty slices (returns all zeros)
- [ ] Score normalizer handles constant distributions
- [ ] Min-max normalization produces values in [0, 1]
- [ ] Z-score normalization produces mean ≈ 0, std_dev ≈ 1
- [ ] Weighted fusion applies correct weights to scores
- [ ] CombSUM sums scores across all lists
- [ ] CombMNZ multiplies by occurrence count
- [ ] All fusion methods handle duplicate entities correctly
- [ ] All fusion methods sort results by score descending

### 7.2 no_std Requirements

- [ ] Crate compiles without std feature
- [ ] Uses only alloc for collections
- [ ] No std imports in core code
- [ ] Conditional compilation for std feature works

### 7.3 API Requirements

- [ ] Fusion<Id> trait is object-safe (can be used as trait object)
- [ ] All fusion types implement Fusion<Id>
- [ ] All public types are documented
- [ ] All public methods are documented
- [ ] Examples provided for non-trivial operations

### 7.4 Performance Requirements

- [ ] RRF fusion: < 1ms for 10 lists of 1000 results each
- [ ] Score normalization: < 100μs for 10,000 scores
- [ ] Memory usage: O(unique entities) for fusion
- [ ] No unnecessary allocations in hot paths

## 8. Test Plan with RRF Correctness Tests

### 8.1 Unit Tests - RRF Fusion

#### Test 1: Basic RRF with k=60
```rust
#[test]
fn test_rrf_basic() {
    let fusion = RrfFusion::with_k(60.0);
    
    let list1 = vec![
        Hit::new(EntityId::new(1), Score(1.0)),  // rank 1
        Hit::new(EntityId::new(2), Score(0.8)),  // rank 2
        Hit::new(EntityId::new(3), Score(0.5)),  // rank 3
    ];
    
    let list2 = vec![
        Hit::new(EntityId::new(2), Score(1.0)),  // rank 1
        Hit::new(EntityId::new(3), Score(0.9)),  // rank 2
        Hit::new(EntityId::new(4), Score(0.7)),  // rank 3
    ];
    
    let results = vec![list1, list2];
    let fused = fusion.fuse(&results);
    
    // Entity 2: rank 1 in list2, rank 2 in list1
    // Score = 60/61 + 60/62 = 0.9836 + 0.9677 = 1.9513
    
    // Entity 3: rank 3 in list1, rank 2 in list2
    // Score = 60/63 + 60/62 = 0.9524 + 0.9677 = 1.9201
    
    // Entity 1: rank 1 in list1 only
    // Score = 60/61 = 0.9836
    
    // Entity 4: rank 3 in list2 only
    // Score = 60/63 = 0.9524
    
    // Order: 2 > 3 > 1 > 4
    assert_eq!(fused[0].id, EntityId::new(2));
    assert_eq!(fused[1].id, EntityId::new(3));
    assert_eq!(fused[2].id, EntityId::new(1));
    assert_eq!(fused[3].id, EntityId::new(4));
    
    // Verify scores (within tolerance)
    assert!((fused[0].score.0 - 1.9513).abs() < 0.001);
    assert!((fused[1].score.0 - 1.9201).abs() < 0.001);
}
```

#### Test 2: RRF with k=1 (high rank sensitivity)
```rust
#[test]
fn test_rrf_k1() {
    let fusion = RrfFusion::with_k(1.0);
    
    let list1 = vec![
        Hit::new(EntityId::new(1), Score(1.0)),  // rank 1
        Hit::new(EntityId::new(2), Score(0.9)),  // rank 2
    ];
    
    let list2 = vec![
        Hit::new(EntityId::new(2), Score(1.0)),  // rank 1
        Hit::new(EntityId::new(3), Score(0.8)),  // rank 2
    ];
    
    let results = vec![list1, list2];
    let fused = fusion.fuse(&results);
    
    // Entity 2: rank 1 in list2, rank 2 in list1
    // Score = 1/2 + 1/3 = 0.5 + 0.333 = 0.833
    
    // Entity 1: rank 1 in list1 only
    // Score = 1/2 = 0.5
    
    // With k=1, top ranks dominate
    assert_eq!(fused[0].id, EntityId::new(2));
    assert!((fused[0].score.0 - 0.833).abs() < 0.01);
}
```

#### Test 3: Empty input
```rust
#[test]
fn test_rrf_empty() {
    let fusion = RrfFusion::new();
    let results: Vec<Vec<Hit<EntityId>>> = vec![];
    let fused = fusion.fuse(&results);
    assert!(fused.is_empty());
}
```

#### Test 4: Single list
```rust
#[test]
fn test_rrf_single_list() {
    let fusion = RrfFusion::new();
    
    let list = vec![
        Hit::new(EntityId::new(1), Score(1.0)),
        Hit::new(EntityId::new(2), Score(0.8)),
        Hit::new(EntityId::new(3), Score(0.5)),
    ];
    
    let results = vec![list.clone()];
    let fused = fusion.fuse(&results);
    
    // Single list should preserve order (all scores monotonic)
    assert_eq!(fused[0].id, EntityId::new(1));
    assert_eq!(fused[1].id, EntityId::new(2));
    assert_eq!(fused[2].id, EntityId::new(3));
    
    // Scores should be monotonically decreasing
    assert!(fused[0].score.0 > fused[1].score.0);
    assert!(fused[1].score.0 > fused[2].score.0);
}
```

#### Test 5: Disjoint sets
```rust
#[test]
fn test_rrf_disjoint() {
    let fusion = RrfFusion::new();
    
    let list1 = vec![
        Hit::new(EntityId::new(1), Score(1.0)),
        Hit::new(EntityId::new(2), Score(0.9)),
    ];
    
    let list2 = vec![
        Hit::new(EntityId::new(3), Score(1.0)),
        Hit::new(EntityId::new(4), Score(0.8)),
    ];
    
    let results = vec![list1, list2];
    let fused = fusion.fuse(&results);
    
    // All entities should appear
    assert_eq!(fused.len(), 4);
    
    // First from each list should have same score (both rank 1)
    assert!((fused[0].score.0 - fused[1].score.0).abs() < 0.001);
}
```

### 8.2 Unit Tests - Score Normalization

#### Test 1: Min-max normalization
```rust
#[test]
fn test_min_max_normalize() {
    let normalizer = ScoreNormalizer::min_max();
    
    let mut scores = vec![
        Score(0.0),
        Score(0.5),
        Score(1.0),
    ];
    
    normalizer.normalize_slice(&mut scores);
    
    assert!((scores[0].0 - 0.0).abs() < 0.001);
    assert!((scores[1].0 - 0.5).abs() < 0.001);
    assert!((scores[2].0 - 1.0).abs() < 0.001);
}
```

#### Test 2: Min-max with negative scores
```rust
#[test]
fn test_min_max_negative() {
    let normalizer = ScoreNormalizer::min_max();
    
    let mut scores = vec![
        Score(-1.0),
        Score(0.0),
        Score(1.0),
    ];
    
    normalizer.normalize_slice(&mut scores);
    
    assert!((scores[0].0 - 0.0).abs() < 0.001);
    assert!((scores[1].0 - 0.5).abs() < 0.001);
    assert!((scores[2].0 - 1.0).abs() < 0.001);
}
```

#### Test 3: Constant distribution
```rust
#[test]
fn test_constant_distribution() {
    let normalizer = ScoreNormalizer::min_max();
    
    let mut scores = vec![Score(0.5), Score(0.5), Score(0.5)];
    
    normalizer.normalize_slice(&mut scores);
    
    // All should be 0.5 (middle of range)
    for score in scores {
        assert!((score.0 - 0.5).abs() < 0.001);
    }
}
```

#### Test 4: Z-score normalization
```rust
#[test]
fn test_z_score_normalize() {
    let normalizer = ScoreNormalizer::z_score();
    
    let mut scores = vec![
        Score(0.0),
        Score(1.0),
        Score(2.0),
        Score(3.0),
    ];
    
    normalizer.normalize_slice(&mut scores);
    
    // Mean should be ~0, std_dev ~1
    let stats = ScoreStats::from_slice(&scores);
    assert!(stats.mean.abs() < 0.001);
    assert!((stats.std_dev - 1.0).abs() < 0.001);
}
```

### 8.3 Unit Tests - Weighted Fusion

#### Test 1: Equal weights
```rust
#[test]
fn test_weighted_equal() {
    let fusion = WeightedFusion::equal_weights(2);
    
    let list1 = vec![
        Hit::new(EntityId::new(1), Score(1.0)),
        Hit::new(EntityId::new(2), Score(0.5)),
    ];
    
    let list2 = vec![
        Hit::new(EntityId::new(1), Score(0.5)),
        Hit::new(EntityId::new(3), Score(1.0)),
    ];
    
    let results = vec![list1, list2];
    let fused = fusion.fuse(&results);
    
    // Entity 1: 1.0 * 0.5 + 0.5 * 0.5 = 0.5 + 0.25 = 0.75
    assert_eq!(fused[0].id, EntityId::new(1));
    assert!((fused[0].score.0 - 0.75).abs() < 0.001);
}
```

#### Test 2: Custom weights
```rust
#[test]
fn test_weighted_custom() {
    let fusion = WeightedFusion::with_weights(vec![0.8, 0.2]);
    
    let list1 = vec![Hit::new(EntityId::new(1), Score(1.0))];
    let list2 = vec![Hit::new(EntityId::new(1), Score(0.0))];
    
    let results = vec![list1, list2];
    let fused = fusion.fuse(&results);
    
    // Entity 1: 1.0 * 0.8 + 0.0 * 0.2 = 0.8
    assert!((fused[0].score.0 - 0.8).abs() < 0.001);
}
```

### 8.4 Unit Tests - CombSUM/CombMNZ

#### Test 1: CombSUM
```rust
#[test]
fn test_comb_sum() {
    let fusion = CombSumFusion::new();
    
    let list1 = vec![Hit::new(EntityId::new(1), Score(0.5))];
    let list2 = vec![Hit::new(EntityId::new(1), Score(0.3))];
    
    let results = vec![list1, list2];
    let fused = fusion.fuse(&results);
    
    // 0.5 + 0.3 = 0.8
    assert!((fused[0].score.0 - 0.8).abs() < 0.001);
}
```

#### Test 2: CombMNZ
```rust
#[test]
fn test_comb_mnz() {
    let fusion = CombMNZFusion::new();
    
    let list1 = vec![Hit::new(EntityId::new(1), Score(0.5))];
    let list2 = vec![Hit::new(EntityId::new(1), Score(0.3))];
    
    let results = vec![list1, list2];
    let fused = fusion.fuse(&results);
    
    // (0.5 + 0.3) * 2 = 1.6
    assert!((fused[0].score.0 - 1.6).abs() < 0.001);
}
```

### 8.5 Integration Tests

```rust
#[test]
fn test_no_std_compatible() {
    // Verify compilation without std
    let fusion = RrfFusion::new();
    let list = vec![Hit::new(EntityId::new(1), Score(1.0))];
    let results = vec![list];
    let _ = fusion.fuse(&results);
}

#[test]
fn test_fusion_trait_object() {
    // Test that Fusion can be used as trait object
    let fusion: Box<dyn Fusion<EntityId>> = Box::new(RrfFusion::new());
    let list = vec![Hit::new(EntityId::new(1), Score(1.0))];
    let results = vec![list];
    let _ = fusion.fuse(&results);
}
```

## 9. Verification Commands

### 9.1 Build Verification

```bash
# Build crate (no_std + alloc)
cd crates/leit_fusion
cargo build --release --no-default-features

# Verify no std dependency
cargo tree --no-default-features | grep std

# Build with alloc feature
cargo build --release --no-default-features --features alloc

# Build with std feature
cargo build --release --features std

# Build documentation
cargo doc --no-deps --document-private-items
```

### 9.2 Test Verification

```bash
# Run all tests
cargo test

# Run tests without std
cargo test --no-default-features --features alloc

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_rrf_basic

# Run tests in release mode
cargo test --release

# Generate test coverage (requires tarpaulin)
cargo tarpaulin --out Html
```

### 9.3 Linting and Formatting

```bash
# Check formatting
cargo fmt --check

# Format code
cargo fmt

# Run clippy
cargo clippy --all-features -- -D warnings

# Check documentation
cargo doc --no-deps -- -D warnings
```

### 9.4 API Verification

```bash
# Check public API
cargo doc --no-deps --open

# Verify trait implementations
cargo test --doc

# Check type signatures
cargo test -- --list
```

## 10. Implementation Checklist

### Phase 1: Core Types
- [ ] `ScoreStats` implementation
- [ ] `NormalizeStrategy` enum
- [ ] `ScoreNormalizer` implementation
- [ ] `Fusion<Id>` trait definition

### Phase 2: RRF Fusion
- [ ] `RrfFusion` implementation
- [ ] RRF score calculation
- [ ] Edge case handling
- [ ] k parameter validation

### Phase 3: Score Normalization
- [ ] Min-max normalization
- [ ] Z-score normalization
- [ ] Statistics computation
- [ ] Edge case handling (empty, constant)

### Phase 4: Additional Fusion Methods
- [ ] `WeightedFusion` implementation
- [ ] `CombSumFusion` implementation
- [ ] `CombMNZFusion` implementation

### Phase 5: Testing
- [ ] Unit tests for RRF (all test cases)
- [ ] Unit tests for normalization
- [ ] Unit tests for weighted fusion
- [ ] Unit tests for CombSUM/CombMNZ
- [ ] Integration tests
- [ ] Property-based tests

### Phase 6: Documentation
- [ ] API documentation
- [ ] Algorithm explanations
- [ ] Examples for all fusion methods
- [ ] Performance notes

### Phase 7: Release
- [ ] All tests passing
- [ ] No clippy warnings
- [ ] Documentation complete
- [ ] no_std verification
- [ ] Benchmark verification
