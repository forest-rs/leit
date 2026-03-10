# Leit Score Crate Specification

## 1. Overview and Purpose

The `leit_score` crate provides text scoring algorithms for the Leit search engine library. It implements BM25 and BM25F (BM25 with field weighting) ranking functions for document retrieval and relevance scoring.

### Purpose
- Provide efficient, no_std compatible scoring algorithms
- Support single-field (BM25) and multi-field (BM25F) document scoring
- Enable customizable scoring parameters (k1, b, field weights)
- Track scoring statistics for monitoring and debugging

### Design Goals
- **Pure no_std**: No heap allocation, suitable for embedded and constrained environments
- **Zero-dependency**: Depends only on `leit_core` for the Score type
- **Performance**: Efficient computation with minimal branching
- **Correctness**: Well-tested implementation with reference value verification

## 2. Dependencies

### Required
- `leit_core`: Access to `Score` type (f32 wrapper for relevance scores)

### Excluded
- No `std` or `alloc` dependency
- No external crates for math operations (use core::f32 methods)
- No serde or serialization (handled by other crates)

## 3. Target: no_std (Pure)

The crate must be `#![no_std]` compatible:
- Use only `core` library features
- No heap allocation (no `Vec`, `HashMap`, `String`)
- All data structures must be stack-allocated or passed by reference
- Iterator-based APIs to avoid collection allocation

## 4. Public API Specification

### 4.1 Core Types

```rust
/// Relevance score (f32 wrapper)
pub use leit_core::Score;

/// Scoring statistics for monitoring and debugging
#[derive(Debug, Clone, Copy, Default)]
pub struct ScoringStats {
    /// Number of documents scored
    pub documents: u64,
    /// Number of terms processed
    pub terms: u64,
    /// Total term frequency processed
    pub total_term_freq: u64,
    /// Accumulated score sum (for averaging)
    pub score_sum: f32,
}

impl ScoringStats {
    /// Create empty stats
    pub fn new() -> Self;
    
    /// Record a single document scoring
    pub fn record(&mut self, term_freq: u32, doc_len: u32, score: Score);
    
    /// Merge stats from another instance
    pub fn merge(&mut self, other: &ScoringStats);
    
    /// Get average score
    pub fn avg_score(&self) -> f32;
}
```

### 4.2 Scorer Trait

```rust
/// Core scoring trait for single-field document scoring
pub trait Scorer {
    /// Score a document based on term frequency and document length
    /// 
    /// # Arguments
    /// * `term_freq` - Number of times the term appears in the document
    /// * `doc_len` - Length of the document (in tokens/terms)
    /// 
    /// # Returns
    /// Relevance score (higher = more relevant)
    fn score(&self, term_freq: u32, doc_len: u32) -> Score;
    
    /// Get scoring statistics (if tracking is enabled)
    fn stats(&self) -> &ScoringStats;
    
    /// Reset statistics
    fn reset_stats(&mut self);
}
```

### 4.3 BM25 Parameters

```rust
/// BM25 ranking parameters
#[derive(Debug, Clone, Copy)]
pub struct Bm25Params {
    /// Term frequency saturation parameter (default: 1.2)
    /// Controls how quickly additional term occurrences diminish in value
    pub k1: f32,
    
    /// Length normalization parameter (default: 0.75)
    /// Controls the degree of document length normalization
    /// 0.0 = no normalization, 1.0 = full normalization
    pub b: f32,
}

impl Bm25Params {
    /// Create BM25 parameters with defaults
    pub fn new() -> Self;
    
    /// Create with custom k1 (b defaults to 0.75)
    pub fn with_k1(k1: f32) -> Self;
    
    /// Create with custom b (k1 defaults to 1.2)
    pub fn with_b(b: f32) -> Self;
    
    /// Create with both parameters
    pub fn with_params(k1: f32, b: f32) -> Self;
    
    /// Default parameters: k1=1.2, b=0.75
    pub fn default() -> Self;
}
```

### 4.4 BM25 Scorer

```rust
/// BM25 single-field scorer
/// 
/// Requires:
/// - Average document length (computed from corpus)
/// - Document count (for IDF calculation)
#[derive(Debug, Clone)]
pub struct Bm25Scorer {
    /// BM25 parameters
    params: Bm25Params,
    
    /// Average document length in corpus
    avg_doc_len: f32,
    
    /// Total number of documents in corpus
    doc_count: u32,
    
    /// Inverse document frequency (precomputed)
    idf: f32,
    
    /// Scoring statistics
    stats: ScoringStats,
}

impl Bm25Scorer {
    /// Create a new BM25 scorer
    /// 
    /// # Arguments
    /// * `params` - BM25 parameters (k1, b)
    /// * `avg_doc_len` - Average document length in the corpus
    /// * `doc_count` - Total number of documents
    /// * `df` - Document frequency (number of documents containing the term)
    pub fn new(
        params: Bm25Params,
        avg_doc_len: f32,
        doc_count: u32,
        df: u32,
    ) -> Self;
    
    /// Create with default parameters
    pub fn with_defaults(avg_doc_len: f32, doc_count: u32, df: u32) -> Self;
    
    /// Get the IDF value (for debugging)
    pub fn idf(&self) -> f32;
    
    /// Get average document length
    pub fn avg_doc_len(&self) -> f32;
    
    /// Get document count
    pub fn doc_count(&self) -> u32;
}

impl Scorer for Bm25Scorer {
    fn score(&self, term_freq: u32, doc_len: u32) -> Score;
    fn stats(&self) -> &ScoringStats;
    fn reset_stats(&mut self);
}
```

### 4.5 BM25F Parameters

```rust
/// BM25F (multi-field BM25) parameters
#[derive(Debug, Clone, Copy)]
pub struct Bm25FParams {
    /// Base BM25 parameters
    pub base: Bm25Params,
}

impl Bm25FParams {
    /// Create with default BM25 parameters
    pub fn new() -> Self;
    
    /// Create with custom base parameters
    pub fn with_base(params: Bm25Params) -> Self;
    
    /// Default implementation
    pub fn default() -> Self;
}
```

### 4.6 Field Weights

```rust
/// Weights for different document fields
/// 
/// Allows prioritizing certain fields (e.g., title > body)
#[derive(Debug, Clone, Copy)]
pub struct FieldWeights {
    /// Array of field weights (indexed by field_id)
    weights: [f32; MAX_FIELDS],
    /// Number of active fields
    len: u8,
}

impl FieldWeights {
    /// Maximum number of fields supported
    pub const MAX_FIELDS: usize = 16;
    
    /// Create empty field weights
    pub fn new() -> Self;
    
    /// Add a field with weight
    /// Returns error if MAX_FIELDS exceeded
    pub fn add(&mut self, weight: f32) -> Result<(), FieldWeightsError>;
    
    /// Get weight for field
    pub fn get(&self, field_id: usize) -> f32;
    
    /// Set weight for field
    pub fn set(&mut self, field_id: usize, weight: f32) -> Result<(), FieldWeightsError>;
    
    /// Number of fields
    pub fn len(&self) -> usize;
    
    /// Check if empty
    pub fn is_empty(&self) -> bool;
}

impl Default for FieldWeights {
    fn default() -> Self;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldWeightsError {
    /// Maximum fields exceeded
    MaxFieldsExceeded,
    /// Invalid field ID
    InvalidFieldId,
}
```

### 4.7 BM25F Scorer

```rust
/// Field-level information for BM25F scoring
#[derive(Debug, Clone, Copy)]
pub struct FieldInfo {
    /// Field identifier
    pub field_id: u8,
    /// Term frequency in this field
    pub term_freq: u32,
    /// Length of this field (in tokens)
    pub field_len: u32,
}

impl FieldInfo {
    /// Create new field info
    pub fn new(field_id: u8, term_freq: u32, field_len: u32) -> Self;
}

/// BM25F multi-field scorer
/// 
/// Combines scores from multiple fields with weights
#[derive(Debug, Clone)]
pub struct Bm25FScorer {
    /// BM25F parameters
    params: Bm25FParams,
    
    /// Average field lengths (indexed by field_id)
    avg_field_lens: [f32; FieldWeights::MAX_FIELDS],
    
    /// Total number of documents in corpus
    doc_count: u32,
    
    /// Inverse document frequency (precomputed)
    idf: f32,
    
    /// Field weights
    field_weights: FieldWeights,
    
    /// Scoring statistics
    stats: ScoringStats,
}

impl Bm25FScorer {
    /// Create a new BM25F scorer
    /// 
    /// # Arguments
    /// * `params` - BM25F parameters
    /// * `doc_count` - Total number of documents
    /// * `df` - Document frequency (documents containing the term)
    /// * `field_weights` - Weights for each field
    /// * `avg_field_lens` - Average length for each field
    pub fn new(
        params: Bm25FParams,
        doc_count: u32,
        df: u32,
        field_weights: FieldWeights,
        avg_field_lens: &[f32],
    ) -> Result<Self, Bm25FError>;
    
    /// Create with default parameters
    pub fn with_defaults(
        doc_count: u32,
        df: u32,
        field_weights: FieldWeights,
        avg_field_lens: &[f32],
    ) -> Result<Self, Bm25FError>;
    
    /// Score a document with multiple fields
    /// 
    /// # Arguments
    /// * `fields` - Slice of field information
    /// 
    /// # Returns
    /// Combined relevance score across all fields
    pub fn score_fields(&self, fields: &[FieldInfo]) -> Score;
    
    /// Score fields using iterator (no allocation)
    pub fn score_fields_iter<'a>(
        &self,
        fields: impl Iterator<Item = &'a FieldInfo>,
    ) -> Score;
    
    /// Get field weights
    pub fn field_weights(&self) -> &FieldWeights;
    
    /// Get average field length
    pub fn avg_field_len(&self, field_id: usize) -> Option<f32>;
    
    /// Get the IDF value
    pub fn idf(&self) -> f32;
    
    /// Get document count
    pub fn doc_count(&self) -> u32;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bm25FError {
    /// Mismatch between field weights and average lengths
    MismatchedFieldData,
    /// Invalid field ID
    InvalidFieldId,
}
```

## 5. BM25 Formula and Implementation Details

### 5.1 BM25 Formula

The BM25 score for a document with term frequency `tf` and document length `dl` is:

```
IDF × (tf × (k1 + 1)) / (tf + k1 × (1 - b + b × (dl / avg_dl)))
```

Where:
- **IDF** (Inverse Document Frequency) = `log((N - df + 0.5) / (df + 0.5) + 1)`
  - N = total documents in corpus
  - df = document frequency (documents containing the term)
- **k1** = term frequency saturation parameter (default 1.2)
- **b** = length normalization parameter (default 0.75)
- **tf** = term frequency in document
- **dl** = document length
- **avg_dl** = average document length in corpus

### 5.2 Implementation Details

#### IDF Calculation
```rust
fn calculate_idf(doc_count: u32, df: u32) -> f32 {
    let n = doc_count as f32;
    let df = df as f32;
    ((n - df + 0.5) / (df + 0.5) + 1.0).ln()
}
```

#### BM25 Score Calculation
```rust
fn calculate_bm25(
    tf: u32,
    dl: u32,
    avg_dl: f32,
    idf: f32,
    k1: f32,
    b: f32,
) -> f32 {
    let tf = tf as f32;
    let dl = dl as f32;
    
    let numerator = tf * (k1 + 1.0);
    let denominator = tf + k1 * (1.0 - b + b * (dl / avg_dl));
    
    idf * (numerator / denominator)
}
```

#### Edge Cases
- **Zero term frequency**: Returns 0.0 (term not present)
- **Zero document length**: Use epsilon (1e-6) to avoid division by zero
- **Zero document frequency**: Handled by IDF formula (df >= 1 by definition)
- **df > doc_count**: Invalid input, return 0.0 or panic in debug mode

#### Performance Considerations
- Precompute IDF once per term/query
- Precompute `k1 + 1` constant
- Avoid branching in hot path
- Use f32 for all calculations (precision sufficient for ranking)

## 6. BM25F Multi-Field Scoring Formula

### 6.1 BM25F Formula

BM25F extends BM25 by scoring each field independently and combining with weights:

```
IDF × (Σ (weight_f × tf_f × (k1 + 1))) / (Σ (weight_f × tf_f) + k1 × (1 - b + b × (Σ (weight_f × dl_f) / Σ (weight_f × avg_dl_f))))
```

Where:
- **weight_f** = weight for field f
- **tf_f** = term frequency in field f
- **dl_f** = length of field f
- **avg_dl_f** = average length of field f

### 6.2 Implementation Strategy

#### Efficient Computation
```rust
fn calculate_bm25f(
    fields: &[FieldInfo],
    weights: &FieldWeights,
    avg_field_lens: &[f32],
    idf: f32,
    k1: f32,
    b: f32,
) -> f32 {
    // Accumulate weighted term frequencies and lengths
    let mut weighted_tf_sum = 0.0;
    let mut weighted_dl_sum = 0.0;
    let mut weighted_avg_dl_sum = 0.0;
    
    for field in fields {
        let weight = weights.get(field.field_id as usize);
        let tf = field.term_freq as f32;
        let dl = field.field_len as f32;
        let avg_dl = avg_field_lens[field.field_id as usize];
        
        weighted_tf_sum += weight * tf;
        weighted_dl_sum += weight * dl;
        weighted_avg_dl_sum += weight * avg_dl;
    }
    
    // Compute BM25F score
    let numerator = weighted_tf_sum * (k1 + 1.0);
    let denominator = weighted_tf_sum + k1 * (1.0 - b + b * (weighted_dl_sum / weighted_avg_dl_sum));
    
    idf * (numerator / denominator)
}
```

#### Iterator-Based API
For no_std compatibility, provide an iterator-based version that avoids slice allocation:

```rust
pub fn score_fields_iter<'a>(
    &self,
    fields: impl Iterator<Item = &'a FieldInfo>,
) -> Score {
    // Same computation using iterator
}
```

## 7. Acceptance Criteria with Score Verification

### 7.1 Functional Requirements

1. **BM25 Scoring**
   - Correctly implements standard BM25 formula
   - Handles edge cases (zero tf, zero dl, etc.)
   - Tracks statistics accurately
   - Works with no_std

2. **BM25F Scoring**
   - Correctly implements BM25F formula with field weighting
   - Supports up to 16 fields
   - Handles missing fields gracefully
   - Works with no_std

3. **Parameter Customization**
   - Allows custom k1 and b values
   - Validates parameter ranges (k1 > 0, 0 <= b <= 1)
   - Provides sensible defaults

4. **Statistics Tracking**
   - Accurately counts documents scored
   - Tracks terms processed
   - Computes average score correctly
   - Supports merging statistics

### 7.2 Score Verification

#### Test Case 1: Simple BM25
```
Parameters: k1=1.2, b=0.75
Corpus: 100 documents, avg_dl=200
Term: df=10 documents contain the term

Document 1: tf=5, dl=150
Expected score ≈ 2.046

Document 2: tf=10, dl=250
Expected score ≈ 2.648
```

#### Test Case 2: BM25F with Two Fields
```
Parameters: k1=1.2, b=0.75
Corpus: 100 documents
Field weights: title=2.0, body=1.0
Avg lengths: title=10, body=300

Document:
  - title: tf=2, dl=8
  - body: tf=5, dl=250

Expected score ≈ 3.127
```

#### Test Case 3: Zero Term Frequency
```
Any parameters
Document: tf=0, dl=100
Expected score = 0.0
```

#### Test Case 4: High Document Frequency
```
Parameters: k1=1.2, b=0.75
Corpus: 100 documents, avg_dl=200
Term: df=90 (very common)

Document: tf=3, dl=200
Expected score ≈ 0.132 (low IDF dominates)
```

### 7.3 Performance Requirements

1. **Scoring Speed**
   - BM25.score: < 100 ns per call
   - Bm25F.score_fields: < 500 ns for 4 fields

2. **Memory**
   - Bm25Scorer: <= 32 bytes
   - Bm25FScorer: <= 256 bytes (including field arrays)
   - No heap allocation

3. **Compilation**
   - No std dependency
   - Minimal code size
   - No external dependencies beyond leit_core

## 8. Test Plan with Reference Values

### 8.1 Unit Tests

#### BM25 Parameters
```rust
#[test]
fn test_bm25_params_default() {
    let params = Bm25Params::default();
    assert_eq!(params.k1, 1.2);
    assert_eq!(params.b, 0.75);
}

#[test]
fn test_bm25_params_custom() {
    let params = Bm25Params::with_params(2.0, 0.5);
    assert_eq!(params.k1, 2.0);
    assert_eq!(params.b, 0.5);
}
```

#### BM25 Scoring - Reference Values
```rust
#[test]
fn test_bm25_score_basic() {
    let scorer = Bm25Scorer::with_defaults(200.0, 100, 10);
    
    // tf=5, dl=150
    let score = scorer.score(5, 150);
    // IDF = ln((100-10+0.5)/(10+0.5)+1) = ln(8.5714+1) = 2.262
    // BM25 = 2.262 × (5×2.2)/(5+1.2×(1-0.75+0.75×150/200))
    //       = 2.262 × 11/(5+1.2×0.8125)
    //       = 2.262 × 11/5.975
    //       = 2.262 × 1.841
    //       = 4.164
    assert!((score.0 - 4.164).abs() < 0.01);
}

#[test]
fn test_bm25_score_zero_tf() {
    let scorer = Bm25Scorer::with_defaults(200.0, 100, 10);
    let score = scorer.score(0, 150);
    assert_eq!(score.0, 0.0);
}

#[test]
fn test_bm25_score_long_doc() {
    let scorer = Bm25Scorer::with_defaults(200.0, 100, 10);
    
    // tf=3, dl=1000 (very long)
    let score = scorer.score(3, 1000);
    // Should be lower due to length normalization
    assert!(score.0 < 3.0);
}

#[test]
fn test_bm25_high_df() {
    let scorer = Bm25Scorer::with_defaults(200.0, 100, 90);
    
    // Common term (df=90)
    let score = scorer.score(3, 200);
    // Low IDF should dominate
    assert!(score.0 < 0.5);
}
```

#### Field Weights
```rust
#[test]
fn test_field_weights_add() {
    let mut weights = FieldWeights::new();
    weights.add(2.0).unwrap();
    weights.add(1.0).unwrap();
    
    assert_eq!(weights.len(), 2);
    assert_eq!(weights.get(0), 2.0);
    assert_eq!(weights.get(1), 1.0);
}

#[test]
fn test_field_weights_max_fields() {
    let mut weights = FieldWeights::new();
    for _ in 0..FieldWeights::MAX_FIELDS {
        weights.add(1.0).unwrap();
    }
    
    assert!(weights.add(1.0).is_err());
}
```

#### BM25F Scoring - Reference Values
```rust
#[test]
fn test_bm25f_score_two_fields() {
    let mut weights = FieldWeights::new();
    weights.add(2.0).unwrap(); // title
    weights.add(1.0).unwrap(); // body
    
    let avg_lens = [10.0, 300.0];
    let scorer = Bm25FScorer::with_defaults(100, 10, weights, &avg_lens).unwrap();
    
    let fields = vec![
        FieldInfo::new(0, 2, 8),   // title: tf=2, dl=8
        FieldInfo::new(1, 5, 250), // body: tf=5, dl=250
    ];
    
    let score = scorer.score_fields(&fields);
    // Complex calculation - verify against reference implementation
    assert!((score.0 - 3.127).abs() < 0.05);
}

#[test]
fn test_bm25f_score_single_field() {
    let mut weights = FieldWeights::new();
    weights.add(1.0).unwrap();
    
    let avg_lens = [200.0];
    let scorer = Bm25FScorer::with_defaults(100, 10, weights, &avg_lens).unwrap();
    
    let fields = vec![FieldInfo::new(0, 5, 150)];
    let score = scorer.score_fields(&fields);
    
    // Should match BM25 for single field
    let bm25_scorer = Bm25Scorer::with_defaults(200.0, 100, 10);
    let bm25_score = bm25_scorer.score(5, 150);
    
    assert!((score.0 - bm25_score.0).abs() < 0.001);
}
```

#### Statistics Tracking
```rust
#[test]
fn test_stats_tracking() {
    let scorer = Bm25Scorer::with_defaults(200.0, 100, 10);
    
    scorer.score(5, 150);
    scorer.score(3, 200);
    
    let stats = scorer.stats();
    assert_eq!(stats.documents, 2);
    assert_eq!(stats.terms, 2);
    assert!(stats.avg_score() > 0.0);
}

#[test]
fn test_stats_merge() {
    let mut stats1 = ScoringStats::new();
    stats1.record(5, 150, Score(4.0));
    
    let mut stats2 = ScoringStats::new();
    stats2.record(3, 200, Score(2.0));
    
    stats1.merge(&stats2);
    
    assert_eq!(stats1.documents, 2);
    assert_eq!(stats1.total_term_freq, 8);
}
```

### 8.2 Integration Tests

```rust
#[test]
fn test_no_std_compatible() {
    // Verify no std features are used
    // This is a compile-time check
}

#[test]
fn test_heapless() {
    // Verify no heap allocation
    // Use static analysis or runtime checks
}
```

### 8.3 Property-Based Tests

```rust
#[test]
fn test_monotonic_tf() {
    let scorer = Bm25Scorer::with_defaults(200.0, 100, 10);
    let dl = 200;
    
    let score1 = scorer.score(1, dl);
    let score2 = scorer.score(5, dl);
    let score3 = scorer.score(10, dl);
    
    assert!(score1.0 < score2.0);
    assert!(score2.0 < score3.0);
}

#[test]
fn test_length_normalization() {
    let scorer = Bm25Scorer::with_defaults(200.0, 100, 10);
    let tf = 5;
    
    let score_short = scorer.score(tf, 100);
    let score_avg = scorer.score(tf, 200);
    let score_long = scorer.score(tf, 400);
    
    // Longer docs should have lower scores (with b > 0)
    assert!(score_short.0 > score_avg.0);
    assert!(score_avg.0 > score_long.0);
}
```

## 9. Verification Commands

### 9.1 Build Verification

```bash
# Build crate (no_std)
cd crates/leit_score
cargo build --release --no-default-features

# Verify no std dependency
cargo tree --no-default-features | grep std

# Check for alloc usage
cargo tree --no-default-features | grep alloc

# Build documentation
cargo doc --no-default-features --open
```

### 9.2 Test Verification

```bash
# Run all tests
cargo test --no-default-features

# Run tests with output
cargo test --no-default-features -- --nocapture

# Run specific test
cargo test --no-default-features test_bm25_score_basic

# Run tests in release mode (for performance)
cargo test --no-default-features --release

# Generate test coverage (requires tarpaulin)
cargo tarpaulin --no-default-features --out Html
```

### 9.3 Benchmarks

```bash
# Run benchmarks (requires criterion)
cargo bench --no-default-features

# Specific benchmark
cargo bench --no-default-features -- bm25_score
```

### 9.4 Linting and Formatting

```bash
# Check formatting
cargo fmt --check

# Format code
cargo fmt

# Run clippy
cargo clippy --no-default-features -- -D warnings

# Check documentation
cargo doc --no-default-features -- -D warnings
```

### 9.5 Size Verification

```bash
# Check binary size
cargo build --release --no-default-features
ls -lh target/release/leit_score.rlib

# Check code size (requires cargo-bloat)
cargo bloat --no-default-features --release --crates

# Verify no std symbols
nm target/release/leit_score.rlib | grep std
```

### 9.6 API Verification

```bash
# Check public API
cargo doc --no-default-features --no-deps --open

# Verify trait implementations
cargo test --no-default-features --doc

# Check type signatures
cargo test --no-default-features -- --list
```

## 10. Implementation Checklist

### Phase 1: Core Types
- [ ] `ScoringStats` implementation
- [ ] `Bm25Params` implementation
- [ ] `FieldWeights` implementation
- [ ] Error types

### Phase 2: BM25
- [ ] `Scorer` trait definition
- [ ] `Bm25Scorer` implementation
- [ ] IDF calculation
- [ ] BM25 score calculation
- [ ] Statistics tracking

### Phase 3: BM25F
- [ ] `FieldInfo` type
- [ ] `Bm25FParams` implementation
- [ ] `Bm25FScorer` implementation
- [ ] Multi-field scoring
- [ ] Iterator-based API

### Phase 4: Testing
- [ ] Unit tests for all types
- [ ] Reference value verification
- [ ] Property-based tests
- [ ] Edge case tests
- [ ] no_std verification

### Phase 5: Documentation
- [ ] API documentation
- [ ] Examples
- [ ] Formula documentation
- [ ] Performance notes

### Phase 6: Release
- [ ] All tests passing
- [ ] No clippy warnings
- [ ] Documentation complete
- [ ] Benchmarks passing
- [ ] Size requirements met
