# Leit Vision

Leit exists to provide durable retrieval infrastructure for Rust applications.

It is not a search application, not a document database, and not a hosted search service. It is a library stack for building retrieval systems that remain understandable, measurable, and replaceable as requirements grow.

## Purpose

Applications own their entities, storage, and product behavior.

Applications define projections that expose retrieval-relevant fields.

Leit consumes those projections, owns the retrieval kernels and index structures built from them, and provides the composition seams needed to turn them into useful ranked results.

The goal is to let an application build serious search and discovery capabilities without being forced into a monolithic engine model or an architecture that becomes difficult to evolve once lexical retrieval is no longer enough.

## What Leit Is

Leit is intended to become:

* a composable lexical retrieval kernel
* a segment-based retrieval storage layer built around immutable index views
* a calm API surface for projecting application entities into retrieval structures
* a foundation for hybrid retrieval, fusion, reranking, and future sparse or graph-derived candidate integration
* an observable system whose performance and ranking behavior can be measured rather than guessed at

Leit is designed for embedding. It should fit into applications, workbenches, and higher-level platforms without taking ownership of the whole system around it.

## What Leit Is Not

Leit is not intended to become:

* a general-purpose database
* a cluster manager or distributed indexing system
* a graph database
* a product-specific search application
* a kitchen-sink crate where every retrieval concern is forced into one layer

Higher-level systems may be built with Leit, but those systems are not Leit itself.

## Intended Shape

Leit should have a strong, simple center.

That center is a small set of retrieval kernel crates with explicit boundaries:

* shared retrieval vocabulary
* text normalization and tokenization
* query representation and planning
* postings traversal
* scoring
* candidate collection
* indexing and segment interpretation

These correspond roughly to the core crates of the Leit kernel.

Around that center, Leit should provide extension seams rather than hidden magic:

* fusion
* orchestration
* reranking
* columnar/filtering/faceting
* vector, sparse, and graph-derived candidate integration
* observability and trace capture

The kernel should stay small enough to reason about and strong enough to grow around.

## Architectural Direction

Leit optimizes for long-term architecture over short-term compatibility.

That means:

* explicit boundaries over clever convenience
* `no_std`-friendly kernel crates where practical
* allocation-conscious hot paths
* sync execution in the kernel, async only at acquisition and orchestration boundaries
* borrowed views and reusable scratch where they materially improve clarity and performance
* concrete low-level errors rather than erased error styles
* replaceable major subsystems rather than sacred ones

The system should be able to evolve from a correct lexical kernel into a richer retrieval stack without rewriting its center each time a new retrieval mode appears.

## Retrieval Posture

Leit should be relevant to modern retrieval, not only to traditional lexical search.

Lexical retrieval remains foundational, but the project should assume that real systems may also need:

* hybrid candidate generation
* ranked-list fusion
* second-stage reranking
* structured filtering and faceting
* sparse retrieval
* graph-derived candidate injection
* conversational, RAG, or agentic orchestration above the kernel

These concerns should be reflected in the architecture early, even when their full implementation arrives later.

## Observability Posture

Leit should be measurable from the beginning.

Performance, memory behavior, allocation patterns, pruning effectiveness, and ranking traces are not optional polish. They are part of the system design.

The project should prefer architectures that make diagnostics possible without forcing heavyweight tracing into every hot path by default.

## Non-Goals Revisited

Several directions may appear attractive as Leit evolves. Some of them are intentionally outside the project’s scope.

This section exists to make those boundaries explicit so the kernel can remain small, durable, and understandable.

### Leit is not a distributed search system

Leit will not implement:

* cluster coordination
* distributed indexing
* sharding or replication
* distributed query routing

Those concerns belong in higher-level systems built on top of Leit.

Leit focuses on single-process retrieval infrastructure that can be embedded inside larger applications or services.

### Leit does not own application storage

Leit indexes projections of application entities.

It does not replace the application's primary storage layer and will not attempt to become:

* a relational database
* a document database
* a graph database
* a system of record

Applications remain responsible for persistence, transactions, and domain integrity.

Leit only stores the retrieval structures needed to support search and discovery.

### Leit does not become an LLM framework

Modern retrieval systems often appear inside LLM-based applications.

Leit supports those environments by providing:

* efficient retrieval kernels
* hybrid retrieval composition seams
* ranked candidate transport

However, Leit will not implement:

* prompt orchestration
* agent frameworks
* tool routing
* model lifecycle management

Those concerns belong in orchestration layers above Leit.

### Leit does not attempt to solve every retrieval technique

Retrieval research continues to evolve rapidly.

Leit intentionally focuses on durable primitives:

* inverted indexes
* candidate generation
* ranking infrastructure
* composable retrieval stages

New retrieval techniques should appear as extensions or adapters, not as modifications to the kernel.

Examples include:

* vector retrieval adapters
* sparse neural retrieval
* graph-derived candidate sources
* learned rerankers

If a new technique requires rewriting the kernel, the design should be reconsidered.

### Leit prioritizes architectural clarity over feature completeness

It is acceptable for Leit to initially support fewer features than a large search engine if the resulting architecture is:

* easier to reason about
* easier to embed
* easier to evolve

A clean kernel with clear extension seams is more valuable than a large system with unclear boundaries.

## Users

Leit is for engineers building retrieval into larger systems.

That includes people building:

* application-local search
* search and discovery features over domain entities
* retrieval layers for assistants or RAG systems
* internal tools and workbenches
* systems that need calm library boundaries instead of a separate search server

It is not primarily optimized for users who want a turnkey hosted search product.

## Success Criteria

Leit is succeeding if:

* Phase 1 produces a small, correct, documented lexical retrieval seam
* later phases add storage, pruning, orchestration, and diagnostics without distorting the kernel
* applications can adopt only the crates they need
* major new retrieval capabilities arrive as bounded extensions rather than architecture invasions
* performance claims are backed by wind tunnels and measurement
* the public API remains calmer than the internal machinery

## Decision Principles

When the right choice is unclear, prefer the option that:

* keeps ownership boundaries crisp
* preserves a small and durable kernel center
* avoids leaking runtime or product policy into low-level crates
* makes performance and allocation behavior easier to measure
* leaves room for hybrid retrieval and observability without overbuilding them too early
* favors explicitness, replaceability, and long-term leverage

## Relationship To Other Project Docs

This document states the project vision.

* [`leit_kernel_handover.md`](./leit_kernel_handover.md) records the broad design-to-implementation handoff.
* [`architecture/README.md`](./architecture/README.md) and the documents under `docs/architecture/` record implementation-facing boundaries and preserved structural decisions.
* `.tickets/` records execution sequencing, dependencies, and work status.

When those documents answer narrower questions, they should do so in a way that remains consistent with the vision stated here.
