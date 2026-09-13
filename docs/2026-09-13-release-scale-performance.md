# Release-scale performance baseline

This document records the first 0.1-oriented scale check for the Phase 1
in-memory search path. It is evidence about a deterministic synthetic workload,
not a general production-capacity claim.

## Workload and method

The seeded corpus has two fields per document: titles contain 3–8 tokens and
bodies contain 20–100 tokens. Terms follow a Zipfian distribution over a fixed
500-word vocabulary. Corpus generation is outside every timed or allocation
window.

Measurements below were taken on arm64 macOS 26.6.2 with Rust 1.97.1. Criterion
results use optimized builds. They are useful for scaling slopes on this machine;
absolute latency should not be compared across machines.

```sh
cargo bench -p leit_wind_tunnel_index --bench indexing -- --noplot
cargo bench -p leit_wind_tunnel_index --bench index_allocations
cargo bench -p leit_wind_tunnel_query --bench query_latency -- \
  --sample-size 10 --warm-up-time 0.5 --measurement-time 1 --noplot
cargo bench -p leit_wind_tunnel_query --bench query_allocations
```

## Index construction

| Documents | Median time | Throughput | Allocation traffic | Retained counted heap | Traffic/document | Retained/document |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1,000 | 19.62 ms | 50.97k docs/s | 20.34 MB | 1.78 MB | 20.34 KB | 1,777 B |
| 10,000 | 197.88 ms | 50.54k docs/s | 202.81 MB | 16.02 MB | 20.28 KB | 1,602 B |
| 100,000 | 2.011 s | 49.72k docs/s | 2.017 GB | 156.89 MB | 20.17 KB | 1,569 B |

Indexing throughput remains within 2.5% across the 100× corpus-size range.
Allocation calls fall from about 200 to 194 per document, while retained bytes
per document also decline. The implementation therefore shows linear time and
stable per-document memory on this workload through 100k documents.

The allocation traffic is still high: building the 100k index requests about
2.0 GB cumulatively to retain about 157 MB. This reflects short-lived analysis
and per-document aggregation objects and is an optimization opportunity, not
evidence of unbounded growth.

`outstanding_bytes` and `peak_outstanding_bytes` are counted heap balances, not
process RSS. The whole-build measurement begins before analyzer and builder
construction and ends while the finished index is alive, making the outstanding
balance a useful approximation of retained index heap. It excludes the generated
source corpus, allocator metadata, fragmentation, stacks, and mapped memory.

## Query latency

Each end-to-end query iteration includes planning, execution, and top-k result
creation while reusing one `ExecutionWorkspace`.

| Query | 1k | 10k | 100k |
| --- | ---: | ---: | ---: |
| Common unqualified term | 28.95 µs | 628.49 µs | 7.619 ms |
| Three-term OR | 73.82 µs | 1.499 ms | 18.467 ms |
| Two-term AND | 51.81 µs | 1.075 ms | 13.011 ms |
| BM25F cross-field term | 35.32 µs | 667.65 µs | 8.089 ms |
| Common fielded term | 6.22 µs | 35.23 µs | 247.63 µs |
| Rare unqualified term | 1.33 µs | 5.16 µs | 62.33 µs |
| Weighted typed eight-term OR | 428.55 µs | 7.110 ms | 82.399 ms |

The common single-term path visits 1,575, 15,631, and 156,550 postings. The
weighted typed path visits 8,435, 84,161, and 839,133 postings. Work units are
therefore linear in corpus size; latency grows somewhat faster as working sets
leave cache. No hidden corpus-sized planning work was observed.

Planning an eight-term query remains flat across the same range:

| Planning path | 1k | 10k | 100k |
| --- | ---: | ---: | ---: |
| Textual OR | 4.85 µs | 4.92 µs | 4.95 µs |
| Weighted typed OR | 4.35 µs | 4.36 µs | 4.40 µs |

## Query allocation behavior

Allocation counts and requested bytes were identical at 1k, 10k, and 100k:

| Operation | Alloc + realloc calls | Requested bytes | Peak counted bytes |
| --- | ---: | ---: | ---: |
| Plan textual eight-term OR | 132 | 10,670 | 3,483 |
| Plan weighted typed eight-term OR | 119 | 10,012 | 3,848 |
| Execute preplanned weighted query after warm-up | 0 | 0 | 0 |
| End-to-end warmed textual single-term search | 23 | 1,248 | 577 |
| End-to-end warmed weighted typed search | 122 | 10,236 | 3,848 |

The weighted options map is constructed and consumed inside the allocation
window, including copies retained by the resulting plan. The allocation work
depends on query shape rather than corpus size.

Weighted typed planning also scales approximately linearly with query terms.
At two configured weights, one, eight, and 32 terms request 1,116, 10,028, and
40,754 bytes. Increasing a 32-term plan from two to 32 configured weights raises
traffic to 54,458 bytes and retained plan storage from 14,464 to 27,520 bytes.
This confirms the current `terms × weights` storage pattern without indicating
runaway behavior at the measured shapes. Applications with unusually many
fields should prefer small weight maps containing only relevant default fields.

## Release interpretation

These results support a narrow claim: the current in-memory implementation has
linear index-build behavior and corpus-independent planning/allocation behavior
through 100k synthetic documents, and warmed preplanned execution performs no
heap allocation.

They do not establish tail latency under concurrent load, million-document
behavior, process RSS, fragmentation, index merge cost, or real-language corpus
behavior. No latency regression threshold is set
from this first local data point; future comparisons should use the same machine
and workload or a dedicated performance runner.

The additional allocation fields are confined to the unpublished wind-tunnel
crate. They change no production crate API or search semantics, so no consumer
migration is required.
