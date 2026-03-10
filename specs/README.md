# Leit Phase 1 Specifications

This directory contains the detailed technical specifications for Leit Phase 1, derived from the source architecture documentation.

## What's Here

These specifications represent the actionable implementation plan for Leit Phase 1, translated from the vision and architecture documents into concrete technical specifications:

- **Kernel Module Specifications** - Detailed requirements for the kernel components
- **Bootstrapping Protocol** - Step-by-step initialization and handover procedures
- **IPC and Messaging** - Inter-process communication patterns and protocols
- **Security Model** - Permission systems, sandboxing, and isolation guarantees
- **Testing Strategy** - Validation plans and acceptance criteria

## How to Use These Specs

These documents are intended for implementers working on Leit Phase 1 components:

1. **Start with the Vision** - Read `../docs/vision.md` to understand the overall project goals and architectural philosophy
2. **Review the Architecture** - Study `../docs/leit_kernel_handover.md` to understand the kernel handover mechanism and system structure
3. **Work Through Specifications** - Each specification document defines requirements, interfaces, and acceptance criteria for a specific component
4. **Reference Design Decisions** - Consult the design decision records (linked below) to understand the rationale behind key technical choices

## Design Decisions

Design decision records document the significant technical choices made during the specification process, including alternatives considered and trade-offs evaluated.

- **[Open Decisions](open-decisions.md)** - Decisions pending discussion and resolution
- **[Decided](decided.md)** - Resolved design decisions with their rationales

## Task Specifications

Implementation tasks are organized into the following specifications:

- **[leit_core](task_leit_core.md)** - Core kernel functionality and base types
- **[leit_text](task_leit_text.md)** - Text processing and tokenization
- **[leit_query](task_leit_query.md)** - Query parsing and representation
- **[leit_score](task_leit_score.md)** - Scoring algorithms and ranking
- **[leit_collect](task_leit_collect.md)** - Document collection management
- **[leit_postings](task_leit_postings.md)** - Postings list structures and compression
- **[leit_index](task_leit_index.md)** - Index construction and maintenance
- **[leit_fusion](task_leit_fusion.md)** - Result fusion from multiple sources

## Workspace Setup

- **[Workspace Setup](workspace-setup.md)** - Development environment configuration and tooling

## Build Sequence Overview

Leit Phase 1 components should be built in the following order to satisfy dependencies:

### Phase 1: Foundation (Week 1)
1. **leit_core** - Base types, error handling, and common utilities
2. **workspace setup** - Development environment, build system, and CI/CD

### Phase 2: Text Processing (Week 1-2)
3. **leit_text** - Tokenization, normalization, and text analysis

### Phase 3: Query & Collection (Week 2-3)
4. **leit_query** - Query parsing and representation (depends on: leit_core, leit_text)
5. **leit_collect** - Document collection management (depends on: leit_core)

### Phase 4: Indexing Components (Week 3-4)
6. **leit_postings** - Postings list structures (depends on: leit_core, leit_text)
7. **leit_score** - Scoring algorithms (depends on: leit_core, leit_text, leit_query)

### Phase 5: Index & Fusion (Week 4-5)
8. **leit_index** - Index construction (depends on: leit_core, leit_postings, leit_score)
9. **leit_fusion** - Result fusion (depends on: leit_core, leit_score)

## Dependency Graph

```
                        ┌─────────────┐
                        │ workspace   │
                        │   setup     │
                        └──────┬──────┘
                               │
                               ▼
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  leit_core  │────▶│  leit_text  │────▶│ leit_query  │
└──────┬──────┘     └─────────────┘     └─────────────┘
       │                                      │
       │                                      │
       ▼                                      ▼
┌─────────────┐                       ┌─────────────┐
│ leit_collect│                       │  leit_score │
└─────────────┘                       └──────┬──────┘
                                             │
                                             ▼
                                      ┌─────────────┐
                                      │leit_postings│
                                      └──────┬──────┘
                                             │
                                             ▼
                                      ┌─────────────┐     ┌─────────────┐
                                      │ leit_index  │────▶│ leit_fusion  │
                                      └─────────────┘     └─────────────┘

Legend:
  ──▶  Direct dependency
  │    Required for compilation
```

### Dependency Summary

| Component | Direct Dependencies | All Dependencies |
|-----------|---------------------|------------------|
| leit_core | None | None |
| leit_text | leit_core | leit_core |
| leit_query | leit_core, leit_text | leit_core, leit_text |
| leit_score | leit_core, leit_text, leit_query | leit_core, leit_text, leit_query |
| leit_collect | leit_core | leit_core |
| leit_postings | leit_core, leit_text | leit_core, leit_text |
| leit_index | leit_core, leit_postings, leit_score | leit_core, leit_text, leit_postings, leit_score, leit_query |
| leit_fusion | leit_core, leit_score | leit_core, leit_text, leit_score, leit_query |

## Reference Documentation

These specifications are derived from the source architecture documentation:

- **[Vision](../docs/vision.md)** - Project vision, goals, and architectural overview
- **[Kernel Handover](../docs/leit_kernel_handover.md)** - Detailed kernel initialization and handover protocol

For questions about specifications or to suggest changes, please refer to the project contribution guidelines.
