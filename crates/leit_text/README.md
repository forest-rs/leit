# leit-text

Text analysis for Leit.

This crate provides:

- `WhitespaceTokenizer` for Phase 1 tokenization
- `UnicodeNormalizer` for canonical Unicode normalization and case mapping
- `Analyzer` for combining tokenization and normalization
- `FieldAnalyzers` for per-field analysis configuration

The default normalizer uses canonical Unicode normalization plus Unicode
lowercasing. Full case folding is available as an opt-in builder mode.

This crate works in `no_std + alloc`. `std` is enabled by default.

## Running tests

From the workspace root:

```bash
cargo test -p leit_text
```
