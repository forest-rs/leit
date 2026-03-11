# leit-fusion

Rank fusion for Leit.

This crate implements Reciprocal Rank Fusion (RRF) for combining multiple
ranked result lists.

This crate provides:

- `RankedResult` for one result in one source ranking
- `FusedResult` for the merged ranking
- `FusionConfig` for RRF parameters
- `fuse` and `fuse_default` for score calculation and final ranking

The implementation is deterministic: ties break by best rank, then by ID.

This crate works in `no_std + alloc`. `std` is enabled by default.

## Running tests

From the workspace root:

```bash
cargo test -p leit_fusion
```
