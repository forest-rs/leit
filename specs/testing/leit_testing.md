# Leit Testing Infrastructure Specification

**Status:** 📋 Specification  
**Phase:** 1  
**Component:** Correctness and Integration Testing  
**Standard:** NASA-style Software Assurance

---

## 1. Overview and Purpose

The Leit testing infrastructure provides correctness verification for the entire Leit Phase 1 stack. This specification defines standards for unit, integration, and composition testing following NASA-style software assurance practices, with emphasis on scoring correctness and risk-based testing.

### Core Responsibilities

- **Unit Test Standards:** Define coverage requirements and test quality standards
- **Integration Testing:** Verify correct interaction between crates
- **Composition Testing:** Validate end-to-end vertical slices
- **Known-Answer Tests:** Ensure scoring algorithms produce correct results
- **Regression Prevention:** Catch correctness issues before they reach users

### Design Philosophy

- **Risk-Based Testing:** Focus testing effort on highest-risk components (scoring correctness)
- **Verification vs Validation:** Separate "are we building it right" from "are we building the right thing"
- **NASA Coverage Standards:** 80-90% for critical modules, 100% for algorithmic core
- **Numerical Accuracy:** Special handling for floating-point algorithms (BM25)
- **Reproducibility:** All tests must be deterministic and reproducible

### Non-Goals

- Performance benchmarking (see `examples/leit_benchmark/`)
- Load testing distributed deployments
- Browser-based testing
- Language-specific analysis beyond English

---

## 2. NASA-Style Test Principles

### 2.1 Risk-Based Testing

Testing effort is prioritized by risk assessment:

| Risk Level | Component | Coverage Requirement | Test Frequency |
|------------|-----------|---------------------|----------------|
| **Critical** | BM25 scoring algorithm | 100% | Every commit |
| **High** | Query execution, posting traversal | 90% | Every commit |
| **Medium** | Indexing, tokenization | 85% | Every PR |
| **Low** | CLI utilities, error messages | 70% | Every merge |

**Risk Determination Criteria:**

- **Impact:** How severely users are affected by failures
- **Complexity:** Algorithmic complexity and likelihood of bugs
- **Frequency:** How often the code path executes
- **Dependencies:** How many other components depend on it

### 2.2 Verification vs Validation

**Verification (Building It Right):**

- Unit tests verify each function behaves as specified
- Integration tests verify components interact correctly
- Property-based tests verify invariants are maintained

**Validation (Building The Right Thing):**

- Known-answer tests verify results match expected outputs
- Composition tests verify the system solves the user's problem
- Scenario tests verify real-world use cases work correctly

### 2.3 Boundary Conditions

Every test suite must cover:

**Input Boundaries:**

- Empty inputs (empty strings, empty collections)
- Minimum/maximum values (numeric limits, document counts)
- Null/None cases where applicable
- Invalid inputs (malformed queries, out-of-range values)

**Algorithm Boundaries:**

- Single document collections
- Single-term queries
- Exact match vs. no match cases
- Division by zero edge cases in scoring

**Example Boundary Test Cases:**

```rust
#[test]
fn test_bm25_single_document_collection() {
    // Edge case: IDF calculation with n=1
    let collection = Collection::with_document_count(1);
    let idf = calculate_idf(&collection, term_id, 1);
    assert!(idf.is_finite(), "IDF should be finite for n=1");
}

#[test]
fn test_query_empty_index() {
    let index = Index::new();
    let results = index.search("rust").unwrap();
    assert_eq!(results.len(), 0, "Empty index should return no results");
}

#[test]
fn test_query_term_not_in_vocabulary() {
    let index = build_test_index();
    let results = index.search("nonexistent_term_xyz123").unwrap();
    assert_eq!(results.len(), 0, "Unknown term should return no results");
}
```

### 2.4 Numerical Accuracy Testing

**BM25 Score Accuracy:**

BM25 scores must be validated within appropriate tolerance:

```rust
#[test]
fn test_bm25_numerical_accuracy() {
    let doc = create_test_document();
    let scorer = BM25Scorer::new(k1=1.2, b=0.75);
    
    // Calculate reference score using high-precision implementation
    let expected_score = calculate_reference_bm25(&doc, "rust");
    
    // Calculate score using implementation under test
    let actual_score = scorer.score(&doc, term_id("rust"));
    
    // Allow 1e-6 relative error due to floating-point arithmetic
    let relative_error = (actual_score - expected_score).abs() / expected_score.abs();
    assert!(relative_error < 1e-6,
        "BM25 score error too large: {:.2e} (expected: {:.6}, actual: {:.6})",
        relative_error, expected_score, actual_score
    );
}
```

**Floating-Point Edge Cases:**

```rust
#[test]
fn test_bm25_zero_document_frequency() {
    // Term doesn't exist in collection
    let df = 0;
    let idf = calculate_idf(n, df);
    assert!(idf == 0.0 || idf.is_finite(), "IDF should be 0 or finite for df=0");
}

#[test]
fn test_bm25_extreme_term_frequency() {
    // Term appears many times in a document
    let doc = create_document_with_term_repeats("rust", 1000);
    let score = scorer.score(&doc, term_id("rust"));
    assert!(score.is_finite(), "Score should be finite even with extreme TF");
}
```

### 2.5 Reproducibility Requirements

All tests must be deterministic:

```rust
#[test]
fn test_query_results_reproducible() {
    let docs = generate_test_documents(/* seed */ 42);
    let index = build_index(&docs);
    
    let results1 = index.search("rust").unwrap();
    let results2 = index.search("rust").unwrap();
    
    // Results should be identical on repeated queries
    assert_eq!(results1, results2, "Query results should be reproducible");
}

#[test]
fn test_indexing_deterministic() {
    let docs = generate_test_documents(/* seed */ 42);
    
    let index1 = build_index(&docs.clone());
    let index2 = build_index(&docs);
    
    // Indexes should be functionally identical
    assert_eq!(index1.document_count(), index2.document_count());
    assert_eq!(index1.search("rust").unwrap(), index2.search("rust").unwrap());
}
```

---

## 3. Test Documentation Standards

### 3.1 Test Documentation Requirements

Every test must document:

**What It Validates:**

```rust
/// Test: BM25 IDF calculation with rare terms
/// 
/// Validates:
/// - Inverse Document Frequency increases as document frequency decreases
/// - IDF formula: log((N - df + 0.5) / (df + 0.5) + 1.0)
/// - Edge case: df = 1 (term appears in exactly one document)
/// 
/// Requirement: leit_score REQ-001 (BM25 Implementation)
/// Traceability: https://docs.leif.rs/requirements/scoring#REQ-001
#[test]
fn test_bm25_idf_rare_term() {
    // Test implementation...
}
```

**Expected Results:**

```rust
/// Test: Term query returns correctly ranked results
/// 
/// Expected behavior:
/// - Returns documents containing the term
/// - Results ranked by BM25 score (descending)
/// - Scores calculated with k1=1.2, b=0.75
/// - Document "doc_001" ranked first (expected score: 2.456)
/// 
/// Pass criteria:
/// - Top result is doc_001 with score 2.456 ± 0.001
/// - All results contain term "rust"
/// - Scores in descending order
#[test]
fn test_term_query_ranking() {
    // Test implementation...
}
```

### 3.2 Pass/Fail Criteria

Every test must have explicit pass/fail criteria:

```rust
#[test]
fn test_phrase_query_proximity() {
    let results = execute_query("\"memory safety\"~2");
    
    // PASS CRITERIA:
    // 1. At least 3 results returned
    // 2. All results contain "memory" and "safety"
    // 3. Term positions differ by ≤ 2 tokens
    
    // FAIL CONDITIONS:
    // - Fewer than 3 results
    // - Results missing either term
    // - Term positions exceed proximity threshold
    
    assert!(results.len() >= 3, "FAIL: Insufficient results (got {})", results.len());
    
    for result in &results {
        verify_term_positions(result, "memory", "safety", 2);
    }
}
```

### 3.3 Requirement Traceability

Tests should trace to documented requirements:

```rust
// Requirement: leit_score REQ-003 (BM25 Parameterization)
// https://docs.leif.rs/requirements/scoring#REQ-003
//
// "The BM25 algorithm SHALL support configurable parameters:
//  - k1: Term saturation parameter [1.0, 2.0], default 1.2
//  - b: Length normalization parameter [0.0, 1.0], default 0.75"

#[test]
fn test_bm25_parameterization() {
    let scorer = BM25Scorer::new(k1=1.5, b=0.5);
    // Verify parameters are used correctly...
}
```

---

## 4. Standard Test Suite Structure

### 4.1 Directory Layout

```
leit/
├── tests/
│   ├── unit/                    # Unit tests per crate
│   │   ├── leit_core/
│   │   │   ├── bitvector_test.rs
│   │   │   └── varint_test.rs
│   │   ├── leit_text/
│   │   │   ├── tokenizer_test.rs
│   │   │   └── stemmer_test.rs
│   │   ├── leit_postings/
│   │   │   ├── iterator_test.rs
│   │   │   └── skip_list_test.rs
│   │   ├── leit_score/
│   │   │   ├── bm25_test.rs         # 100% coverage required
│   │   │   └── collect_test.rs
│   │   ├── leit_query/
│   │   │   ├── parser_test.rs
│   │   │   └── planner_test.rs
│   │   ├── leit_index/
│   │   │   ├── writer_test.rs
│   │   │   └── reader_test.rs
│   │   └── leit_collect/
│   │       └── topk_test.rs
│   │
│   ├── integration/             # Multi-crate integration tests
│   │   ├── index_query_test.rs      # Index → Query → Results
│   │   ├── tokenizer_postings_test.rs  # Text → Postings
│   │   └── score_collect_test.rs     # Score → Collect
│   │
│   ├── composition/             # Full vertical slice tests
│   │   ├── wikipedia_test.rs
│   │   ├── ecommerce_test.rs
│   │   └── short_documents_test.rs
│   │
│   └── fixtures/                # Test data and fixtures
│       ├── documents/
│       │   ├── wikipedia_100.json
│       │   └── ecommerce_100.json
│       ├── queries/
│       │   ├── known_answers.json
│       │   └── edge_cases.json
│       └── expected/
│           ├── bm25_scores.json
│           └── rankings.json
│
└── examples/
    └── leit_benchmark/          # Performance benchmarking (moved from testing spec)
        ├── README.md
        ├── benchmarks/
        │   ├── indexing.rs
        │   └── queries.rs
        └── results/
```

### 4.2 Unit Tests (tests/unit/)

**Purpose:** Verify individual functions and modules work correctly

**Example Unit Test:**

```rust
// tests/unit/leit_score/bm25_test.rs

use leit_score::BM25Scorer;

#[test]
fn test_bm25_idf_calculation() {
    /// Validates IDF formula: log((N - df + 0.5) / (df + 0.5) + 1.0)
    /// Requirement: leit_score REQ-001
    
    let n = 1000; // Collection size
    
    // Common term (appears in 100 documents)
    let idf_common = BM25Scorer::calculate_idf(n, 100);
    assert!(idf_common > 0.0 && idf_common < 2.0,
        "IDF for common term should be low");
    
    // Rare term (appears in 1 document)
    let idf_rare = BM25Scorer::calculate_idf(n, 1);
    assert!(idf_rare > idf_common,
        "IDF for rare term should be higher than common term");
    
    // Verify monotonically decreasing function
    for df in 1..100 {
        let idf = BM25Scorer::calculate_idf(n, df);
        let idf_next = BM25Scorer::calculate_idf(n, df + 1);
        assert!(idf >= idf_next,
            "IDF should decrease as document frequency increases");
    }
}

#[test]
fn test_bm25_tf_normalization() {
    /// Validates TF normalization formula
    /// TF_norm = (TF * (k1 + 1)) / (TF + k1 * (1 - b + b * doc_len / avg_doc_len))
    
    let k1 = 1.2;
    let b = 0.75;
    let doc_len = 300;
    let avg_doc_len = 200;
    
    // Verify TF saturation
    let tf_1 = BM25Scorer::normalize_tf(1, k1, b, doc_len, avg_doc_len);
    let tf_10 = BM25Scorer::normalize_tf(10, k1, b, doc_len, avg_doc_len);
    let tf_100 = BM25Scorer::normalize_tf(100, k1, b, doc_len, avg_doc_len);
    
    assert!(tf_100 > tf_10 && tf_10 > tf_1,
        "Normalized TF should increase with raw TF");
    
    // But with diminishing returns
    let ratio_1_10 = tf_10 / tf_1;
    let ratio_10_100 = tf_100 / tf_10;
    assert!(ratio_1_10 > ratio_10_100,
        "TF normalization should show diminishing returns");
}
```

### 4.3 Integration Tests (tests/integration/)

**Purpose:** Verify multiple crates work together correctly

**Example Integration Test:**

```rust
// tests/integration/index_query_test.rs

use leit_index::{Index, IndexWriter};
use leit_query::{QueryParser, Planner};
use leit_postings::PostingsIterator;
use leit_score::BM25Scorer;
use leit_collect::TopKCollector;

#[test]
fn test_index_to_query_pipeline() {
    /// Validates: Index → Query → Postings → Score → Collect
    /// Integration: leit_index + leit_query + leit_postings + leit_score + leit_collect
    
    // 1. Build index (leit_index)
    let mut index = Index::new_memory();
    let docs = load_test_documents("fixtures/documents/wikipedia_100.json");
    
    for doc in &docs {
        index.insert(doc).unwrap();
    }
    index.commit().unwrap();
    
    // 2. Parse query (leit_query)
    let query = QueryParser::new().parse("title:rust AND safety").unwrap();
    
    // 3. Plan query (leit_query)
    let planner = Planner::new();
    let plan = planner.plan(&query, &index).unwrap();
    
    // 4. Execute query (leit_postings + leit_score)
    let scorer = BM25Scorer::new(1.2, 0.75);
    let collector = TopKCollector::new(10);
    let results = index.search_with_scorer(&plan, &scorer, collector).unwrap();
    
    // 5. Validate results
    assert!(!results.is_empty(), "Query should return results");
    assert!(results.len() <= 10, "Should return at most 10 results");
    
    // Verify scores are calculated
    for result in &results {
        assert!(result.score > 0.0, "All results should have positive scores");
    }
    
    // Verify descending order
    for i in 1..results.len() {
        assert!(results[i-1].score >= results[i].score,
            "Results should be sorted by score descending");
    }
}
```

### 4.4 Composition Tests (tests/composition/)

**Purpose:** Verify end-to-end vertical slices work correctly

**Example Composition Test:**

```rust
// tests/composition/wikipedia_test.rs

use leit_index::Index;
use leit_query::QueryParser;

#[test]
fn test_wikipedia_scenario_end_to_end() {
    /// Validates: Full vertical slice for Wikipedia-like documents
    /// Composition: All Phase 1 crates working together
    /// Scenario: Search Wikipedia abstracts
    /// 
    /// Expected Results (from fixtures/expected/wikipedia_100.json):
    /// - Query "rust" returns 15 documents
    /// - Top result: "doc_042" with score 2.456 ± 0.001
    /// - All results contain "rust" in title or body
    
    // 1. Load test data
    let docs = load_fixture("wikipedia_100.json");
    let expected = load_expected_results("wikipedia_100_rust.json");
    
    // 2. Build index
    let mut index = Index::new_memory();
    for doc in &docs {
        index.insert(doc).unwrap();
    }
    index.commit().unwrap();
    
    // 3. Execute query
    let query = QueryParser::new().parse("rust").unwrap();
    let results = index.search(&query).unwrap();
    
    // 4. Validate against expected results
    assert_eq!(results.len(), expected.count,
        "Result count mismatch: expected {}, got {}",
        expected.count, results.len()
    );
    
    // 5. Verify top result
    let top = &results[0];
    assert_eq!(top.doc_id, expected.top_result.doc_id,
        "Top result mismatch: expected {}, got {}",
        expected.top_result.doc_id, top.doc_id
    );
    
    assert!((top.score - expected.top_result.score).abs() < 0.001,
        "Top result score mismatch: expected {:.3}, got {:.3}",
        expected.top_result.score, top.score
    );
    
    // 6. Verify all results contain query term
    for result in &results {
        let doc = index.get_document(&result.doc_id).unwrap();
        assert!(
            doc.title.contains("rust") || doc.body.contains("rust"),
            "Result {} does not contain query term 'rust'",
            result.doc_id
        );
    }
}
```

### 4.5 Known-Answer Tests

**Purpose:** Validate scoring correctness against reference implementations

**Example Known-Answer Test:**

```rust
// tests/unit/leit_score/known_answer_test.rs

#[test]
fn test_bm25_known_answers() {
    /// Validates BM25 scores match reference implementation
    /// Test data from: tests/fixtures/expected/bm25_scores.json
    /// Reference: Hand-calculated scores for small collection
    
    let test_cases = load_known_answers("fixtures/expected/bm25_scores.json");
    
    for case in test_cases {
        let doc = create_document(&case.document);
        let collection = create_collection(&case.collection);
        
        let scorer = BM25Scorer::new(case.k1, case.b);
        let actual_score = scorer.score(&doc, &collection, &case.term);
        
        assert!((actual_score - case.expected_score).abs() < 1e-6,
            "BM25 score mismatch for doc '{}', term '{}': \
             expected {:.10}, got {:.10}",
            case.document.id, case.term, case.expected_score, actual_score
        );
    }
}
```

**Fixture Format (tests/fixtures/expected/bm25_scores.json):**

```json
{
  "test_cases": [
    {
      "description": "Single document, single term match",
      "document": {
        "id": "doc_001",
        "terms": {"rust": 5},
        "length": 100
      },
      "collection": {
        "document_count": 1,
        "total_length": 100,
        "doc_freqs": {"rust": 1}
      },
      "term": "rust",
      "k1": 1.2,
      "b": 0.75,
      "expected_score": 2.4567890123
    },
    {
      "description": "Multiple documents, rare term",
      "document": {
        "id": "doc_042",
        "terms": {"memory": 3},
        "length": 250
      },
      "collection": {
        "document_count": 100,
        "total_length": 25000,
        "doc_freqs": {"memory": 1}
      },
      "term": "memory",
      "k1": 1.2,
      "b": 0.75,
      "expected_score": 4.1234567890
    }
  ]
}
```

---

## 5. Coverage Requirements

### 5.1 Coverage Targets by Risk Level

| Component | Risk Level | Line Coverage | Branch Coverage | Mutation Score |
|-----------|------------|---------------|-----------------|----------------|
| `leit_score::bm25` | Critical | 100% | 100% | ≥ 90% |
| `leit_postings::iterator` | High | 95% | 90% | ≥ 80% |
| `leit_query::planner` | High | 90% | 85% | ≥ 75% |
| `leit_index::writer` | Medium | 85% | 80% | ≥ 70% |
| `leit_text::tokenizer` | Medium | 85% | 80% | ≥ 70% |
| `leit_collect::topk` | Medium | 85% | 80% | ≥ 70% |
| CLI utilities | Low | 70% | 60% | N/A |

### 5.2 Coverage Measurement

```bash
# Generate coverage report
cargo test --workspace
cargo cov -- report --lcov --output-path lcov.info

# Check coverage against thresholds
#!/bin/bash
cargo cov -- report | grep -A 10 "File"

# Fail if critical components below 100%
COVERAGE=$(cargo cov -- report | grep "leit_score/src/bm25.rs" | awk '{print $4}' | sed 's/%//')
if (( $(echo "$COVERAGE < 100" | bc -l) )); then
    echo "FAIL: BM25 coverage below 100%: $COVERAGE%"
    exit 1
fi
```

### 5.3 Mutation Testing

Use mutation testing to verify test quality:

```bash
# Install cargo-mutants
cargo install cargo-mutants

# Run mutation testing on scoring module
cargo mutants --package leit_score --threads 4

# Expected outcome:
# - Mutants caught: ≥ 90% for BM25 module
# - Uncaught mutants require additional tests
```

---

## 6. Test Data and Fixtures

### 6.1 Fixture Organization

```
tests/fixtures/
├── documents/
│   ├── wikipedia_100.json       # 100 Wikipedia-style documents
│   ├── wikipedia_1k.json        # 1K Wikipedia-style documents
│   ├── ecommerce_100.json       # 100 e-commerce product descriptions
│   └── short_100.json           # 100 short documents (tweets)
│
├── queries/
│   ├── term_queries.json        # Known-answer term queries
│   ├── phrase_queries.json      # Known-answer phrase queries
│   ├── boolean_queries.json     # Known-answer boolean queries
│   └── edge_cases.json          # Edge case queries
│
└── expected/
    ├── bm25_scores.json         # Reference BM25 scores
    ├── rankings.json            # Reference rankings
    └── counts.json              # Reference result counts
```

### 6.2 Document Fixture Format

```json
{
  "documents": [
    {
      "id": "doc_001",
      "title": "Rust Programming Language",
      "body": "Rust is a systems programming language focused on safety, concurrency, and performance.",
      "timestamp": "2024-01-15T10:30:00Z"
    }
  ]
}
```

### 6.3 Query Fixture Format

```json
{
  "queries": [
    {
      "id": "q_001",
      "query": "title:rust",
      "type": "term",
      "expected_results": [
        {"doc_id": "doc_001", "score": 2.456, "rank": 1}
      ],
      "expected_count": 1
    },
    {
      "id": "q_002",
      "query": "(rust OR cpp) AND safety",
      "type": "boolean",
      "expected_results": [
        {"doc_id": "doc_001", "score": 3.123, "rank": 1},
        {"doc_id": "doc_042", "score": 1.987, "rank": 2}
      ],
      "expected_count": 2
    }
  ]
}
```

---

## 7. CI Integration

### 7.1 Correctness-Focused CI Pipeline

```yaml
# .github/workflows/test.yml
name: Correctness Tests

on: [pull_request, push]

jobs:
  unit-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      
      - name: Run unit tests
        run: |
          cargo test --workspace --lib
      
      - name: Check coverage
        run: |
          cargo install cargo-cov
          cargo cov -- report --lcov --output-path lcov.info
      
      - name: Verify coverage thresholds
        run: |
          ./scripts/check_coverage.sh

  integration-tests:
    runs-on: ubuntu-latest
    needs: unit-tests
    steps:
      - uses: actions/checkout@v3
      
      - name: Run integration tests
        run: |
          cargo test --workspace --test '*'

  composition-tests:
    runs-on: ubuntu-latest
    needs: [unit-tests, integration-tests]
    steps:
      - uses: actions/checkout@v3
      
      - name: Run composition tests
        run: |
          cargo test --workspace --test composition_*
      
      - name: Validate against known answers
        run: |
          cargo test --workspace known_answer -- --nocapture

  mutation-tests:
    runs-on: ubuntu-latest
    if: github.event_name == 'push' && github.ref == 'refs/heads/main'
    steps:
      - uses: actions/checkout@v3
      
      - name: Run mutation tests
        run: |
          cargo install cargo-mutants
          cargo mutants --package leit_score --threads 4
      
      - name: Verify mutation score
        run: |
          ./scripts/check_mutants.sh
```

### 7.2 Test Reporting

**CI Output Format:**

```
=== Leit Correctness Test Results ===

Unit Tests: PASSED (142/142)
  ✓ leit_core::bitvector (100% coverage)
  ✓ leit_text::tokenizer (92% coverage)
  ✓ leit_score::bm25 (100% coverage) ⚠ CRITICAL
  ✓ leit_query::parser (89% coverage)
  ✓ leit_query::planner (88% coverage)
  ✓ leit_postings::iterator (94% coverage)
  ✓ leit_index::writer (85% coverage)
  ✓ leit_collect::topk (86% coverage)

Integration Tests: PASSED (23/23)
  ✓ index_to_query (5 tests)
  ✓ tokenizer_to_postings (4 tests)
  ✓ score_to_collect (6 tests)
  ✓ parser_to_planner (8 tests)

Composition Tests: PASSED (3/3)
  ✓ wikipedia_scenario (100 docs, 50 queries) - 2.3s
  ✓ ecommerce_scenario (100 docs, 50 queries) - 1.8s
  ✓ short_documents_scenario (100 docs, 50 queries) - 1.2s

Known-Answer Tests: PASSED (15/15)
  ✓ BM25 reference scores (10 cases)
  ✓ Ranking verification (3 scenarios)
  ✓ Result counts (5 queries)

Coverage Summary:
  Critical components (100% required): 100% ✓
  High-risk components (90% required): 91.2% ✓
  Medium-risk components (85% required): 86.8% ✓

Mutation Testing:
  leit_score::bm25: 94.5% mutants caught ✓
  leit_query::planner: 78.2% mutants caught ✓

Total Duration: 45.2s
Status: ALL TESTS PASSED ✓
```

### 7.3 Failure Reporting

On test failure, provide actionable diagnostics:

```rust
#[test]
fn test_bm25_score_accuracy() {
    let results = calculate_bm25_scores(&test_cases);
    
    for (idx, result) in results.iter().enumerate() {
        let expected = &test_cases[idx].expected_score;
        let error = (result - expected).abs();
        
        if error > TOLERANCE {
            panic!(
                "BM25 score mismatch at case {}:\n\
                 Document: {}\n\
                 Term: {}\n\
                 Expected: {:.10}\n\
                 Actual: {:.10}\n\
                 Error: {:.2e}\n\
                 Tolerance: {:.2e}\n\
                 \n\
                 Diagnostic:\n\
                 - TF: {}\n\
                 - DF: {}\n\
                 - Doc length: {}\n\
                 - Avg doc length: {}",
                idx,
                test_cases[idx].document.id,
                test_cases[idx].term,
                expected,
                result,
                error,
                TOLERANCE,
                test_cases[idx].tf,
                test_cases[idx].df,
                test_cases[idx].doc_length,
                test_cases[idx].avg_doc_length
            );
        }
    }
}
```

---

## 8. Acceptance Criteria Checklist

### Unit Tests

- [ ] All modules have unit tests
- [ ] Coverage meets risk-based thresholds
- [ ] All tests document what they validate
- [ ] All tests have explicit pass/fail criteria
- [ ] Critical paths (BM25) have 100% coverage
- [ ] Boundary conditions tested
- [ ] Error cases tested

### Integration Tests

- [ ] All crate-to-crate interactions tested
- [ ] Data flow between crates validated
- [ ] Error propagation across crate boundaries tested
- [ ] Integration points have explicit tests

### Composition Tests

- [ ] End-to-end scenarios work correctly
- [ ] Real-world use cases validated
- [ ] Known-answer tests pass
- [ ] Scoring correctness validated
- [ ] All fixtures load correctly

### Documentation

- [ ] All tests have descriptive names
- [ ] Complex tests have documentation comments
- [ ] Requirements traceability established
- [ ] Test data documented
- [ ] Fixture formats documented

### CI Integration

- [ ] Unit tests run on every commit
- [ ] Integration tests run on every PR
- [ ] Composition tests run on every merge
- [ ] Coverage checked against thresholds
- [ ] Failures produce actionable diagnostics

---

## 9. Verification Commands

### Run All Tests

```bash
# Run all tests
cargo test --workspace

# Run with output
cargo test --workspace -- --nocapture

# Run specific test
cargo test --workspace test_bm25_score_accuracy

# Run tests for specific crate
cargo test --package leit_score
```

### Run Specific Test Suites

```bash
# Unit tests only
cargo test --workspace --lib

# Integration tests only
cargo test --workspace --test '*'

# Composition tests only
cargo test --workspace --test composition_*

# Known-answer tests only
cargo test --workspace known_answer
```

### Coverage Analysis

```bash
# Generate coverage report
cargo install cargo-cov
cargo cov -- report --lcov --output-path lcov.info

# View HTML report
cargo cov -- report --html
open covreport/index.html

# Check specific file coverage
cargo cov -- report | grep "bm25.rs"
```

### Mutation Testing

```bash
# Install mutation testing tool
cargo install cargo-mutants

# Run mutation tests
cargo mutants --package leit_score --threads 4

# Run with output
cargo mutants --package leit_score -- --nocapture

# Check specific function
cargo mutants --package leit_score --function calculate_bm25
```

### Fixture Generation

```bash
# Generate test fixtures (if using generator)
cargo run --bin leit-testgen -- \
    --scenario wikipedia \
    --size 100 \
    --output tests/fixtures/documents/wikipedia_100.json \
    --queries tests/fixtures/queries/term_queries.json
```

---

This specification provides a comprehensive framework for correctness-focused testing of the Leit search engine, following NASA-style software assurance practices with emphasis on scoring accuracy and risk-based test coverage.
