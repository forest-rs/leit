# Codec Tradeoff Analysis: DeltaVarint vs BlockDelta

**Status:** Evidence-based analysis from SCENARIO-0070 codec benchmarks (ITER-0002, STORY-0006 AC-3).

**Measurement:** `cargo bench -p leit_wind_tunnel_index --bench codec_compare` on the deterministic wind-tunnel corpus (SEED=42, 1K and 10K documents, Zipfian term distribution). Encode measures codec work plus allocation of each encoded output `Vec`. Decode reuses preallocated doc/TF buffers; correctness validation runs before, not inside, the timed loop. Criterion reports throughput in postings per second.

---

## Summary

Both codecs compress postings lists to ~25–27% of the uncompressed baseline (8 bytes per posting).

- **DeltaVarint**: single-block, varint-encoded deltas, ~2.03–2.05 bytes/posting
- **BlockDelta**: 128-doc blocks with per-block headers, ~2.10–2.19 bytes/posting

Latency results from the earlier allocation-and-assertion-inclusive loop are intentionally omitted. Fresh results from the corrected loop must be recorded before latency is used as decision evidence.

---

## Tradeoff Rationale

### DeltaVarint: Simplicity
- **Single stream**: no block metadata to parse.
- **Simplest codec**: delta encoding + varints, lowest complexity on the read path.
- **Trade-off**: no block structure means future block-aware features (selective decode, skip, WAND doc-range pruning) require full decode.
- **Encode path**: linear delta and varint encoding.

### BlockDelta: Block-aware future evolution
- **128-doc blocks**: each block independently decodable; enables Phase 3 features (selective block skip, WAND pruning with block-level doc ranges).
- **Per-block header overhead**: first_doc, last_doc, doc_bytes_len increase encoded size slightly vs DeltaVarint.
- **Trade-off**: per-block headers add parsing and encoding work; blocks do not improve memory footprint (compression ratio is similar).

---

## Conclusion

**For Phase 2 (v1)**: DeltaVarint is sufficient and simpler; it achieves a similar compression ratio without block metadata.

**For Phase 3+ (selective decode, block-aware WAND)**: BlockDelta's block structure enables those features without full decode.

The compression measurements indicate that **compression efficiency is not the differentiator**. The decision is currently **architectural**: DeltaVarint for simplicity in v1, BlockDelta for extensibility in v2+; corrected latency results may refine that tradeoff.

**Current production choice**: DeltaVarint is the default; BlockDelta is implemented and tested in parallel for Phase 3 integration.
