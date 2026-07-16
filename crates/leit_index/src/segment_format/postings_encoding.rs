// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Frozen DEC-22 mapping between table discriminators and codec payload markers.

use leit_postings::codec::CodecId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PostingsEncoding {
    LegacyRawV1,
    DeltaVarint,
    BlockDelta,
}

impl PostingsEncoding {
    pub(crate) const fn from_kind(kind: u32) -> Option<Self> {
        match kind {
            0 => Some(Self::LegacyRawV1),
            1 => Some(Self::DeltaVarint),
            2 => Some(Self::BlockDelta),
            _ => None,
        }
    }

    pub(crate) const fn for_codec(codec_id: CodecId) -> Self {
        match codec_id {
            CodecId::DeltaVarint => Self::DeltaVarint,
            CodecId::BlockDelta => Self::BlockDelta,
        }
    }

    pub(crate) const fn kind(self) -> u32 {
        match self {
            Self::LegacyRawV1 => 0,
            Self::DeltaVarint => 1,
            Self::BlockDelta => 2,
        }
    }

    pub(crate) const fn expected_marker(self) -> Option<u8> {
        match self {
            Self::LegacyRawV1 => None,
            Self::DeltaVarint => Some(CodecId::DeltaVarint.to_u8()),
            Self::BlockDelta => Some(CodecId::BlockDelta.to_u8()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dec_22_mapping_is_exact_and_reversible() {
        for (encoding, kind, marker) in [
            (PostingsEncoding::LegacyRawV1, 0, None),
            (PostingsEncoding::DeltaVarint, 1, Some(0)),
            (PostingsEncoding::BlockDelta, 2, Some(1)),
        ] {
            assert_eq!(PostingsEncoding::from_kind(kind), Some(encoding));
            assert_eq!(encoding.kind(), kind);
            assert_eq!(encoding.expected_marker(), marker);
        }
        assert_eq!(PostingsEncoding::from_kind(3), None);
        assert_eq!(
            PostingsEncoding::for_codec(CodecId::DeltaVarint),
            PostingsEncoding::DeltaVarint
        );
        assert_eq!(
            PostingsEncoding::for_codec(CodecId::BlockDelta),
            PostingsEncoding::BlockDelta
        );
    }
}
