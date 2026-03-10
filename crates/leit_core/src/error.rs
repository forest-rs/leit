//! Core error type for Leif.
//!
//! CoreError provides a unified error type for core operations throughout
//! the Leif system.

use core::fmt;

#[cfg(test)]
use alloc::string::ToString;

/// Core error type for Leif.
///
/// This error type uses `&'static str` for all error messages to avoid
/// allocations and ensure compatibility with `no_std` environments.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum CoreError {
    /// The requested entity was not found.
    NotFound {
        /// Type of entity that was not found.
        entity_type: &'static str,
        /// Identifier that was not found.
        id: u64,
    },

    /// The requested operation is not supported.
    Unsupported {
        /// Description of what is unsupported.
        operation: &'static str,
    },

    /// Invalid input provided.
    InvalidInput {
        /// Description of the validation error.
        reason: &'static str,
    },

    /// A limit was exceeded.
    LimitExceeded {
        /// The limit that was exceeded.
        limit: &'static str,
        /// Current value that exceeded the limit.
        value: u64,
        /// Maximum allowed value.
        max: u64,
    },

    /// An invariant was violated.
    InvariantViolated {
        /// Description of the invariant.
        invariant: &'static str,
    },

    /// Out of memory.
    OutOfMemory,
}

impl CoreError {
    /// Creates a NotFound error.
    ///
    /// # Examples
    ///
    /// ```
    /// use leit_core::CoreError;
    ///
    /// let err = CoreError::not_found("Document", 42);
    /// assert!(matches!(err, CoreError::NotFound { .. }));
    /// ```
    #[inline]
    pub fn not_found(entity_type: &'static str, id: u64) -> Self {
        Self::NotFound { entity_type, id }
    }

    /// Creates an Unsupported error.
    ///
    /// # Examples
    ///
    /// ```
    /// use leit_core::CoreError;
    ///
    /// let err = CoreError::unsupported("custom_analyzer");
    /// assert!(matches!(err, CoreError::Unsupported { .. }));
    /// ```
    #[inline]
    pub fn unsupported(operation: &'static str) -> Self {
        Self::Unsupported { operation }
    }

    /// Creates an InvalidInput error.
    ///
    /// # Examples
    ///
    /// ```
    /// use leit_core::CoreError;
    ///
    /// let err = CoreError::invalid_input("empty field name");
    /// assert!(matches!(err, CoreError::InvalidInput { .. }));
    /// ```
    #[inline]
    pub fn invalid_input(reason: &'static str) -> Self {
        Self::InvalidInput { reason }
    }

    /// Creates a LimitExceeded error.
    ///
    /// # Examples
    ///
    /// ```
    /// use leit_core::CoreError;
    ///
    /// let err = CoreError::limit_exceeded("max_fields", 1000, 100);
    /// assert!(matches!(err, CoreError::LimitExceeded { .. }));
    /// ```
    #[inline]
    pub fn limit_exceeded(limit: &'static str, value: u64, max: u64) -> Self {
        Self::LimitExceeded { limit, value, max }
    }

    /// Creates an InvariantViolated error.
    ///
    /// # Examples
    ///
    /// ```
    /// use leit_core::CoreError;
    ///
    /// let err = CoreError::invariant_violated("segment must be locked");
    /// assert!(matches!(err, CoreError::InvariantViolated { .. }));
    /// ```
    #[inline]
    pub fn invariant_violated(invariant: &'static str) -> Self {
        Self::InvariantViolated { invariant }
    }
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { entity_type, id } => {
                write!(f, "{} not found: {}", entity_type, id)
            }
            Self::Unsupported { operation } => {
                write!(f, "operation not supported: {}", operation)
            }
            Self::InvalidInput { reason } => {
                write!(f, "invalid input: {}", reason)
            }
            Self::LimitExceeded { limit, value, max } => {
                write!(f, "limit exceeded: {} (value: {}, max: {})", limit, value, max)
            }
            Self::InvariantViolated { invariant } => {
                write!(f, "invariant violated: {}", invariant)
            }
            Self::OutOfMemory => {
                write!(f, "out of memory")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for CoreError {}

#[cfg(feature = "std")]
impl From<CoreError> for std::io::Error {
    #[inline]
    fn from(err: CoreError) -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::Other, err)
    }
}

#[cfg(feature = "alloc")]
impl From<alloc::collections::TryReserveError> for CoreError {
    #[inline]
    fn from(_: alloc::collections::TryReserveError) -> Self {
        Self::OutOfMemory
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_found() {
        let err = CoreError::not_found("Document", 42);
        assert!(matches!(err, CoreError::NotFound { .. }));
        assert_eq!(err.to_string(), "Document not found: 42");
    }

    #[test]
    fn test_unsupported() {
        let err = CoreError::unsupported("custom_analyzer");
        assert!(matches!(err, CoreError::Unsupported { .. }));
        assert_eq!(err.to_string(), "operation not supported: custom_analyzer");
    }

    #[test]
    fn test_invalid_input() {
        let err = CoreError::invalid_input("empty field name");
        assert!(matches!(err, CoreError::InvalidInput { .. }));
        assert_eq!(err.to_string(), "invalid input: empty field name");
    }

    #[test]
    fn test_limit_exceeded() {
        let err = CoreError::limit_exceeded("max_fields", 1000, 100);
        assert!(matches!(err, CoreError::LimitExceeded { .. }));
        assert_eq!(
            err.to_string(),
            "limit exceeded: max_fields (value: 1000, max: 100)"
        );
    }

    #[test]
    fn test_invariant_violated() {
        let err = CoreError::invariant_violated("segment must be locked");
        assert!(matches!(err, CoreError::InvariantViolated { .. }));
        assert_eq!(
            err.to_string(),
            "invariant violated: segment must be locked"
        );
    }

    #[test]
    fn test_out_of_memory() {
        let err = CoreError::OutOfMemory;
        assert_eq!(err.to_string(), "out of memory");
    }

    #[test]
    fn test_error_clone() {
        let err1 = CoreError::not_found("Document", 42);
        let err2 = err1.clone();
        assert_eq!(err1, err2);
    }

    #[test]
    fn test_error_eq() {
        let err1 = CoreError::not_found("Document", 42);
        let err2 = CoreError::not_found("Document", 42);
        assert_eq!(err1, err2);

        let err3 = CoreError::not_found("Document", 43);
        assert_ne!(err1, err3);
    }
}
