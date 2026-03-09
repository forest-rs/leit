# leit-index

Main indexing and search interface.

This crate serves as the boundary layer that integrates all other Leit components.

## Features

- `std` - Enable standard library support (enabled by default)
- `alloc` - Enable alloc support (automatically enabled with `std`)
- `serde` - Enable serde serialization support

## Dependencies

- `leit-core` - Core types and traits
- `leit-score` - Scoring algorithms
- `leit-query` - Query types and parsers
- `leit-text` - Text analysis and tokenization
- `leit-postings` - Posting list data structures
- `leit-fusion` - Result fusion algorithms
- `leit-collect` - Result collection algorithms
