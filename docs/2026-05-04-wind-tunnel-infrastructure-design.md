# Wind Tunnel Infrastructure Design

## Purpose

Establish Criterion-based benchmark infrastructure for measuring indexing
throughput and query latency across the Leit retrieval stack. This is the
first Phase 2 deliverable and provides the measurement foundation for all
subsequent performance work (postings compression, cursor wiring, segment
evolution).

## Scope

Three new workspace crates. Synthetic corpus generation only. No external
dataset downloads in this iteration.

## Crate Layout

```text
crates/leit_wind_tunnel/            — shared corpus generator, fixture types
crates/leit_wind_tunnel_index/      — Criterion benchmarks for indexing throughput
crates/leit_wind_tunnel_query/      — Criterion benchmarks for query latency
```

All three crates are `publish = false`. The existing `leit_benchmark` crate
remains unchanged — it serves a different purpose (deterministic smoke test
with exact hit-ID assertions).

Future wind tunnel crates (`leit_wind_tunnel_postings`,
`leit_wind_tunnel_wand`) follow the same pattern when their Phase 2/3 work
arrives.

### Dependency graph

```text
leit_wind_tunnel
  ├── rapidhash (deterministic hashing via versioned API, e.g. rapidhash::v1)
  ├── leit_core (FieldId, ScoredHit, Score)
  └── (no Criterion dependency)

leit_wind_tunnel_index
  ├── leit_wind_tunnel
  ├── criterion (0.5.x)
  ├── leit_index
  └── leit_text

leit_wind_tunnel_query
  ├── leit_wind_tunnel
  ├── criterion (0.5.x)
  ├── leit_index
  └── leit_text
```

## Synthetic Corpus Generator

`leit_wind_tunnel` provides a `CorpusGenerator` that produces deterministic
synthetic documents.

### Properties

- **Parameterized by size**: default configurations at 1K and 10K documents;
  caller can request any count.
- **Deterministic**: uses `rapidhash` (versioned API, e.g. `rapidhash::v1`)
  to derive all content. `hash(seed, doc_id, field, position)` selects
  vocabulary terms. The versioned API guarantees output stability across crate
  versions. No PRNG state, no mutable generator — pure function from
  `(seed, doc_id)` to document.
- **Zipfian vocabulary distribution**: a fixed pool of 500 common English
  words. Term selection follows a Zipfian distribution with exponent s=1.0
  (matching natural language frequency curves) so posting lists exhibit
  realistic length skew. Implementation: precompute a cumulative distribution
  table over the vocabulary at startup, hash each `(seed, doc_id, field,
  position)` tuple to a `u64`, normalize to `[0, 1)`, then binary-search the
  CDF table to select the vocabulary index. No external Zipf crate needed.
- **Multi-field documents**: each document has a `title` field (short, 3–8
  tokens) and a `body` field (longer, 20–100 tokens), matching the two-field
  setup used throughout the test suite.

### Query Fixtures

The generator also produces query fixtures for each benchmark shape, drawn
from the same vocabulary to guarantee hits:

- **Single term**: one vocabulary term (from the frequent end of the
  distribution to ensure hits)
- **Multi-term OR**: three terms joined by OR
- **Multi-term AND**: two terms joined by AND (both from frequent terms)
- **Fielded**: `title:term` syntax
- **BM25F cross-field**: unfielded term that expands across both default
  fields

### Output Types

The generator produces simple struct types that can be consumed by any wind
tunnel crate. These types are designed so a future real-corpus loader (BEIR,
Wikipedia) can produce the same types without changing the benchmark harness
code.

## Wind Tunnel Index Benchmarks

`leit_wind_tunnel_index` measures indexing throughput.

### Benchmark file

`benches/indexing.rs`

### Benchmark groups

| Group | Description |
|---|---|
| `index_build/1k` | Build `InMemoryIndex` from 1K synthetic documents |
| `index_build/10k` | Build `InMemoryIndex` from 10K synthetic documents |

Each iteration: construct `InMemoryIndexBuilder`, register field aliases
(`title` at `FieldId(1)`, `body` at `FieldId(2)`), index all documents, call
`build_index()`. Criterion handles warmup, iteration count, and statistical
analysis.

## Wind Tunnel Query Benchmarks

`leit_wind_tunnel_query` measures query latency across all five execution
paths.

### Benchmark file

`benches/query_latency.rs`

### Benchmark groups

Each group runs at both 1K and 10K corpus sizes:

| Group | Execution path exercised |
|---|---|
| `single_term/{size}` | Single-term direct evaluation (the `collect_term` fast path) |
| `multi_term_or/{size}` | General boolean evaluator, OR disjunction |
| `multi_term_and/{size}` | General boolean evaluator, AND conjunction |
| `bm25f_cross_field/{size}` | `TermExpansion` BM25F aggregation |
| `fielded/{size}` | Explicit field targeting (`title:term`) |

These benchmarks establish baselines for each execution path before Phase 2
optimizations (cursor wiring, compressed postings) change them.

### Benchmark parameters

Each iteration calls `workspace.search(index, query, limit, scorer, filter)`
with:

- `limit`: 10 (top-10 retrieval)
- `scorer`: `SearchScorer::bm25()` for single-term, multi-term, fielded, and
  AND queries; `SearchScorer::bm25f()` for the BM25F cross-field group
- `filter`: `NoFilter`

The index is built once outside the timed region using Criterion's setup
mechanism. `ExecutionWorkspace` is reused across iterations to match realistic
usage patterns (amortized allocation).

## Future Real-Corpus Path

Not part of this deliverable. The architecture accommodates it:

- `leit_wind_tunnel` gains an optional `datasets` module (feature-gated
  behind a `datasets` feature).
- That module handles downloading, caching (`target/wind-tunnel-data/`), and
  parsing BEIR-format corpora (SciFact, FiQA, etc.) into the same fixture
  types the synthetic generator produces.
- Wind tunnel crates opt into real-corpus benchmarks by enabling the feature.

The key constraint: the generator's output types must be stable enough that
real corpora plug in without changing benchmark harness code.

## Testing

- `leit_wind_tunnel` has unit tests for the corpus generator:
  - Determinism: same seed produces identical output
  - Correct document counts at requested sizes
  - Vocabulary distribution sanity (frequent terms appear more often)
  - Query fixtures produce non-empty terms
- Wind tunnel benchmark crates do not need unit tests — Criterion benchmarks
  either run or they don't.
- CI runs `cargo test -p leit_wind_tunnel` but does NOT run Criterion
  benchmarks (too slow, too noisy for CI). Benchmarks are run manually or in
  a dedicated performance job.

## Relationship to Existing Code

- `leit_benchmark` is unchanged. It remains the deterministic smoke test.
- The wind tunnel crates isolate Criterion and benchmark-specific dependencies
  so primary crates avoid heavy dev-dependencies (per the handover doc).
- Wind tunnel crates follow the same workspace conventions: Linebender lint
  set, copyright headers, `workspace = true` for package metadata.

## Success Criteria

1. `cargo bench -p leit_wind_tunnel_index` produces Criterion output for
   indexing throughput at 1K and 10K.
2. `cargo bench -p leit_wind_tunnel_query` produces Criterion output for all
   five query shapes at both corpus sizes.
3. `cargo test -p leit_wind_tunnel` passes, verifying corpus generator
   determinism.
4. No changes to existing crates.
5. Criterion's HTML reports are generated in `target/criterion/` for visual
   inspection.
