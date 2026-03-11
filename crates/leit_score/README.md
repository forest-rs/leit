# leit-score

Scoring algorithms for Leit.

This crate provides:

- `Scorer` as the scoring trait
- `Bm25Scorer` for single-field lexical scoring
- `Bm25FScorer` for multi-field lexical scoring
- `CombinedScorer` for simple score composition
- `ScoringStats` and `FieldStats` for scorer inputs

Phase 1 focuses on lexical scoring. The trait surface is broad enough to add
other scoring families later.
