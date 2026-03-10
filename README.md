# Leit

A no_std-compatible search engine library for Rust.

## Overview

Leit is a modular search engine library designed to work in both standard and embedded environments. It provides full-text search capabilities with a layered architecture that allows using only the components you need.

## Crates

- **leit-core**: Core types and traits
- **leit-score**: Scoring algorithms (BM25, etc.)
- **leit-query**: Query types and parsers
- **leit-text**: Text analysis and tokenization
- **leit-postings**: Posting list data structures and compression
- **leit-fusion**: Query result fusion and combination algorithms
- **leit-collect**: Result collection and top-K selection
- **leit-index**: Main indexing and search interface

## Features

- `no_std` + `alloc` support for embedded systems
- Modular architecture with clear layer boundaries
- Optional serialization via serde

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
