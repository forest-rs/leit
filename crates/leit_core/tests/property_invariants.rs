// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Property-based invariant tests for `leit_core`.

use std::ops::{Add, Mul, Sub};
use std::panic;

use leit_core::{Score, ScoredHit};
use proptest::prelude::*;

proptest! {
    #[test]
    fn finite_scores_round_trip(value in -1_000_000.0_f32..1_000_000.0_f32) {
        let score = Score::new(value);
        prop_assert_eq!(score.as_f32().to_bits(), value.to_bits());
    }

    #[test]
    fn non_finite_scores_are_rejected(
        value in prop_oneof![Just(f32::NAN), Just(f32::INFINITY), Just(f32::NEG_INFINITY)]
    ) {
        prop_assert!(panic::catch_unwind(|| Score::new(value)).is_err());
    }

    #[test]
    fn bounded_score_arithmetic_stays_finite(
        a in -1_000_000.0_f32..1_000_000.0_f32,
        b in -1_000_000.0_f32..1_000_000.0_f32,
        weight in -100.0_f32..100.0_f32,
    ) {
        let lhs = Score::new(a);
        let rhs = Score::new(b);

        prop_assert!(Add::add(lhs, rhs).as_f32().is_finite());
        prop_assert!(Sub::sub(lhs, rhs).as_f32().is_finite());
        prop_assert!(Mul::mul(lhs, weight).as_f32().is_finite());
    }

    #[test]
    fn scored_hit_order_matches_score_then_id(
        left in -10_000.0_f32..10_000.0_f32,
        right in -10_000.0_f32..10_000.0_f32,
        left_id in any::<u32>(),
        right_id in any::<u32>(),
    ) {
        prop_assume!((left - right).abs() > f32::EPSILON || left_id != right_id);

        let left_hit = ScoredHit::new(left_id, Score::new(left));
        let right_hit = ScoredHit::new(right_id, Score::new(right));

        let expected = match left.total_cmp(&right) {
            core::cmp::Ordering::Equal => left_id.cmp(&right_id),
            other => other,
        };

        prop_assert_eq!(left_hit.cmp(&right_hit), expected);
    }
}

#[test]
fn score_addition_saturates_at_max() {
    let saturated = Add::add(Score::MAX, Score::ONE);

    assert_eq!(saturated, Score::MAX);
    assert!(saturated.as_f32().is_finite());
}

#[test]
fn score_subtraction_saturates_at_bounds() {
    assert_eq!(Sub::sub(Score::MIN, Score::ONE), Score::MIN);
    assert_eq!(Sub::sub(Score::MAX, Score::MIN), Score::MAX);
}

#[test]
fn score_multiplication_clamps_non_finite_inputs() {
    assert_eq!(Mul::mul(Score::MAX, 2.0), Score::MAX);
    assert_eq!(Mul::mul(Score::MAX, -2.0), Score::MIN);
    assert_eq!(Mul::mul(Score::ONE, f32::NAN), Score::ZERO);
}

#[test]
fn score_new_rejects_all_non_finite_values() {
    assert!(panic::catch_unwind(|| Score::new(f32::NAN)).is_err());
    assert!(panic::catch_unwind(|| Score::new(f32::INFINITY)).is_err());
    assert!(panic::catch_unwind(|| Score::new(f32::NEG_INFINITY)).is_err());
}
