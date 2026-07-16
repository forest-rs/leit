# Reusable Execution Infrastructure Design

## Objective

ITER-0007 establishes honest allocation baselines and reusable query/indexing infrastructure before
ITER-0008 performs advanced profiling. It changes the in-memory postings layout without losing an
executable pre-change comparison path.

## Considered approaches

### Separate benchmark-only reference index — selected

Freeze the current BTreeMap-of-Vec index/evaluator behind `bench-internals`. Build the reference and
optimized indexes separately from identical named fixture inputs. This preserves an executable
independent baseline without retaining reference storage in production or widening inspection APIs.

### One runtime index with two evaluators — rejected

This could preserve evaluator logic but cannot preserve the old physical layout after the production
index becomes a flat arena. Keeping both layouts in one runtime value would contaminate memory and
cache measurements.

### Recorded output snapshots only — rejected

Golden outputs can prove compatibility but cannot support ITER-0008's controlled pre/post benchmark.
The reference must remain executable.

## Architecture

### Reference boundary

`ReferenceExecutionIndex` owns an independently frozen BTreeMap-of-Vec postings representation and the
matching evaluator. A `#[cfg(feature = "bench-internals")] #[doc(hidden)] pub` façade exposes only
fixture construction plus primitive statistics/ranked-result snapshots needed by external integration
and benchmark targets. The façade is absent when the feature is disabled and exposes neither
`PostingEntry` nor general postings inspection. Enabling the explicit feature is the enforceable access
boundary; it is not a security boundary.

Reference and optimized indexes are built from the same named document inputs and analyzer
configuration. Comparison uses the same query plan, filter, top-k, corpus statistics, and stable
`SearchScorer` primitives. The reference independently freezes storage, traversal, and evaluator
composition; hard-coded fixture score bits pin the shared scorer primitives against drift.
Relevant statistics, ordered document IDs, and score bits must match exactly across a named matrix:
single-field BM25 term, fielded OR/AND/NOT score composition, unfielded multi-field BM25F, and a deterministic
nontrivial filter that excludes one otherwise matching document. Each case also runs at top-1 and a
nontruncating top-k.

### Allocation instrumentation

`leit_wind_tunnel` owns a reusable allocator wrapper. Only benchmark/test binaries install it as the
global allocator. A counted region follows `warm up → reset → enable → workload → disable → snapshot`.
Fixture construction, planning, assertions, formatting, and result destruction remain outside.
Measurement uses one exclusive active lease plus owner-thread attribution. Nested or simultaneous
leases are rejected; allocator callbacks count only while running on the recorded owner thread.
Allocations on worker threads are excluded, and a workload that relies on workers is outside the
ITER-0007 counter contract. Only successful (non-null) operations count. `alloc` records one call and
the requested `Layout::size`; `dealloc` records one call and that layout size; successful `realloc`
records one realloc call, the old layout size as released bytes, and `new_size` as requested allocated
bytes (not merely growth). Peak/live derivation remains ITER-0008.

The counter uses `thread_local! { static COUNTING: Cell<bool> = const { Cell::new(false) }; }` plus a
process-wide atomic exclusive lease. Allocator callbacks access TLS only through `LocalKey::try_with`
and treat teardown/access failure as disabled, so callbacks neither lazily allocate nor panic/unwind.
Lease acquisition uses compare-exchange Acquire, counters reset before owner TLS becomes active,
callbacks update Relaxed atomic counters only when that flag is active, and RAII drop clears TLS before
releasing the lease with Release. Thus owner state is published before counting, remains valid through
disable, and foreign threads observe their const-initialized false flag. Self-tests cover disabled work,
reset isolation, allocation, reallocation,
deallocation, failure exclusion where testable, foreign-thread exclusion, lease rejection, and
panic-safe disable.

### Reusable execution state

The existing explicit `ExecutionWorkspace` remains the caller-owned entry point; no optional workspace
API is added. Workspace-owned evaluation/scoring scratch is cleared without shrinking. The existing
`Collector::begin_query` remains the reset contract. `TopKCollector::finish_into(&mut Vec<ScoredHit>)`
is a nonbreaking inherent extraction seam: it clears (without shrinking) the warmed caller sink,
repeatedly pops the heap into that sink, then reverses it to reproduce `finish` ordering without a sort
allocation. Heap popping retains the heap allocation. Results and exact tie ordering equal `finish`;
both heap and result-sink capacities remain stable after warmup.

Compressed decode is measured separately from the default in-memory query path. A prepared compressed
postings view borrows workspace-owned SoA decode scratch. Encoding and fixture setup are excluded from
the counted window. Warmed fitting decode performs no allocation/deallocation; one larger list may grow
each required buffer once, after which capacity remains stable.

### Production layout

The optimized `InMemoryIndex` stores one flat `Vec<PostingEntry>` plus `TermId`-indexed ranges. Its
existing infallible internal builder derives exactly one range while enumerating each canonical dense
term: range-table length equals term-table length, slot `i` belongs to `TermId(i)`, and ranges are
ordered, nonoverlapping, and bounded by the arena. No externally supplied/sparse ID controls range
allocation. Tests assert these builder-proven invariants; lookup uses checked ID conversion plus `get`.
`PostingEntry` remains two u32 fields (8 bytes, alignment 4). A term resolves through one checked range
lookup, then iterates contiguous values without per-posting heap objects.

Decode scratch remains parallel `Vec<DocId>` and `Vec<TermFreq>` buffers with four-byte elements.
Serialized block metadata remains 12-byte little-endian records. Both legacy and compressed writers
emit each u32 with `to_le_bytes`; serialization and access remove native/bytemuck record casts.
Readers use `checked_mul(block_index, 12)` followed by `checked_add(start, 12)` before one bounded,
potentially unaligned `bytes.get(start..end)` and explicit LE decoding. Known-byte, overflow, and
unaligned-read tests enforce portability.
These are structural guarantees only. Cache residency, cache misses, and latency remain ITER-0008.

## Data flows

Reference parity: named fixture → reference index + optimized index → identical plan/scorer/filter →
statistics and exact ranked-result comparison.

Query allocation: named fixture/plan warmup plus an N-sized pool of result sinks prewarmed to top-k
capacity outside measurement → reset counter → N preplanned executions using the retained
workspace/collector and one distinct sink per query → snapshot → assertions and destruction. The sink
pool proves every result remains live without charging setup allocation to the execution window.

Index allocation: named documents prepared outside measurement → insertion window snapshot →
finalization window snapshot → baseline report. Merge allocation remains ITER-0008.

## Failure and compatibility behavior

Normal builds must not expose or compile the legacy reference adapter. Counter windows reject invalid
nested/concurrent use instead of silently mixing events. Reusable buffers grow safely when required;
capacity stability is asserted only after warmup. Existing direct search behavior and no_std library
builds remain compatible.

## Evidence plan

Implementation proceeds in nine reviewed tasks: freeze reference; scoped counter; collector/results
reuse; workspace scratch; decode reuse; query baseline; indexing baseline; flat layout/direct access;
composed parity and closure. Each task uses RED/GREEN tests plus paired spec and quality reviews.

Closure requires concrete commands for SCENARIO-0007, 0023, 0081, 0085, and 0087; all impacted
scenarios; 9/9 sentinels; workspace tests; all-target/all-feature Clippy; formatting/TOML/copyright;
rustdoc; relevant no_std builds; and iteration artifact validators. Baselines record observed values
without invented performance thresholds.
