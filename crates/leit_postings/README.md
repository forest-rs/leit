# leit-postings

Postings storage and traversal for Leit.

This crate provides:

- `Posting` and `PostingsList` for inverted-list storage
- `TermDictionary` for term to `TermId` mapping
- `DocCursor`, `TfCursor`, and `BlockCursor` for layered postings traversal
- `InMemoryPostings` and `InMemoryCursor` for the Phase 1 in-memory path

Postings lists preserve document order so higher layers can rely on cursor
semantics during query execution.

This crate works in `no_std + alloc`. `std` is enabled by default.
