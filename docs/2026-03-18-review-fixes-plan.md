# Code Review Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Clean up file structure, remove noisy section headers, then fix three code review findings: multi-field default query expansion, correct BM25F scoring semantics, and RRF duplicate ID deduplication.

**Architecture:** Structural cleanup first (split oversized files, remove `// ====` banners), then semantic fixes: (1) `PlanningContext` accepts multiple default fields, unfielded terms expand to `OR(field1:term, ...)`. (2) BM25F evaluation aggregates per-field stats per document before scoring once. (3) RRF fusion dedupes IDs per-list.

**Tech Stack:** Rust, no new dependencies.

**Known limitations accepted in this plan:**
- `SearchDictionary::resolve_term` returns `Some(SEARCH_MISSING_TERM_ID)` for terms not in the index (when an analyzer exists), so multi-field expansion won't skip fields at plan time — it creates phantom term nodes that match zero postings. Correct results, slightly larger query plans.
- `try_execute_root` only fast-paths single `Term` roots, so multi-field queries (now OR roots) won't get block-skipping. Acceptable for correctness; can be optimized later.
- BM25F `doc_frequency` is summed across per-field posting list lengths, which overcounts documents appearing in multiple fields. Correct computation requires a cross-field document frequency map, out of scope.

---

## Phase 1: Structural Cleanup

### Task 1: Split `leit_query/src/lib.rs` into modules

The 909-line `lib.rs` mixes query builder DSL, fluent functions, planner, parser, and lowering. Split into focused modules.

**Files:**
- Create: `crates/leit_query/src/builder.rs`
- Create: `crates/leit_query/src/planner.rs`
- Modify: `crates/leit_query/src/lib.rs`

- [ ] **Step 1: Create `builder.rs`**

Move from `lib.rs` into `crates/leit_query/src/builder.rs`:
- `QueryBuilder` struct and all its `impl` methods (lines 39-158)
- The four fluent functions: `term()`, `term_with_field()`, `phrase()`, `phrase_with_slop()` (lines 165-196)
- Helper `query_node_id()` (line 46) — used by both builder and planner, but keep in builder and make `pub(crate)`

The file should start with the copyright header and `use` the needed types from `crate::types`.

- [ ] **Step 2: Create `planner.rs`**

Move from `lib.rs` into `crates/leit_query/src/planner.rs`:
- `Planner` struct and all its `impl` (lines 199-276)
- `Phase1Expr` enum and its `depth()` method (lines 284-310)
- `parse_phase1_query()` (lines 312-371)
- `lower_phase1_expr()` (lines 373-440)
- Helper `checked_len_plus_one()` (line 50)

Import `query_node_id` from `crate::builder`.

- [ ] **Step 3: Update `lib.rs`**

Reduce `lib.rs` to:
- Crate-level attributes (`#![no_std]`, crate doc comment)
- `extern crate` declarations
- `mod types;`, `mod builder;`, `mod planner;`
- `pub use` re-exports from all three modules (preserving the existing public API exactly)

- [ ] **Step 4: Verify compilation and tests**

Run: `cargo test -p leit_query`
Expected: All existing tests pass. Public API unchanged.

- [ ] **Step 5: Commit**

```
refactor: split leit_query lib.rs into builder and planner modules
```

---

### Task 2: Split `leit_index/src/memory.rs` into modules

The 921-line `memory.rs` mixes index builder, search execution, and evaluation engine. Split into focused modules.

**Files:**
- Create: `crates/leit_index/src/builder.rs`
- Create: `crates/leit_index/src/search.rs`
- Modify: `crates/leit_index/src/memory.rs`
- Modify: `crates/leit_index/src/lib.rs`

- [ ] **Step 1: Create `builder.rs`**

Move from `memory.rs` into `crates/leit_index/src/builder.rs`:
- `IndexBuilder` trait (lines 24-37)
- `BuildState` struct (lines 89-126)
- `BlockConfig` struct and `Default` impl (lines 100-111)
- `InMemoryIndexBuilder` struct, its inherent methods, and `impl IndexBuilder` (lines 282-443)
- `build_posting_blocks()` free function (lines 781-826)

Keep internal types `TermEntry`, `PostingEntry`, `FieldMetadata`, `PostingBlock` in `memory.rs` since they're shared between builder and index (they're `pub(crate)`).

- [ ] **Step 2: Create `search.rs`**

Move from `memory.rs` into `crates/leit_index/src/search.rs`:
- `ExecutionWorkspace` struct, its `impl`, and `ScratchSpace` impl (lines 136-280)
- `ExecutionStats` struct (lines 143-153)
- `SearchScorer` enum and all its methods (lines 155-211)
- `SearchDictionary` struct and its `TermDictionary` impl (lines 828-848)

- [ ] **Step 3: Slim down `memory.rs`**

What remains in `memory.rs`:
- Internal data structs: `TermEntry`, `PostingEntry`, `FieldMetadata`, `PostingBlock`, `EvalResult`
- Helper functions: `is_non_unit_boost()`, `u32_to_f32()`
- `InMemoryIndex` struct and all its methods (evaluation engine)
- `impl FieldRegistry for InMemoryIndex` and `impl TermDictionary for InMemoryIndex`
- The unit test for posting blocks

- [ ] **Step 4: Update `lib.rs`**

Add `mod builder;` and `mod search;`. Update `pub use` to re-export from the new modules. The public API must stay identical.

- [ ] **Step 5: Verify compilation and tests**

Run: `cargo test -p leit_index`
Expected: All existing tests pass. Public API unchanged.

- [ ] **Step 6: Commit**

```
refactor: split leit_index memory.rs into builder and search modules
```

---

### Task 3: Remove `// ====` section headers across all crates

Remove the `// ============...` banner comments from all files. These exist in: `leit_core`, `leit_score`, `leit_query`, `leit_text`, `leit_postings`, `leit_collect`.

**Files:**
- Modify: all `src/lib.rs` files listed above, plus `crates/leit_query/src/types.rs`

- [ ] **Step 1: Remove all `// ====` banners**

Delete every 3-line block of the form:
```
// ============================================================================
// Section Name
// ============================================================================
```

across all crate `lib.rs` files and `types.rs`. The types and doc comments that follow them are self-explanatory.

- [ ] **Step 2: Verify compilation**

Run: `cargo check --workspace`
Expected: PASS (purely cosmetic change).

- [ ] **Step 3: Commit**

```
refactor: remove section banner comments across all crates
```

---

## Phase 2: Semantic Fixes

### Task 4: RRF Fusion Duplicate ID Deduplication

Simplest, most isolated fix. No cross-crate dependencies.

**Files:**
- Create: `crates/leit_fusion/tests/dedup.rs`
- Modify: `crates/leit_fusion/src/lib.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/leit_fusion/tests/dedup.rs`:

```rust
// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use leit_fusion::{fuse_default, RankedResult};

#[test]
fn duplicate_ids_in_single_list_are_deduped_to_best_rank() {
    let list_with_dupes = vec![
        RankedResult::new("a", 1),
        RankedResult::new("b", 2),
        RankedResult::new("a", 3), // duplicate of "a"
    ];
    let clean_list = vec![
        RankedResult::new("b", 1),
        RankedResult::new("c", 2),
    ];

    let fused = fuse_default(&[list_with_dupes, clean_list]);

    // "a" should only contribute ONE reciprocal-rank term from list 0 (rank 1),
    // not two terms (rank 1 + rank 3).
    let a_result = fused.iter().find(|r| r.id == "a").expect("a should be in results");
    let b_result = fused.iter().find(|r| r.id == "b").expect("b should be in results");

    // "a": 1/(60+1) from list 0 only = 1/61
    // "b": 1/(60+2) from list 0 + 1/(60+1) from list 1 = 1/62 + 1/61
    assert!(
        b_result.score > a_result.score,
        "b (in both lists) should outscore a (in one list only): b={}, a={}",
        b_result.score, a_result.score
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p leit_fusion --test dedup -- --nocapture`
Expected: FAIL

- [ ] **Step 3: Implement the dedup fix**

In `crates/leit_fusion/src/lib.rs`, replace the rank collection loop in `fuse()`:

```rust
// OLD:
    for list in ranked_lists {
        for result in list {
            ranks_by_id
                .entry(result.id.clone())
                .or_default()
                .push(result.rank);
        }
    }
```

With:

```rust
    for list in ranked_lists {
        let mut best_in_list: BTreeMap<&str, usize> = BTreeMap::new();
        for result in list {
            let entry = best_in_list.entry(&result.id).or_insert(usize::MAX);
            *entry = (*entry).min(result.rank);
        }
        for (id, rank) in best_in_list {
            ranks_by_id
                .entry(String::from(id))
                .or_default()
                .push(rank);
        }
    }
```

- [ ] **Step 4: Run all fusion tests**

Run: `cargo test -p leit_fusion`
Expected: PASS

- [ ] **Step 5: Commit**

```
feat: dedupe duplicate IDs per-list in RRF fusion
```

---

### Task 5: Multi-Field Default Query Expansion — `PlanningContext` + Lowering

Change `PlanningContext` from single `default_field` to `default_fields: Vec<FieldId>` and update lowering to expand unfielded terms to OR.

**Files:**
- Modify: `crates/leit_query/src/types.rs`
- Modify: `crates/leit_query/src/planner.rs` (was `lib.rs` before Task 1 split)

- [ ] **Step 1: Change `PlanningContext` struct**

In `crates/leit_query/src/types.rs`:

Remove `Copy` from the derive (struct now contains `Vec`). Change `default_field: leit_core::FieldId` to `default_fields: Vec<leit_core::FieldId>`.

- [ ] **Step 2: Update constructor and builder methods**

In `crates/leit_query/src/types.rs`:

- `new()`: initialize `default_fields: Vec::new()`
- Add `with_default_fields(mut self, fields: Vec<FieldId>) -> Self`
- Change `with_default_field` to set `default_fields = alloc::vec![field]` (backward compat)
- `with_default_boost`: remove `const fn` (struct contains `Vec`)
- Update `Debug` impl to reference `default_fields`

- [ ] **Step 3: Update `lower_phase1_expr` for multi-field expansion**

In `crates/leit_query/src/planner.rs`, replace the `Phase1Expr::Term` arm's unfielded branch:

- If `default_fields.len() == 1`: resolve against that single field (same as before)
- If `default_fields.is_empty()`: return `ParseError`
- If multiple: iterate `default_fields`, resolve each, collect into `child_ids`. If 0 matched: `UnknownTerm`. If 1 matched: return that term node directly. If 2+: wrap in `QueryNode::Or`.

Full code for this replacement is in the previous plan revision (search for "Multiple default fields: expand to OR").

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p leit_query`
Expected: PASS

- [ ] **Step 5: Commit**

```
feat: expand unfielded terms to OR over multiple default fields
```

---

### Task 6: Add Planner Tests for Multi-Field Defaults

**Files:**
- Modify: `crates/leit_query/tests/planner_phase1.rs`

- [ ] **Step 1: Add tests**

Add `test_planner_expands_bare_term_across_multiple_default_fields` — verifies "rust" with `with_default_fields(vec![title, body])` produces an OR root with 2 Term children.

Add `test_planner_multi_field_skips_fields_where_term_is_absent` — verifies "memory" (only in body) with both fields as defaults produces a single Term node, not OR.

Full test code is in the previous plan revision.

- [ ] **Step 2: Run planner tests**

Run: `cargo test -p leit_query --test planner_phase1`
Expected: PASS

- [ ] **Step 3: Commit**

```
test: add planner tests for multi-field default expansion
```

---

### Task 7: Update `InMemoryIndex` Default Fields

**Files:**
- Modify: `crates/leit_index/src/memory.rs`
- Modify: `crates/leit_index/src/search.rs` (after Task 2 split)

- [ ] **Step 1: Change `default_field()` to `default_fields()`**

In `memory.rs`, replace `fn default_field(&self) -> FieldId` with:

```rust
    fn default_fields(&self) -> Vec<FieldId> {
        let fields: Vec<FieldId> = self.field_stats.values().map(|s| s.field_id).collect();
        if fields.is_empty() {
            self.field_names.values().copied().collect()
        } else {
            fields
        }
    }
```

- [ ] **Step 2: Update `ExecutionWorkspace::plan()`**

In `search.rs`, change `plan()` to call `index.default_fields()` and use `with_default_fields()`.

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p leit_index`
Expected: PASS

- [ ] **Step 4: Commit**

```
feat: InMemoryIndex expands unfielded queries across all indexed fields
```

---

### Task 8: Update Existing Tests + Add Multi-Field Search Test

**Files:**
- Modify: `crates/leit_index/tests/search_behavior.rs`

- [ ] **Step 1: Update `field_qualified_and_mixed_scope_queries_are_stable`**

Change the `mixed_and` assertion from `BTreeSet::from([1, 2])` to `BTreeSet::from([1, 2, 3])` — bare "beta" now expands across both fields, matching doc 3 via field 2.

- [ ] **Step 2: Add `bare_query_matches_documents_in_any_indexed_field`**

Index two docs: doc 1 with "rust" in title, doc 2 with "rust" in body. Bare search for "rust" should return both.

Full test code is in the previous plan revision.

- [ ] **Step 3: Run tests**

Run: `cargo test -p leit_index --test search_behavior`
Expected: PASS

- [ ] **Step 4: Commit**

```
test: update search behavior tests for multi-field query expansion
```

---

### Task 9: Fix BM25F Scoring Semantics

**Files:**
- Modify: `crates/leit_index/src/search.rs` (for `score_term_fields`)
- Modify: `crates/leit_index/src/memory.rs` (for OR evaluation path)

- [ ] **Step 1: Add `score_term_fields` method to `SearchScorer`**

In `search.rs`, add method that takes `field_hits: &[(FieldId, u32, u32, f32)]`. BM25 arm: sum per-field scores with `_field` prefix for unused binding. BM25F arm: build `Vec<FieldStats>`, average the `avg_doc_length` values, call `scorer.score()` once.

Full code is in the previous plan revision.

- [ ] **Step 2: Change OR evaluation for BM25F**

In `memory.rs`, replace the `QueryNode::Or` arm in `evaluate_node`. When `SearchScorer::Bm25F` and all children are `Term` nodes: collect per-doc per-field hits, then call `score_term_fields` once per document. Otherwise fall back to summing (same as BM25 path).

The AND node path is intentionally unchanged — each OR child represents a distinct query term, so summing is correct.

Full code is in the previous plan revision.

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p leit_index`
Expected: PASS

- [ ] **Step 4: Commit**

```
feat: correct BM25F scoring to aggregate field stats before scoring
```

---

### Task 10: Add BM25F Cross-Field Scoring Test

**Files:**
- Modify: `crates/leit_index/tests/search_behavior.rs`

- [ ] **Step 1: Add `search_bm25f` helper and test**

Add `search_bm25f()` helper. Add `bm25f_aggregates_field_stats_for_cross_field_matches` test: doc 1 has "rust" in both fields, doc 2 has "rust" in title only. BM25F search for "rust" should rank doc 1 higher.

Full test code is in the previous plan revision.

- [ ] **Step 2: Run test**

Run: `cargo test -p leit_index --test search_behavior bm25f_aggregates`
Expected: PASS

- [ ] **Step 3: Commit**

```
test: add BM25F cross-field aggregation regression test
```

---

### Task 11: Update Benchmark Assertions

**Files:**
- Modify: `crates/leit_benchmark/src/lib.rs`

- [ ] **Step 1: Update benchmark expected hit shapes**

"rust" now matches docs 1 and 2. "retrieval" now matches docs 1 and 3. "unicode" unchanged (vec![4]).

Use `contains` assertions instead of exact vec equality for the first two queries.

- [ ] **Step 2: Run benchmark tests**

Run: `cargo test -p leit_benchmark`
Expected: PASS

- [ ] **Step 3: Commit**

```
fix: update benchmark assertions for multi-field query expansion
```

---

### Task 12: Full Workspace Verification

- [ ] **Step 1:** `cargo fmt --all --check`
- [ ] **Step 2:** `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [ ] **Step 3:** `cargo doc --workspace --locked --all-features --no-deps --document-private-items`
- [ ] **Step 4:** `cargo test --workspace --all-features`
- [ ] **Step 5:** Fix any issues and commit
- [ ] **Step 6:** `typos`
- [ ] **Step 7:** `taplo fmt --check`
