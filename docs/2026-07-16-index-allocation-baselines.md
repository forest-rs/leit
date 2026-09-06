# Index Allocation Baselines

Command:

```text
rtk proxy cargo test -p leit_wind_tunnel --test index_allocation_baseline -- --nocapture --test-threads=1
```

The observed values below are local allocation observations for the named,
deterministic 100-document fixture. Insertion and finalization use separate
allocation-counting phases. Fixture generation, analyzer and builder setup,
assertions, error diagnostics, and report formatting are excluded from both
phases. The retained corpus and finished-index measurement owners are destroyed
only after both snapshots and all checks, so that owner destruction is excluded.
Transient allocation and deallocation intrinsic to `index_document` and
`build_index` is included in its respective phase. Index merge is excluded.
These observations establish no latency or allocation regression threshold.

```text
allocation-baseline fixture=index-100 phase=insertion alloc_calls=21907 realloc_calls=964 dealloc_calls=19633 allocated_bytes=2061052 released_bytes=1883646
allocation-baseline fixture=index-100 phase=finalization alloc_calls=785 realloc_calls=203 dealloc_calls=0 allocated_bytes=224272 released_bytes=50432
```
