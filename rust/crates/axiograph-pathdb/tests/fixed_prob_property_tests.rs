//! Property tests for fixed-point probabilities used in certificates.
//!
//! These are the probabilities that cross the trusted boundary (Lean checker),
//! so we want strong invariants:
//! - numerator is always in `[0, FIXED_POINT_DENOMINATOR]`
//! - multiplication matches the documented integer semantics
//! - IEEE754→fixed-point conversion never produces out-of-range numerators

use axiograph_pathdb::certificate::{FixedPointProbability, FIXED_POINT_DENOMINATOR};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 512,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn fixed_prob_try_new_accepts_in_range(n in 0u32..=FIXED_POINT_DENOMINATOR) {
        let p = FixedPointProbability::try_new(n).expect("in-range numerator");
        prop_assert_eq!(p.numerator(), n);
        let f = p.to_f32();
        prop_assert!(f >= 0.0 && f <= 1.0);
    }

    #[test]
    fn fixed_prob_try_new_rejects_out_of_range(n in (FIXED_POINT_DENOMINATOR + 1)..=u32::MAX) {
        prop_assert!(FixedPointProbability::try_new(n).is_none());
    }

    #[test]
    fn fixed_prob_mul_matches_integer_semantics(
        a in 0u32..=FIXED_POINT_DENOMINATOR,
        b in 0u32..=FIXED_POINT_DENOMINATOR,
    ) {
        let pa = FixedPointProbability::try_new(a).unwrap();
        let pb = FixedPointProbability::try_new(b).unwrap();
        let got = pa.mul(pb).numerator();
        let expected = (((a as u64) * (b as u64)) / (FIXED_POINT_DENOMINATOR as u64)) as u32;
        prop_assert_eq!(got, expected);
    }

    #[test]
    fn fixed_prob_mul_is_monotone_in_left_arg(
        a0 in 0u32..=FIXED_POINT_DENOMINATOR,
        a1 in 0u32..=FIXED_POINT_DENOMINATOR,
        b in 0u32..=FIXED_POINT_DENOMINATOR,
    ) {
        let (a_lo, a_hi) = if a0 <= a1 { (a0, a1) } else { (a1, a0) };
        let p_lo = FixedPointProbability::try_new(a_lo).unwrap();
        let p_hi = FixedPointProbability::try_new(a_hi).unwrap();
        let pb = FixedPointProbability::try_new(b).unwrap();

        prop_assert!(p_lo.mul(pb).numerator() <= p_hi.mul(pb).numerator());
    }

    #[test]
    fn fixed_prob_from_f32_bits_is_always_in_range(bits in any::<u32>()) {
        let p = FixedPointProbability::from_f32_bits(bits);
        prop_assert!(p.numerator() <= FIXED_POINT_DENOMINATOR);
    }

    #[test]
    fn fixed_prob_from_f32_bits_handles_sign_and_nan_inf(bits in any::<u32>()) {
        let sign = bits >> 31;
        let exp = (bits >> 23) & 0xff;
        let frac = bits & 0x7fffff;

        let p = FixedPointProbability::from_f32_bits(bits);

        // Negatives clamp to 0 before other checks.
        if sign != 0 {
            prop_assert_eq!(p.numerator(), 0);
            return Ok(());
        }

        // NaNs clamp to 0; +∞ clamps to 1.0.
        if exp == 255 && frac != 0 {
            prop_assert_eq!(p.numerator(), 0);
        }
        if exp == 255 && frac == 0 {
            prop_assert_eq!(p.numerator(), FIXED_POINT_DENOMINATOR);
        }
    }
}

#[test]
fn fixed_prob_from_f32_bits_known_values() {
    // 0.0
    assert_eq!(FixedPointProbability::from_f32_bits(0x0000_0000).numerator(), 0);
    // 1.0
    assert_eq!(
        FixedPointProbability::from_f32_bits(0x3f80_0000).numerator(),
        FIXED_POINT_DENOMINATOR
    );
    // -0.0 (clamps to 0 because sign bit is set)
    assert_eq!(FixedPointProbability::from_f32_bits(0x8000_0000).numerator(), 0);
    // +∞
    assert_eq!(
        FixedPointProbability::from_f32_bits(0x7f80_0000).numerator(),
        FIXED_POINT_DENOMINATOR
    );
    // NaN
    assert_eq!(
        FixedPointProbability::from_f32_bits(0x7fc0_0001).numerator(),
        0
    );
}

