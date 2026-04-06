# Leit Technical Handover

## Composable Retrieval Infrastructure for Rust

This document describes the architecture and initial implementation plan for **Leit**, a modular retrieval system for Rust.

Leit provides the core infrastructure needed to build modern search and discovery systems:

* lexical retrieval
* structured filtering
* faceting
* ranking
* hybrid retrieval pipelines
* reranking and observability hooks

Leit is not a search application or database. It is **retrieval infrastructure** designed to operate over application-defined entities.

Applications retain ownership of their domain models and storage. Leit builds retrieval projections over those models and returns scored entity identifiers.

Leit should be relevant to the retrieval reality of 2026, not just the lexical-search reality of an earlier era. That means treating hybrid retrieval, reranking, RAG/agentic workflows, and observability as part of the architecture rather than late optional extras.

## Glossary

A few terms are used consistently throughout this document.

* **Entity** — an application-owned object that may be indexed.
* **Projection** — a retrieval-oriented view derived from an entity.
* **Segment** — an immutable unit of index storage and traversal.
* **Posting** — an occurrence summary for a term in an indexed unit.
* **Collector** — a result-accumulation component, typically for top-k or aggregations.
* **Query program** — the compact arena/program representation used for execution-facing queries.
* **Workspace / scratch** — reusable temporary memory used during planning and execution.
* **Hit** — a scored retrieval result keyed by entity ID.

## Architecture Overview

At a high level, Leit separates application data models from retrieval infrastructure. Applications project their entities into retrieval structures, which are then executed by the kernel and optionally composed through higher-level pipelines.

```text
application entities
        ↓
projection
        ↓
retrieval kernel
 (query / postings / scoring)
        ↓
segment storage
        ↓
pipeline orchestration
 (fusion / rerank / hybrid)
        ↓
results (entity IDs)
```

This separation is deliberate:

* **applications** own domain objects and storage
* **projections** expose retrieval-relevant fields
* the **kernel** performs fast lexical retrieval
* the **segment layer** manages serialized indexes
* the **pipeline layer** composes hybrid retrieval workflows

The result is a retrieval stack that can be embedded into applications without forcing them into a document-oriented database or search server model.

## Quick Start

A small end-to-end example helps tie the concepts together.

```rust
struct Note {
    id: NoteId,
    title: String,
    body: String,
    language: Language,
}

struct NoteProjection;

impl Projection<Note> for NoteProjection {
    type Id = NoteId;

    fn entity_id(&self, entity: &Note) -> Self::Id {
        entity.id
    }

    fn for_each_text_field(
        &self,
        entity: &Note,
        f: &mut dyn FnMut(FieldId, &str),
    ) {
        f(FieldId(0), entity.title.as_str());
        f(FieldId(1), entity.body.as_str());
    }
}

let projection = NoteProjection;
let mut builder = InMemoryIndexBuilder::new();

for note in notes.iter() {
    builder.add_entity(note, &projection);
}

let index = builder.finish();

let parsed = parse_query("rust retrieval");
let query_program = planner.lower_to_program(&parsed, &mut planner_scratch)?;
let plan = planner.build_plan(&query_program, &mut planner_scratch);

let mut collector = TopKCollector::new(10);
let mut workspace = ExecutionWorkspace::default();

let hits = index.execute(&plan, &mut workspace, &mut collector);

for hit in hits {
    let note = note_store.get(hit.id);
    println!("{} -> {:?}", note.title, hit.score);
}
```

This example is intentionally schematic, but it shows the intended split:

* the application owns the entity type
* a projection exposes retrieval fields
* the index stores derived retrieval structures rather than domain objects
* queries are lowered into a compact execution form
* execution uses explicit workspaces and collectors
* results return application-owned IDs

## 1. Philosophy and Design Principles

### Retrieval over entities

Leit operates over **entities**, not documents.

Applications define their own domain objects. Leit indexes retrieval projections derived from those objects.

Search results return **entity identifiers**.

### Modular components

Leit decomposes retrieval into focused crates.

Core retrieval components are independent and composable:

* text normalization
* query representation
* postings structures
* scoring
* candidate pruning
* ranking
* filtering

Applications may depend only on the components they need.

### `no_std` first where possible

Leit should be designed to support `no_std` in core and kernel crates wherever practical.

This is not ideological window dressing. It has concrete architectural consequences:

* clearer crate boundaries
* stronger separation between pure algorithms and IO/storage concerns
* fewer accidental dependencies
* better portability to constrained environments
* more disciplined allocation behavior

Crates that should strongly target `no_std` + `alloc` where feasible include:

* `leit_core`
* `leit_query`
* `leit_score`
* `leit_collect`
* large parts of `leit_postings`
* large parts of `leit_text`

Crates that are more likely to require `std` include:

* `leit_index`
* memory-mapped segment readers/writers
* filesystem-backed segment management
* some benchmark and tooling crates

The design should preserve a clean split between:

* portable retrieval kernels
* storage/runtime integration layers

### Minimal allocations on hot paths

Leit should be designed from the beginning to avoid unnecessary allocation in hot paths.

This affects API shape significantly.

Hot-path code should avoid:

* returning freshly allocated `Vec`s during query execution
* boxing iterators in inner loops
* cloning query/state structures during traversal
* allocating temporary strings while scoring or advancing postings

This implies several design preferences:

* iterator-like traversal over postings
* caller-provided buffers/workspaces where appropriate
* compact value types
* arena or scratch-space strategies for planning and execution
* explicit separation between query parsing/rewriting and hot-path evaluation

Allocation-heavy convenience APIs may exist at higher layers, but kernel APIs should stay tight.

### Segment-based indexing

Leit uses immutable index segments to support efficient indexing and querying.

Segments enable:

* concurrent reads and writes
* memory-mapped storage
* predictable performance

### Retrieval pipelines

Queries are executed as pipelines consisting of stages.

For Leit, hybrid retrieval should be treated as the normal operating model rather than a late optional feature. A realistic modern pipeline often includes multiple first-stage retrieval sources followed by fusion and optional reranking.

```text
query
  ↓
lexical retrieval
  ↓
vector / graph / external retrieval
  ↓
fusion
  ↓
optional reranking
  ↓
results
```

### Benchmarking from the start

Leit should treat benchmarking as part of the architecture, not as a late add-on.

Retrieval systems are deeply shaped by performance characteristics:

* latency
* throughput
* cache behavior
* allocation behavior
* memory footprint
* pruning effectiveness

The project should include dedicated benchmark and performance-analysis crates from the start.

Following existing project conventions, these should be called **wind tunnels**.

This allows primary crates to avoid pulling in heavy dev-dependencies such as Criterion.

Recommended performance crates:

* `leit_wind_tunnel_index`
* `leit_wind_tunnel_query`
* `leit_wind_tunnel_postings`
* `leit_wind_tunnel_wand`

These crates should be used to evaluate:

* indexing throughput
* query latency distributions
* top-k behavior under different workloads
* memory and allocation profiles
* pruning effectiveness
* field weighting and scoring tradeoffs

### Async at the boundary, not in the kernels

Leit’s core retrieval kernels should be synchronous by design.

This includes:

* query programs
* query planning kernels
* postings cursors
* scorers
* collectors
* pruning logic
* hot-path segment traversal over already-resident data

This is important for several reasons:

* tighter hot loops
* more predictable control flow
* lower abstraction overhead in inner loops
* simpler `no_std`-friendly core crates
* cleaner benchmarking
* clearer separation between retrieval execution and resource orchestration

Async is still appropriate at the boundary for tasks such as:

* background indexing pipelines
* segment acquisition/loading
* remote/object-store fetches
* merge scheduling/orchestration
* external vector-service integration
* higher-level hybrid query orchestration across sources

The design principle should be:

* async for acquisition/orchestration
* sync for hot-path retrieval execution

In other words, Leit should avoid letting “how bytes arrive” infect “how retrieval executes.”

## 2. Kernel and Library Architecture

Crate names should use underscores, not hyphens.

The initial crate layout should look like this.

```text
leit_core
leit_text
leit_query
leit_postings
leit_score
leit_collect
leit_index
```

Additional crates may follow later:

```text
leit_facet
leit_columnar
leit_wand
leit_vector
leit_fusion
leit_rerank
leit_pipeline
leit_sparse
leit_graph
```

Support and tooling crates:

```text
leit_examples
leit_wind_tunnel_index
leit_wind_tunnel_query
leit_wind_tunnel_postings
leit_wind_tunnel_wand
```

### Crate intent

#### `leit_core`

Common identifiers, basic result types, small shared traits, and low-level retrieval vocabulary.

**Target:** `no_std` + `alloc` only if needed.

#### `leit_text`

Normalization, tokenization, and text-analysis primitives.

**Target:** mostly `no_std` + `alloc`; keep locale-heavy or platform-heavy integrations out of the kernel path.

Text functionality should prefer **ICU4X** where possible and useful rather than re-encoding CLDR-derived data or rebuilding locale data handling from scratch.

That means the crate should, where practical, lean on ICU4X for:

* Unicode-aware segmentation/tokenization support
* locale-aware normalization behavior where relevant
* script/language-sensitive text handling
* future expansion into richer multilingual analysis

The goal is not to make `leit_text` depend on every heavy text feature at once, but to align with existing, maintained Unicode/i18n infrastructure instead of inventing a parallel ecosystem.

#### `leit_query`

Query IR, rewrite support, and planning-oriented query structures.

**Target:** `no_std` + `alloc`.

#### `leit_postings`

Postings representations, traversal traits, skip/block metadata, and compression-facing types.

**Target:** split carefully.
Core cursor, codec, and traversal kernels should strongly aim for `no_std` + `alloc`; storage adapters may require `std` elsewhere.

#### `leit_score`

BM25/BM25F and related scoring kernels.

**Target:** `no_std`.

#### `leit_collect`

Top-k collectors, grouping collectors, aggregation-oriented collection interfaces.

**Target:** `no_std` + `alloc`.

#### `leit_index`

Entity projection, segment build/read orchestration, and higher-level execution entry points.

**Target:** likely `std` for v1.
This is the natural boundary for filesystem, mmap, and orchestration concerns.

#### `leit_wand`

Block-Max WAND and related pruning infrastructure.

**Target:** `no_std` + `alloc` where possible.

#### `leit_columnar`

Doc values / columnar fields for sorting, faceting, and filtering.

**Target:** mixed.
Columnar kernels can likely remain `no_std` friendly, while storage/runtime integration may require `std`.

#### `leit_facet`

Facet dictionaries, filtering expressions, aggregation support.

**Target:** `no_std` + `alloc` for core logic.

#### `leit_vector`

Interfaces and adapters for vector candidate sources.

**Target:** likely mixed.
Kernel-side fusion interfaces can remain small; external integrations may require `std` and async orchestration above the kernel layer.

#### `leit_fusion`

Result fusion, hybrid retrieval, and reranking support.

**Target:** `no_std` + `alloc` for core fusion algorithms.

#### `leit_rerank`

Second-stage reranking over bounded candidate sets.

**Target:** mixed.
Core traits and candidate/result structures can remain small; actual model/runtime integrations may require `std`, async orchestration, or external runtimes.

#### `leit_pipeline`

Higher-level orchestration for hybrid retrieval pipelines.

**Target:** likely `std` for v1.
This is the natural home for running multiple retrievers, combining candidate sets, and orchestrating fusion/reranking workflows.

#### `leit_sparse`

Support for learned sparse retrieval and weighted sparse query/document representations.

**Target:** mixed.
Core weighted-term/query structures should stay lean; model-generation paths are external concerns.

#### `leit_graph`

Adapters and integration boundaries for graph-based retrieval or graph-augmented candidate generation.

**Target:** mixed.
Leit should not become a graph database, but it should have a clean place for graph-derived candidate sources and adapters.

### Kernel Layer

The following crates form the **core retrieval kernel**. These crates should remain small, composable, and as `no_std` friendly as possible.

* `leit_core`
* `leit_query`
* `leit_postings`
* `leit_score`
* `leit_collect`
* parts of `leit_text`

They implement the core mechanics of retrieval but do **not** orchestrate hybrid pipelines or external systems.

## Core Types (`leit_core`)

The `leit_core` crate defines common identifiers and shared structures.

### v1 decision

Use typed newtypes for Leit-owned identifiers rather than plain type aliases.

This improves:

* type safety
* API clarity
* debugability
* future-proofing for trait impls and helper methods
* accidental-mixup resistance in arena/program and segment code

A likely direction is:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FieldId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TermId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SegmentId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QueryNodeId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CursorSlotId(pub u32);
```

Other internal IDs should follow the same pattern where they are part of the public or semi-public architecture.

Entity identifiers should remain application-defined:

```rust
pub trait EntityId: Copy + Eq + core::hash::Hash {}
```

`Send` and `Sync` should not be part of the core `EntityId` bound in `leit_core`, since that would unnecessarily push host-threading assumptions into a `no_std`-friendly crate.

If higher-level crates or application integrations need additional threading bounds, they can add them at those boundaries.

Search results and ranked transport types should use a single shared scored-hit vocabulary rather than defining two identical structs.

A better v1 direction is:

```rust
pub struct ScoredHit<Id> {
    pub id: Id,
    pub score: Score,
}

pub struct RankedList<Id> {
    pub hits: Vec<ScoredHit<Id>>,
}
```

`ScoredHit` is the common result element used by:

* kernel-facing collectors such as `TopKCollector`
* ranked transport between orchestration stages
* fusion and reranking layers

If the project still wants a separate `Hit<Id>` type later, it should only exist if it carries meaningfully different semantics from `ScoredHit<Id>` (for example, an unscored or minimally-scored result shape). v1 should avoid carrying two identical structs with different names.

This shared vocabulary should remain lightweight and should not become a place where orchestration policy accumulates.

Optional match explanations:

```rust
pub enum Match {
    Term { field: FieldId, term: TermId },
    Facet { field: FieldId },
}
```

It is also worth introducing a small score-bound vocabulary early rather than leaving everything as raw `f32`.

For example, conceptually:

```rust
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Score(pub f32);

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct ScoreBound(pub f32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorStatus {
    Ready,
    Exhausted,
}
```

Even if these remain thin wrappers, they make intent clearer in cursor, collector, and WAND APIs.

Other small newtypes are likely warranted as the design sharpens, for example:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FilterExprId(pub u32);
```

The core crate should stay small and `no_std` friendly.

## Text Processing (`leit_text`)

Responsibilities:

* normalization
* tokenization
* optional stemming hooks
* stopword handling hooks
* future synonym expansion hooks

### v1 decision

`leit_text` should prefer ICU4X where possible and useful rather than rebuilding CLDR-derived tables or parallel locale infrastructure.

This is important for correctness, maintenance, and future multilingual growth.

The intent is not to make every text-analysis path depend on heavyweight functionality, but to align the crate with an existing Unicode/i18n ecosystem instead of inventing another one.

### ICU4X integration strategy

The expected bias is:

* tokenization/segmentation should be pluggable
* ICU4X-backed segmentation should be one of the primary implementations
* normalization should have a trait-based surface so ICU4X-backed normalization can be used where appropriate
* ICU4X-heavy functionality should be feature-gated rather than forced into every build

A likely feature layout is:

* a minimal default feature set with lightweight text primitives
* an optional `icu4x` feature enabling ICU4X-backed normalization and segmentation
* room for future optional features for stemming/synonyms if those become separate integrations

Likely ICU4X components to consider include:

* `icu_segmenter` for Unicode-aware word/line segmentation where useful
* `icu_normalizer` for normalization support where appropriate

Minimal API direction:

```rust
pub trait TokenSink {
    fn push_token(&mut self, token: &str);
}

pub trait Tokenizer {
    fn tokenize_into(&self, text: &str, sink: &mut dyn TokenSink);
}

pub trait Normalizer {
    fn normalize_into(&self, input: &str, out: &mut dyn NormalizedTextSink);
}
```

The important design point is to avoid forcing allocation in the primary tokenization path.

Preferred style:

* push into caller-provided sinks
* avoid returning `Vec<Token>` in the kernel path
* allow higher-level convenience wrappers separately

Future capabilities:

* multilingual tokenization
* locale-aware normalization
* stemming adapters
* synonym expansion hooks
* identifier-aware token handling
* ICU4X-backed segmentation and locale-sensitive analysis where appropriate

### Open questions

* which ICU4X components should be considered part of the default `leit_text` feature set versus optional integrations
* how aggressively tokenization should distinguish identifier/code-like text from natural-language text
* where normalization responsibilities should stop so that `leit_text` remains a retrieval primitive rather than a full NLP stack

## Query Representation (`leit_query`)

Queries should not be modeled primarily as heap-shaped recursive trees in the core execution path.

Instead, Leit should treat queries as **arena-backed query programs** made up of compact terms/nodes referenced by stable indices.

This is a better fit for the project's goals:

* tighter memory layout
* fewer small allocations
* easier rewriting/normalization
* better cache behavior
* easier reuse of query storage and scratch state
* clearer separation between parse form and execution form

A tree-shaped AST may still exist at the edges for parsing or debugging, but it should not be the main internal representation.

The crate should distinguish clearly between:

* parse-friendly query structures
* normalized query arena/program
* execution plans derived from the arena form

### v1 decision

v1 should lean fully into a compact query program representation:

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

This is an explicit v1 commitment, not merely a design-space suggestion.

Important properties:

* child relationships are index-based rather than pointer/box-based
* sibling groups are stored compactly in a shared `children` buffer
* query normalization can rewrite arena contents without recursive heap surgery
* the execution layer can attach side tables and scratch state keyed by node ID
* hot execution operates on canonicalized term handles rather than generic/string-like payloads

This style is the preferred long-term fit for:

* `no_std` + `alloc`
* allocation discipline
* query planning
* future instrumentation/explainability
* efficient hot-path execution

### Phase 1 requirement

Before Phase 1 implementation begins, the project should treat the following as resolved:

* the execution-facing query form uses `TermId`, not a generic term payload
* filter expressions are represented by stable IDs rather than embedded ad hoc structures in hot execution
* parsing/debugging ASTs, if present, are edge representations and not the main internal form

### Query execution should follow the same arena/program style

This approach should not stop at query storage.

Execution should also be structured around compact programs, side tables, and reusable scratch state rather than recursive evaluators that allocate transient structures.

That means Leit should explicitly distinguish among:

* query program storage
* execution plans
* per-query scratch/workspaces
* collector state
* pruning state

A likely direction is for planning to produce side-table-based execution metadata keyed by `QueryNodeId`.

Examples of execution-side data that may live in separate arrays or workspaces:

* resolved term handles / term dictionary lookups
* field statistics references
* scorer configuration per node
* estimated upper bounds
* postings cursor slots
* boolean evaluation state
* WAND pivot/pruning state
* explainability/match capture flags

Conceptually, this may look like:

```rust
pub struct ExecutionPlan {
    pub node_kinds: Vec<ExecNodeKind>,
    pub cursor_slots: Vec<CursorSlotId>,
    pub bounds: Vec<ScoreBound>,
    pub root: QueryNodeId,
}

pub struct QueryScratch {
    pub cursors: Vec<CursorState>,
    pub doc_state: Vec<DocEvalState>,
    pub score_state: Vec<ScoreState>,
}
```

The exact types will evolve, but the important design idea is that evaluation state should be:

* index-addressable
* reusable across queries where appropriate
* separable from immutable query structure
* measurable and benchmarkable as a first-class performance concern

This is especially important for later support of:

* WAND / Block-Max WAND
* grouped collectors
* explainability capture
* allocation-controlled execution in hot paths

Leit should therefore avoid a design where the public query representation is arena-based but the actual evaluator falls back to recursive tree walking with ad hoc temporary allocation.

### Open questions

* whether filter expressions should live in the same arena/program or in a parallel side structure
* how much normalization should happen in `leit_query` versus later planning stages

## Postings Structures (`leit_postings`)

Inverted indexes map terms to postings lists.

Basic posting structure:

```rust
pub struct Posting {
    pub doc: u32,
    pub tf: u32,
}
```

Future variants may include:

* positions
* payloads
* block summaries
* upper bounds

The crate should focus on traversal and representation, not just storage.

Important design concerns:

* cache-friendly layouts
* compressed representations
* skip data
* block metadata for pruning
* minimal branching in inner loops

Preferred API direction is cursor-based traversal without allocation.

### v1 decision

v1 should use a layered cursor design rather than one large all-knowing cursor trait.

A preferred direction is:

```rust
pub trait DocCursor {
    fn current_doc(&self) -> u32;
    fn advance(&mut self) -> CursorStatus;
    fn advance_to(&mut self, target: u32) -> CursorStatus;
}

pub trait TfCursor: DocCursor {
    fn current_tf(&self) -> u32;
}

pub trait BlockCursor: DocCursor {
    fn block_end_doc(&self) -> u32;
    fn block_max_score(&self) -> ScoreBound;
}
```

This keeps simple cases lean while leaving a direct path toward serious pruning architecture.

Key design points:

* `advance_to` is fundamental and should not be treated as an afterthought
* cursor APIs should expose enough structure for later WAND / Block-Max WAND use
* decoded state and scratch buffers should be reusable
* hot-path cursor advancement should avoid allocation entirely

### Sharper API sketch

The cursor design should account explicitly for borrowed segment data and reusable decode scratch.

A stronger direction is to make cursor construction separate from traversal and to allow scratch to be provided by the caller.

Conceptually:

```rust
pub struct PostingsView<'a> {
    pub bytes: &'a [u8],
    pub block_meta: &'a [BlockMeta],
}

pub struct DecodeScratch {
    pub docs: Vec<u32>,
    pub tfs: Vec<u32>,
}

pub trait CursorFactory<'a> {
    type Cursor: DocCursor + 'a;

    fn open_doc_cursor(
        &'a self,
        postings: PostingsView<'a>,
        scratch: &'a mut DecodeScratch,
    ) -> Self::Cursor;
}
```

The exact trait split may change, but the architecture should lean toward:

* immutable borrowed segment/postings views
* mutable caller-owned scratch/workspace
* cursor state as a lightweight traversal object

For higher-performance paths, it may make sense to separate block decode from cursor control more explicitly.

Conceptually:

```rust
pub trait BlockDecoder {
    fn decode_block(
        &self,
        block: BlockId,
        out_docs: &mut [u32],
        out_tfs: &mut [u32],
    ) -> usize;
}
```

That is not necessarily the public v1 API, but it is a useful design pressure: traversal and decode should be separable enough to benchmark independently.

### Borrowing and workspace guidance

The preferred bias is:

* segment/postings data borrowed from immutable views
* decode buffers/workspaces borrowed mutably from explicit scratch
* cursors themselves lightweight and short-lived

This reduces hidden allocation, makes benchmark behavior more legible, and improves flexibility for `no_std` buffer-backed execution.

### Open questions

* whether cursor implementations should own decode buffers or borrow them from scratch/workspaces
* whether `tf` and positions/payloads should be layered as separate capabilities or represented through a more unified but still allocation-free access pattern
* how aggressively block-aware capabilities should be present in v1 public traits versus internal-only initial implementations
* whether decode scratch should be per-cursor, per-query, or pooled at the execution-workspace level

## Scoring (`leit_score`)

Scoring policies should be independent of index structures.

### Sharper API sketch

```rust
pub struct CorpusStats {
    pub doc_count: u32,
    pub avg_doc_len: f32,
}

pub struct TermStats {
    pub doc_freq: u32,
}

pub trait TermScorer {
    fn score(
        &self,
        tf: u32,
        doc_len: u32,
        term: TermStats,
        corpus: CorpusStats,
    ) -> Score;
}

pub trait MaxScoreScorer: TermScorer {
    fn max_score(
        &self,
        max_tf: u32,
        max_doc_len: u32,
        term: TermStats,
        corpus: CorpusStats,
    ) -> ScoreBound;
}
```

BM25 implementation:

```rust
pub struct Bm25 {
    pub k1: f32,
    pub b: f32,
}
```

This crate should strongly target `no_std`.

Future scoring policies:

* BM25F
* exact-match boosts
* field priors
* hybrid lexical/vector score combination kernels

### Phase 1 requirement

Phase 1 should ship with a plain `TermScorer` interface and may keep max-score support behind a separate extension trait or internal interface until WAND work begins.

### Open questions

* whether `CorpusStats` and `TermStats` should remain plain structs or grow typed wrappers/newtypes for individual fields
* how early BM25F should be introduced relative to simpler term-scorer kernels

## Candidate Collection (`leit_collect`)

Collectors maintain the best K results during query evaluation.

Important design concerns:

* minimal allocation during collection
* stable and predictable top-k behavior
* support for reusable collector state/buffers
* extension points for grouping and aggregations

The crate should support:

* top-k collectors
* possibly bounded min-heaps
* future grouping collectors
* future diagnostic collectors

### v1 decision

The collector API should lean toward explicit lifecycle methods so collectors can participate more directly in optimized execution.

Conceptually:

```rust
pub trait Collector<Id> {
    type Output;

    fn begin_query(&mut self);
    fn collect(&mut self, id: Id, score: Score);
    fn finish_query(&mut self) -> Self::Output;
    fn threshold(&self) -> ScoreBound;
}
```

Important notes:

* `threshold()` becomes more important once WAND-style pruning is introduced
* collectors should ideally be reusable across queries to reduce allocation churn
* the collector API should leave room for grouped or per-segment collection

Additional collector families to anticipate:

* `TopKCollector`
* `GroupingCollector`
* `FacetCollector`
* `ExplainCollector`

It may also be useful to distinguish between:

* score-only collectors
* collectors that need match/explainability data
* collectors that aggregate counts rather than rank hits

That split matters because explainability capture can otherwise leak overhead into hot paths that do not need it.

### Sharper API sketch

The top-k collector should be reusable and should separate result storage from temporary heap state where practical.

Conceptually:

```rust
pub struct TopKCollector<Id> {
    k: u32,
    threshold: ScoreBound,
    heap: Vec<ScoredHit<Id>>,
}

pub struct ScoredHit<Id> {
    pub id: Id,
    pub score: Score,
}
```

Likely behavior:

* `begin_query()` resets length/state without freeing capacity
* `collect()` updates a bounded min-heap or equivalent structure
* `finish_query()` returns results in score-sorted order, ideally reusing owned storage

For lower-allocation APIs, it may be useful to provide both:

* an owning collector that returns `Vec<ScoredHit<Id>>`
* a collector that writes final ordered hits into a caller-provided buffer/sink

For example, conceptually:

```rust
pub trait HitSink<Id> {
    fn push_hit(&mut self, hit: ScoredHit<Id>);
}
```

That would allow some execution paths to avoid allocating final result vectors repeatedly.

### Collector workspace guidance

Collector temporary state should be considered part of the reusable execution workspace story.

That means:

* heap capacity should be reused across queries
* explainability capture buffers should be opt-in
* grouped/faceted collectors should make their aggregation storage explicit rather than hidden behind convenience layers

### Open questions

* whether `begin_query` / `finish_query` should remain required methods or be provided with default no-op behavior in helper traits/adapters
* whether thresholds should be represented as a scalar score only or as a richer bound type from the start
* how much collector logic should be segment-local versus cross-segment in the public API
* whether the primary `TopKCollector` should expose stable tie-breaking policy explicitly in the API/docs from day one

### Storage and Segment System

The storage layer manages index construction, segment formats, and segment traversal.

Key crate:

* `leit_index`

This layer bridges between kernel execution and serialized segment representations.

## Index Construction and Execution (`leit_index`)

The index crate orchestrates:

* entity projections
* tokenization
* inverted index construction
* segment creation
* query execution entry points
* segment loading from files, memory maps, or caller-provided buffers

Index building should use **projections**.

Projection trait direction:

```rust
pub trait Projection<E> {
    type Id;

    fn entity_id(&self, entity: &E) -> Self::Id;

    fn for_each_text_field(
        &self,
        entity: &E,
        f: &mut dyn FnMut(FieldId, &str),
    );
}
```

This shape is preferred over returning `Vec<(FieldId, &str)>`, because it avoids forcing allocation.

Applications implement projections for their entities.

Leit indexes the projected retrieval view rather than owning application objects.

### v1 decision

`leit_index` should support both:

* `std`-based loading from filesystem / mmap-backed sources
* buffer-based loading from caller-provided byte slices

The buffer-based path is important and should work in `no_std` + `alloc` environments where possible.

That implies a deliberate split between:

* segment format parsing and traversal over `&[u8]`
* `std`-based acquisition layers for files, mmap, and orchestration

In other words, the filesystem/mmap path may live at the `std` boundary, but the ability to interpret a segment from an in-memory buffer should not depend on `std`.

### Sharper API sketch

The segment-reading side should expose lightweight borrowed views over serialized data.

```rust
pub struct SegmentView<'a> {
    pub header: SegmentHeader,
    pub bytes: &'a [u8],
}

impl<'a> SegmentView<'a> {
    pub fn open(bytes: &'a [u8]) -> Result<Self, SegmentError>;

    pub fn field_table(&self) -> FieldTableView<'a>;
    pub fn lexicon(&self) -> LexiconView<'a>;
    pub fn postings_table(&self) -> PostingsTableView<'a>;
    pub fn columnar(&self) -> Option<ColumnarView<'a>>;
}
```

This is the important bias:

* parsing a segment from bytes should be possible without `std`
* segment views should borrow rather than copy whenever possible
* helper views should stay lightweight and composable

It is also useful to distinguish between validation levels.

```rust
pub enum ValidationMode {
    HeaderOnly,
    Structural,
    Full,
}

impl<'a> SegmentView<'a> {
    pub fn open_with_validation(
        bytes: &'a [u8],
        mode: ValidationMode,
    ) -> Result<Self, SegmentError>;
}
```

That would allow callers to trade startup cost for stronger upfront guarantees.

For `std` environments, acquisition layers can sit above this.

```rust
pub struct MmapSegment;

impl MmapSegment {
    pub fn open(path: &Path) -> Result<Self, SegmentError>;
    pub fn as_view(&self) -> SegmentView<'_>;
}
```

### Error handling strategy

Leit should use explicit crate-local error enums and structured validation errors rather than `anyhow`, `Box<dyn Error>`, or similar erased error styles.

This matters for:

* `no_std` friendliness
* stable low-level APIs
* predictable allocation behavior
* clearer segment validation/reporting

A likely direction is:

```rust
#[non_exhaustive]
pub enum SegmentError {
    Truncated { offset: u32 },
    BadMagic { found: u32 },
    UnsupportedVersion { found: u32 },
    BadOffset { offset: u32 },
    BadSectionLayout,
    InvalidBlockMeta { block: u32 },
}
```

Higher-level crates may wrap or enrich these errors, but the low-level crates should keep errors concrete and allocation-light.

Where useful, low-level errors should carry small structured context such as offsets, versions, or block identifiers rather than relying on heap-allocated strings.

### Open questions

* how much validation should happen eagerly at segment-open time versus lazily during traversal
* whether segment views should borrow directly from `&[u8]` everywhere or allow partially decoded helper structures
* how strongly v1 should separate builder-only types from read-only segment view types
* whether some small metadata structures should be copied out eagerly to simplify traversal while still preserving the overall borrowed-view model

## Concurrency and Parallelism

Parallel query execution is primarily a higher-level concern, likely centered in `leit_index` and above rather than in the kernel crates.

The core design should accommodate parallelism without forcing it into every API.

The intended bias is:

* kernel crates stay synchronous and allocation-disciplined
* per-query workspaces are typically thread-local / executor-local
* parallel segment search can happen above the kernel level
* higher-level collectors may need merge/combine support when segment-local execution is parallelised

This implies a likely split between:

* local collectors used inside a single segment or worker
* mergeable collector outputs or mergeable partial results at a higher layer

Parallelism is not a Phase 1 requirement, but the architecture should not block:

* searching multiple segments in parallel
* parallel hybrid retrieval across candidate sources
* background segment merges/index builds

## Query Execution

Query execution consists of several stages.

### Query planning

Queries are converted into executable structures.

Examples:

```text
term query
boolean query
filter query
```

Planning may allocate, but the resulting execution structures should be compact and reusable where possible.

The planner should produce execution-side data that matches the arena/program style described earlier, rather than reintroducing recursive evaluator objects.

A sharper API direction is to make planning explicit:

```rust
pub trait QueryPlanner {
    type Plan;

    fn build_plan(
        &self,
        query: &QueryProgram,
        workspace: &mut PlannerScratch,
    ) -> Self::Plan;
}
```

A likely v1 bias is that planning canonicalizes queries onto compact term/field handles before hot execution begins.

That keeps hot-path execution away from string-like payloads and makes bounds/cursor-slot side tables more compact.

### Candidate generation

Terms are resolved to postings lists.

Boolean operators determine candidate documents.

This stage must avoid allocating candidate sets naively in the common case.

Execution should instead be expressed through cursor advancement, side-table state, and collector thresholds.

### Scoring

Candidates are scored using the configured scoring policy.

Scoring loops should be designed to minimize:

* allocations
* virtual dispatch in hot loops where avoidable
* repeated decoding of unchanged state

A likely design direction is to separate:

* term-level scoring kernels
* query-level score accumulation
* pruning/bounds logic

This makes it easier to benchmark and evolve each layer independently.

### Top-k collection

Only the highest-scoring results are retained.

### Execution workspace

Leit should likely have an explicit reusable execution workspace type.

Conceptually:

```rust
pub struct ExecutionWorkspace {
    pub cursor_scratch: CursorScratch,
    pub scorer_scratch: ScorerScratch,
    pub collector_scratch: CollectorScratch,
    pub planner_scratch: PlannerScratch,
}
```

The exact shape will evolve, but the important design goal is to make transient execution memory:

* explicit
* reusable
* measurable in benchmarks
* separable from immutable query/index structures

This is important both for performance and for preserving `no_std`-friendly discipline in core crates.

A more detailed direction is to let sub-workspaces be owned independently so wind tunnels can isolate costs.

Conceptually:

```rust
pub struct CursorScratch {
    pub decode: DecodeScratch,
    pub cursor_state: Vec<CursorRuntimeState>,
}

pub struct PlannerScratch {
    pub node_flags: Vec<u8>,
    pub temp_children: Vec<QueryNodeId>,
}

pub struct CollectorScratch {
    pub heap_indices: Vec<u32>,
}
```

The exact fields are placeholders, but the pattern matters: scratch should be explicit enough that individual subsystems can be benchmarked, tuned, and reused independently.

## Segment Architecture

Indexes should consist of multiple immutable segments.

Each segment contains:

```text
term dictionary
postings lists
field statistics
optional stored fields
optional columnar fields
```

Segments are merged periodically to improve query performance.

Segment design should support:

* memory mapping
* sequential scanning
* cache-friendly access patterns
* minimal pointer chasing

A strong early architectural split should exist between:

* segment format / storage
* segment traversal APIs
* higher-level query execution

A more explicit format sketch is worthwhile from the start.

At a high level, a segment will likely need:

* segment header
* field table
* term dictionary / lexicon section
* postings metadata table
* postings data blocks
* optional skip/block summary sections
* optional stored-field section
* optional columnar section
* footer / checks / versioning

### v1 decision

For v1, `leit_index` should be treated as the primary boundary for filesystem, mmap, and orchestration concerns, while still supporting `no_std`-friendly segment parsing/traversal from caller-provided buffers.

The segment format should be designed so block metadata is a first-class citizen rather than something bolted on later.

The format should also preserve a clean split between:

* immutable on-disk / mmap-friendly representation
* lightweight traversal views
* higher-level execution orchestration

### Sharper format sketch

A useful v1 shape is to keep the segment header small and fixed enough to locate major sections quickly.

Conceptually:

```rust
pub struct SegmentHeader {
    pub version: u32,
    pub field_table_offset: u32,
    pub lexicon_offset: u32,
    pub postings_table_offset: u32,
    pub postings_data_offset: u32,
    pub block_meta_offset: u32,
    pub columnar_offset: u32,
}
```

This may evolve to 64-bit offsets or more explicit optional-section handling, but the design should strongly favor:

* direct section lookup
* easy validation of offset ranges
* low-cost borrowed views over each section

Term dictionaries, postings metadata, and block metadata should all be reachable without forcing full decode or heap reconstruction.

### Versioning and migration

Leit should assume from the start that segment formats will evolve.

That means the format needs an explicit versioning story, even in v1.

The intended bias is:

* segments declare a format version in the header
* readers must reject unsupported future versions cleanly
* backward compatibility should be considered deliberately rather than implicitly promised forever
* migration/conversion can happen through explicit rewrite tools or segment rebuild paths rather than trying to support every legacy layout in hot code paths

For long-lived applications, the likely operational model is:

* old segments remain readable for a bounded set of supported versions
* when support is dropped, segments are rewritten/rebuilt through tooling

### Open questions

* fixed-width vs variable-width metadata tables
* offset encoding strategy
* whether term dictionary and postings metadata should be tightly coupled or separately addressable
* what data must be mmap-friendly from day one
* what can remain build-time-only or in-memory-only initially
* how much versioning/checking infrastructure belongs in the initial format
* whether offsets should be uniformly 64-bit from the start or use a narrower v1 format with a clear upgrade path

## Hard Architecture Section

This section is important. Leit should not stop at toy-engine architecture.

### Postings compression

Compressed postings are essential for realistic performance and memory behavior.

Areas to cover early:

* doc ID delta encoding
* term-frequency encoding
* block-based compression
* tradeoffs between decode cost and memory footprint

The design should allow experimentation with multiple codecs rather than baking in one rigid approach too early.

### Skip data

Skip structures are required for efficient boolean query execution and `advance_to` behavior.

Questions to resolve:

* granularity of skip points
* block-local vs list-global skip data
* encoding format
* interaction with compression blocks

### Block metadata

Modern top-k retrieval benefits from block summaries.

At minimum, the architecture should anticipate storing per-block metadata such as:

* max score bounds
* doc ranges
* decode offsets
* block-local length or normalization hints where useful

This is the foundation for Block-Max WAND.

### v1 decision

Treat block metadata as a first-class part of the segment architecture from the start.

The preferred direction is **sidecar metadata per postings list or per postings stream section**, rather than tightly inlining all metadata into the postings blocks themselves.

That bias is motivated by:

* easier traversal experimentation
* clearer mmap access patterns
* simpler decode kernels for the postings blocks themselves
* cleaner evolution of block summary formats

This is still an area for validation in wind tunnels, but the architecture should begin with a strong sidecar bias rather than assuming fully inline metadata.

### Open questions

* whether the sidecar should be physically adjacent to postings blocks or grouped in larger tables
* how much information belongs in each block summary for v1
* whether doc-range information should be implicit from block structure or explicitly stored
* how merge operations should rebuild and validate block summaries

### Block-Max WAND

Leit should be designed with Block-Max WAND in mind from the start.

This means:

* postings/storage APIs must expose enough block structure
* score upper bounds must have a place in the representation
* query execution should not assume naive score-all-candidates evaluation

Even if the first implementation is simpler, the architecture should leave a direct path toward:

* WAND
* Block-Max WAND
* related top-k pruning strategies

### Memory layout

Memory layout needs explicit attention.

Areas to consider:

* struct packing
* SoA vs AoS tradeoffs
* contiguous decode buffers
* minimizing pointer chasing
* making hot working sets small and cache-friendly

### Segment merging

Segment merge strategy affects both performance and architecture.

Topics to cover:

* merge policy
* recomputation of field/corpus statistics
* codec re-encoding during merges
* preservation/rebuilding of block metadata

### Allocation discipline

A serious Leit implementation should treat allocation behavior as a first-class performance topic.

This means measuring and designing for:

* query-time allocations
* indexing-time allocation churn
* scratch workspace reuse
* collector reuse
* decode buffer reuse

Hot-path design should favor caller-provided workspaces and explicit reusable scratch structures where they materially improve behavior.

## Filtering and Faceting (`leit_facet`, future work)

Filtering allows queries to restrict results by structured values.

Examples:

```text
kind = "article"
language = "en"
size < 1000
```

Faceting allows aggregations over structured fields.

Examples:

```text
counts by kind
counts by language
counts by tag
```

These capabilities should be implemented in dedicated crates, likely with close interaction with `leit_columnar`.

## 4. Orchestration Layer (Hybrid Retrieval, Pipelines, and Reranking)

The orchestration layer composes multiple retrieval sources and coordinates fusion, reranking, and higher‑level workflows.

Typical crates in this layer include:

* `leit_fusion`
* `leit_pipeline`
* `leit_rerank`

These crates are expected to depend on `std` and may incorporate async orchestration, external model runtimes, or integration with other systems.

## Hybrid Retrieval

Leit should be designed to support hybrid pipelines as a first-class concern.

Examples of candidate sources:

```text
BM25 results
vector search results
graph traversal results
external signal results
```

These candidate sets can be combined using fusion strategies such as:

* reciprocal rank fusion
* weighted scoring
* learned reranking

### 2026 posture

Hybrid retrieval is no longer a niche extension. For many modern systems, it is the default retrieval posture.

Leit should therefore make room early for:

* multiple retriever types
* fusion across incomparable score spaces
* bounded candidate-set reranking
* graph- or structured-source candidate injection

### Pipeline orchestration

Orchestration should operate on ranked candidate lists rather than on kernel collectors.

A useful direction is a small retriever abstraction.

```rust
pub struct RankedList<Id> {
    pub hits: Vec<ScoredHit<Id>>,
}

pub trait Retriever<Ctx, Id> {
    fn retrieve(&self, ctx: &Ctx) -> RankedList<Id>;
}
```

This keeps orchestration-layer retrieval separate from kernel-level collection.

Kernel crates may still use `Collector` internally, but orchestration code should primarily compose `RankedList`-producing retrievers.

### Fusion strategies

`leit_fusion` should not be treated as a distant extra. It should be part of the early architectural story.

At minimum, the system should leave room for:

* Reciprocal Rank Fusion (RRF)
* weighted fusion across sources
* score normalization strategies where appropriate
* fusion-time traceability / provenance

Conceptually:

```rust
pub trait FusionStrategy<Id> {
    fn fuse(
        &self,
        inputs: &[RankedList<Id>],
        out: &mut dyn HitSink<Id>,
    );
}
```

### Reranking

Leit should also acknowledge a second-stage reranking layer.

A likely separation is:

* first-stage retrieval: fast lexical / vector / graph candidate generation
* second-stage reranking: more expensive learned or feature-rich refinement over a bounded candidate set

This suggests a dedicated `leit_rerank` crate.

Conceptually:

```rust
pub trait Reranker<Id, Doc> {
    fn rerank(
        &self,
        query: &QueryProgram,
        docs: &[Doc],
        hits: &mut [ScoredHit<Id>],
    );
}
```

The actual runtime behind a reranker may be an ONNX runtime, an external model service, or some other integration. Leit does not need to own that runtime to make room for the abstraction.

### Sparse neural retrieval

Leit should also leave room for learned sparse retrieval, where documents and/or queries are represented as weighted sparse term vectors rather than simple integer TF counts.

That has implications for:

* postings/value representations
* weighted query expansion
* scoring APIs that may need to handle non-integer term weights

This is a good fit for a future `leit_sparse` crate rather than overloading the initial lexical kernel with every sparse-neural concern.

## 5. Workflow Integration (RAG and Agentic Systems)

This section is intentionally lighter-weight than the kernel sections, but it is still important for setting the direction of the project.

Leit should be usable not only as a lexical retrieval kernel, but also as part of the retrieval substrate for:

* RAG systems
* conversational assistants
* agentic multi-step workflows
* memory-augmented applications

It captures **integration boundaries and likely extension points** for those systems, even where the exact API surface is not yet frozen.

## RAG, agentic workflows, and stateful retrieval

The kernel remains stateless, but its APIs allow higher-level orchestration to maintain session context.

Applications are expected to layer conversational state, caching, and repeated retrieval loops above the kernel, likely in `leit_pipeline` or in application-specific orchestration.

The important architectural point is that Leit should not make these workflows awkward. Reusable workspaces, query transformation hooks, ranked-list transport, and fusion boundaries all exist in part because modern retrieval is often embedded in longer-running interactive systems rather than one-shot search boxes.

## 6. Graph and Structured Retrieval Integration

This section is also intentionally boundary-oriented rather than fully specified, but it is included because graph- and structure-aware retrieval are now part of the practical retrieval landscape.

Leit intentionally does not implement a graph database, but it must integrate cleanly with graph-derived candidate sources.

## Graph-augmented retrieval

Leit should explicitly acknowledge graph-augmented retrieval and graph-derived candidate generation.

The intent is not for Leit to become a graph database. Instead, it should define a clean integration boundary.

Possible roles for graph-aware integration include:

* graph traversal that yields candidate entity IDs
* graph-derived expansion of the retrieval set
* graph/context signals used during reranking or fusion

This can fit naturally as:

* an application responsibility above Leit, or
* a future `leit_graph` adapter layer that turns graph traversal results into ranked or bounded candidate sets for fusion/reranking.

That boundary matters because some real retrieval tasks in 2026 are not purely lexical or vector-based. They involve walking a structured graph, a citation network, or a typed relationship model to generate candidate sets before fusion or reranking.

For now, this section should be read as design guidance for integration rather than as a frozen crate-level contract.

## 7. Observability and Relevance Tuning

Leit should care not only about internal performance but also about external explainability and relevance tuning.

The existing `Match` type is a start, but practitioners will need richer tooling to understand why a result is good or bad.

Areas Leit should support over time:

* explainability of lexical score contributions
* provenance of fused candidate lists
* reranking trace capture where available
* query telemetry export
* relevance evaluation and debugging support

This does not all need to land in Phase 1, but the architecture should make room for:

* `ExplainCollector`
* telemetry hooks in orchestration layers
* stable trace structures that can be consumed by tooling or workbenches

## 8. Implementation Phases

### Phase 1

Core retrieval and early hybrid architecture.

Crates:

```text
leit_core
leit_text
leit_query
leit_postings
leit_score
leit_collect
leit_index
leit_fusion
```

Capabilities:

* in-memory inverted index
* BM25 scoring
* boolean queries
* top-k ranking
* basic fusion primitives and ranked-list combination support
* benchmark harnesses in wind tunnel crates
* buffer-backed segment view opening and validation

#### Must-resolve before Phase 1 implementation

* query execution form uses canonicalized `TermId` in `QueryProgram`
* `EntityId` remains `no_std`-safe and does not require `Send + Sync` in `leit_core`
* error handling uses concrete low-level enums rather than erased error types
* `leit_index` supports borrowed buffer-backed segment views in addition to `std` acquisition paths
* `leit_postings` settles on the v1 layered cursor shape (`DocCursor`, `TfCursor`, optional block-aware extension)
* `leit_collect` settles on reusable collector lifecycle and threshold semantics
* `leit_fusion` defines at least one baseline fusion path, likely RRF, for combining ranked lists

#### Can remain open during Phase 1

* whether filter expressions live in the main query arena or an adjacent side structure
* whether some small segment metadata is copied eagerly for convenience
* exact ICU4X feature partitioning inside `leit_text`

### Phase 2

Performance-oriented architecture and pipeline composition.

Add:

* compressed postings
* segment readers/writers
* memory-mapped segments
* segment merging
* allocation profiling in wind tunnels
* early `leit_pipeline` orchestration for composing retrievers and fusion flows

#### Must-resolve before Phase 2 implementation

* v1 postings/block layout suitable for compressed traversal
* decode scratch ownership model
* segment header/offset strategy sufficient for stable serialized views

### Phase 3

Advanced retrieval kernels and richer explainability.

Add:

* skip data
* WAND / Block-Max WAND foundations
* columnar fields
* filtering
* faceting
* early observability / explainability capture beyond simple lexical matches

#### Must-resolve before Phase 3 implementation

* whether block metadata remains sidecar and its exact physical organization
* whether max-score support stays in an extension trait or becomes part of broader public scorer-facing APIs
* skip/block summary interaction with compressed postings and merging

### Phase 4

Expanded hybrid, reranking, and graph/sparse integration.

Add:

* vector candidate sources
* pipeline orchestration expansion
* reranking pipelines
* explainability improvements
* sparse retrieval support
* graph-derived candidate integration

#### Must-resolve before Phase 4 implementation

* hybrid result fusion interfaces beyond the initial baseline
* score normalization/boundary expectations across retrieval sources
* explainability payload strategy for mixed retrieval pipelines
* reranker integration boundaries and candidate/document handoff structure
* graph-derived candidate and sparse weighted-term integration boundaries

## 9. Benchmarks and Wind Tunnels

The repository should include dedicated benchmark and performance-analysis crates.

These are not optional extras.

Examples:

* `leit_wind_tunnel_index`
* `leit_wind_tunnel_query`
* `leit_wind_tunnel_postings`
* `leit_wind_tunnel_wand`

These crates should own Criterion and similar tooling so that primary crates avoid heavy dev-dependencies.

Wind tunnels should support:

* reproducible benchmark scenarios
* comparisons across codecs and execution strategies
* allocation counting/profiling
* query mix experiments
* top-k sensitivity studies

A useful practice is to treat benchmark scenarios as named fixtures that can be reused as architecture evolves.

## 10. Documentation Expectations

Documentation quality should be treated as part of the deliverable, not as cleanup after implementation.

The project should expect:

* all public items documented
* each crate to have a top-level crate doc
* each primary crate to include at least one runnable root-level example
* examples to live in dedicated example crates or example modules rather than bloating core crates with heavy dev-dependencies
* public APIs to explain allocation behavior and hot-path expectations where relevant

This is especially important for a library stack like Leit, where the architecture is non-trivial and users need guidance to adopt the intended patterns.

## 11. Testing

In addition to wind tunnels, the workspace should include:

* focused unit tests in the primary crates
* small integration-style examples in `leit_examples`
* correctness tests for query execution and scoring
* property-style tests where useful for codecs and traversers

## 12. Non‑Goals

Leit is not intended to provide:

* a search server
* distributed indexing
* cluster management
* document storage

These concerns belong in higher-level systems.

## 13. Summary

Leit provides a composable retrieval stack for Rust.

It decomposes modern search infrastructure into modular crates that can be embedded inside applications.

By separating domain models from retrieval projections, Leit allows applications to build powerful discovery systems without adopting a monolithic search engine.

The project should emphasize from the start:

* `no_std` friendliness where practical
* minimal allocations in hot paths
* clean crate boundaries
* serious performance architecture
* benchmarking through dedicated wind tunnel crates

## 14. Decision Register

The following table summarizes the key architectural decisions and open questions that gate implementation phases. This replaces earlier meta‑discussion about *having* a decision register and instead provides the actual register used by the project.

| Topic                                                             | Status      | Phase   | Notes                                                               |
| ----------------------------------------------------------------- | ----------- | ------- | ------------------------------------------------------------------- |
| Query representation uses `QueryProgram` with `TermId`            | **Decided** | Phase 1 | Execution operates on canonicalized term handles                    |
| `EntityId` trait excludes `Send + Sync` in kernel crates          | **Decided** | Phase 1 | Threading bounds applied only in higher layers                      |
| Layered postings cursors (`DocCursor`, `TfCursor`, `BlockCursor`) | **Decided** | Phase 1 | Enables later WAND integration                                      |
| `ExecutionWorkspace` reusable across queries                      | **Decided** | Phase 1 | Designed to reduce allocation churn in conversational/RAG workflows |
| Fusion baseline (RRF)                                             | **Decided** | Phase 1 | Implemented in `leit_fusion`                                        |
| Segment views loadable from `&[u8]` (no_std path)                 | **Decided** | Phase 1 | `std` acquisition layer optional                                    |
| Error handling via concrete enums                                 | **Decided** | Phase 1 | No `anyhow` or boxed errors in kernel crates                        |
| Filter expression storage (arena vs side structure)               | **Decided** | Phase 1 | Stored as `QueryNode::Filter`/`ExternalFilter` variants in existing arena |
| Decode scratch ownership model                                    | Open        | Phase 2 | Impacts postings decode API                                         |
| Segment metadata layout (inline vs sidecar blocks)                | Open        | Phase 2 | Requires wind‑tunnel benchmarking                                   |
| WAND / Block‑Max WAND implementation strategy                     | Open        | Phase 3 | Depends on block metadata structure                                 |
| Hybrid pipeline orchestration APIs                                | Open        | Phase 2 | Introduced via `leit_pipeline`                                      |
| Reranker integration boundary                                     | Open        | Phase 4 | Likely external runtime integration                                 |
| Sparse neural retrieval support                                   | Open        | Phase 4 | May require weighted postings                                       |
| Graph candidate integration model                                 | Open        | Phase 4 | Adapter layer rather than graph engine                              |

Maintaining this table allows the kernel implementation to proceed while clearly tracking which design questions remain unresolved.
