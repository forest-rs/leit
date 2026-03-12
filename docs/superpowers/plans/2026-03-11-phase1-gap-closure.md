# Phase 1 Gap Closure Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the remaining Phase 1 gaps against the handoff by canonicalizing execution-time query handles, settling reusable collector lifecycle semantics, completing the layered postings cursor surface, and landing a real benchmark harness.

**Architecture:** Keep the kernel explicit. Execution-facing query state should use canonical term handles, collectors should have explicit per-query lifecycle and threshold semantics, postings should expose the layered cursor extension seam needed for later pruning work, and benchmarking should live outside the primary crates in a dedicated harness crate.

**Tech Stack:** Rust workspace crates, cargo tests, property tests, Criterion or a lightweight benchmark binary crate, GitHub Actions.

---

## File Map

**Likely modify:**
- `crates/leit_query/src/lib.rs`
- `crates/leit_query/src/types.rs`
- `crates/leit_query/tests/planner_phase1.rs`
- `crates/leit_query/tests/property_invariants.rs`
- `crates/leit_collect/src/lib.rs`
- `crates/leit_collect/tests/property_invariants.rs`
- `crates/leit_index/src/memory.rs`
- `crates/leit_index/tests/boolean_execution.rs`
- `crates/leit_index/tests/search_behavior.rs`
- `crates/leit_postings/src/lib.rs`
- `crates/leit_postings/tests/property_invariants.rs`
- `Cargo.toml`
- `README.md`
- `docs/design.md`

**Likely create:**
- `crates/leit_bench/Cargo.toml`
- `crates/leit_bench/src/main.rs`
- `crates/leit_bench/README.md`
- `crates/leit_bench/fixtures/` or a minimal inline benchmark workload

**Potential deletes/replacements:**
- replace ad hoc scorerless search helpers in `crates/leit_index/src/memory.rs`
- retire or rewrite tests that assume the current collector/search convenience API

---

## Chunk 1: Canonicalized Execution Query Form

### Task 1: Lock the intended execution form in failing tests

**Files:**
- Modify: `crates/leit_query/tests/planner_phase1.rs`
- Modify: `crates/leit_query/tests/property_invariants.rs`

- [ ] **Step 1: Write a failing planner test for canonical term handles**

Add a test that plans a textual query twice against the same dictionary and asserts the execution-facing result uses the same `TermId` handles rather than preserving raw term strings.

- [ ] **Step 2: Run the targeted query tests to verify the failure**

Run: `cargo test -p leit_query --test planner_phase1 canonical -- --nocapture`

Expected: FAIL because the current public execution-adjacent query form still centers `QueryProgram` on strings and only canonicalizes later in `PlannedQueryProgram`.

- [ ] **Step 3: Write a failing invariant test for the public execution-facing representation**

Add a test in `property_invariants.rs` asserting that the execution-facing representation exposed to `leit_index` does not require string term lookup once planning completes.

- [ ] **Step 4: Run the targeted invariant test to verify the failure**

Run: `cargo test -p leit_query --test property_invariants execution_facing -- --nocapture`

Expected: FAIL for the same reason.

### Task 2: Refactor `leit_query` so execution uses canonical handles

**Files:**
- Modify: `crates/leit_query/src/lib.rs`
- Modify: `crates/leit_query/src/types.rs`
- Modify: `crates/leit_index/src/memory.rs`

- [ ] **Step 1: Decide and codify the split**

Keep one calm user-facing query form for construction/parsing if needed, but make the execution-facing plan type the only thing `leit_index` consumes. Do not let raw term strings survive past planning.

- [ ] **Step 2: Implement the minimal code to pass the canonical-handle tests**

Refactor `Planner` and its output types so execution inputs are canonicalized and validated before index execution starts.

- [ ] **Step 3: Update `leit_index` to consume only the canonicalized execution form**

Remove any remaining string-dependent execution assumptions from `memory.rs`.

- [ ] **Step 4: Re-run the focused query tests**

Run: `cargo test -p leit_query --test planner_phase1`

Expected: PASS

- [ ] **Step 5: Re-run the query invariant tests**

Run: `cargo test -p leit_query --test property_invariants`

Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/leit_query/src/lib.rs crates/leit_query/src/types.rs crates/leit_query/tests/planner_phase1.rs crates/leit_query/tests/property_invariants.rs crates/leit_index/src/memory.rs
git commit -m "refactor: canonicalize execution query handles"
```

---

## Chunk 2: Reusable Collector Lifecycle and Threshold Semantics

### Task 3: Write failing tests for reusable collector lifecycle

**Files:**
- Modify: `crates/leit_collect/tests/property_invariants.rs`
- Modify: `crates/leit_index/tests/boolean_execution.rs`

- [ ] **Step 1: Add a failing collector reuse test**

Write a test that reuses one `TopKCollector` across two logical queries and asserts old hits do not leak into the second run.

- [ ] **Step 2: Run the collector test to verify it fails**

Run: `cargo test -p leit_collect --test property_invariants reuse -- --nocapture`

Expected: FAIL because the trait has no explicit lifecycle/reset contract.

- [ ] **Step 3: Add a failing integration test for explicit threshold/output behavior**

Write an index-level test that exercises collector reset, collection, and finalization explicitly.

- [ ] **Step 4: Run the integration test to verify it fails**

Run: `cargo test -p leit_index --test boolean_execution collector -- --nocapture`

Expected: FAIL because `Collector` currently only exposes `collect`, `can_skip`, and `len`.

### Task 4: Implement explicit collector lifecycle

**Files:**
- Modify: `crates/leit_collect/src/lib.rs`
- Modify: `crates/leit_collect/tests/property_invariants.rs`
- Modify: `crates/leit_index/src/memory.rs`
- Modify: `crates/leit_index/tests/boolean_execution.rs`

- [ ] **Step 1: Add explicit lifecycle methods to `Collector`**

Introduce a minimal lifecycle such as `begin_query`, `collect`, `threshold`, and `finish_query`, with names aligned to the handoff rather than hidden reset semantics.

- [ ] **Step 2: Implement the minimal `TopKCollector` changes**

Reuse capacity across queries, keep threshold explicit, and make finalization/output a first-class part of the API.

- [ ] **Step 3: Update `leit_index` execution to use the new collector lifecycle**

Make query execution start and finish the collector explicitly instead of assuming fresh ownership on each call.

- [ ] **Step 4: Re-run focused collector tests**

Run: `cargo test -p leit_collect --test property_invariants`

Expected: PASS

- [ ] **Step 5: Re-run the affected `leit_index` tests**

Run: `cargo test -p leit_index --test boolean_execution`

Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/leit_collect/src/lib.rs crates/leit_collect/tests/property_invariants.rs crates/leit_index/src/memory.rs crates/leit_index/tests/boolean_execution.rs
git commit -m "refactor: add reusable collector lifecycle"
```

---

## Chunk 3: Layered Postings Cursor Surface

### Task 5: Add a failing test for the block-aware cursor seam

**Files:**
- Modify: `crates/leit_postings/tests/property_invariants.rs`

- [ ] **Step 1: Add a compile-level or unit-level test for a block-aware extension trait**

The test does not need real block skipping yet. It should assert that a public extension seam exists and can be implemented or queried.

- [ ] **Step 2: Run the postings tests to verify it fails**

Run: `cargo test -p leit_postings --test property_invariants block -- --nocapture`

Expected: FAIL because only `DocCursor` and `TfCursor` exist today.

### Task 6: Implement the public extension seam without overbuilding Phase 2

**Files:**
- Modify: `crates/leit_postings/src/lib.rs`
- Modify: `crates/leit_postings/tests/property_invariants.rs`

- [ ] **Step 1: Add the public block-aware extension trait**

Add a small `BlockCursor`-style trait or equivalent extension trait that does not force an implementation strategy yet.

- [ ] **Step 2: Provide the minimal in-memory implementation or explicit unsupported behavior**

Do only enough to stabilize the Phase 1 surface. Do not implement compressed postings or real pruning logic here.

- [ ] **Step 3: Re-run postings tests**

Run: `cargo test -p leit_postings --test property_invariants`

Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/leit_postings/src/lib.rs crates/leit_postings/tests/property_invariants.rs
git commit -m "feat: add block-aware postings cursor seam"
```

---

## Chunk 4: Benchmark Harness and Phase 1 Docs

### Task 7: Add a failing repository-level expectation for a real benchmark crate

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add the benchmark crate to the workspace manifest before implementation**

This should fail until the crate exists.

- [ ] **Step 2: Run workspace metadata or build to verify the failure**

Run: `cargo check -p leit_bench`

Expected: FAIL because the crate has not been created yet.

### Task 8: Implement the minimal wind-tunnel benchmark harness

**Files:**
- Create: `crates/leit_bench/Cargo.toml`
- Create: `crates/leit_bench/src/main.rs`
- Create: `crates/leit_bench/README.md`
- Modify: `Cargo.toml`
- Modify: `README.md`
- Modify: `docs/design.md`

- [ ] **Step 1: Create the benchmark crate skeleton**

Add a small standalone benchmark binary crate in the workspace. Keep dependencies light and keep it outside the kernel crates.

- [ ] **Step 2: Implement one deterministic indexing/query benchmark path**

Use the current in-memory Phase 1 stack on a fixed synthetic workload. The point is to land a real harness, not a full FTSB clone.

- [ ] **Step 3: Document how to run it**

Update crate README and root docs with the exact command.

- [ ] **Step 4: Re-run the targeted crate build**

Run: `cargo build -p leit_bench`

Expected: PASS

- [ ] **Step 5: Re-run full workspace verification**

Run: `cargo fmt --all --check`

Expected: PASS

Run: `cargo clippy --all-targets -- -D warnings`

Expected: PASS

Run: `cargo test`

Expected: PASS

Run: `cargo doc --no-deps`

Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/leit_bench README.md docs/design.md
git commit -m "feat: add phase1 benchmark harness"
```

---

## Chunk 5: Final API and Doc Reconciliation

### Task 9: Reconcile `leit_index` Phase 1 docs with the explicit execution direction

**Files:**
- Modify: `crates/leit_index/src/memory.rs`
- Modify: `README.md`
- Modify: `docs/design.md`

- [ ] **Step 1: Add a failing test if the scorerless convenience path is being removed now**

If Phase 1 cleanup includes removing `search`/`search_with_workspace`, write the migration tests first.

- [ ] **Step 2: Implement the minimal API cleanup**

Either:
- remove the scorerless helpers now, or
- document them explicitly as temporary Phase 1 convenience wrappers and keep them out of the steering story.

- [ ] **Step 3: Re-run affected index tests**

Run: `cargo test -p leit_index --test search_behavior`

Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/leit_index/src/memory.rs README.md docs/design.md crates/leit_index/tests/search_behavior.rs
git commit -m "refactor: align phase1 execution api"
```

---

Plan complete and saved to `docs/superpowers/plans/2026-03-11-phase1-gap-closure.md`. Ready to execute?
