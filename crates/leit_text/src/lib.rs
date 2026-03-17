// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Text analysis for Leit retrieval system.
//!
//! This crate provides tokenization and normalization for text indexing
//! and querying. Phase 1 keeps analysis intentionally simple while treating
//! Unicode normalization as part of the default contract.

#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::ops::Range;

use icu_casemap::CaseMapper;
use icu_locid::LanguageIdentifier;
use leit_core::FieldId;
use unicode_normalization::UnicodeNormalization;

// ============================================================================
// Token
// ============================================================================

/// A token produced by tokenization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token<'a> {
    /// The token text.
    pub text: &'a str,
    /// Position in the token stream (0-indexed).
    pub position: u32,
    /// Byte offset range in the original text.
    pub byte_range: Range<usize>,
}

impl<'a> Token<'a> {
    /// Create a new token.
    pub const fn new(text: &'a str, position: u32, start: usize, end: usize) -> Self {
        Self {
            text,
            position,
            byte_range: start..end,
        }
    }
}

/// An owned token with owned text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedToken {
    /// The token text.
    pub text: String,
    /// Position in the token stream (0-indexed).
    pub position: u32,
    /// Byte offset range in the original text.
    pub byte_range: Range<usize>,
}

impl OwnedToken {
    /// Create a new owned token.
    pub const fn new(text: String, position: u32, start: usize, end: usize) -> Self {
        Self {
            text,
            position,
            byte_range: start..end,
        }
    }

    /// Create an owned token from a borrowed token.
    pub fn from_token(token: &Token<'_>) -> Self {
        Self {
            text: String::from(token.text),
            position: token.position,
            byte_range: token.byte_range.clone(),
        }
    }
}

// ============================================================================
// Tokenizer Trait
// ============================================================================

/// Trait for tokenizing text.
pub trait Tokenizer {
    /// Tokenize the given text.
    fn tokenize<'a>(&self, text: &'a str, output: &mut Vec<Token<'a>>);
}

// ============================================================================
// Normalizer Trait
// ============================================================================

/// Trait for normalizing text.
pub trait Normalizer {
    /// Normalize text, returning an owned String.
    fn normalize(&self, text: &str) -> String;

    /// Check if normalization is needed for this text.
    fn needs_normalize(&self, text: &str) -> bool;
}

// ============================================================================
// WhitespaceTokenizer
// ============================================================================

/// A simple whitespace tokenizer.
#[derive(Clone, Copy, Debug, Default)]
pub struct WhitespaceTokenizer;

impl WhitespaceTokenizer {
    /// Create a new whitespace tokenizer.
    pub const fn new() -> Self {
        Self
    }

    /// Tokenize text into a callback without allocation.
    pub fn tokenize_into<'a, F>(&self, text: &'a str, position_offset: u32, mut callback: F)
    where
        F: FnMut(Token<'a>),
    {
        let mut position = position_offset;
        let mut start = 0_usize;
        let mut in_token = false;

        for (i, c) in text.char_indices() {
            if c.is_whitespace() {
                if in_token {
                    callback(Token::new(&text[start..i], position, start, i));
                    position = position.checked_add(1).expect("token position overflow");
                    in_token = false;
                }
            } else if !in_token {
                start = i;
                in_token = true;
            }
        }

        if in_token {
            callback(Token::new(&text[start..], position, start, text.len()));
        }
    }
}

impl Tokenizer for WhitespaceTokenizer {
    fn tokenize<'a>(&self, text: &'a str, output: &mut Vec<Token<'a>>) {
        output.clear();
        self.tokenize_into(text, 0, |token| output.push(token));
    }
}

// ============================================================================
// UnicodeNormalizer
// ============================================================================

/// Canonical Unicode normalization applied before case mapping.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CanonicalForm {
    /// Do not apply canonical normalization.
    None,
    /// Normalize to NFC.
    #[default]
    Nfc,
    /// Normalize to NFKC.
    Nfkc,
}

/// Case mapping applied after canonical normalization.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CaseMapping {
    /// Do not apply case mapping.
    None,
    /// Apply Unicode lowercase mapping.
    ///
    /// This uses the root locale, which is appropriate for locale-independent search
    /// normalization but not for language-specific text presentation.
    #[default]
    Lowercase,
    /// Apply locale-independent full Unicode case folding.
    ///
    /// This is stronger than lowercase mapping and can expand characters such as `ß`
    /// into multiple scalar values for case-insensitive matching.
    Fold,
}

/// Builder for [`UnicodeNormalizer`].
#[derive(Clone, Copy, Debug, Default)]
pub struct UnicodeNormalizerBuilder {
    canonical_form: CanonicalForm,
    case_mapping: CaseMapping,
}

impl UnicodeNormalizerBuilder {
    /// Configure the canonical form used during normalization.
    #[must_use]
    pub const fn canonical_form(mut self, canonical_form: CanonicalForm) -> Self {
        self.canonical_form = canonical_form;
        self
    }

    /// Configure the case mapping used during normalization.
    #[must_use]
    pub const fn case_mapping(mut self, case_mapping: CaseMapping) -> Self {
        self.case_mapping = case_mapping;
        self
    }

    /// Build the configured normalizer.
    pub const fn build(self) -> UnicodeNormalizer {
        UnicodeNormalizer {
            canonical_form: self.canonical_form,
            case_mapping: self.case_mapping,
        }
    }
}

/// A normalizer that canonicalizes Unicode text and applies configurable case mapping.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnicodeNormalizer {
    canonical_form: CanonicalForm,
    case_mapping: CaseMapping,
}

impl UnicodeNormalizer {
    /// Create a new Unicode normalizer with default settings.
    pub const fn new() -> Self {
        Self::builder().build()
    }

    /// Create a builder for configuring Unicode normalization.
    pub const fn builder() -> UnicodeNormalizerBuilder {
        UnicodeNormalizerBuilder {
            canonical_form: CanonicalForm::Nfc,
            case_mapping: CaseMapping::Lowercase,
        }
    }

    /// Return the configured canonical form.
    pub const fn canonical_form(&self) -> CanonicalForm {
        self.canonical_form
    }

    /// Return the configured case mapping.
    pub const fn case_mapping(&self) -> CaseMapping {
        self.case_mapping
    }
}

impl Normalizer for UnicodeNormalizer {
    fn normalize(&self, text: &str) -> String {
        let canonicalized = apply_canonical_form(text, self.canonical_form);
        let case_mapped = apply_case_mapping(&canonicalized, self.case_mapping);
        apply_canonical_form(&case_mapped, self.canonical_form)
    }

    fn needs_normalize(&self, text: &str) -> bool {
        if needs_canonical_form(text, self.canonical_form) {
            return true;
        }
        if needs_case_mapping(text, self.case_mapping) {
            return true;
        }
        // Case mapping may introduce non-canonical sequences, so re-check
        // after applying case mapping.
        if self.case_mapping != CaseMapping::None {
            let case_mapped = apply_case_mapping(text, self.case_mapping);
            if needs_canonical_form(&case_mapped, self.canonical_form) {
                return true;
            }
        }
        false
    }
}

fn apply_canonical_form(text: &str, canonical_form: CanonicalForm) -> String {
    match canonical_form {
        CanonicalForm::None => text.to_string(),
        CanonicalForm::Nfc => text.nfc().collect(),
        CanonicalForm::Nfkc => text.nfkc().collect(),
    }
}

fn apply_case_mapping(text: &str, case_mapping: CaseMapping) -> String {
    let case_mapper = CaseMapper::new();
    match case_mapping {
        CaseMapping::None => text.to_string(),
        CaseMapping::Lowercase => {
            let langid = LanguageIdentifier::default();
            case_mapper.lowercase_to_string(text, &langid)
        }
        CaseMapping::Fold => case_mapper.fold_string(text),
    }
}

fn needs_canonical_form(text: &str, canonical_form: CanonicalForm) -> bool {
    use unicode_normalization::IsNormalized;
    use unicode_normalization::is_nfc_quick;
    use unicode_normalization::is_nfkc_quick;
    match canonical_form {
        CanonicalForm::None => false,
        CanonicalForm::Nfc => is_nfc_quick(text.chars()) != IsNormalized::Yes,
        CanonicalForm::Nfkc => is_nfkc_quick(text.chars()) != IsNormalized::Yes,
    }
}

fn needs_case_mapping(text: &str, case_mapping: CaseMapping) -> bool {
    let case_mapper = CaseMapper::new();
    match case_mapping {
        CaseMapping::None => false,
        CaseMapping::Lowercase => {
            let langid = LanguageIdentifier::default();
            case_mapper.lowercase_to_string(text, &langid) != text
        }
        CaseMapping::Fold => case_mapper.fold_string(text) != text,
    }
}

// ============================================================================
// Analyzer
// ============================================================================

/// Combines a tokenizer with zero or more normalizers.
pub struct Analyzer {
    tokenizer: Box<dyn Tokenizer>,
    normalizers: Vec<Box<dyn Normalizer>>,
}

impl core::fmt::Debug for Analyzer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Analyzer")
            .field("normalizers_count", &self.normalizers.len())
            .finish_non_exhaustive()
    }
}

impl Analyzer {
    /// Create a new analyzer.
    pub fn new<T: Tokenizer + 'static>(tokenizer: T) -> Self {
        Self {
            tokenizer: Box::new(tokenizer),
            normalizers: Vec::new(),
        }
    }

    /// Add a normalizer to the pipeline.
    #[must_use]
    pub fn with_normalizer<N: Normalizer + 'static>(mut self, normalizer: N) -> Self {
        self.normalizers.push(Box::new(normalizer));
        self
    }

    /// Analyze text, returning owned tokens with normalized text.
    pub fn analyze<'a>(&self, text: &'a str) -> Vec<(Token<'a>, String)> {
        let mut tokens = Vec::new();
        self.tokenizer.tokenize(text, &mut tokens);

        tokens
            .into_iter()
            .map(|token| {
                let mut normalized = token.text.to_string();
                for normalizer in &self.normalizers {
                    normalized = normalizer.normalize(&normalized);
                }
                (token, normalized)
            })
            .collect()
    }
}

// ============================================================================
// FieldAnalyzers
// ============================================================================

/// Registry of analyzers per field.
pub struct FieldAnalyzers {
    analyzers: Vec<Option<Analyzer>>,
}

impl core::fmt::Debug for FieldAnalyzers {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FieldAnalyzers")
            .field("analyzers_count", &self.analyzers.len())
            .finish_non_exhaustive()
    }
}

impl FieldAnalyzers {
    /// Create an empty registry.
    pub const fn new() -> Self {
        Self {
            analyzers: Vec::new(),
        }
    }

    /// Maximum supported field count.
    ///
    /// Prevents OOM from a large `FieldId` value resizing the internal `Vec`.
    const MAX_FIELDS: usize = 1024;

    /// Register an analyzer for a field.
    ///
    /// # Panics
    ///
    /// Panics if the field index exceeds `Self::MAX_FIELDS`.
    pub fn set(&mut self, field: FieldId, analyzer: Analyzer) {
        let idx = field.as_u32() as usize;
        assert!(
            idx < Self::MAX_FIELDS,
            "field index {idx} exceeds maximum supported field count {}",
            Self::MAX_FIELDS,
        );
        if idx >= self.analyzers.len() {
            let new_len = idx.checked_add(1).expect("field analyzer index overflow");
            self.analyzers.resize_with(new_len, || None);
        }
        self.analyzers[idx] = Some(analyzer);
    }

    /// Get the analyzer for a field.
    pub fn get(&self, field: FieldId) -> Option<&Analyzer> {
        self.analyzers.get(field.as_u32() as usize)?.as_ref()
    }
}

impl Default for FieldAnalyzers {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_whitespace_tokenizer() {
        let tokenizer = WhitespaceTokenizer::new();
        let mut tokens = Vec::new();
        tokenizer.tokenize("hello world test", &mut tokens);

        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].text, "hello");
        assert_eq!(tokens[1].text, "world");
        assert_eq!(tokens[2].text, "test");
    }

    #[test]
    fn test_unicode_normalizer() {
        let normalizer = UnicodeNormalizer::new();
        let result = normalizer.normalize("Hello World");
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_unicode_no_change() {
        let normalizer = UnicodeNormalizer::new();
        let result = normalizer.normalize("hello world");
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_unicode_normalizer_unicode_case_and_canonical_equivalence() {
        let normalizer = UnicodeNormalizer::new();
        let result = normalizer.normalize("E\u{301} ΣΠΊΤΙ ÜBER");
        assert_eq!(result, "é σπίτι über");
    }

    #[test]
    fn test_unicode_needs_normalize_for_unicode_and_canonical_text() {
        let normalizer = UnicodeNormalizer::new();

        assert!(normalizer.needs_normalize("E\u{301}"));
        assert!(normalizer.needs_normalize("ÜBER"));
        assert!(!normalizer.needs_normalize("é"));
    }

    #[test]
    fn test_unicode_normalizer_builder_supports_canonical_only_mode() {
        let normalizer = UnicodeNormalizer::builder()
            .case_mapping(CaseMapping::None)
            .build();

        assert_eq!(normalizer.normalize("E\u{301}"), "É");
    }

    #[test]
    fn test_unicode_normalizer_builder_supports_case_fold_mode() {
        let normalizer = UnicodeNormalizer::builder()
            .case_mapping(CaseMapping::Fold)
            .build();

        assert_eq!(normalizer.normalize("Straße"), "strasse");
        assert_eq!(normalizer.normalize("STRASSE"), "strasse");
    }

    #[test]
    fn test_unicode_normalizer_accessors_reflect_builder_configuration() {
        let normalizer = UnicodeNormalizer::builder()
            .canonical_form(CanonicalForm::Nfkc)
            .case_mapping(CaseMapping::Fold)
            .build();

        assert_eq!(normalizer.canonical_form(), CanonicalForm::Nfkc);
        assert_eq!(normalizer.case_mapping(), CaseMapping::Fold);
    }

    #[test]
    fn test_owned_token_from_token() {
        let token = Token::new("Hello", 0, 0, 5);
        let owned = OwnedToken::from_token(&token);

        assert_eq!(owned.text, "Hello");
        assert_eq!(owned.position, 0);
        assert_eq!(owned.byte_range, 0..5);
    }

    #[test]
    fn test_analyzer_returns_owned_tokens() {
        let analyzer =
            Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new());

        let result = analyzer.analyze("Hello World");

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0.text, "Hello");
        assert_eq!(result[0].1, "hello");
        assert_eq!(result[0].0.position, 0);
        assert_eq!(result[1].0.text, "World");
        assert_eq!(result[1].1, "world");
        assert_eq!(result[1].0.position, 1);
    }
}
