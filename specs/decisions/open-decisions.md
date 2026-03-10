# Open Design Decisions

This document tracks design decisions that need to be made during the Leif implementation. Each decision includes options, trade-offs, timing, dependencies, and current status.

---

## Decision 1: Filter Expression Storage (Phase 1)

**Question:** How should filter expressions be stored in the arena for efficient comparison during query evaluation?

### Options Being Considered

**Option A: Flat Array in Arena**
- Store filter expressions contiguously in the arena
- Each expression includes: `field_id`, `operator`, `value`
- Fixed-size records for simple access

**Option B: Side Structure with Pointers**
- Store expression data in arena, maintain separate index structure
- Pointer-based references from queries to expressions
- More complex indirection but flexible layout

**Option C: Hybrid Approach**
- Fixed metadata in arena, variable-length values in side buffer
- Balances cache locality with space efficiency

### Trade-offs

| Option | Pros | Cons |
|--------|------|------|
| **Flat Array** | • Cache-friendly sequential access<br>• Simple memory management<br>• No indirection overhead | • Wasted space if values vary widely<br>• Harder to update/modify<br>• Arena fragmentation risk |
| **Side Structure** | • Flexible value sizes<br>• Easy to add/remove expressions<br>• Better arena utilization | • Pointer chasing hurts cache<br>• More complex lifecycle management<br>• Additional allocation overhead |
| **Hybrid** | • Good balance of both worlds<br>• Efficient for common cases | • Most complex implementation<br>• Two memory management systems |

### Decision Timing
**Must be decided before:** Phase 1 implementation begins (query parsing and storage layer)

### Dependencies
- Query executor design (depends on access patterns)
- Arena layout and allocation strategy
- Filter comparison implementation
- Testing strategy for filter-heavy queries

### Status
**Open**

---

## Decision 2: Decode Scratch Ownership (Phase 2)

**Question:** Which component owns and manages the decode scratch buffers used during posting list decompression?

### Options Being Considered

**Option A: Per-Query Scratch Pools**
- Query executor allocates scratch pools at query start
- Pools passed to iterators as needed
- Scratch freed when query completes

**Option B: Per-Iterator Scratch**
- Each iterator allocates its own scratch on demand
- Iterator manages scratch lifetime
- More granular but potentially fragmented

**Option C: Global Scratch Cache**
- Thread-local cache of reusable scratch buffers
- Iterators borrow from cache, return when done
- Amortizes allocation cost across queries

### Trade-offs

| Option | Pros | Cons |
|--------|------|------|
| **Per-Query Pools** | • Clear ownership model<br>• Predictable memory usage<br>• Easy to reason about lifetimes | • May over-allocate for simple queries<br>• Need to size pools correctly<br>• Potential contention |
| **Per-Iterator** | • Allocates only what's needed<br>• Simple implementation<br>• Independent iterator lifecycle | • Fragmented allocation pattern<br>• Harder to track total usage<br>• Potential for leaks |
| **Global Cache** | • Amortized allocation overhead<br>• Reuse across queries<br>• Performance-optimized | • Complex lifecycle management<br>• Thread-safety concerns<br>• Cache tuning complexity |

### Decision Timing
**Must be decided before:** Phase 2 implementation begins (iterator design and posting list access)

### Dependencies
- Iterator API design
- Memory allocation strategy
- Concurrency model (single-threaded vs parallel queries)
- Performance profiling infrastructure

### Status
**Open**

---

## Decision 3: Segment Metadata Layout (Phase 2)

**Question:** What format should be used for storing segment metadata to enable efficient merging and segment selection?

### Options Being Considered

**Option A: Fixed Binary Format**
- Structured binary layout with fixed field sizes
- Field offsets known at compile time
- Direct memory mapping possible

**Option B: Variable-Length Encoding**
- Varint encoding for numeric fields
- Compact representation for sparse data
- Requires parsing on access

**Option C: Self-Describing Format**
- TLV (Type-Length-Value) style format
- Extensible for future metadata types
- More parsing overhead

### Trade-offs

| Option | Pros | Cons |
|--------|------|------|
| **Fixed Binary** | • Zero-copy access possible<br>• Fast random access<br>• Simple implementation | • Wasted space for small values<br>• Hard to extend<br>• Versioning challenges |
| **Variable-Length** | • Space efficient<br>• Handles wide value ranges well | • Requires parsing on every access<br>• No zero-copy<br>• More complex code |
| **Self-Describing** | • Future-proof and extensible<br>• Backward compatibility easier | • Highest parsing overhead<br>• Largest storage overhead<br>• Complex implementation |

### Decision Timing
**Must be decided before:** Phase 2 implementation begins (segment file format and merge logic)

### Dependencies
- Segment file format specification
- Merge algorithm implementation
- Segment selection strategy
- Indexer checkpoint/recovery design

### Status
**Open**

---

## Decision 4: WAND Implementation Strategy (Phase 3)

**Question:** Which approach should be used for implementing Block-Max WAND to enable efficient top-k query processing?

### Options Being Considered

**Option A: Classic Block-Max WAND**
- Partition posting lists into fixed-size blocks
- Store max impact per block
- Skip non-competitive blocks during traversal

**Option B: Dynamic Block Sizing**
- Adapt block size based on posting characteristics
- Smaller blocks for high-frequency terms
- Larger blocks for low-frequency terms

**Option C: Two-Phase Hybrid**
- Pre-select candidates with coarse blocks
- Refine with finer-grained evaluation
- Additional overhead for better precision

### Trade-offs

| Option | Pros | Cons |
|--------|------|------|
| **Classic Block-Max** | • Well-studied algorithm<br>• Predictable performance<br>• Simpler implementation | • Suboptimal for skewed data<br>• Fixed block size trade-off<br>• May skip too much/little |
| **Dynamic Sizing** | • Adapts to data distribution<br>• Better for realistic corpora | • Complex block management<br>• Higher indexing cost<br>• Harder to tune |
| **Two-Phase Hybrid** | • Best precision/recall balance<br>• Flexible for different query types | • Highest runtime overhead<br>• More complex implementation<br>• May not always be faster |

### Decision Timing
**Must be decided before:** Phase 3 implementation begins (top-k query optimization)

### Dependencies
- Block max impact computation during indexing
- Upper bound score estimation strategy
- Top-k heap management
- Query planning and cost estimation

### Status
**Open** (deferred until Phase 3)

---

## Decision Tracking Summary

| Decision | Phase | Priority | Status |
|----------|-------|----------|--------|
| Filter Expression Storage | 1 | High | Open |
| Decode Scratch Ownership | 2 | High | Open |
| Segment Metadata Layout | 2 | High | Open |
| WAND Implementation | 3 | Medium | Open (Deferred) |

## Notes

- All decisions should be documented with rationale once made
- Update this file as decisions are made or new questions emerge
- Reference ADRs (Architecture Decision Records) for final decisions
- Consider prototyping for high-impact decisions
