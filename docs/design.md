# Leit Design

## Purpose

Leit is modular retrieval infrastructure for Rust applications.

It is not a search application, a database, or a hosted search service.
Applications own their entities, storage, and product behavior. Leit owns the
retrieval structures, kernels, and composition seams built over application
projections.

The current codebase implements a Phase 1 retrieval stack that is small,
testable, and suitable for later extension.

The design goals are:

- small crate boundaries
- explicit data flow
- reusable execution state
- `no_std` plus `alloc` support for the library path
- easy replacement of storage, analysis, and scoring components
- measurable behavior instead of hidden performance characteristics
- a small kernel that can grow into hybrid retrieval without redesigning the center

## System posture

Leit operates over application-defined entities.

Applications project those entities into retrieval-oriented fields. Leit indexes
the projections and returns scored entity identifiers. It does not take
ownership of the application data model or system-of-record storage.

The intended flow is:

```text
application entities
        -> projection
        -> retrieval kernel
        -> segment storage
        -> optional fusion / reranking / orchestration
        -> scored entity IDs
```

## Workspace structure

- `leit_core`: shared identifiers, scores, hits, and workspace traits
- `leit_text`: tokenization and Unicode normalization
- `leit_query`: query construction and planning
- `leit_postings`: postings storage and cursor traversal
- `leit_score`: lexical scoring algorithms
- `leit_collect`: result collection
- `leit_fusion`: result fusion
- `leit_index`: in-memory indexing, search, and segment access

`leit-integration-tests` is the cross-crate verification crate. It is not part
of the embedded library surface.

## Kernel boundary

The current workspace is the retrieval kernel, not the full future platform.

The kernel owns:

- shared retrieval vocabulary
- text normalization and tokenization
- query representation and planning
- postings traversal
- scoring
- candidate collection
- in-memory indexing and segment interpretation

Higher-level orchestration should stay outside the kernel. That includes:

- cross-source orchestration and routing
- product-specific serving logic
- application-owned workflow composition

The boundary is important:

- the kernel may expose lexical and vector retrieval backends
- the kernel may expose scoring kernels for lexical, vector, or learned ranking
- the kernel should not hard-code one retrieval mode as the meaning of "search"
- fusion, reranking, and multi-stage workflow composition should sit above the
  kernel-facing execution APIs

## Current architecture

### Core model

`leit_core` owns the common language of the system:

- typed IDs such as `FieldId`, `TermId`, and `QueryNodeId`
- `Score` as a finite score type with saturating arithmetic
- `ScoredHit` as the shared result record
- `ScratchSpace` and `Workspace` for reusable execution state

These types are the boundary between crates. Higher layers should not invent
parallel versions.

### Text analysis

Phase 1 analysis is intentionally simple:

- `WhitespaceTokenizer` splits on Unicode whitespace
- `UnicodeNormalizer` applies canonical Unicode normalization
- the default case mapping is Unicode lowercase
- full Unicode case folding is available as an option

Index-time and query-time text go through the same analyzer path.

### Query model

`leit_query` separates two concerns:

- user-facing query structure through `QueryProgram`
- execution-facing validated plans through `PlannedQueryProgram`

The planner lowers textual queries into execution plans with explicit limits and
feature requirements. Planned programs reject invalid roots, unreachable nodes,
and cycles.

This split is deliberate. User-facing query structure should stay calm and easy
to inspect, while execution-facing query programs should stay compact and
validated.

### Postings and scoring

`leit_postings` provides sorted postings lists and cursor traits.

`leit_score` provides:

- BM25
- BM25F
- scorer composition through `CombinedScorer`

The current implemented scoring kernels are lexical, but scorer ownership is a
policy concern rather than an index concern. The index owns corpus facts such
as postings, field statistics, and document lengths. Execution combines those
facts with an explicit scoring policy.

That distinction should remain true as Leit grows:

- lexical scoring should not be the only execution model
- vector similarity and embedding-based ranking should fit without forcing the
  index layer to pretend they are term scorers
- learned sparse retrieval may require non-integer term weights
- hybrid retrieval should be able to combine multiple candidate sources without
  collapsing them into one score space too early

### Collection and fusion

`leit_collect` owns result collection. `TopKCollector` exposes a minimum-score
boundary so execution can skip work that cannot change the final top-k.

`leit_fusion` implements Reciprocal Rank Fusion with deterministic ordering.

### Index and segments

`leit_index` keeps build-time and query-time concerns separate:

- `InMemoryIndexBuilder` builds an in-memory inverted index
- `InMemoryIndex` owns immutable retrieval data
- `ExecutionWorkspace` reuses execution scratch state
- `SegmentView` validates and reads serialized segment bytes

Execution should stay explicit and component-oriented. The intended direction is
closer to:

- plan the query separately
- choose an execution-time scoring policy explicitly
- reuse a caller-owned `ExecutionWorkspace`
- pass results into a caller-owned collector

The workspace is the natural home for mutable execution state. The index should
remain an immutable source of retrieval facts rather than becoming the owner of
ranking policy.

The segment format is a borrowed Phase 1 format with a header, section
directory, and sections for term dictionary, field metadata, postings metadata,
and postings payload.

## Platform support

The library crates default to `std`, but the main library path is designed for
`no_std` plus `alloc` builds by disabling default features.

This applies to:

- `leit_core`
- `leit_text`
- `leit_query`
- `leit_postings`
- `leit_score`
- `leit_collect`
- `leit_fusion`
- `leit_index`

Crates that are more likely to remain `std`-leaning are the integration,
storage-management, tooling, and benchmark layers around the kernel.

## Execution and allocation posture

Kernel execution should stay synchronous and explicit.

Async belongs at acquisition and orchestration boundaries, not in the core
retrieval loop.

Hot paths should avoid unnecessary allocation. The current design already leans
on:

- compact score and hit types
- explicit workspaces and scratch
- borrowed segment views
- collector-driven result accumulation

Higher-level convenience APIs may allocate more freely, but the kernel should
keep allocation behavior visible.

This posture also preserves room for non-postings execution models. A future
vector backend or embedding-based retriever may not traverse postings at all,
but it should still be able to participate in the same explicit execution,
collection, and fusion story.

## Observability and benchmarking

Observability is part of the design, not follow-up polish.

The system should stay measurable in terms of:

- latency
- memory use
- allocation behavior
- pruning effectiveness
- ranking behavior

The current workspace does not yet have the full benchmark and diagnostics
layer, but the architecture should preserve room for wind tunnels, trace
capture, and other measurement tooling without forcing heavy instrumentation
into every hot path.

## Verified invariants

The current implementation relies on these invariants:

- `Score` values remain finite
- planned query graphs are connected and acyclic
- postings remain sorted by document ID
- Unicode normalization is consistent between indexing and querying
- top-k collection and fusion are deterministic

These invariants are covered by unit tests, property tests, and cross-crate
integration tests.

## Explicit non-goals

Leit is not intended to become:

- a distributed search system
- a primary data store
- a graph database
- an LLM or agent framework
- a kitchen-sink monolith that erases crate boundaries

New retrieval techniques should enter as bounded extensions or adapters. If a
feature requires rewriting the kernel center, the design should be reconsidered.

## Near-term open work

- refactor `leit_index` execution around explicit execution-time scorer
  selection rather than scorerless search helpers
- wire BM25F into execution through that explicit scorer path
- expand the segment format beyond the current Phase 1 representation
- add more analyzers and tokenizers beyond whitespace tokenization
- evaluate additional scoring families beyond BM25 and BM25F
- define how vector candidate sources and embedding-based ranking fit at the
  kernel boundary without overfitting the execution model to postings
- add the benchmark and observability layer described in the project vision
- define the projection-facing API more explicitly in the public design
- reconcile or remove remaining scaffold files that are outside the active
  library path
