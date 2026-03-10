# leit_text Crate Specification

## Overview and Purpose

The `leit_text` crate provides text analysis capabilities for the leif IR system, including tokenization, normalization, and field-level analysis. It serves as the foundational text processing layer used by leif's indexing and query subsystems.

**Key Goals:**
- Provide flexible tokenization strategies (basic whitespace and Unicode-aware via ICU4X)
- Support text normalization (lowercasing, Unicode normalization forms)
- Enable field-specific analyzer composition
- Maintain `no_std` compatibility with alloc support
- Offer optional ICU4X integration for Unicode-aware text processing

**Design Philosophy:**
- Trait-based abstraction for extensibility
- Zero-allocation paths where possible
- Minimal dependencies in core mode
- Full Unicode support via feature flag

## Dependencies

### Required Dependencies
- `leit_core` - Core types and error handling

### Optional Dependencies (ICU4X feature)
- `icu_segmenter` - Unicode text segmentation (tokenizer)
- `icu_normalizer` - Unicode normalization (NFC, NFD, NFKC, NFKD)
- `icu_casemapper` - Case folding and mapping

### Dev Dependencies
- `criterion` - Benchmarking suite
- `proptest` - Property-based testing

## Target Configuration

### Primary Target: `no_std + alloc`
- **Default**: Built without std library
- **Alloc support**: Required for dynamic data structures (Vec, String, HashMap)
- **Platform support**: All tier-1 platforms supported by leit_core

### Secondary Target: `std`
- **Feature**: `std` feature flag
- **Purpose**: Enhanced testing, profiling, and development ergonomics
- **Compatibility**: Full API compatibility with no_std target

### ICU4X Feature Target
- **Feature**: `icu` or `icu4x` (naming TBD)
- **Requires**: `alloc` (ICU4X needs heap allocation)
- **Optional std**: Can be used with or without std feature

## Public API Specification

### Core Traits

#### `Token` Trait
Represents a single token extracted from text.

```rust
/// Represents a single token with position and metadata
pub trait Token {
    /// Returns the token's text content
    fn text(&self) -> &str;
    
    /// Returns the byte offset in the original text where this token starts
    fn byte_start(&self) -> usize;
    
    /// Returns the byte offset in the original text where this token ends
    fn byte_end(&self) -> usize;
    
    /// Returns the character offset (code point position) if available
    fn char_start(&self) -> Option<usize>;
    
    /// Returns the character offset if available
    fn char_end(&self) -> Option<usize>;
    
    /// Token type (word, punctuation, symbol, etc.)
    fn token_type(&self) -> TokenType;
}

/// Classification of token types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenType {
    /// Alphabetic word characters
    Word,
    /// Numeric characters
    Number,
    /// Alphanumeric combinations
    Alphanumeric,
    /// Whitespace (rarely emitted, typically filtered)
    Whitespace,
    /// Punctuation marks
    Punctuation,
    /// Symbols (currency, math, etc.)
    Symbol,
    /// Other or unclassified
    Other,
}
```

#### `Tokenizer` Trait
Splits text into tokens.

```rust
/// Splits text into a stream of tokens
pub trait Tokenizer {
    /// Tokenize the input text, returning tokens in order
    /// 
    /// # Errors
    /// Returns a TokenizeError if tokenization fails (e.g., invalid UTF-8)
    fn tokenize<'a>(&self, text: &'a str) -> Result<Vec<Box<dyn Token + 'a>>, TokenizeError>;
    
    /// Tokenize and call the provided callback for each token
    /// Allows for streaming/lazy processing without full allocation
    fn tokenize_each<F>(&self, text: &str, mut callback: F) -> Result<(), TokenizeError>
    where
        F: FnMut(&dyn Token),
    {
        for token in self.tokenize(text)? {
            callback(&*token);
        }
        Ok(())
    }
}

/// Errors that can occur during tokenization
#[derive(Debug, thiserror::Error)]
pub enum TokenizeError {
    #[error("Input text is not valid UTF-8")]
    InvalidUtf8,
    
    #[error("ICU4X error: {0}")]
    Icu(#[from] Box<dyn std::error::Error + Send + Sync>),
    
    #[error("Tokenizer configuration error: {0}")]
    Configuration(String),
}
```

#### `Normalizer` Trait
Transforms token text into normalized form.

```rust
/// Normalizes text according to specific rules
pub trait Normalizer {
    /// Normalize the input text
    /// 
    /// Returns a Cow<str> to allow for zero-copy when no normalization is needed
    fn normalize<'a>(&self, text: &'a str) -> Result<Cow<'a, str>, NormalizeError>;
}

/// Errors that can occur during normalization
#[derive(Debug, thiserror::Error)]
pub enum NormalizeError {
    #[error("Input text is not valid UTF-8")]
    InvalidUtf8,
    
    #[error("Normalization error: {0}")]
    Normalization(String),
    
    #[error("ICU4X error: {0}")]
    Icu(#[from] Box<dyn std::error::Error + Send + Sync>),
}
```

### Concrete Tokenizers

#### `WhitespaceTokenizer`
Simple tokenizer that splits on Unicode whitespace.

```rust
/// Tokenizer that splits text on Unicode whitespace boundaries
/// 
/// This is the simplest tokenizer, suitable for:
/// - Space-separated languages (English, etc.)
/// - Prototyping and testing
/// - Scenarios where ICU4X is not available
#[derive(Debug, Clone, Default)]
pub struct WhitespaceTokenizer {
    /// Whether to emit whitespace tokens (default: false)
    pub emit_whitespace: bool,
    /// Whether to track character offsets (default: false)
    pub track_char_offsets: bool,
}

impl WhitespaceTokenizer {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn with_emit_whitespace(mut self, emit: bool) -> Self {
        self.emit_whitespace = emit;
        self
    }
    
    pub fn with_char_offsets(mut self, track: bool) -> Self {
        self.track_char_offsets = track;
        self
    }
}

impl Tokenizer for WhitespaceTokenizer {
    fn tokenize<'a>(&self, text: &'a str) -> Result<Vec<Box<dyn Token + 'a>>, TokenizeError> {
        // Implementation uses Unicode whitespace detection
        // text.split_unicode_whitespace()
    }
}
```

#### `IcuTokenizer`
Unicode-aware tokenizer using ICU4X.

```rust
/// Unicode-aware tokenizer using ICU4X segmenter
/// 
/// Provides language-appropriate tokenization for:
/// - CJK languages (Chinese, Japanese, Korean)
/// - Thai, Khmer, Lao (space-separated words)
/// - German compound words
/// - Agglutinative languages
#[cfg(feature = "icu")]
#[derive(Debug, Clone)]
pub struct IcuTokenizer {
    /// Language identifier for locale-specific tokenization
    locale: Option<icu_locid::LanguageIdentifier>,
    /// Segmenter backend (can be configured for specific rules)
    segmenter: icu_segmenter::Segmenter,
}

#[cfg(feature = "icu")]
impl IcuTokenizer {
    /// Create a new tokenizer with default settings
    pub fn new() -> Result<Self, TokenizeError> {
        Ok(Self {
            locale: None,
            segmenter: icu_segmenter::Segmenter::new_auto(),
        })
    }
    
    /// Create a tokenizer for a specific locale
    pub fn with_locale(locale: &str) -> Result<Self, TokenizeError> {
        let lang_id: icu_locid::LanguageIdentifier = locale.parse()
            .map_err(|e| TokenizeError::Configuration(format!("Invalid locale: {}", e)))?;
        
        Ok(Self {
            locale: Some(lang_id),
            segmenter: icu_segmenter::Segmenter::new_auto(),
        })
    }
    
    /// Create a tokenizer with explicit segmenter configuration
    pub fn with_segmenter(segmenter: icu_segmenter::Segmenter) -> Self {
        Self {
            locale: None,
            segmenter,
        }
    }
}

#[cfg(feature = "icu")]
impl Tokenizer for IcuTokenizer {
    fn tokenize<'a>(&self, text: &'a str) -> Result<Vec<Box<dyn Token + 'a>>, TokenizeError> {
        // Implementation uses ICU4X segmenter
    }
}
```

### Concrete Normalizers

#### `LowercaseNormalizer`
Converts text to lowercase using Unicode case mapping.

```rust
/// Converts text to lowercase
/// 
/// In no_std mode: uses basic ASCII lowercase
/// In icu feature: uses full Unicode case folding
#[derive(Debug, Clone, Copy, Default)]
pub struct LowercaseNormalizer {
    /// Whether to use Unicode-aware case folding (icu feature only)
    #[cfg(feature = "icu")]
    unicode: bool,
}

impl LowercaseNormalizer {
    pub fn new() -> Self {
        Self::default()
    }
    
    #[cfg(feature = "icu")]
    pub fn unicode() -> Self {
        Self { unicode: true }
    }
    
    #[cfg(not(feature = "icu"))]
    pub fn unicode() -> Self {
        // No-op when ICU is not available
        Self
    }
}

impl Normalizer for LowercaseNormalizer {
    fn normalize<'a>(&self, text: &'a str) -> Result<Cow<'a, str>, NormalizeError> {
        #[cfg(feature = "icu")]
        if self.unicode {
            // Use ICU4X casemapper
            return Ok(Cow::Owned(/* ICU case folding */));
        }
        
        // Basic ASCII fallback (works in no_std)
        if text.chars().all(|c| c.is_ascii()) {
            Ok(Cow::Owned(text.to_ascii_lowercase()))
        } else {
            // For non-ASCII without ICU, return as-is
            // Alternative: implement basic Unicode lowercase
            Ok(Cow::Borrowed(text))
        }
    }
}
```

#### `UnicodeNfcNormalizer`
Normalizes text to Unicode NFC form.

```rust
/// Unicode Normalization Form C (NFC)
/// 
/// Canonical composition: combines characters where possible
/// Recommended for most use cases
#[cfg(feature = "icu")]
#[derive(Debug, Clone, Copy, Default)]
pub struct UnicodeNfcNormalizer;

#[cfg(feature = "icu")]
impl UnicodeNfcNormalizer {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(feature = "icu")]
impl Normalizer for UnicodeNfcNormalizer {
    fn normalize<'a>(&self, text: &'a str) -> Result<Cow<'a, str>, NormalizeError> {
        // Use ICU4X normalizer
        Ok(Cow::Owned(/* ICU normalization to NFC */))
    }
}

/// Additional normalization forms can be added:
/// - UnicodeNfdNormalizer (NFD)
/// - UnicodeNfkcNormalizer (NFKC)
/// - UnicodeNfkdNormalizer (NFKD)
```

### Analyzer

#### `Analyzer`
Composes tokenization and normalization.

```rust
/// Text analyzer combining tokenization and normalization
#[derive(Debug, Clone)]
pub struct Analyzer {
    /// Tokenizer component
    tokenizer: Box<dyn Tokenizer>,
    /// Normalizer chain (applied in order)
    normalizers: Vec<Box<dyn Normalizer>>,
}

impl Analyzer {
    /// Create a new analyzer with the given tokenizer
    pub fn new(tokenizer: Box<dyn Tokenizer>) -> Self {
        Self {
            tokenizer,
            normalizers: Vec::new(),
        }
    }
    
    /// Add a normalizer to the processing chain
    /// Normalizers are applied in the order they are added
    pub fn add_normalizer(mut self, normalizer: Box<dyn Normalizer>) -> Self {
        self.normalizers.push(normalizer);
        self
    }
    
    /// Analyze text, returning processed tokens
    pub fn analyze<'a>(&self, text: &'a str) -> Result<Vec<ProcessedToken<'a>>, AnalyzeError> {
        let mut results = Vec::new();
        
        for token in self.tokenizer.tokenize(text)? {
            let mut token_text = Cow::Borrowed(token.text());
            
            // Apply all normalizers
            for normalizer in &self.normalizers {
                token_text = normalizer.normalize(&token_text)?;
            }
            
            results.push(ProcessedToken {
                text: token_text.into_owned(),
                byte_start: token.byte_start(),
                byte_end: token.byte_end(),
                token_type: token.token_type(),
            });
        }
        
        Ok(results)
    }
    
    /// Create a basic whitespace + lowercase analyzer (no ICU needed)
    pub fn basic_whitespace() -> Self {
        Self::new(Box::new(WhitespaceTokenizer::new()))
            .add_normalizer(Box::new(LowercaseNormalizer::new()))
    }
    
    /// Create a Unicode-aware analyzer (requires icu feature)
    #[cfg(feature = "icu")]
    pub fn unicode() -> Result<Self, AnalyzeError> {
        Ok(Self::new(Box::new(IcuTokenizer::new()?))
            .add_normalizer(Box::new(LowercaseNormalizer::unicode()))
            .add_normalizer(Box::new(UnicodeNfcNormalizer::new())))
    }
}

/// A processed token with normalized text
#[derive(Debug, Clone)]
pub struct ProcessedToken<'a> {
    pub text: String,
    pub byte_start: usize,
    pub byte_end: usize,
    pub token_type: TokenType,
    // PhantomData for lifetime if needed
    _phantom: core::marker::PhantomData<&'a ()>,
}

/// Errors during analysis
#[derive(Debug, thiserror::Error)]
pub enum AnalyzeError {
    #[error("Tokenization error: {0}")]
    Tokenize(#[from] TokenizeError),
    
    #[error("Normalization error: {0}")]
    Normalize(#[from] NormalizeError),
}
```

### Field Analyzers

#### `FieldAnalyzers`
Manages analyzers for different document fields.

```rust
/// Registry of analyzers for different fields
#[derive(Debug, Default)]
pub struct FieldAnalyzers {
    /// Map from field name to analyzer
    analyzers: HashMap<String, Analyzer>,
    /// Default analyzer for unregistered fields
    default: Option<Analyzer>,
}

impl FieldAnalyzers {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Register an analyzer for a specific field
    pub fn register(&mut self, field: impl Into<String>, analyzer: Analyzer) {
        self.analyzers.insert(field.into(), analyzer);
    }
    
    /// Set the default analyzer
    pub fn set_default(&mut self, analyzer: Analyzer) {
        self.default = Some(analyzer);
    }
    
    /// Get the analyzer for a field, falling back to default
    pub fn get(&self, field: &str) -> Option<&Analyzer> {
        self.analyzers.get(field)
            .or(self.default.as_ref())
    }
    
    /// Analyze text for a specific field
    pub fn analyze_field(&self, field: &str, text: &str) -> Result<Vec<ProcessedToken>, AnalyzeError> {
        match self.get(field) {
            Some(analyzer) => analyzer.analyze(text),
            None => Err(AnalyzeError::Configuration(format!(
                "No analyzer registered for field '{}' and no default set",
                field
            ))),
        }
    }
    
    /// Create a standard configuration for typical document fields
    pub fn standard() -> Self {
        let mut registry = Self::new();
        
        // Title: preserve original case, no stemming
        registry.register("title", Analyzer::new(Box::new(WhitespaceTokenizer::new())));
        
        // Body: lowercase, normalize
        registry.register("body", Analyzer::basic_whitespace());
        
        // Tags: simple tokenization
        registry.register("tags", Analyzer::new(Box::new(WhitespaceTokenizer::new())));
        
        // Set default
        registry.set_default(Analyzer::basic_whitespace());
        
        registry
    }
}
```

## Feature Flags

```toml
[features]
default = ["alloc"]

# Enable alloc support (required for no_std)
alloc = []

# Enable std library support (better ergonomics for development)
std = ["alloc", "leit_core/std"]

# Enable ICU4X integration for Unicode support
icu = ["alloc", "dep:icu_segmenter", "dep:icu_normalizer", "dep:icu_casemapper", "dep:icu_locid"]

# Full feature set for development/testing
full = ["std", "icu"]

[dependencies]
leit_core = { path = "../leit_core", default-features = false }

# Optional ICU4X dependencies
icu_segmenter = { version = "2.0", optional = true }
icu_normalizer = { version = "2.0", optional = true }
icu_casemapper = { version = "2.0", optional = true }
icu_locid = { version = "2.0", optional = true }

[dev-dependencies]
criterion = "0.5"
proptest = "1.0"
```

## Implementation Notes

### no_std Compatibility

1. **Allocation Strategy**
   - All public APIs accept `&str` and return `Vec` or `Cow<str>`
   - No stack allocation of large buffers
   - Use `alloc::vec::Vec` and `alloc::string::String` from `alloc` crate

2. **Error Handling**
   - Use `thiserror` crate (no_std compatible) for error types
   - All errors implement `core::error::Error` + `Display` + `Debug`
   - Avoid `std::error::Error` specifically

3. **Unicode Handling**
   - In no_std mode: Use `char` methods from core (is_whitespace, is_ascii, etc.)
   - In ICU mode: Delegate all Unicode handling to ICU4X
   - Fallback behavior should be documented clearly

4. **Trait Objects**
   - Use `dyn Trait` for tokenizer/normalizer abstraction
   - Consider `Box<dyn Trait>` for owned storage
   - Use `&dyn Trait` for temporary borrows

5. **Testing Strategy**
   - Unit tests in `#[cfg(test)]` (works with no_std + alloc)
   - Integration tests in `tests/` directory (use std feature)
   - Property tests with proptest for edge cases

### Performance Considerations

1. **Tokenization**
   - `WhitespaceTokenizer`: O(n) scan with minimal allocation
   - `IcuTokenizer`: Depends on ICU4X, generally O(n)

2. **Normalization**
   - Zero-allocation path for already-normalized text (Cow::Borrowed)
   - Reuse buffers where possible
   - Consider batch normalization for multiple tokens

3. **Caching**
   - Consider lazy initialization of ICU4X segmenters/normalizers
   - Locale-specific resources can be expensive to create

### Documentation Requirements

- All public items must have rustdoc comments
- Include examples for major types
- Document no_std compatibility clearly
- Document ICU feature behavior and fallbacks

## Acceptance Criteria Checklist

### Core Functionality
- [ ] `Token` trait with all required methods implemented
- [ ] `Tokenizer` trait with `tokenize` and `tokenize_each` methods
- [ ] `Normalizer` trait with `normalize` method
- [ ] `WhitespaceTokenizer` functional in no_std mode
- [ ] `LowercaseNormalizer` with ASCII fallback in no_std mode
- [ ] `Analyzer` composes tokenizer + normalizers correctly
- [ ] `FieldAnalyzers` manages per-field analyzers with defaults

### ICU4X Integration
- [ ] `IcuTokenizer` wraps ICU4X segmenter correctly
- [ ] `UnicodeNfcNormalizer` uses ICU4X normalizer
- [ ] `LowercaseNormalizer::unicode()` uses ICU4X casemapper
- [ ] Feature flag enables ICU dependencies correctly
- [ ] ICU types are properly gated with `#[cfg(feature = "icu")]`

### no_std Compatibility
- [ ] Crate builds with `default-features = false`
- [ ] All tests pass in no_std + alloc configuration
- [ ] No std library dependencies in core code paths
- [ ] Error types work without std
- [ ] Documentation clearly indicates no_std support

### Testing
- [ ] Unit tests for all tokenizer implementations
- [ ] Unit tests for all normalizer implementations
- [ ] Integration tests for analyzer composition
- [ ] Property tests for edge cases (empty strings, Unicode edge cases)
- [ ] Benchmarks for tokenizer/normalizer performance
- [ ] Tests pass with and without ICU feature

### Documentation
- [ ] All public APIs have rustdoc comments
- [ ] Examples for major types (Analyzer, FieldAnalyzers)
- [ ] Feature flag documentation in README
- [ ] no_std compatibility notes in crate-level docs
- [ ] ICU4X integration guide

### Error Handling
- [ ] All errors implement Display, Debug, and Error
- [ ] Error types are comprehensive and descriptive
- [ ] Error conversion (From) works correctly
- [ ] Invalid UTF-8 is handled appropriately
- [ ] ICU errors are properly wrapped

## Test Plan

### Unit Tests

**Token Trait Tests**
- Verify all token methods return correct values
- Test token type classification for various inputs
- Edge cases: empty tokens, tokens at string boundaries

**WhitespaceTokenizer Tests**
- Basic whitespace splitting
- Multiple consecutive whitespace
- Leading/trailing whitespace
- Unicode whitespace (non-breaking space, em space, etc.)
- Empty input
- Character offset tracking (when enabled)
- Token type classification (word, number, punctuation, etc.)

**IcuTokenizer Tests** (icu feature only)
- English word boundaries
- CJK segmentation (Chinese, Japanese, Korean samples)
- Thai word segmentation
- German compound words
- Locale-specific tokenization
- Empty input
- Comparison with whitespace tokenizer

**LowercaseNormalizer Tests**
- ASCII uppercase → lowercase
- ASCII mixed case → lowercase
- No-op on already lowercase
- Unicode case folding (with icu feature)
- Empty string
- Non-ASCII without ICU (verify behavior)

**UnicodeNfcNormalizer Tests** (icu feature only)
- Composed characters remain composed
- Decomposed characters → composed
- Mixed normalization levels → NFC
- Empty string
- Known NFC test vectors

**Analyzer Tests**
- Single normalizer application
- Multiple normalizers in sequence
- Zero normalizers (identity transformation)
- Tokenization only (no normalization)
- Empty input
- Error propagation

**FieldAnalyzers Tests**
- Register and retrieve analyzers
- Default analyzer fallback
- Missing analyzer with no default (error)
- Standard configuration
- Multiple fields with different analyzers

### Property-Based Tests

**Tokenizer Properties**
- tokenize(text).iter().map(|t| t.text()).collect() should reconstruct text (with whitespace separators)
- All tokens should have byte_start < byte_end
- All tokens should have byte offsets within original text bounds
- Token sequence should be in order (monotonically increasing offsets)

**Normalizer Properties**
- normalize("") should return Ok("")
- normalize(normalize(text)) should equal normalize(text) (idempotence)
- Length may change but should never panic
- Output should always be valid UTF-8

**Analyzer Properties**
- analyze("") should return empty vector (not error)
- Token count should match tokenizer output
- All token text should pass through all normalizers

### Integration Tests

**End-to-End Scenarios**
1. Index a document with multiple fields using standard configuration
2. Search tokenization should match index tokenization
3. ICU and non-ICU analyzers produce compatible results for ASCII text
4. Error handling for invalid UTF-8 input

**Performance Benchmarks**
- WhitespaceTokenizer throughput (MB/s)
- IcuTokenizer throughput (MB/s)
- LowercaseNormalizer latency
- UnicodeNfcNormalizer latency
- Full analysis pipeline latency
- Memory allocation counts

### Test Data

**Multilingual Test Corpus**
- English (ASCII and extended Latin)
- German (with compound words, umlauts)
- French (accents, ligatures)
- Spanish (accents, inverted punctuation)
- Russian (Cyrillic)
- Chinese (CJK characters)
- Japanese (Hiragana, Katakana, Kanji, mixed)
- Korean (Hangul)
- Thai (complex word boundaries)
- Arabic (right-to-left text)

**Edge Case Coverage**
- Empty strings
- Single character strings
- Whitespace-only strings
- Mixed Unicode normalization forms
- Combining characters
- Emoji sequences
- Zero-width characters
- Invalid Unicode sequences (error paths)

## Verification Commands

### Build Verification

```bash
# Build no_std + alloc (minimal configuration)
cargo build --no-default-features --features alloc

# Build with std
cargo build --features std

# Build with ICU4X
cargo build --features icu

# Build full feature set
cargo build --features full

# Build docs (including private items)
cargo doc --document-private-items --features full

# Check documentation links
cargo doc --features full
```

### Test Verification

```bash
# Run unit tests (no_std)
cargo test --no-default-features --features alloc

# Run unit tests (with ICU)
cargo test --features icu

# Run integration tests
cargo test --features full

# Run with detailed output
cargo test --features full -- --nocapture

# Run specific test
cargo test --features full test_whitespace_tokenizer

# Run ignored tests (typically slow benchmarks)
cargo test --features full -- --ignored
```

### Lint and Format Verification

```bash
# Check formatting
cargo fmt --check

# Apply formatting
cargo fmt

# Run clippy (no_std)
cargo clippy --no-default-features --features alloc

# Run clippy (full features)
cargo clippy --features full -- -D warnings
```

### Benchmark Verification

```bash
# Run benchmarks (requires std feature)
cargo bench --features full

# Run specific benchmark
cargo bench --features full -- bench_whitespace_tokenizer
```

### Example Verification

```bash
# Run examples (if examples/ directory exists)
cargo run --example basic_tokenizer --features std

# Run example with ICU
cargo run --example unicode_tokenizer --features icu
```

### Cross-Compilation Verification (if applicable)

```bash
# For no_std targets (requires appropriate toolchain)
cargo build --no-default-features --features alloc --target thumbv7em-none-eabihf

# Check for WASM
cargo build --no-default-features --features alloc --target wasm32-unknown-unknown
```

### Documentation Example Tests

```bash
# Run doctests
cargo test --features full --doc

# Run doctests with specific feature
cargo test --features icu --doc
```

## Success Metrics

1. **Functionality**: All acceptance criteria items pass
2. **Test Coverage**: >90% line coverage for public APIs
3. **Performance**: 
   - WhitespaceTokenizer: >100 MB/s throughput
   - IcuTokenizer: >10 MB/s throughput
   - Memory allocations minimal and predictable
4. **Compatibility**: All target configurations build and test successfully
5. **Documentation**: All public APIs documented with examples
6. **Safety**: No unsafe code unless absolutely necessary (documented)

## Open Questions

1. Should we support additional normalization forms beyond NFC (NFD, NFKC, NFKD)?
2. Should `Token` trait be object-safe (dyn Token)? Current design assumes yes.
3. Should we add stemming support? (Out of scope for initial version)
4. Should we add stop-word filtering? (Out of scope for initial version)
5. Should `IcuTokenizer` expose more granular configuration of segmenter rules?
6. Should we support custom token types beyond the predefined `TokenType` enum?
7. Should we add position increment support for phrase queries? (Deferred to indexing layer)
