# Leit Architectural Decisions (Decided)

This document records architectural decisions that have been made for the Leit retrieval system. Each decision includes the rationale, implementation implications, and affected crates.

## Decision 001: QueryProgram with TermId

**Status:** ✅ Decided (Phase 1)

### What was decided
Query execution uses a compact `QueryProgram` representation that operates on canonicalized `TermId` handles rather than generic string-like payloads.

```rust
pub struct QueryProgram {
    pub nodes: Vec<QueryNode>,
    pub children: Vec<QueryNodeId>,
    pub root: QueryNodeId,
}

pub enum QueryNode {
    Term { field: FieldId, term: TermId },
    And { children: Range<u32> },
    Or { children: Range<u32> },
    Filter { input: QueryNodeId, filter: FilterExprId },
}
```

### Rationale
- **Tighter memory layout:** Index-based relationships avoid pointer/box overhead
- **Fewer small allocations:** Arena storage eliminates recursive heap allocations
- **Easier rewriting/normalization:** Query rewrites modify arena contents without recursive surgery
- **Better cache behavior:** Compact representation improves hot-path performance
- **Reusable storage:** Query state and scratch can be keyed by stable node IDs
- **Clean separation:** Parse form and execution form are distinct

### Implications for implementation
- Child relationships are index-based (ranges in shared buffer), not pointer-based
- Query normalization rewrites arena contents directly
- Execution attaches side tables keyed by `QueryNodeId`
- Hot path operates on integer term handles, not strings
- Parsing/debugging ASTs are edge representations only

### Affected crates
- `leit_query` (primary)
- `leit_core` (TermId definition)
- `leit_index` (query planning)

---

## Decision 002: EntityId Excludes Send + Sync in Kernel

**Status:** ✅ Decided (Phase 1)

### What was decided
The `EntityId` trait in `leit_core` does NOT include `Send + Sync` bounds. Threading constraints are applied only at higher layers when needed.

```rust
pub trait EntityId: Copy + Eq + core::hash::Hash {}
```

### Rationale
- **no_std friendliness:** Avoids pushing host-threading assumptions into portable kernel crates
- **Application flexibility:** Applications define their own entity identifiers
- **Clearer boundaries:** Threading model expressed at orchestration layer, not in core retrieval vocabulary
- **Broader compatibility:** Works in embedded or constrained environments without threading

### Implications for implementation
- Core types remain portable to no_std environments
- Higher-level crates can add Send + Sync bounds when appropriate
- Entity identifiers stay application-owned and defined

### Affected crates
- `leit_core` (EntityId trait definition)
- All kernel crates (no threading assumptions in hot path)
- `leit_index` (may add threading bounds at boundary)

---

## Decision 003: Layered Postings Cursors

**Status:** ✅ Decided (Phase 1)

### What was decided
Postings traversal uses a layered cursor architecture with distinct cursor types for different access levels:

```rust
// Document ID cursor
pub trait DocCursor {
    fn doc_id(&self) -> EntityId;
    fn advance_to(&mut self, target: EntityId);
}

// Term frequency cursor (extends DocCursor)
pub trait TfCursor: DocCursor {
    fn term_freq(&self) -> u32;
}

// Block-aware cursor (extends TfCursor)
pub trait BlockCursor: TfCursor {
    fn block_max_score(&self) -> Score;
    fn advance_block(&mut self);
}
```

### Rationale
- **Incremental complexity:** Basic queries don't pay for block-level features
- **Clean WAND path:** Block-max scores naturally fit in extension trait
- **Zero-cost abstractions:** vtable dispatch or monomorphization at appropriate layer
- **Future-proofing:** Enables Block-Max WAND without breaking basic cursor API

### Implications for implementation
- Basic boolean queries use only `DocCursor`
- BM25 scoring uses `TfCursor`
- Block-Max WAND uses `BlockCursor`
- Cursor implementations may implement multiple traits
- Clear upgrade path from basic to advanced query execution

### Affected crates
- `leit_postings` (cursor traits and implementations)
- `leit_score` (BM25 uses TfCursor)
- `leit_wand` (future Block-Max WAND uses BlockCursor)

---

## Decision 004: ExecutionWorkspace Reuse Pattern

**Status:** ✅ Decided (Phase 1)

### What was decided
Query execution uses reusable `ExecutionWorkspace` structs that can be allocated once and reused across multiple queries, particularly in conversational/RAG workflows.

```rust
pub struct ExecutionWorkspace {
    // Reusable buffers for query execution
    cursor_scratch: Vec<u8>,
    collector_state: Vec<ScoredHit<Id>>,
    // ... other reusable state
}
```

### Rationale
- **Reduces allocation churn:** Conversational systems issue many similar queries in sequence
- **Predictable performance:** Avoids GC-like pauses from repeated allocations
- **Allocation discipline:** Makes hot-path allocation behavior explicit
- **RAG-friendly:** Matches the pattern of repeated retrieval with varying queries

### Implications for implementation
- Workspace is passed as explicit parameter to execution APIs
- Caller owns workspace lifecycle
- Clear API distinction between reusable and per-query state
- Example: `index.execute(&plan, &mut workspace, &mut collector)`

### Affected crates
- `leit_query` (planning workspaces)
- `leit_index` (execution workspaces)
- `leit_collect` (collector state reuse)

---

## Decision 005: RRF Fusion as Baseline

**Status:** ✅ Decided (Phase 1)

### What was decided
Reciprocal Rank Fusion (RRF) is the baseline fusion strategy in `leit_fusion`. Hybrid retrieval is treated as a first-class concern from Phase 1.

```rust
pub trait FusionStrategy<Id> {
    fn fuse(
        &self,
        inputs: &[RankedList<Id>],
        out: &mut dyn HitSink<Id>,
    );
}
```

### Rationale
- **2026 posture:** Hybrid retrieval is now default for many systems
- **Score independence:** RRF works across incomparable score spaces
- **Simplicity:** Easy to implement and understand
- **Foundation:** Provides baseline for more sophisticated fusion strategies

### Implications for implementation
- Orchestration operates on `RankedList<Id>`, not kernel collectors
- Fusion accepts multiple retrieval sources (lexical, vector, graph, etc.)
- Score normalization/fusion remains separate from kernel scoring
- Clear boundary between retrieval and fusion

### Affected crates
- `leit_fusion` (fusion implementations)
- `leit_pipeline` (orchestration)
- `leit_rerank` (uses fused results)

---

## Decision 006: Segment Views from &[u8]

**Status:** ✅ Decided (Phase 1)

### What was decided
Segment parsing and traversal operates on borrowed `&[u8]` views, enabling no_std-friendly access to serialized segments without requiring std::fs or mmap.

```rust
pub struct SegmentView<'a> {
    data: &'a [u8],
    // ... parsed views derived from data
}

impl<'a> SegmentView<'a> {
    pub fn open(data: &[u8]) -> Result<SegmentView> {
        // Validate and parse without copying
    }
}
```

### Rationale
- **no_std path:** Kernel crates work with borrowed buffers, not filesystem APIs
- **Flexibility:** Supports mmap, stdio, network, or in-memory sources uniformly
- **Zero-copy:** Validation and parsing use borrowed views over original data
- **Clear boundary:** Storage acquisition (std) separated from traversal (no_std)

### Implications for implementation
- `leit_index` provides std-based acquisition layer (optional)
- `leit_postings` traversal works on borrowed views
- Segment format designed for easy validation and offset lookup
- Clean split between `std` orchestration and `no_std` kernels

### Affected crates
- `leit_index` (acquisition layer, optional std)
- `leit_postings` (borrowed views, no_std)
- `leit_score` (works on borrowed data)

---

## Decision 007: Concrete Enum Errors (No anyhow in Kernel)

**Status:** ✅ Decided (Phase 1)

### What was decided
Kernel crates use concrete enum error types rather than erased errors (anyhow, Box<dyn Error>). Error handling is explicit and structured.

```rust
pub enum QueryError {
    InvalidSyntax { position: usize },
    UnknownField(FieldId),
    TermNotFound(TermId),
}

pub enum IndexError {
    InvalidSegment,
    UnsupportedVersion(u32),
    CorruptedData,
}
```

### Rationale
- **no_std compatibility:** anyhow requires std
- **Explicit handling:** Enum exhaustiveness ensures all cases are considered
- **No overhead:** No dynamic dispatch or boxing
- **Better ergonomics in no_std:** Pattern matching works without downcasting
- **Clear failure modes:** API surface shows exactly what can go wrong

### Implications for implementation
- Each crate defines its own error types
- Errors are not erased in kernel APIs
- Higher-level crates may consolidate or wrap kernel errors
- anyhow may be used in CLI tools or examples, not kernel crates

### Affected crates
- All kernel crates: `leit_core`, `leit_text`, `leit_query`, `leit_postings`, `leit_score`, `leit_collect`
- `leit_index` (may use anyhow at boundary, not in kernel)

---

## Decision 008: Sync Execution in Kernel, Async at Boundaries

**Status:** ✅ Decided (Phase 1)

### What was decided
Core retrieval kernels are synchronous. Async is used only at boundaries for acquisition and orchestration.

**Synchronous (kernel):**
- Query programs and planning kernels
- Postings cursors and traversal
- Scorers and collectors
- Pruning logic
- Segment traversal over resident data

**Async (boundaries):**
- Background indexing pipelines
- Segment acquisition/loading
- Remote/object-store fetches
- Merge scheduling
- External vector-service integration
- Hybrid query orchestration

### Rationale
- **Tighter hot loops:** No async overhead in inner loops
- **Predictable control flow:** Easier reasoning about hot-path execution
- **Lower abstraction overhead:** No futures machinery in scoring/traversal
- **Simpler no_std:** Async traits require more complex dependencies
- **Clearer separation:** "How bytes arrive" separate from "how retrieval executes"

### Implications for implementation
- Kernel APIs are sync fn, not async fn
- Async wrappers live in orchestration layer
- Blocking I/O during acquisition is acceptable at boundary
- Hot path remains pure algorithm, not async-aware

### Affected crates
- Kernel crates (all sync): `leit_query`, `leit_postings`, `leit_score`, `leit_collect`
- Boundary crates (async at edges): `leit_index`, `leit_pipeline`, `leit_rerank`

---

## Decision 009: no_std + alloc for Kernel Crates

**Status:** ✅ Decided (Phase 1)

### What was decided
Kernel crates target `no_std + alloc` wherever practical. Only storage/orchestration layers depend on `std`.

**no_std + alloc targets:**
- `leit_core` (common types)
- `leit_query` (query programs)
- `leit_score` (BM25 kernels)
- `leit_collect` (top-k collectors)
- `leit_postings` (cursor traits, codecs)
- `leit_text` (tokenization, normalization)
- `leit_facet` (core logic)
- `leit_wand` (Block-Max WAND)
- `leit_fusion` (fusion algorithms)

**Likely std:**
- `leit_index` (filesystem, mmap)
- Memory-mapped segment readers/writers
- Benchmark and tooling crates

### Rationale
- **Clearer boundaries:** Forces separation of algorithms from I/O
- **Portability:** Works in embedded or constrained environments
- **Fewer accidental dependencies:** Prevents pulling in std-locked APIs
- **Disciplined allocation:** Makes allocation behavior explicit
- **Better architecture:** Cleaner crate boundaries emerge naturally

### Implications for implementation
- Kernel APIs avoid std::fs, std::io, std::thread
- Collections use alloc::vec, alloc::sync, not std versions
- Segment views work on &[u8], not file handles
- std-dependent features factored into separate crates

### Affected crates
- All kernel crates (no_std target)
- `leit_index` (std boundary, provides no_std-compatible views)

---

## Decision 010: Allocation Discipline

**Status:** ✅ Decided (Phase 1)

### What was decided
Hot-path code follows strict allocation discipline to avoid unnecessary allocations. This affects API shape and usage patterns.

**Avoid in hot paths:**
- Returning freshly allocated Vecs during execution
- Boxing iterators in inner loops
- Cloning query/state structures during traversal
- Allocating temporary strings while scoring

**Prefer:**
- Iterator-like traversal over postings
- Caller-provided buffers/workspaces
- Compact value types
- Arena or scratch-space strategies
- Explicit separation between planning and execution

### Rationale
- **Performance:** Allocation is a major cost in retrieval systems
- **Predictability:** Avoids GC-like pauses
- **Cache efficiency:** Tight loops with minimal heap traffic
- **RAG/conversational patterns:** Repeated queries benefit from reuse

### Implications for implementation
- Execution APIs take workspace parameters
- Postings cursors iterate without allocating
- Query programs reused across executions
- Allocation-heavy convenience APIs at higher layers only

### Affected crates
- All kernel crates (hot-path discipline)
- `leit_index` (provides convenient wrappers if needed)
- Wind tunnels (measure allocation behavior)

---

## Summary

These decisions form the foundation for Leit's Phase 1 implementation:

1. **QueryProgram with TermId** — Compact arena-based query execution
2. **EntityId without Send + Sync** — no_std-friendly core types
3. **Layered cursors** — Incremental complexity, WAND-ready
4. **ExecutionWorkspace reuse** — Allocation discipline for conversational systems
5. **RRF fusion baseline** — Hybrid retrieval as first-class concern
6. **Segment views from &[u8]** — Zero-copy, no_std traversal
7. **Concrete enum errors** — Explicit, no_std-compatible error handling
8. **Sync kernels, async boundaries** — Clean separation of concerns
9. **no_std + alloc for kernels** — Portable, disciplined architecture
10. **Allocation discipline** — Minimal hot-path allocations

All decisions are marked as **Decided** and should be treated as resolved before Phase 1 implementation begins.
