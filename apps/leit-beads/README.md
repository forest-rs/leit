# Beads search with Leit

A small, read-only CLI for finding work across repositories' Beads JSONL
snapshots. This is an unpublished application crate: a practical example of
assembling Leit, with ingestion and presentation kept outside the core library.

From the workspace root:

```sh
cargo run --release -p leit-beads -- search \
  --repo ../underwood --repo ../overstory --repo ../tavolo 'atlas lifetime'
cargo run --release -p leit-beads -- search --repo ../underwood \
  --status open --label capability:memory --any 'residency eviction'
cargo run --release -p leit-beads -- search --repo ../tavolo --json tav-qxk.3.2
```

Or install locally with `cargo install --path apps/leit-beads`, then use
`leit-beads search --help`. No server, database, or Beads executable is needed.
The default source is the current directory's `.beads/issues.jsonl`.
`--export FILE` accepts an explicit snapshot; both source options are repeatable.

## Search behavior

Queries are literal text. All distinct terms must match somewhere in an issue;
`--any` accepts any term. Quotes, colons, and `AND` have no query-language meaning.
Use `--` before query text beginning with a dash. Up to 32 distinct terms are
accepted. An existing full issue ID, compared without ASCII case sensitivity,
performs an exact lookup with no relevance score. Filters still apply.

The app tokenizer splits whitespace, ASCII punctuation, and common typographic
quotes/dashes. Thus `Foo::bar()` and `foo_bar` both produce `foo` and `bar`.
Leit's Unicode normalizer applies NFC and case folding. There is no stemming,
fuzzy matching, synonym expansion, phrase search, or language-specific word
segmentation. `lifetime` and `lifetimes` are different terms.

BM25F weights are ID 5, title 3, description 1.5, design 1, acceptance criteria 1,
notes (including close reason) 1, and comments 0.7. These are starting values,
not relevance tuning validated by a labeled dataset. Labels, status, priority,
and issue type are exact metadata filters, not indexed prose. Repeated status,
type, or priority filters accept any value within that category; every repeated
label and every category must match. All statuses are included by default.

Results include source, metadata, dependencies, and a short prose excerpt.
Excerpts choose the prose field containing the most distinct query terms, then
show a window around its first match; they may not show every matched term.
Dependency types are displayed as exported, without traversing a dependency
graph or inferring whether an issue is ready to work on.

## Input and output contract

Each nonblank UTF-8 JSONL line must be an object with nonempty `id` and `title`.
An optional `_type` must be `issue`. Optional string fields and arrays may be
absent or null; supplied values must have the expected type. Priority is an
integer from 0 through 4. Comments contain nonempty `text`; dependencies contain
`depends_on_id` and an optional `type`. Unknown fields are ignored. This adapter
supports the snapshot shape found in the neighboring repositories; it does not
promise compatibility with every Beads version or storage backend.

Malformed records and duplicate IDs within one snapshot fail the command with
path and line number; no partial results are emitted. Canonical source paths
are deduplicated. The same ID across different snapshots remains separate.
Files are limited to 64 MiB each and records to 1 MiB; total index memory is not
bounded by those limits. Inputs are never modified.

`--json` emits one object with `query`, `mode`, `terms`, `total`, `results`,
`sources`, `timings_ms`, and `execution`. Results carry their canonical source
path as well as a display repository name. Exact lookup scores are null.
A successful search, including zero matches, exits 0; errors exit 2. JSON is an
experimental app interface and may evolve on this branch.

Source metadata includes file modification time and missing-comment counts.
**File modification time does not establish export freshness.** Live SQLite or
Dolt databases are not read. Repositories without an export need an explicit
snapshot. A declared comment count larger than the provided comment array is
reported; missing text cannot be searched. There is no automatic refresh.

## What this exercise exposes

The smoke corpus contained 235 issues from three neighboring repositories.
Searching `atlas lifetime` returned three matches, led by the shared glyph-atlas
integration issue. But other matches mention an atlas under **non-goals**:
lexical retrieval cannot distinguish desired work from explicitly excluded work.
Read the excerpt and issue before treating a result as a recommendation.

The app uses `ExecutionWorkspace::plan_program` with `PlanOptions` for its
typed query and BM25F field weights. The workspace attaches the filter slots
and applies the weights as part of the same planning call; see
[src/search.rs](src/search.rs) for the integration point.

There are also concrete limits in the current library integration:

- The provided whitespace tokenizer retains punctuation. A domain tokenizer is
  necessary for engineering prose. The existing tokenizer trait makes this
  possible without changing core crates, but applications must supply it.
- Each invocation parses and indexes the full snapshots. On the smoke corpus,
  a debug run spent about 334 ms indexing versus 0.2 ms searching; this is an
  observation, not a benchmark. Persistent or incremental ingestion would
  matter more than query micro-optimization for repeated CLI use.
- Exact result counts require exhaustive collection; `--limit` bounds output,
  not search work. Execution counters describe Leit execution only, not all
  loading/tokenization work. No memory measurement or score explanation is
  supplied by this app.

These limits are deliberate scope boundaries for the experiment, not claims
that persistence, incremental updates, or semantic search already exist.
