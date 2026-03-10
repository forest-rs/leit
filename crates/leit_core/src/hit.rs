//! Hit type for search results.
//!
//! The Hit type represents a single search result containing an entity ID
//! and its relevance score.

use core::cmp::Ordering;
use core::fmt;

use crate::Score;

/// A single search result hit.
///
/// Contains an entity ID and its relevance score, representing one match
/// in a search result set.
///
/// # Ordering
///
/// Hits are ordered by score in descending order (higher scores come first).
/// For hits with equal scores, the entity ID is used as a tiebreaker.
/// This makes them useful with `BinaryHeap` and sorting operations where
/// you want the best results at the front.
///
/// # Type Parameters
///
/// - `Id`: The entity identifier type (must implement `Copy` and `Ord`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Hit<Id> {
    /// The entity identifier.
    pub id: Id,

    /// The relevance score.
    pub score: Score,
}

impl<Id: Copy> Hit<Id> {
    /// Creates a new hit with the given ID and score.
    ///
    /// # Examples
    ///
    /// ```
    /// use leit_core::{Hit, Score};
    ///
    /// let hit = Hit::new(42u32, Score::new(0.85));
    /// assert_eq!(hit.id, 42);
    /// assert_eq!(hit.score.into_f32(), 0.85);
    /// ```
    #[inline]
    pub fn new(id: Id, score: Score) -> Self {
        Self { id, score }
    }

    /// Creates a new hit with score 1.0 (perfect score).
    ///
    /// # Examples
    ///
    /// ```
    /// use leit_core::Hit;
    ///
    /// let hit = Hit::perfect(42u32);
    /// assert_eq!(hit.id, 42);
    /// assert_eq!(hit.score.into_f32(), 1.0);
    /// ```
    #[inline]
    pub fn perfect(id: Id) -> Self {
        Self {
            id,
            score: Score::ONE,
        }
    }

    /// Creates a new hit with score 0.0 (zero score).
    ///
    /// # Examples
    ///
    /// ```
    /// use leit_core::Hit;
    ///
    /// let hit = Hit::zero(42u32);
    /// assert_eq!(hit.id, 42);
    /// assert_eq!(hit.score.into_f32(), 0.0);
    /// ```
    #[inline]
    pub fn zero(id: Id) -> Self {
        Self {
            id,
            score: Score::ZERO,
        }
    }

    /// Returns true if the score is zero.
    ///
    /// # Examples
    ///
    /// ```
    /// use leit_core::{Hit, Score};
    ///
    /// let hit = Hit::zero(42u32);
    /// assert!(hit.is_zero());
    ///
    /// let hit = Hit::new(42u32, Score::new(0.5));
    /// assert!(!hit.is_zero());
    /// ```
    #[inline]
    pub fn is_zero(&self) -> bool {
        self.score.is_zero()
    }
}

impl<Id: Copy + Ord> Eq for Hit<Id> {}

impl<Id: Copy + Ord> PartialOrd for Hit<Id> {
    /// Hits are compared by score (descending).
    ///
    /// Higher scores are considered "greater" so they sort to the front.
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match self.score.partial_cmp(&other.score) {
            Some(Ordering::Equal) => Some(self.id.cmp(&other.id)),
            other => other,
        }
    }
}

impl<Id: Copy + Ord> Ord for Hit<Id> {
    /// Hits are compared by score (descending).
    ///
    /// Higher scores are considered "greater" so they sort to the front.
    /// For equal scores, IDs are compared for deterministic ordering.
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        match self.score.into_f32().total_cmp(&other.score.into_f32()) {
            Ordering::Equal => self.id.cmp(&other.id),
            other => other,
        }
    }
}

#[cfg(feature = "std")]
impl<Id: fmt::Display + Copy> fmt::Display for Hit<Id> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})", self.id, self.score.into_f32())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn test_hit_creation() {
        let hit = Hit::new(42u32, Score::new(0.85));
        assert_eq!(hit.id, 42);
        assert_eq!(hit.score.into_f32(), 0.85);
    }

    #[test]
    fn test_hit_perfect() {
        let hit = Hit::perfect(42u32);
        assert_eq!(hit.id, 42);
        assert_eq!(hit.score.into_f32(), 1.0);
    }

    #[test]
    fn test_hit_zero() {
        let hit = Hit::zero(42u32);
        assert_eq!(hit.id, 42);
        assert_eq!(hit.score.into_f32(), 0.0);
    }

    #[test]
    fn test_hit_is_zero() {
        let hit = Hit::zero(42u32);
        assert!(hit.is_zero());

        let hit = Hit::new(42u32, Score::new(0.5));
        assert!(!hit.is_zero());
    }

    #[test]
    fn test_hit_ordering() {
        let high = Hit::new(1u32, Score::new(0.9));
        let mid = Hit::new(2u32, Score::new(0.5));
        let low = Hit::new(3u32, Score::new(0.1));

        // Higher scores should be "greater"
        assert!(high > mid);
        assert!(mid > low);
        assert!(high > low);

        // Test ordering with same scores (ID is used as tiebreaker)
        let a = Hit::new(1u32, Score::new(0.5));
        let b = Hit::new(2u32, Score::new(0.5));
        // Both have same score, so ordering is by ID (ascending)
        assert_eq!(a.cmp(&b), Ordering::Less);
        assert_eq!(b.cmp(&a), Ordering::Greater);
    }

    #[test]
    fn test_hit_sort() {
        let mut hits = vec![
            Hit::new(1u32, Score::new(0.1)),
            Hit::new(2u32, Score::new(0.9)),
            Hit::new(3u32, Score::new(0.5)),
        ];

        hits.sort();

        assert_eq!(hits[0].id, 2); // 0.9
        assert_eq!(hits[1].id, 3); // 0.5
        assert_eq!(hits[2].id, 1); // 0.1
    }
}
