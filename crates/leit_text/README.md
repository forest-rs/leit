# leit-text

Text analysis and tokenization for the Leit search library.

## Overview

The `leit_text` crate provides text analysis capabilities for the Leit IR system, including tokenization, normalization, and field-level analysis. It serves as the foundational text processing layer used by Leit's indexing and query subsystems.

## Features

- `alloc` - Enable alloc support (default, required for dynamic data structures)
- `std` - Enable standard library support (better ergonomics for development)
- `icu` - Enable ICU4X integration for Unicode-aware text processing
- `serde` - Enable serde serialization support
- `full` - Enable all features (std + icu)

## no_std Compatibility

This crate is `no_std` compatible and supports the following configurations:

- **no_std + alloc**: Minimal configuration with heap allocation support
- **std**: Standard library support for enhanced testing and development
- **icu**: Optional ICU4X integration for Unicode-aware text processing (requires alloc)

## Components

### Tokenization

- **WhitespaceTokenizer**: Simple whitespace-based tokenizer that splits text on Unicode whitespace boundaries. Suitable for space-separated languages and scenarios where ICU4X is not available.

### Normalization

- **LowercaseNormalizer**: Converts text to lowercase using ASCII-only case mapping in no_std mode.

### Analysis

- **Analyzer**: Combines tokenization with normalization pipelines for text processing.
- **FieldAnalyzers**: Registry for managing analyzers per document field.

## Dependencies

- `leit-core` - Core types and traits
- `icu_segmenter` (optional) - Unicode text segmentation
- `icu_normalizer` (optional) - Unicode normalization
- `icu_casemapper` (optional) - Case folding and mapping
- `icu_locid` (optional) - Locale identifiers

## Example

```rust
use leit_text::{WhitespaceTokenizer, LowercaseNormalizer, Analyzer, Tokenizer};

// Create a basic analyzer with tokenization and normalization
let analyzer = Analyzer::new(WhitespaceTokenizer::new())
    .with_normalizer(LowercaseNormalizer::new());

// Analyze text
let text = "Hello World";
let mut tokens = Vec::new();
let mut buffer = String::new();
analyzer.analyze(text, &mut tokens, &mut buffer);

// Tokens will be lowercased: "hello", "world"
```

## License

Licensed under the same terms as the Leit project.
