//! Score type for relevance scores and weights.
//!
//! The Score type represents floating-point relevance scores with automatic
//! clamping to the [0.0, 1.0] range for relevance scores.

use core::fmt;
use core::ops::{Add, AddAssign, Mul, MulAssign, Sub, SubAssign};

/// Relevance score or weight.
///
/// Represents a floating-point score in the range [0.0, 1.0] for relevance,
/// or any f32 value for weights/boosts.
///
/// # Relevance Scores
///
/// When created via `Score::new()`, values are automatically clamped to [0.0, 1.0].
/// This ensures relevance scores are always within the valid range.
///
/// # Weights and Boosts
///
/// For weights and boosts that may exceed [0.0, 1.0], use `Score::new_unchecked()`
/// to bypass clamping.
#[derive(Clone, Copy, PartialEq, PartialOrd, Debug)]
#[repr(transparent)]
pub struct Score(pub f32);

impl Score {
    /// Zero score.
    pub const ZERO: Score = Score(0.0);

    /// Perfect score.
    pub const ONE: Score = Score(1.0);

    /// Creates a new score, clamped to [0.0, 1.0].
    ///
    /// # Examples
    ///
    /// ```
    /// use leit_core::Score;
    ///
    /// let score = Score::new(0.85);
    /// assert_eq!(score.into_f32(), 0.85);
    ///
    /// let clamped = Score::new(1.5);
    /// assert_eq!(clamped.into_f32(), 1.0);
    ///
    /// let negative = Score::new(-0.5);
    /// assert_eq!(negative.into_f32(), 0.0);
    /// ```
    #[inline]
    pub fn new(score: f32) -> Self {
        Self(score.clamp(0.0, 1.0))
    }

    /// Creates a score without clamping.
    ///
    /// This bypasses the [0.0, 1.0] clamping and is useful for weights and
    /// boosts that may exceed the normal relevance range.
    ///
    /// # Examples
    ///
    /// ```
    /// use leit_core::Score;
    ///
    /// let weight = Score::new_unchecked(2.5);
    /// assert_eq!(weight.into_f32(), 2.5);
    /// ```
    #[inline]
    pub const fn new_unchecked(score: f32) -> Self {
        Self(score)
    }

    /// Returns the underlying f32 value.
    #[inline]
    pub const fn into_f32(self) -> f32 {
        self.0
    }

    /// Checks if this is a zero score.
    #[inline]
    pub const fn is_zero(self) -> bool {
        self.0 == 0.0
    }

    /// Checks if this is a perfect score.
    #[inline]
    pub const fn is_one(self) -> bool {
        self.0 == 1.0
    }
}

impl From<f32> for Score {
    #[inline]
    fn from(value: f32) -> Self {
        Self::new(value)
    }
}

impl From<Score> for f32 {
    #[inline]
    fn from(score: Score) -> Self {
        score.0
    }
}

impl Add for Score {
    type Output = Self;

    #[inline]
    fn add(self, other: Self) -> Self {
        Self::new_unchecked(self.0 + other.0)
    }
}

impl AddAssign for Score {
    #[inline]
    fn add_assign(&mut self, other: Self) {
        self.0 += other.0;
    }
}

impl Sub for Score {
    type Output = Self;

    #[inline]
    fn sub(self, other: Self) -> Self {
        Self::new_unchecked(self.0 - other.0)
    }
}

impl SubAssign for Score {
    #[inline]
    fn sub_assign(&mut self, other: Self) {
        self.0 -= other.0;
    }
}

impl Mul<f32> for Score {
    type Output = Self;

    #[inline]
    fn mul(self, rhs: f32) -> Self {
        Self::new_unchecked(self.0 * rhs)
    }
}

impl MulAssign<f32> for Score {
    #[inline]
    fn mul_assign(&mut self, rhs: f32) {
        self.0 *= rhs;
    }
}

#[cfg(feature = "std")]
impl fmt::Display for Score {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_score_creation() {
        let score = Score::new(0.85);
        assert_eq!(score.into_f32(), 0.85);
    }

    #[test]
    fn test_score_clamp_upper() {
        let clamped = Score::new(1.5);
        assert_eq!(clamped.into_f32(), 1.0);
    }

    #[test]
    fn test_score_clamp_lower() {
        let clamped = Score::new(-0.5);
        assert_eq!(clamped.into_f32(), 0.0);
    }

    #[test]
    fn test_score_unchecked() {
        let weight = Score::new_unchecked(2.5);
        assert_eq!(weight.into_f32(), 2.5);
    }

    #[test]
    fn test_score_constants() {
        assert_eq!(Score::ZERO.into_f32(), 0.0);
        assert_eq!(Score::ONE.into_f32(), 1.0);
    }

    #[test]
    fn test_score_is_zero() {
        assert!(Score::ZERO.is_zero());
        assert!(Score::new(0.0).is_zero());
        assert!(!Score::ONE.is_zero());
    }

    #[test]
    fn test_score_is_one() {
        assert!(Score::ONE.is_one());
        assert!(Score::new(1.0).is_one());
        assert!(!Score::ZERO.is_one());
    }

    #[test]
    fn test_score_arithmetic() {
        let a = Score::new(0.3);
        let b = Score::new(0.4);
        let sum = a + b;
        assert!((sum.into_f32() - 0.7).abs() < 1e-6);

        let diff = b - a;
        assert!((diff.into_f32() - 0.1).abs() < 1e-6);

        let mut s = Score::new(0.5);
        s += Score::new(0.2);
        assert!((s.into_f32() - 0.7).abs() < 1e-6);

        let mut s = Score::new(0.8);
        s -= Score::new(0.3);
        assert!((s.into_f32() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_score_multiply() {
        let score = Score::new(0.5);
        let result = score * 2.0;
        assert_eq!(result.into_f32(), 1.0);

        let mut s = Score::new(0.75);
        s *= 0.5;
        assert_eq!(s.into_f32(), 0.375);
    }
}
