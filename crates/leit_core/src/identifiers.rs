//! Core identifier types for Leif.
//!
//! All identifier types are newtype wrappers around `u32` for efficiency
//! and cache-friendliness. They use `#[repr(transparent)]` to ensure
//! they have the same representation as their inner type.

use core::fmt;
use core::hash::Hash;

/// Unique identifier for a field in the schema.
///
/// FieldIds are used to identify fields within a schema, providing
/// efficient storage and comparison operations.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct FieldId(pub u32);

impl FieldId {
    /// Creates a new FieldId.
    #[inline]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Returns the underlying value.
    #[inline]
    pub const fn into_u32(self) -> u32 {
        self.0
    }
}

impl fmt::Debug for FieldId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "F{}", self.0)
    }
}

#[cfg(feature = "std")]
impl fmt::Display for FieldId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "F{}", self.0)
    }
}

/// Unique identifier for a term in the inverted index.
///
/// TermIds identify terms in the inverted index, allowing efficient
/// term lookup and posting list access.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct TermId(pub u32);

impl TermId {
    /// Creates a new TermId.
    #[inline]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Returns the underlying value.
    #[inline]
    pub const fn into_u32(self) -> u32 {
        self.0
    }
}

impl fmt::Debug for TermId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "T{}", self.0)
    }
}

#[cfg(feature = "std")]
impl fmt::Display for TermId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "T{}", self.0)
    }
}

/// Unique identifier for a segment (data shard).
///
/// Segments are horizontal partitions of the index data. Each segment
/// is self-contained and can be searched independently.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct SegmentId(pub u32);

impl SegmentId {
    /// Creates a new SegmentId.
    #[inline]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Returns the underlying value.
    #[inline]
    pub const fn into_u32(self) -> u32 {
        self.0
    }

    /// Special sentinel value indicating "no segment".
    pub const INVALID: SegmentId = SegmentId(u32::MAX);
}

impl fmt::Debug for SegmentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "S{}", self.0)
    }
}

#[cfg(feature = "std")]
impl fmt::Display for SegmentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "S{}", self.0)
    }
}

/// Unique identifier for a node in a query plan tree.
///
/// QueryNodeIds identify nodes in the query execution plan, allowing
/// efficient tracking of query state and intermediate results.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct QueryNodeId(pub u32);

impl QueryNodeId {
    /// Creates a new QueryNodeId.
    #[inline]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Returns the underlying value.
    #[inline]
    pub const fn into_u32(self) -> u32 {
        self.0
    }
}

impl fmt::Debug for QueryNodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Q{}", self.0)
    }
}

#[cfg(feature = "std")]
impl fmt::Display for QueryNodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Q{}", self.0)
    }
}

/// Slot identifier for cursor positions during query execution.
///
/// CursorSlotIds identify positions in the cursor table used during
/// query execution, allowing efficient tracking of iteration state.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct CursorSlotId(pub u32);

impl CursorSlotId {
    /// Creates a new CursorSlotId.
    #[inline]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Returns the underlying value.
    #[inline]
    pub const fn into_u32(self) -> u32 {
        self.0
    }

    /// Maximum number of cursor slots supported.
    pub const MAX_SLOTS: u32 = 256;
}

impl fmt::Debug for CursorSlotId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "C{}", self.0)
    }
}

#[cfg(feature = "std")]
impl fmt::Display for CursorSlotId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "C{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_id() {
        let id = FieldId::new(42);
        assert_eq!(id.into_u32(), 42);
    }

    #[test]
    fn test_term_id() {
        let id = TermId::new(100);
        assert_eq!(id.into_u32(), 100);
    }

    #[test]
    fn test_segment_id() {
        let id = SegmentId::new(5);
        assert_eq!(id.into_u32(), 5);
        assert_eq!(SegmentId::INVALID.into_u32(), u32::MAX);
    }

    #[test]
    fn test_query_node_id() {
        let id = QueryNodeId::new(10);
        assert_eq!(id.into_u32(), 10);
    }

    #[test]
    fn test_cursor_slot_id() {
        let id = CursorSlotId::new(3);
        assert_eq!(id.into_u32(), 3);
        assert_eq!(CursorSlotId::MAX_SLOTS, 256);
    }
}
