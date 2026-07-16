# Reusable Execution Infrastructure Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Preserve an independent executable reference, add trustworthy allocation measurement, reuse query/decode storage, flatten finalized postings, and record reproducible query/index allocation baselines.

**Architecture:** Maintain an independent BTreeMap-of-Vec representation and evaluator composition in a `bench-internals`-only module while sharing stable scorer primitives. Keep the caller-owned `ExecutionWorkspace` and collector reset contracts, add reusable scratch and a scoped owner-thread allocator counter, then convert finalized `InMemoryIndex` postings to a dense range table over one arena while retaining exact reference parity and hard-coded score-bit goldens.

**Tech Stack:** Rust 2024, `no_std` + `alloc` core crates, `std::alloc::GlobalAlloc` instrumentation in `leit_wind_tunnel`, Criterion bench crates, Cargo feature gates, Jujutsu (`jj`), project scenario/citation validators.

---

Implementation protocol: use @test-driven-development for every RED/GREEN pair, @implementing-tasks for one fresh implementer per task, and @verification-before-completion before every commit and iteration closure. Do not put iteration/story/scenario IDs in Rust code, rustdoc, test names, or commit subjects.

## File responsibility map

- `crates/leit_index/src/reference_execution.rs`: feature-gated frozen BTreeMap-of-Vec index, copied evaluator, and narrow primitive snapshot façade.
- `crates/leit_index/tests/reference_execution_parity.rs`: named BM25/BM25F/boolean/filter parity matrix and feature-boundary proof.
- `crates/leit_wind_tunnel/src/allocation.rs`: allocator wrapper, exclusive owner-thread measurement lease, frozen counter semantics, and snapshots; never installs a global allocator itself.
- `crates/leit_wind_tunnel/tests/allocation_counter.rs`: test-binary global allocator plus isolation, failure, concurrency, foreign-thread, and unwind tests.
- `crates/leit_collect/src/lib.rs`: capacity-preserving `TopKCollector::finish_into` seam.
- `crates/leit_collect/tests/finish_into.rs`: ordering, tie, and capacity regression tests.
- `crates/leit_index/src/search.rs`: reusable execution/evaluation/decode scratch owned by `ExecutionWorkspace`, plus gated capacity snapshots.
- `crates/leit_postings/Cargo.toml`, `crates/leit_collect/Cargo.toml`, and `crates/leit_index/Cargo.toml`: explicit `bench-internals` forwarding for proof-only capacity/layout adapters.
- `crates/leit_index/src/memory.rs`: evaluator methods consuming workspace scratch and finalized flat posting access.
- `crates/leit_index/src/builder.rs`: dense term-order conversion from build-time maps into the finalized posting arena/range table.
- `crates/leit_index/src/segment_format/block_meta.rs`: logical 12-byte record definition without native-record byte casts.
- `crates/leit_index/src/segment_format/writer.rs`: explicit little-endian emission for legacy and compressed block records.
- `crates/leit_index/src/segment_format/readers.rs`: checked indexed, bounded, unaligned-safe block-record decoding.
- `crates/leit_index/tests/reusable_execution.rs`: preplanned workspace reuse and compressed decode behavior.
- `crates/leit_index/tests/hot_layout.rs`: exact posting/range/SoA/block-byte structural proofs.
- `crates/leit_wind_tunnel/tests/query_allocation_baseline.rs`: retained-result fresh-versus-reused totals for named queries.
- `crates/leit_wind_tunnel/tests/index_allocation_baseline.rs`: separate insertion/finalization totals.
- `crates/leit_wind_tunnel_query/Cargo.toml` and `crates/leit_wind_tunnel_index/Cargo.toml`: enable only the explicit bench/test features needed by measurement targets.
- `docs/2026-07-15-hot-layout-tradeoffs.md`: exact byte budgets, access paths, and explicit absence of cache claims.
- `docs/2026-07-15-iteration-7-allocation-baselines.md`: observed command output for named query/index windows, with no latency or invented regression gate.
- `docs/superpowers/iterations/{behavior-scenarios.md,behavior-corpus.md,coverage-ledger.md,roadmap.md,iteration-log.md,progress.md}` and `requirements/EPIC-006.md`: closure evidence and status.

## Chunk 1: Reference, instrumentation, and reusable query state

### Task 1: Freeze the benchmark-only reference executor

**Files:**
- Create: `crates/leit_index/src/reference_execution.rs`
- Create: `crates/leit_index/tests/reference_statistics.rs`
- Create: `crates/leit_index/tests/reference_execution_parity.rs`
- Create: `crates/leit_index/tests/reference_feature_boundary.rs`
- Modify: `crates/leit_index/src/lib.rs:24-50`
- Verify: `crates/leit_index/Cargo.toml:22-25`

- [ ] **Step 1: Write the separate statistics-only target**

In `reference_statistics.rs`, define one two-field document slice and `make_analyzers()` that constructs a fresh registry per index. Build the optimized index and missing reference from identical aliases/documents; assert only `(document_count, field_id/doc_count/total_terms)` tuples. Do not create/import `execute_snapshot` or the parity target yet.

```rust
let reference = ReferenceExecutionIndex::from_documents(make_analyzers(), &aliases, &documents)?;
assert_eq!(reference.statistics_snapshot(), optimized_statistics(&optimized));
```

- [ ] **Step 2: Run the feature-enabled test and observe RED**

Run: `rtk cargo test -p leit_index --features bench-internals --test reference_statistics`

Expected: FAIL with unresolved import `leit_index::ReferenceExecutionIndex`.

- [ ] **Step 3: Implement only frozen construction/statistics**

Add the gated module/re-export. Copy construction/statistics into private `ReferencePosting`, `ReferenceTerm`, and BTreeMap-of-Vec storage; expose only `from_documents` and `statistics_snapshot`. Rerun Step 2. Expected GREEN; `execute_snapshot` is not declared.

```rust
#[doc(hidden)]
pub struct ReferenceExecutionIndex { /* all fields private */ }

#[doc(hidden)]
impl ReferenceExecutionIndex {
    pub fn from_documents(
        analyzers: FieldAnalyzers,
        aliases: &[(FieldId, &str)],
        documents: &[(u32, &[(FieldId, &str)])],
    ) -> Result<Self, IndexError>;

    pub fn statistics_snapshot(&self) -> (u32, Vec<(u32, u32, u32)>);

}
```

The module must not return `PostingEntry`, a postings slice/map, a cursor, or an `ExecutableIndex` reference. Deterministic construction assigns the same canonical term IDs as the optimized builder so the shared plan is meaningful.

- [ ] **Step 4: Create parity target with only single-term BM25 RED**

Create `reference_execution_parity.rs` with the shared fixture/filter helpers and only `fielded_bm25_term_scores_match`; plan once on optimized and use the same plan/top-1/top-16 on both. Run its filtered command. Expected compile RED: `execute_snapshot` is absent.

- [ ] **Step 5: Declare façade and implement only leaf scoring**

Now declare `execute_snapshot(&ExecutionPlan, SearchScorer, &F, usize)`. Copy term traversal/collection while calling stable `SearchScorer` primitives; every non-Term node returns private `ReferenceEvalError::UnsupportedNode(NodeKind)`, mapped at the façade to an existing `IndexError`, with an in-module behavioral assertion of the exact private kind before mapping. Rerun Step 4. Expected GREEN with exact bits.

- [ ] **Step 6: Write the fielded OR RED row**

Add only `fielded_or_combines_term_scores`; run: `rtk cargo test -p leit_index --features bench-internals --test reference_execution_parity fielded_or_combines_term_scores`. Expected runtime RED through private `UnsupportedNode(Or)`; target compiles.

- [ ] **Step 7: Implement boolean operators and rerun Step 6**

Copy `Or`/term expansion, `And`, `Not`, and `ConstantScore` BTreeSet/BTreeMap behavior. Expected GREEN with exact bits/order.

- [ ] **Step 8: Write the filter RED row**

Add only `fielded_term_rejects_document_29`; run: `rtk cargo test -p leit_index --features bench-internals --test reference_execution_parity fielded_term_rejects_document_29`. Expected runtime RED through private `UnsupportedNode(ExternalFilter)`; target compiles.

- [ ] **Step 9: Implement only filtering and rerun Step 8**

Copy `ExternalFilter` dispatch/retention. Expected GREEN at top-1/top-16.

- [ ] **Step 10: Write the BM25F RED row**

Add only `unfielded_bm25f_combines_field_hits`; run: `rtk cargo test -p leit_index --features bench-internals --test reference_execution_parity unfielded_bm25f_combines_field_hits`. Expected runtime RED through private `UnsupportedNode(TermExpansion)`; target compiles.

- [ ] **Step 11: Implement only BM25F and rerun Step 10**

Copy the `eval_bm25f_term_expansion` composition exactly (zero-TF fields, average lengths, boosts, ties) while calling the stable `score_term_fields` primitive. Expected GREEN with exact bits.

- [ ] **Step 12: Prove the real Cargo feature boundary**

In `reference_feature_boundary.rs`, derive the dependency path from `PathBuf::from(env!("CARGO_MANIFEST_DIR"))`, write a unique temp consumer importing `ReferenceExecutionIndex`, copy and offline-normalize the workspace lock for that standalone root, and run both nested checks with `--locked --offline` plus a unique target. Assert feature-off unresolved import; rewrite only the dependency with `features = ["bench-internals"]` and assert success. Cleanup via guard.

Run: `rtk cargo test -p leit_index --test reference_feature_boundary -- --test-threads=1`

Expected: PASS after observing the feature-off compile failure and feature-on compile success.

- [ ] **Step 13: Prove the complete matrix and isolated normal docs**

Run: `rtk cargo test -p leit_index --features bench-internals --test reference_execution_parity`

Expected: PASS for all eight matrix rows.

The reference independently freezes storage, traversal, and evaluator composition while intentionally sharing stable `SearchScorer` primitives. Hard-coded score-bit goldens for fielded BM25, BM25 OR composition, unfielded BM25F field aggregation, and ConstantScore pin those primitive outputs against shared-scorer drift.

Run: `rtk cargo check -p leit_index --no-default-features --features std`

Expected: PASS without compiling `reference_execution`.

Run: `normal_doc_target="$(mktemp -d /private/tmp/leit-normal-doc.XXXXXX)"`, then `CARGO_TARGET_DIR="$normal_doc_target" rtk cargo rustdoc -p leit_index --no-default-features --features std -- -D warnings`.

Expected: PASS; `rtk proxy rg -n "ReferenceExecutionIndex|ReferencePosting" "$normal_doc_target/doc/leit_index"` returns no matches; archive/remove the unique directory through its guard.

- [ ] **Step 14: Commit the frozen reference executor**

```bash
rtk jj file track crates/leit_index/src/reference_execution.rs crates/leit_index/tests/reference_statistics.rs crates/leit_index/tests/reference_execution_parity.rs crates/leit_index/tests/reference_feature_boundary.rs
rtk jj commit -m "test(index): preserve reference execution oracle"
```

### Task 2: Add the scoped shared allocation counter

**Files:**
- Create: `crates/leit_wind_tunnel/src/allocation.rs`
- Create: `crates/leit_wind_tunnel/tests/allocation_counter.rs`
- Modify: `crates/leit_wind_tunnel/src/lib.rs:44-48`

- [ ] **Step 1: Write counter contract tests with a test-binary global allocator**

Install `static GLOBAL: CountingAllocator<System> = CountingAllocator::new(System)` only in `allocation_counter.rs`. Test disabled/reset/alloc/dealloc/realloc semantics, foreign-thread exclusion, nested/concurrent rejection, unwind recovery, and post-finish exclusion. Test null alloc/realloc only by direct unsafe `GlobalAlloc` calls on `CountingAllocator<NullAllocator>`; never construct a `Vec` with the null allocator.

- [ ] **Step 2: Run the counter test and observe RED**

Run: `rtk cargo test -p leit_wind_tunnel --test allocation_counter -- --test-threads=1`

Expected: FAIL with unresolved module `leit_wind_tunnel::allocation`.

- [ ] **Step 3: Implement the non-installing wrapper and exclusive lease**

On `allocation.rs` and the direct-allocator test scope only, add `#[expect(unsafe_code, reason = "test/benchmark GlobalAlloc wrapper delegates to the inner allocator")]`; this covers the unsafe impl/method bodies and direct `NullAllocator` calls while the workspace deny remains effective everywhere else. Implement `CountingAllocator<A> { inner: A, counters: Counters }`, where `Counters` owns only this wrapper's atomic call/byte fields. A separate process-wide `static LEASED` serializes all wrappers. Implement `pub const fn new(inner: A) -> Self` and `unsafe impl<A: GlobalAlloc> GlobalAlloc`. Use:

```rust
thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
}
static LEASED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AllocationSnapshot {
    pub alloc_calls: u64,
    pub realloc_calls: u64,
    pub dealloc_calls: u64,
    pub allocated_bytes: u64,
    pub released_bytes: u64,
}

impl<A: GlobalAlloc> CountingAllocator<A> {
    pub fn try_start_counting(&'static self) -> Result<AllocationLease<'static>, AllocationCounterError>;
}
impl AllocationLease<'_> {
    pub fn finish(self) -> AllocationSnapshot;
}
```

Acquire process-wide `LEASED`, reset `self.counters`, then enable TLS; injected TLS failure releases `LEASED`. Give the lease `active` and `PhantomData<Rc<()>>`. `finish`/`Drop` use nonpanicking `try_with`; finish disables, snapshots wrapper counters while exclusive, releases, then marks inactive. In `allocation.rs` `#[cfg(test)]`, inject TLS failure and test release with `rtk cargo test -p leit_wind_tunnel --lib allocation`; integration tests cover `catch_unwind`, ignored post-panic work, and reacquisition.

- [ ] **Step 4: Run focused tests and library checks**

Run: `rtk cargo test -p leit_wind_tunnel --test allocation_counter -- --test-threads=1`

Expected: PASS; `finish` excludes allocations made after disable, nested/concurrent acquisition returns `AlreadyActive`, foreign-thread counters remain unchanged, and panic-drop cleanup permits reacquisition.

Run: `rtk cargo clippy -p leit_wind_tunnel --all-targets -- -D warnings`

Expected: PASS; no library target contains `#[global_allocator]`, the scoped `#[expect(unsafe_code, reason = "test/benchmark GlobalAlloc wrapper delegates to the inner allocator")]` is fulfilled, and no broader unsafe-code allowance appears.

- [ ] **Step 5: Commit allocator instrumentation**

```bash
rtk jj file track crates/leit_wind_tunnel/src/allocation.rs crates/leit_wind_tunnel/tests/allocation_counter.rs
rtk jj commit -m "feat(wind-tunnel): add scoped allocation counter"
```

### Task 3: Preserve collector and result-sink capacity

**Files:**
- Modify: `crates/leit_collect/Cargo.toml:18-24`
- Modify: `crates/leit_index/Cargo.toml:22-32`
- Modify: `crates/leit_collect/src/lib.rs:155-210`
- Create: `crates/leit_collect/tests/finish_into.rs`

- [ ] **Step 1: Write ordering and capacity tests**

Add `bench-internals = []` to `leit_collect` and forward it from `leit_index`'s feature. Warm a collector with more than `k` candidates and a sink reserved to at least `k`; assert nonzero retained hits. Compare `finish_into` with `finish` including ties, then run two begin/refill/finish_into cycles and assert gated heap capacity plus sink capacity never grows.

- [ ] **Step 2: Run the focused test and observe RED**

Run: `rtk cargo test -p leit_collect --features bench-internals --test finish_into`

Expected: FAIL because `TopKCollector::finish_into` does not exist.

- [ ] **Step 3: Add the nonbreaking extraction seam**

```rust
pub fn finish_into(&mut self, output: &mut Vec<ScoredHit<Id>>) {
    output.clear();
    while let Some(ReverseHit(hit)) = self.heap.pop() {
        output.push(hit);
    }
    output.reverse();
    self.min_score = Score::MIN;
}
```

Adapt the pattern to the actual `ReverseHit` fields. Do not use `mem::take`, `sort`, `collect`, or replace either allocation. Keep `finish` unchanged for compatibility.

Under `bench-internals`, add hidden `benchmark_heap_capacity()`; normal builds expose no capacity inspection.

- [ ] **Step 4: Run collector regression tests**

Run: `rtk cargo test -p leit_collect --features bench-internals --test finish_into`

Expected: PASS with exact order and stable capacities.

Run: `rtk cargo test -p leit_collect --all-features`

Expected: PASS for unit and property suites.

- [ ] **Step 5: Commit reusable extraction**

```bash
rtk jj file track crates/leit_collect/tests/finish_into.rs
rtk jj commit -m "feat(collect): reuse top-k result sinks"
```

### Task 4: Reuse preplanned execution scratch

**Files:**
- Modify: `crates/leit_index/src/search.rs:16-22,152-331`
- Modify: `crates/leit_index/src/memory.rs:81-99,220-407,753-908`
- Create: `crates/leit_index/tests/reusable_execution.rs`
- Create: `crates/leit_index/tests/reusable_execution_capacities.rs`
- Create: `crates/leit_wind_tunnel/tests/execution_allocation.rs`
- Modify: `crates/leit_wind_tunnel/Cargo.toml:18-25`

- [ ] **Step 1: Capture the existing DAG/statistics GREEN guard**

Build a valid DAG whose term child occurs under both `Or` and `And`; assert exact result bits and current `ExecutionStats` (shared occurrence traversed twice), without capacity APIs. Run: `rtk cargo test -p leit_index --test reusable_execution shared_child_preserves_visit_stats`. Expected characterization GREEN before refactor.

- [ ] **Step 2: Add the missing-capacity compile RED**

In the separate capacity target, call absent `benchmark_scratch_capacities`; run: `rtk cargo test -p leit_index --features bench-internals --test reusable_execution_capacities`. Expected compile RED without blocking characterization target compilation.

- [ ] **Step 3: Add borrow-safe occurrence stack/frame pool**

Add workspace-owned `work_stack`, `frame_pool`, `free_frames`, terms/fields/doc/scoring buffers, and two reserved spare frame indices. Use `evaluate_occurrence(index, program, root, scratch: &mut EvaluationScratch, stats: &mut ExecutionStats)`: `execute` splits scratch from separate `last_stats`. Pop each `WorkItem` by value; shared-child edges push new occurrences. Scope immutable child borrows to copy through a checked split-slice helper into the spare index, end borrows, then swap accumulator/spare indices—never Vecs/`mem::take`. Validation precludes cycles; missing nodes and impossible private phases safely complete empty without a new public error. Rerun Step 1. Expected GREEN; Step 2 remains intentionally RED until Step 11.

- [ ] **Step 4: Interim review checkpoint A**

Before routing the existing evaluator, obtain paired spec/quality approval for occurrence traversal, shared-child revisit, pool ownership, and invariant handling.

- [ ] **Step 5: Add sorted-combinator RED tests**

Test score-adding union, score-summing intersection, difference, constant-score, and filter helpers. Run: `rtk cargo test -p leit_index memory::tests::scratch_`. Expected RED: helpers absent.

- [ ] **Step 6: Route every evaluator entry through scratch**

Implement two-index doc-sorted operations into reserved frames. Thread the split scratch/stats through `execute`, scored/unscored direct roots, both `ConstantScore` fast paths, and fallback; remove/bypass old allocating evaluate methods/results. Rerun Step 5 plus `rtk cargo test -p leit_index --test boolean_execution --test filter_execution --test search_behavior`. Expected GREEN with exact order/stats.

- [ ] **Step 7: Write the complete warmed-allocation RED matrix**

Add rows for scored direct-root BM25/BM25F, BM25F fallback, OR/AND/NOT, scored ConstantScore, ExternalFilter, plus unscored direct-root, unscored ConstantScore, and unscored fallback. Warm all state. Scored windows use a warmed `TopKCollector` and prewarmed sink with exactly lease→execute→`finish_into`→`lease.finish`; retained-hit parity, capacities, assertions, formatting, and destruction are checked afterward. Unscored windows use a warmed reusable `CountCollector`: `execute` invokes its `begin_query` reset inside the lease, then `lease.finish` disables measurement before the primitive count is read/finished outside; assert exact fresh-path count parity, stable workspace capacities, and zero calls. Run: `rtk cargo test -p leit_wind_tunnel --test execution_allocation -- --test-threads=1`. Expected RED: one or more rows allocate.

- [ ] **Step 8: Replace BM25F maps/temp vectors with sorted scratch**

Populate `terms: Vec<(FieldId,TermId,f32)>`, `fields: Vec<(FieldId,f32,f32)>`, and `doc_hits: Vec<DocFieldHit>`; sort/dedup fields, sort/coalesce hits by `(doc_id,field_id)`, scan unique docs into the output frame, and refill `field_hits` (with per-field averages) plus `scoring_fields` (including zero-TF fields). Derive doc frequency in the scan. No BM25F BTreeMap/BTreeSet/temp Vec remains.

- [ ] **Step 9: Install exact allocation-free scoring seams**

Use `score_bm25_term(Bm25Scorer, tf, doc_len, avg_doc_len, doc_count, doc_freq, boost) -> Score`, `score_bm25_fields(Bm25Scorer, &[FieldHit], doc_count, doc_freq, boost) -> Score`, and `score_bm25f_fields(Bm25FScorer, &[FieldStats], avg_doc_len, doc_count, doc_freq, boost) -> Score`; direct BM25F uses stack `[FieldStats; 1]`. Rerun Step 7. Expected every row exactly zero calls.

- [ ] **Step 10: Write complete capacity RED evidence**

Extend the separate capacity target with named nonzero/stability assertions for outer work-stack/frame-pool/free-list, every inner frame, terms/fields/doc-hits/field-hits/scoring-fields, and both reserved spare frames. Run: `rtk cargo test -p leit_index --features bench-internals --test reusable_execution_capacities`. Expected RED: façade absent.

- [ ] **Step 11: Implement capacity façade and rerun Step 10**

Expose hidden `BenchmarkScratchCapacities` with every named outer/inner/shared capacity. Each fixture asserts used capacities nonzero after warmup and the full snapshot identical after two runs. Rerun Step 10. Expected GREEN.

- [ ] **Step 12: Interim review checkpoint B, regressions, and commit**

Obtain paired approval for all routing/allocation evidence. Run: `rtk cargo test -p leit_index --test boolean_execution --test filter_execution --test search_behavior`. Expected PASS.

```bash
rtk jj commit -m "feat(index): reuse preplanned execution scratch"
```

## Chunk 2: Decode reuse, baselines, flat layout, and closure

### Task 5: Reuse compressed decode scratch

**Files:**
- Modify: `crates/leit_postings/Cargo.toml:20-27`
- Modify: `crates/leit_postings/src/codec.rs` in both decoder implementations
- Modify: `crates/leit_index/Cargo.toml:22-32`
- Modify: `crates/leit_index/src/search.rs:16-22,152-331`
- Modify: `crates/leit_index/src/memory.rs:220-305,673-741`
- Modify: `crates/leit_postings/src/cursor.rs:95-145`
- Modify: `crates/leit_wind_tunnel/Cargo.toml:18-25`
- Extend: `crates/leit_wind_tunnel/tests/allocation_counter.rs`
- Extend: `crates/leit_index/tests/reusable_execution.rs`

- [ ] **Step 1: Write warmed fitting/growth decode tests**

Prepare encoded DeltaVarint and BlockDelta lists before measurement. Warm the workspace-owned decode scratch, decode a fitting list, then a larger list, then the fitting list again. Assert exact postings, parallel docs/TF lengths, stable capacities after warmup, and at most one capacity growth per required buffer for the larger list.

- [ ] **Step 2: Run the decode test and observe RED**

Run: `rtk cargo test -p leit_index --features bench-internals --test reusable_execution compressed_decode_reuses_workspace_scratch`

Expected: FAIL because compressed evaluation creates a local `DecodeScratch::with_capacity` in `memory.rs`.

- [ ] **Step 3: Route compressed decode through workspace ownership**

Add `bench-internals = []` to `leit_postings` and extend the Task-3 `leit_index` forwarding with `leit_postings/bench-internals`. In wind-tunnel dev-dependencies, enable the index feature and add `leit_postings` with `std`. Add gated decode capacities and a production `decode_prepared_postings(PostingsView, warmed sink)` seam; remove hot-path encoding/local scratch. Both decoders reserve their declared posting count once per insufficient SoA buffer.

- [ ] **Step 4: Verify allocation-free fitting decode with the shared counter**

Extend `crates/leit_wind_tunnel/tests/allocation_counter.rs` with encoded-list tests whose global allocator windows contain only prepared cursor open/decode/traverse. Measure a larger-than-capacity decode and assert `realloc_calls <= 2`, then snapshot both capacities; measure the following fitting decode and assert zero calls/bytes plus unchanged capacities. Run: `rtk cargo test -p leit_wind_tunnel --test allocation_counter decode_scratch -- --test-threads=1`

Expected: PASS; larger decode performs at most one realloc per docs/TF buffer, and the following fitting decode performs zero alloc/realloc/dealloc calls and bytes.

Run: `rtk cargo test -p leit_index --features bench-internals --test reusable_execution`

Expected: PASS for both codecs and capacity-growth assertions.

- [ ] **Step 5: Commit workspace-owned decode reuse**

```bash
rtk jj commit -m "feat(index): reuse compressed decode scratch"
```

### Task 6: Record the query allocation baseline

**Files:**
- Create: `crates/leit_wind_tunnel/tests/query_allocation_baseline.rs`
- Modify: `crates/leit_wind_tunnel/Cargo.toml:18-25`
- Modify: `crates/leit_wind_tunnel_query/Cargo.toml:17-25`

- [ ] **Step 1: Write the retained-result baseline test**

Use a named deterministic 100-document query fixture and `N = 32`. Build/plan outside measurement. Warm one workspace, collector, and 32 distinct top-k sinks, then measure 32 preplanned executions, one into each sink, keeping every result live through the snapshot. Separately measure the equivalent fresh path while retaining all 32 freshly created workspaces, collectors, and sinks until after its snapshot. Define `allocation_ops = alloc_calls + realloc_calls`; assert exact result parity, `reused_ops < fresh_ops`, and `reused_32_ops <= reused_1_ops + 1` while continuing to report both component counters.

- [ ] **Step 2: Run the baseline test and observe RED**

Run: `rtk cargo test -p leit_wind_tunnel --test query_allocation_baseline -- --nocapture --test-threads=1`

Expected: FAIL before the test target can import all required `leit_collect`, `leit_query`, and gated index seams.

- [ ] **Step 3: Wire test-only dependencies and exact measurement windows**

Add required workspace crates only under `[dev-dependencies]`. Install `CountingAllocator<System>`. The reused helper returns snapshot plus retained sinks; the fresh helper returns snapshot plus retained `(ExecutionWorkspace, TopKCollector, sink)` tuples. Preallocate ownership pools outside measurement, and drop neither path's state until snapshot/assertions; fixture/planning/reservation/formatting/destruction remain outside each lease.

- [ ] **Step 4: Run and capture stable facts, not performance thresholds**

Run: `rtk cargo test -p leit_wind_tunnel --test query_allocation_baseline -- --nocapture --test-threads=1`

Expected: PASS; output prints alloc/realloc separately plus `allocation_ops` and bytes; reused ops are strictly lower than fresh and remain O(1) from 1 to 32 retained results.

- [ ] **Step 5: Commit the executable query baseline**

```bash
rtk jj file track crates/leit_wind_tunnel/tests/query_allocation_baseline.rs
rtk jj commit -m "test(wind-tunnel): measure query allocation reuse"
```

### Task 7: Separate index insertion and finalization baselines

**Files:**
- Create: `crates/leit_wind_tunnel/tests/index_allocation_baseline.rs`
- Modify: `crates/leit_wind_tunnel/Cargo.toml:18-25`
- Modify: `crates/leit_wind_tunnel_index/Cargo.toml:17-30`
- Create: `docs/2026-07-15-iteration-7-allocation-baselines.md`

- [ ] **Step 1: Write independent indexing-window tests**

Generate one named 100-document fixture and analyzers before either window. Measure only repeated `index_document` calls in the insertion lease. Disable and snapshot, then measure only `builder.build_index()` in a fresh finalization lease. Keep the finished index alive until both snapshots and assertions are complete; exclude merge work.

- [ ] **Step 2: Run the indexing test and observe RED**

Run: `rtk cargo test -p leit_wind_tunnel --test index_allocation_baseline -- --nocapture --test-threads=1`

Expected: FAIL because the indexing baseline target and report do not exist.

- [ ] **Step 3: Implement the two scoped windows and report format**

Print one machine-readable line per phase:

```text
allocation-baseline fixture=index-100 phase=insertion alloc_calls=<n> realloc_calls=<n> dealloc_calls=<n> allocated_bytes=<n> released_bytes=<n>
allocation-baseline fixture=index-100 phase=finalization alloc_calls=<n> realloc_calls=<n> dealloc_calls=<n> allocated_bytes=<n> released_bytes=<n>
```

Record the observed lines and exact command in the baseline document. State that values are local observations, insertion/finalization are separate, fixture/assert/format/destruction are excluded, merge is excluded, and no latency/allocation regression threshold is established.

- [ ] **Step 4: Verify test and prose agree**

Run: `rtk cargo test -p leit_wind_tunnel --test index_allocation_baseline -- --nocapture --test-threads=1`

Expected: PASS with two nonempty phase snapshots.

Run: `rtk proxy rg -n "fixture=index-100|phase=insertion|phase=finalization|merge is excluded|no latency" docs/2026-07-15-iteration-7-allocation-baselines.md`

Expected: six matching evidence lines.

- [ ] **Step 5: Commit indexing baselines**

```bash
rtk jj file track crates/leit_wind_tunnel/tests/index_allocation_baseline.rs docs/2026-07-15-iteration-7-allocation-baselines.md
rtk jj commit -m "test(wind-tunnel): separate indexing allocation phases"
```

### Task 8: Flatten postings and make block bytes explicitly portable

**Files:**
- Modify: `crates/leit_index/src/memory.rs:37-63,110-199,1020-1111`
- Modify: `crates/leit_index/src/index_surface.rs:26-46`
- Modify: `crates/leit_index/src/lib.rs:38-46`
- Modify: `crates/leit_index/src/builder.rs:29-39,211-279`
- Modify: `crates/leit_index/src/merge.rs` at every `InMemoryIndex::new`/postings access site
- Modify: `crates/leit_index/src/serialization.rs` at postings traversal sites
- Modify: `crates/leit_index/src/segment_format/block_meta.rs:28-69`
- Modify: `crates/leit_index/src/segment_format/writer.rs:313-524`
- Modify: `crates/leit_index/src/segment_format/readers.rs:674-819`
- Create: `crates/leit_index/tests/hot_layout.rs`
- Create: `docs/2026-07-15-hot-layout-tradeoffs.md`

- [ ] **Step 1: Write structural RED tests**

In `memory.rs` unit tests, assert crate-private `PostingEntry` size 8/alignment 4 and its crate-private getters' two u32 values. Integration `hot_layout.rs` sees only gated primitive `benchmark_posting_layout()` ranges/lengths/element-size/addresses; assert dense ordered bounded ranges and 8-byte stride. Assert gated primitive SoA 4-byte layout, unaligned known LE reads, and private checked-offset overflow in reader unit tests.

- [ ] **Step 2: Run the layout test and observe RED**

Run: `rtk cargo test -p leit_index --features bench-internals --test hot_layout`

Expected: FAIL because finalized postings remain `BTreeMap<TermId, Vec<PostingEntry>>` and writers still call `bytemuck::bytes_of`.

- [ ] **Step 3: Build the finalized dense arena once**

Replace finalized storage with:

```rust
struct PostingRange { start: usize, end: usize }

pub struct InMemoryIndex {
    posting_arena: Vec<PostingEntry>,
    posting_ranges: Vec<PostingRange>,
    /* existing non-posting fields */
}

fn postings_for_term(&self, term: TermId) -> Option<&[PostingEntry]> {
    let range = self.posting_ranges.get(usize::try_from(term.as_u32()).ok()?)?;
    self.posting_arena.get(range.start..range.end)
}
```

Make `PostingEntry` and its constructor/getters crate-private and remove its public re-export. Remove the PostingEntry-bearing method from public `ExecutableIndex`; add a crate-private direct-slice access trait/helper for execution/writers/merge, and rewrite external tests to derive expected primitive postings from fixtures or gated snapshots. At finalization enumerate canonical terms, append each build-time vector once, and push one checked dense range; internal lookup performs one checked range access.

Under forwarded `bench-internals`, return only primitive range tuples, lengths, element sizes, and addresses from `benchmark_posting_layout`/`benchmark_layout`; do not expose `PostingEntry` fields, mutable storage, or general postings inspection.

- [ ] **Step 4: Remove native block-record casts**

Remove `Pod`/`Zeroable` and record casts. Add shared `push_block_metadata` appending the three `to_le_bytes()` fields; both `encode_postings` and `encode_compressed_postings` must call it. Add exact tests `legacy_writer_emits_known_block_metadata_bytes` and `compressed_writer_emits_known_block_metadata_bytes` (run compressed for DeltaVarint and BlockDelta): locate the emitted section and compare it byte-for-byte with concatenated expected LE arrays, thereby proving both paths invoke equivalent encoding. Reader access remains checked `index*12`, checked `start+12`, one `get`, and explicit `from_le_bytes`.

- [ ] **Step 5: Verify structure, behavior, and no_std**

Run: `rtk cargo test -p leit_index --features bench-internals --test hot_layout`

Expected: PASS for dense ranges, strides, known bytes, unaligned read, and arithmetic errors.

Run: `rtk cargo test -p leit_index`

Expected: PASS for search, merge, and segment suites.

Run: `rtk cargo check -p leit_index --no-default-features --target aarch64-unknown-linux-gnu`

Expected: PASS without `std`.

Run: `rtk proxy rg -n "bytemuck::bytes_of\(&?(entry|BlockMetadataEntry)|bytemuck::cast.*BlockMetadataEntry" crates/leit_index/src`

Expected: no matches.

- [ ] **Step 6: Record exact structural tradeoffs and commit**

Document 8-byte/alignment-4 posting records, 4-byte SoA elements, 12-byte LE block records, dense-range invariants, one checked lookup, byte-count formulas, and explicitly state that these are structural guarantees with no cache-hit/cache-miss/latency claim.

```bash
rtk jj file track crates/leit_index/tests/hot_layout.rs docs/2026-07-15-hot-layout-tradeoffs.md
rtk jj commit -m "feat(index): flatten postings and portable block records"
```

### Task 9: Compose parity and close the iteration

**Files:**
- Extend: `crates/leit_index/tests/reference_execution_parity.rs`
- Extend: `crates/leit_wind_tunnel/tests/query_allocation_baseline.rs`
- Modify: `docs/2026-07-15-iteration-7-allocation-baselines.md`
- Modify: `docs/superpowers/iterations/behavior-scenarios.md:264-292,750-778,2445-2465,2540-2564,2595-2621`
- Modify: `docs/superpowers/iterations/behavior-corpus.md:13,29,86,90,92`
- Modify: `docs/superpowers/iterations/requirements/EPIC-006.md`
- Modify: `docs/superpowers/iterations/coverage-ledger.md`
- Modify: `docs/superpowers/iterations/roadmap.md:430-440`
- Modify: `docs/superpowers/iterations/iteration-log.md`
- Modify: `docs/superpowers/iterations/progress.md`

- [ ] **Step 1: Re-run the full reference matrix after every layout/reuse change**

Extend the parity test to ensure every named case is built independently from the same inputs after flat-layout finalization. Run: `rtk cargo test -p leit_index --features bench-internals --test reference_execution_parity`

Expected: PASS for statistics, ordered IDs, and exact score bits at top-1 and top-16.

- [ ] **Step 2: Replace all five scenario commands with concrete automation**

Set SCENARIO-0007 to `mise exec -- cargo test -p leit_index --features bench-internals --test reusable_execution`; SCENARIO-0023 to `mise exec -- cargo test -p leit_wind_tunnel --test query_allocation_baseline -- --test-threads=1`; SCENARIO-0081 to `mise exec -- cargo test -p leit_wind_tunnel --test index_allocation_baseline -- --test-threads=1`; SCENARIO-0085 to `mise exec -- cargo test -p leit_index --features bench-internals --test reference_execution_parity`; SCENARIO-0087 to `mise exec -- cargo test -p leit_index --features bench-internals --test hot_layout`. Copy the same commands into `behavior-corpus.md` and update coverage/status records without changing deferred ITER-0008 ownership.

- [ ] **Step 3: Run impacted scenarios and allocation evidence**

Run each of the five commands above.

Expected: all PASS; query output reports fresh/reused totals and O(1) retained-result evidence; indexing output reports separate insertion/finalization totals; no command remains `TBD` for the five scenarios.

- [ ] **Step 4: Run the nine sentinel commands from `behavior-corpus.md`**

Execute every row marked `sentinel`, preserving its recorded command exactly.

Expected: 9/9 PASS; record the nine command outcomes in `iteration-log.md`.

- [ ] **Step 5: Run workspace quality gates**

Run: `rtk cargo test --workspace --all-features`

Expected: PASS for the complete workspace.

Run: `rtk cargo clippy --workspace --all-targets --all-features -- -D warnings`

Expected: PASS with zero warnings.

Run: `rtk cargo fmt --all -- --check`

Expected: PASS.

Run: `rtk proxy taplo fmt --check --diff`

Expected: PASS.

Run: `rtk proxy bash .github/copyright.sh`

Expected: `All files have correct copyright headers.`

Run: `RUSTDOCFLAGS="-D warnings" rtk cargo doc --workspace --all-features --no-deps`

Expected: PASS with zero rustdoc warnings.

Run: `rtk cargo check -p leit_postings --no-default-features --target aarch64-unknown-linux-gnu`

Expected: PASS.

Run: `rtk cargo check -p leit_index --no-default-features --target aarch64-unknown-linux-gnu`

Expected: PASS.

- [ ] **Step 6: Validate iteration artifacts**

Run: `rtk proxy python3 /Users/ndn/.codex/skills/running-an-iteration/scripts/check_citations.py docs/superpowers/iterations`

Expected: PASS with every requirement citation resolving.

Run: `rtk proxy python3 /private/tmp/prime-radiant-iterative-development/skills/extracting-requirements/scripts/validate_scenarios.py docs/superpowers/iterations/behavior-scenarios.md`

Expected: PASS with all scenario cards structurally valid.

Run: `rtk proxy python3 /Users/ndn/.codex/skills/running-an-iteration/scripts/validate_iteration_log.py docs/superpowers/iterations/iteration-log.md`

Expected: PASS with every iteration-log record structurally valid.

Run separately for each owned ID: `rtk proxy rg -n -A28 "## SCENARIO-0007" docs/superpowers/iterations/behavior-scenarios.md | rtk proxy rg "TBD"` (repeat with `0023`, `0081`, `0085`, and `0087`), plus `rtk proxy rg -n "SCENARIO-(0007|0023|0081|0085|0087).*TBD" docs/superpowers/iterations/behavior-corpus.md`.

Expected: no matches for the five owned scenarios.

- [ ] **Step 7: Mark closure and commit evidence**

Mark the iteration and owned stories done only after all commands pass. Record observed allocation values without converting them into gates, preserve ITER-0008 advanced reporting/cache work as pending, and update `.agent/memory/working/WORKSPACE.md` plus `python3 .agent/tools/memory_reflect.py iterative-development "closed reusable execution infrastructure" "five impacted scenarios, nine sentinels, workspace quality gates, and artifact validators passed"`.

```bash
rtk jj commit -m "docs: close reusable execution infrastructure"
```
