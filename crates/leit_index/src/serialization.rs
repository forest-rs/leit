// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Fallible, codec-selected serialization of an immutable logical index.

use alloc::vec::Vec;
use core::fmt;

use leit_core::{SegmentLocalDocId, TermFreq};
use leit_postings::codec::{BLOCK_DOC_COUNT, CodecId, encoded_body_len_from_iter};

use crate::error::{IndexError, SegmentError, ValidationMode};
use crate::memory::InMemoryIndex;
use crate::segment_format::view::SegmentView;
use crate::segment_format::writer::write_compressed_segment;

/// Errors produced while preparing an owned serialized segment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SegmentWriteError {
    /// A codec body plus its marker cannot fit the v1 per-term `u32` payload length.
    TermPayloadTooLarge {
        /// Exact codec body length, excluding the marker.
        body_len: u64,
        /// Codec marker length included by the postings table.
        marker_len: u64,
    },
    /// Exact checked length arithmetic overflowed.
    LengthOverflow,
    /// Codec output did not match its exact preflight length.
    EncodedLengthMismatch {
        /// Exact marker-exclusive length from preflight.
        expected_body_len: u64,
        /// Marker-exclusive length emitted by the codec.
        actual_body_len: u64,
    },
    /// The logical index term count differs from the completed serialization plan.
    SerializationPlanMismatch {
        /// Number of terms captured by preflight.
        planned_terms: usize,
        /// Number of terms presented to the writer.
        actual_terms: usize,
    },
    /// A term's planned codec block boundaries differ from its postings cardinality.
    SerializationBlockPlanMismatch {
        /// Zero-based term position in lexicon order.
        term_index: usize,
        /// Number of block boundaries captured by preflight.
        planned_blocks: usize,
        /// Number of blocks implied by the codec and postings cardinality.
        actual_blocks: usize,
    },
    /// A non-codec field cannot be represented by the v1 layout.
    Index(IndexError),
    /// The assembled bytes failed segment validation.
    Validation(SegmentError),
}

impl fmt::Display for SegmentWriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TermPayloadTooLarge {
                body_len,
                marker_len,
            } => write!(
                f,
                "term payload body {body_len} plus marker {marker_len} exceeds v1 u32 length"
            ),
            Self::LengthOverflow => write!(f, "segment length arithmetic overflowed"),
            Self::EncodedLengthMismatch {
                expected_body_len,
                actual_body_len,
            } => write!(
                f,
                "codec emitted body length {actual_body_len}, expected {expected_body_len}"
            ),
            Self::SerializationPlanMismatch {
                planned_terms,
                actual_terms,
            } => write!(
                f,
                "serialization plan contains {planned_terms} terms, index contains {actual_terms}"
            ),
            Self::SerializationBlockPlanMismatch {
                term_index,
                planned_blocks,
                actual_blocks,
            } => write!(
                f,
                "serialization plan term {term_index} contains {planned_blocks} blocks, postings require {actual_blocks}"
            ),
            Self::Index(error) => write!(f, "segment field encoding failed: {error}"),
            Self::Validation(error) => write!(f, "prepared segment failed validation: {error}"),
        }
    }
}

impl core::error::Error for SegmentWriteError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Index(error) => Some(error),
            Self::Validation(error) => Some(error),
            Self::TermPayloadTooLarge { .. }
            | Self::LengthOverflow
            | Self::EncodedLengthMismatch { .. }
            | Self::SerializationPlanMismatch { .. }
            | Self::SerializationBlockPlanMismatch { .. } => None,
        }
    }
}

impl From<IndexError> for SegmentWriteError {
    fn from(value: IndexError) -> Self {
        Self::Index(value)
    }
}

/// An owned segment whose sizing, assembly, and structural validation have completed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedSegment {
    bytes: Vec<u8>,
}

impl PreparedSegment {
    /// Consume this prepared segment and return its validated bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

/// Validate a marker-inclusive v1 term payload length without allocating.
pub(crate) fn validate_term_payload_len(
    body_len: u64,
    marker_len: u64,
) -> Result<u32, SegmentWriteError> {
    let payload_len = body_len
        .checked_add(marker_len)
        .ok_or(SegmentWriteError::LengthOverflow)?;
    u32::try_from(payload_len).map_err(|_| SegmentWriteError::TermPayloadTooLarge {
        body_len,
        marker_len,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PlannedTermLength {
    pub(crate) body_len: u64,
    pub(crate) payload_len: u32,
    pub(crate) block_offsets: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SerializationPlan {
    pub(crate) term_lengths: Vec<PlannedTermLength>,
}

fn build_serialization_plan(
    index: &InMemoryIndex,
    codec_id: CodecId,
) -> Result<SerializationPlan, SegmentWriteError> {
    let mut term_lengths = Vec::with_capacity(index.postings().len());
    for postings in index.postings().values() {
        let (body_len, block_offsets) = match codec_id {
            CodecId::DeltaVarint => {
                let body_len = encoded_body_len_from_iter(
                    codec_id,
                    postings.iter().map(|posting| {
                        (
                            SegmentLocalDocId::new(posting.doc_id),
                            TermFreq::new(posting.term_freq),
                        )
                    }),
                )
                .ok_or(SegmentWriteError::LengthOverflow)?;
                let offsets = if postings.is_empty() {
                    Vec::new()
                } else {
                    alloc::vec![0_u64]
                };
                (body_len, offsets)
            }
            CodecId::BlockDelta => {
                let mut body_len = 0_u64;
                let mut offsets = Vec::with_capacity(postings.len().div_ceil(BLOCK_DOC_COUNT));
                for block in postings.chunks(BLOCK_DOC_COUNT) {
                    offsets.push(
                        body_len
                            .checked_add(1)
                            .ok_or(SegmentWriteError::LengthOverflow)?,
                    );
                    let block_len = encoded_body_len_from_iter(
                        codec_id,
                        block.iter().map(|posting| {
                            (
                                SegmentLocalDocId::new(posting.doc_id),
                                TermFreq::new(posting.term_freq),
                            )
                        }),
                    )
                    .ok_or(SegmentWriteError::LengthOverflow)?;
                    body_len = body_len
                        .checked_add(block_len)
                        .ok_or(SegmentWriteError::LengthOverflow)?;
                }
                (body_len, offsets)
            }
        };
        let payload_len = validate_term_payload_len(body_len, 1)?;
        let block_offsets = block_offsets
            .into_iter()
            .map(|offset| u32::try_from(offset).map_err(|_| SegmentWriteError::LengthOverflow))
            .collect::<Result<Vec<_>, _>>()?;
        term_lengths.push(PlannedTermLength {
            body_len,
            payload_len,
            block_offsets,
        });
    }
    Ok(SerializationPlan { term_lengths })
}

/// Prepare codec-selected segment bytes without consuming or mutating the logical index.
pub fn prepare_serialization(
    index: &InMemoryIndex,
    codec_id: CodecId,
) -> Result<PreparedSegment, SegmentWriteError> {
    // Complete every per-term size and block-boundary check before allocating output sections.
    let plan = build_serialization_plan(index, codec_id)?;
    let bytes = write_compressed_segment(index, codec_id, &plan)?;
    SegmentView::open_with_validation(&bytes, ValidationMode::Full)
        .map_err(SegmentWriteError::Validation)?;
    Ok(PreparedSegment { bytes })
}

#[cfg(test)]
mod tests {
    use super::*;

    use alloc::vec;
    use leit_core::FieldId;
    use leit_query::TermDictionary;
    use leit_text::{
        AnalysisSchemaId, Analyzer, FieldAnalyzers, UnicodeNormalizer, WhitespaceTokenizer,
    };

    use crate::{ExecutableIndex, InMemoryIndexBuilder, prepare_merge};

    fn analyzers() -> FieldAnalyzers {
        let mut analyzers = FieldAnalyzers::with_schema_id(
            AnalysisSchemaId::new(8_084).expect("fixture schema ID is nonzero"),
        );
        analyzers.set(
            FieldId::new(1),
            Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
        );
        analyzers
    }

    fn source() -> InMemoryIndex {
        let mut builder = InMemoryIndexBuilder::new(analyzers());
        builder.register_field_alias(FieldId::new(1), "body");
        builder
            .index_document(7, &[(FieldId::new(1), "common rare")])
            .expect("fixture document should index");
        builder.build_index()
    }

    #[test]
    fn oversized_payload_rejection_leaves_merged_index_usable() {
        let merged = prepare_merge(vec![source(), source()], analyzers())
            .expect("matching schemas should prepare")
            .execute();

        assert_eq!(
            validate_term_payload_len(u64::from(u32::MAX) - 1, 1),
            Ok(u32::MAX),
            "largest marker-inclusive u32 payload must be accepted"
        );
        assert_eq!(
            validate_term_payload_len(u64::from(u32::MAX), 1),
            Err(SegmentWriteError::TermPayloadTooLarge {
                body_len: u64::from(u32::MAX),
                marker_len: 1,
            }),
            "one byte beyond the marker-inclusive u32 limit must reject"
        );

        let common = merged
            .index()
            .resolve_term(FieldId::new(1), "common")
            .expect("query planning must remain usable after rejection");
        assert_eq!(
            ExecutableIndex::postings(merged.index(), common)
                .expect("postings must remain usable after rejection")
                .len(),
            2,
            "both source postings must survive synthetic size rejection"
        );
        let prepared = prepare_serialization(merged.index(), CodecId::BlockDelta)
            .expect("the same borrowed index must serialize after synthetic rejection");
        assert!(
            !prepared.into_bytes().is_empty(),
            "prepared bytes must publish atomically"
        );
    }

    #[test]
    fn serialization_plan_records_codec_block_boundaries() {
        let mut builder = InMemoryIndexBuilder::new(analyzers());
        for doc_id in 0..129 {
            builder
                .index_document(doc_id, &[(FieldId::new(1), "common")])
                .expect("fixture document should index");
        }
        let index = builder.build_index();
        let postings = index
            .postings()
            .values()
            .next()
            .expect("fixture term should have postings");

        let delta_plan =
            build_serialization_plan(&index, CodecId::DeltaVarint).expect("delta plan should fit");
        assert_eq!(delta_plan.term_lengths[0].block_offsets, vec![0]);

        let block_plan =
            build_serialization_plan(&index, CodecId::BlockDelta).expect("block plan should fit");
        let first_body_len = encoded_body_len_from_iter(
            CodecId::BlockDelta,
            postings[..128].iter().map(|posting| {
                (
                    SegmentLocalDocId::new(posting.doc_id),
                    TermFreq::new(posting.term_freq),
                )
            }),
        )
        .expect("fixture block size should fit");
        assert_eq!(
            block_plan.term_lengths[0].block_offsets,
            vec![
                1,
                1 + u32::try_from(first_body_len).expect("fixture block size fits u32")
            ]
        );
    }
}
