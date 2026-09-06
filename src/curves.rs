//! Tone curve model shared by the CPU editor and the GPU evaluator.
//!
//! A layer's curve adjustment is a monotone 0..=1 → 0..=1 map, stored as
//! 33 control values on a uniform grid (every 1/32 of the input range).
//! The GPU turns the control points into a 256-entry LUT with Catmull-Rom
//! interpolation and applies it in linear light; [`Curve::eval`] mirrors
//! that interpolation exactly for UI drawing, tests and project fixtures.

use anyhow::{Result, ensure};

/// Control values per curve, one per 1/32 step of the input range.
pub const CURVE_POINTS: usize = 33;
/// LUT resolution the GPU applies.
pub const LUT_SIZE: usize = 256;

/// The four curves, applied in this order after the master levels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    Rgb,
    Red,
    Green,
    Blue,
}
impl Channel {
    pub const ALL: [Self; 4] = [Self::Rgb, Self::Red, Self::Green, Self::Blue];
    pub fn name(self) -> &'static str {
        match self {
            Self::Rgb => "RGB",
            Self::Red => "Red",
            Self::Green => "Green",
            Self::Blue => "Blue",
        }
    }
}

/// Monotone 0..=1 → 0..=1 curve on a uniform control grid.
#[derive(Clone, Debug)]
pub struct Curve {
    points: [f32; CURVE_POINTS],
}

impl PartialEq for Curve {
    /// Bitwise comparison: untouched points are NaN markers and NaN == NaN
    /// must hold for "these two curves are the same adjustment" to work
    /// in document snapshots, undo and tests.
    fn eq(&self, other: &Self) -> bool {
        self.points
            .iter()
            .map(|value| value.to_bits())
            .eq(other.points.iter().map(|value| value.to_bits()))
    }
}

impl Default for Curve {
    fn default() -> Self {
        Self {
            points: [(); CURVE_POINTS].map(|_| f32::NAN),
        }
    }
}

impl Curve {
    pub fn points(&self) -> &[f32; CURVE_POINTS] {
        &self.points
    }

    pub(crate) fn points_mut(&mut self) -> &mut [f32; CURVE_POINTS] {
        &mut self.points
    }

    /// The control value for grid index, or None where untouched.
    pub fn get(&self, index: usize) -> Option<f32> {
        self.points.get(index).copied().filter(|v| v.is_finite())
    }

    /// Set a control point. Output is clamped to 0..=1.
    pub fn set(&mut self, index: usize, value: f32) -> Result<()> {
        ensure!(
            index < CURVE_POINTS,
            "Curve control index {} is out of range",
            index
        );
        ensure!(value.is_finite(), "Curve control value must be finite");
        let value = value.clamp(0.0, 1.0);
        // Normalize -0.0 so bitwise equality sees one zero.
        self.points[index] = if value == 0.0 { 0.0 } else { value };
        Ok(())
    }

    /// Remove a control point so the curve follows its neighbors again.
    pub fn reset_point(&mut self, index: usize) -> Result<()> {
        ensure!(
            index < CURVE_POINTS,
            "Curve control index {} is out of range",
            index
        );
        self.points[index] = f32::NAN;
        Ok(())
    }

    /// Drop every control point.
    pub fn reset(&mut self) {
        self.points = [(); CURVE_POINTS].map(|_| f32::NAN);
    }

    pub fn is_neutral(&self) -> bool {
        self.points.iter().all(|v| !v.is_finite())
    }

    /// The exact value the GPU evaluator applies at `x` in 0..=1: Catmull-Rom
    /// interpolation through the defined control points, clamped to 0..=1.
    /// Untouched points are interpolated past, not treated as zero.
    pub fn eval(&self, x: f32) -> f32 {
        if self.is_neutral() {
            return x;
        }
        let x = x.clamp(0.0, 1.0);
        let t = x * (CURVE_POINTS - 1) as f32;
        let i = ((t as usize).min(CURVE_POINTS - 2)) as isize;
        let f = t - i as f32;
        let value = |index: isize| -> f32 {
            let index = index.clamp(0, CURVE_POINTS as isize - 1);
            match self.get(index as usize) {
                Some(v) => v,
                // Interpolate past untouched points from the nearest defined
                // neighbors, falling back to the anchored identity.
                None => {
                    let before = (0..=index)
                        .rev()
                        .find_map(|k| self.get(k as usize))
                        .unwrap_or(0.0);
                    let after = (index + 1..CURVE_POINTS as isize)
                        .find_map(|k| self.get(k as usize))
                        .unwrap_or(1.0);
                    before
                        + (after - before)
                            * ((index as f32 - self.last_defined_before(index)) / 1.0)
                                .clamp(0.0, 1.0)
                }
            }
        };
        let p1 = value(i);
        let p2 = value(i + 1);
        let p0 = value(i - 1);
        let p3 = value(i + 2);
        // Catmull-Rom with the tangent scaled to the unit grid step.
        let v = 0.5
            * ((2.0 * p1)
                + (-p0 + p2) * f
                + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * f * f
                + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * f * f * f);
        v.clamp(0.0, 1.0)
    }

    fn last_defined_before(&self, index: isize) -> f32 {
        (0..=index)
            .rev()
            .find_map(|k| self.get(k as usize).map(|_| k as f32))
            .unwrap_or(-1.0)
    }
}

/// Master levels: input black/white points and gamma applied before curves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Levels {
    /// Input value mapped to output 0. 0.0..=0.99, strictly below white.
    pub black: f32,
    /// Midtone exponent; 1.0 is neutral, > 1 darkens, < 1 lightens.
    pub gamma: f32,
    /// Input value mapped to output 1. 0.01..=1.0, strictly above black.
    pub white: f32,
}

impl Default for Levels {
    fn default() -> Self {
        Self {
            black: 0.0,
            gamma: 1.0,
            white: 1.0,
        }
    }
}

impl Levels {
    pub fn is_neutral(&self) -> bool {
        *self == Self::default()
    }

    pub fn correct(&self, x: f32) -> f32 {
        let black = self.black.clamp(0.0, 0.99);
        let white = self.white.clamp(0.01, 1.0).max(black + 0.01);
        let gamma = if self.gamma.is_finite() && self.gamma > 0.0 {
            self.gamma
        } else {
            1.0
        };
        let t = ((x - black) / (white - black)).clamp(0.0, 1.0);
        t.powf(1.0 / gamma)
    }
}

/// How a corrected linear sample is mapped. Shared by shader and fixtures.
/// Returns (levels_black, levels_white, 1/gamma).
pub(crate) fn levels_uniform(levels: &Levels) -> [f32; 3] {
    let black = levels.black.clamp(0.0, 0.99);
    let white = levels.white.clamp(0.01, 1.0).max(black + 0.01);
    let gamma = if levels.gamma.is_finite() && levels.gamma > 0.0 {
        levels.gamma
    } else {
        1.0
    };
    [black, white, 1.0 / gamma]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_curve_is_identity() {
        let curve = Curve::default();
        for step in 0..=32 {
            let x = step as f32 / 32.0;
            assert!((curve.eval(x) - x).abs() < 1e-6, "x={x}");
        }
        assert!(curve.is_neutral());
    }

    #[test]
    fn defined_points_are_hit_exactly() {
        let mut curve = Curve::default();
        curve.set(8, 0.25).unwrap();
        curve.set(24, 0.75).unwrap();
        assert!((curve.eval(0.25) - 0.25).abs() < 1e-6);
        assert!((curve.eval(0.75) - 0.75).abs() < 1e-6);
    }

    #[test]
    fn steepened_curve_monotone_and_bounded() {
        let mut curve = Curve::default();
        for index in 0..CURVE_POINTS {
            let x = index as f32 / (CURVE_POINTS - 1) as f32;
            curve.set(index, x * x).unwrap();
        }
        let mut last = 0.0;
        for step in 0..=256 {
            let v = curve.eval(step as f32 / 256.0);
            assert!((0.0..=1.0).contains(&v));
            assert!(v >= last - 1e-3, "non-monotone at {}", step as f32 / 256.0);
            last = v;
        }
    }

    #[test]
    fn levels_neutral_and_clamped() {
        let levels = Levels::default();
        for step in 0..=32 {
            let x = step as f32 / 32.0;
            assert!((levels.correct(x) - x).abs() < 1e-6);
        }
        let crushed = Levels {
            black: 0.25,
            gamma: 1.0,
            white: 0.75,
        };
        assert_eq!(crushed.correct(-1.0), 0.0);
        assert_eq!(crushed.correct(2.0), 1.0);
        assert!((crushed.correct(0.5) - 0.5).abs() < 1e-6);
        assert!(crushed.correct(0.25) < crushed.correct(0.5));
    }

    #[test]
    fn malformed_curves_are_rejected() {
        let mut curve = Curve::default();
        assert!(curve.set(0, f32::NAN).is_err());
        assert!(curve.set(33, 0.5).is_err());
        curve.reset_point(4).unwrap();
        assert!(curve.get(4).is_none());
    }
}
