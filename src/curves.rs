//! Tone curve model shared by the CPU editor and the GPU evaluator.
//!
//! A layer's curve adjustment is a monotone 0..=1 → 0..=1 map, stored as
//! 33 control values on a uniform grid (every 1/32 of the input range).
//! The GPU samples a 256-entry LUT with linear
//! interpolation and applies it in linear light. [`Curve::eval`] is the
//! piecewise-linear reference; LUT sampling approximates it near control knots.

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
        let low = (0..index).rev().find_map(|i| self.get(i)).unwrap_or(0.0);
        let high = (index + 1..CURVE_POINTS)
            .find_map(|i| self.get(i))
            .unwrap_or(1.0);
        let value = value.clamp(low, high);
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

    /// Piecewise-linear interpolation between defined points, with implicit
    /// (0, 0) and (1, 1) endpoints. Ordered points cannot overshoot.
    pub fn eval(&self, x: f32) -> f32 {
        let x = x.clamp(0.0, 1.0);
        let t = x * (CURVE_POINTS - 1) as f32;
        let mut previous = (0.0, self.get(0).unwrap_or(0.0));
        for index in 1..CURVE_POINTS {
            if let Some(value) = self
                .get(index)
                .or_else(|| (index == CURVE_POINTS - 1).then_some(1.0))
            {
                if t <= index as f32 {
                    let fraction = (t - previous.0) / (index as f32 - previous.0);
                    return previous.1 + (value - previous.1) * fraction;
                }
                previous = (index as f32, value);
            }
        }
        previous.1
    }

    pub(crate) fn validate(&self) -> Result<()> {
        let mut previous = 0.0;
        for value in self.points {
            if value.is_nan() {
                continue;
            }
            ensure!(
                value.is_finite() && (previous..=1.0).contains(&value),
                "Curve points must be finite, bounded and monotone"
            );
            previous = value;
        }
        Ok(())
    }
}

/// Master levels: input black/white points and gamma applied before curves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Levels {
    /// Input value mapped to output 0. 0.0..=0.99, strictly below white.
    pub black: f32,
    /// Midtone exponent; 1.0 is neutral, > 1 lightens, < 1 darkens.
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
    fn sparse_points_interpolate_without_steps_and_clamp_to_neighbors() {
        let mut curve = Curve::default();
        curve.set(16, 0.25).unwrap();
        assert_eq!(curve.eval(0.0), 0.0);
        assert_eq!(curve.eval(0.25), 0.125);
        assert_eq!(curve.eval(0.5), 0.25);
        assert_eq!(curve.eval(0.75), 0.625);
        assert_eq!(curve.eval(1.0), 1.0);
        curve.set(8, 0.9).unwrap();
        assert_eq!(curve.get(8), Some(0.25));
        let mut previous = 0.0;
        for x in 0..=1024 {
            let value = curve.eval(x as f32 / 1024.0);
            assert!(value >= previous);
            previous = value;
        }
    }

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
