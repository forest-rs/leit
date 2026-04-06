// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use core::fmt;

use leit_query::QueryError;

use crate::segment::SectionKind;

/// Errors produced while building or exporting an in-memory index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IndexError {
    /// A document ID was indexed more than once.
    DuplicateDocument(u32),
    /// A field was indexed without a registered analyzer.
    MissingAnalyzer(leit_core::FieldId),
    /// Execution required scores but no scorer was supplied.
    MissingScorer,
    /// A size or offset does not fit in the on-disk format.
    ValueOutOfRange,
    /// Structured filter predicates require columnar storage (Phase 3).
    UnsupportedFilterPredicate,
    /// Query planning failed.
    Query(QueryError),
}

impl fmt::Display for IndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateDocument(id) => write!(f, "duplicate document ID: {id}"),
            Self::MissingAnalyzer(field) => {
                write!(f, "missing analyzer for field {}", field.as_u32())
            }
            Self::MissingScorer => write!(f, "execution requires a scorer but none was provided"),
            Self::UnsupportedFilterPredicate => write!(
                f,
                "structured filter predicates require columnar storage (not yet implemented)"
            ),
            Self::ValueOutOfRange => write!(f, "value out of range for on-disk format"),
            Self::Query(err) => write!(f, "query error: {err}"),
        }
    }
}

impl core::error::Error for IndexError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Query(err) => Some(err),
            Self::DuplicateDocument(_)
            | Self::MissingAnalyzer(_)
            | Self::MissingScorer
            | Self::UnsupportedFilterPredicate
            | Self::ValueOutOfRange => None,
        }
    }
}

/// Errors produced while opening or validating a borrowed segment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SegmentError {
    /// The buffer does not start with the expected magic bytes.
    InvalidMagic,
    /// The segment version is not supported by this reader.
    UnsupportedVersion(u16),
    /// The fixed-size header was truncated.
    TruncatedHeader,
    /// The section directory was truncated.
    TruncatedDirectory,
    /// A section kind in the directory is not known to this reader.
    InvalidSectionKind(u32),
    /// A section appears more than once.
    DuplicateSection(SectionKind),
    /// A required section is missing.
    MissingSection(SectionKind),
    /// A section offset/length points outside the buffer.
    OutOfBoundsSection(SectionKind),
    /// Two declared sections overlap.
    OverlappingSections {
        /// The first overlapping section.
        first: SectionKind,
        /// The second overlapping section.
        second: SectionKind,
    },
}

impl fmt::Display for SegmentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMagic => write!(f, "invalid segment magic bytes"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported segment version: {v}"),
            Self::TruncatedHeader => write!(f, "truncated segment header"),
            Self::TruncatedDirectory => write!(f, "truncated section directory"),
            Self::InvalidSectionKind(k) => write!(f, "invalid section kind: {k}"),
            Self::DuplicateSection(k) => write!(f, "duplicate section: {k:?}"),
            Self::MissingSection(k) => write!(f, "missing required section: {k:?}"),
            Self::OutOfBoundsSection(k) => write!(f, "out-of-bounds section: {k:?}"),
            Self::OverlappingSections { first, second } => {
                write!(f, "overlapping sections: {first:?} and {second:?}")
            }
        }
    }
}

impl core::error::Error for SegmentError {}
