//! EntityId trait for entity identification.
//!
//! The EntityId trait provides a common interface for types that can serve
//! as entity identifiers throughout the Leif system.

use core::fmt;
use core::hash::Hash;

/// Trait for entity identifiers used throughout Leif.
///
/// This trait provides a common interface for all identifier types, enabling
/// generic code that works with any entity identifier type.
///
/// # Required Bounds
///
/// All EntityId implementations must be:
/// - `Copy`: Cheap to copy, no heap allocation
/// - `PartialEq + Eq`: Comparable for equality
/// - `PartialOrd + Ord`: Comparable for ordering
/// - `Hash`: Can be used in hash-based collections
/// - `Debug`: Can be debug-formatted
/// - `Display`: Can be displayed to users
/// - `Send + Sync`: Safe to share across threads
/// - `'static`: Has no borrowed data
pub trait EntityId:
    Copy
    + PartialEq
    + Eq
    + PartialOrd
    + Ord
    + Hash
    + fmt::Debug
    + fmt::Display
    + Send
    + Sync
    + 'static
{
    /// Convert to u64 for serialization/hashing.
    fn as_u64(self) -> u64;

    /// Convert from u64.
    fn from_u64(id: u64) -> Self;

    /// Create a new ID with the given value.
    fn new(id: u32) -> Self;

    /// Get the underlying u32 value.
    fn into_u32(self) -> u32;

    /// Check if this is an invalid/sentinel ID.
    ///
    /// Returns `true` if this ID represents the special sentinel value
    /// indicating "no ID" or "invalid ID".
    fn is_invalid(self) -> bool;
}

// Blanket implementations for standard integer types

impl EntityId for u32 {
    #[inline]
    fn as_u64(self) -> u64 {
        self as u64
    }

    #[inline]
    fn from_u64(id: u64) -> Self {
        id as u32
    }

    #[inline]
    fn new(id: u32) -> Self {
        id
    }

    #[inline]
    fn into_u32(self) -> u32 {
        self
    }

    #[inline]
    fn is_invalid(self) -> bool {
        self == u32::MAX
    }
}

impl EntityId for u64 {
    #[inline]
    fn as_u64(self) -> u64 {
        self
    }

    #[inline]
    fn from_u64(id: u64) -> Self {
        id
    }

    #[inline]
    fn new(id: u32) -> Self {
        id as u64
    }

    #[inline]
    fn into_u32(self) -> u32 {
        self as u32
    }

    #[inline]
    fn is_invalid(self) -> bool {
        self == u64::MAX || (self as u32) == u32::MAX
    }
}

/// Macro to generate EntityId implementations for newtype wrappers.
///
/// # Example
///
/// ```rust,ignore
/// struct MyId(u32);
///
/// impl_entity_id!(MyId);
/// ```
#[macro_export]
macro_rules! impl_entity_id {
    ($ty:ident) => {
        impl $crate::EntityId for $ty {
            #[inline]
            fn as_u64(self) -> u64 {
                self.0 as u64
            }

            #[inline]
            fn from_u64(id: u64) -> Self {
                Self(id as u32)
            }

            #[inline]
            fn new(id: u32) -> Self {
                Self(id)
            }

            #[inline]
            fn into_u32(self) -> u32 {
                self.0
            }

            #[inline]
            fn is_invalid(self) -> bool {
                self.0 == u32::MAX
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_u32_entity_id() {
        let id: u32 = 42;
        assert_eq!(id.as_u64(), 42);
        assert_eq!(<u32 as EntityId>::from_u64(100), 100);
        assert_eq!(<u32 as EntityId>::new(50), 50);
        assert_eq!(id.into_u32(), 42);
        assert!(!id.is_invalid());
        assert!(u32::MAX.is_invalid());
    }

    #[test]
    fn test_u64_entity_id() {
        let id: u64 = 42;
        assert_eq!(id.as_u64(), 42);
        assert_eq!(<u64 as EntityId>::from_u64(100), 100);
        assert_eq!(<u64 as EntityId>::new(50), 50);
        assert_eq!(id.into_u32(), 42);
        assert!(!id.is_invalid());
    }
}
