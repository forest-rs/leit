use leit_query::QueryError;

use crate::segment::SectionKind;

/// Errors produced while building or exporting an in-memory index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IndexError {
    /// A document ID was indexed more than once.
    DuplicateDocument(u32),
    /// A field was indexed without a registered analyzer.
    MissingAnalyzer(leit_core::FieldId),
    /// A size or offset does not fit in the on-disk format.
    ValueOutOfRange,
    /// Query planning failed.
    Query(QueryError),
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
