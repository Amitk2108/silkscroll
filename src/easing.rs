//! Easing curves that shape how one wheel notch is spread out over time.
//!
//! Every curve maps normalized time `t` in `[0, 1]` to normalized distance in
//! `[0, 1]`, starts at 0, ends at exactly 1 and never moves backwards.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Easing {
    /// Short exponential push followed by a long exponential glide.
    #[default]
    Pulse,
    /// Cubic ease-out: fast start, gentle stop.
    EaseOut,
    /// Constant speed.
    Linear,
}

impl Easing {
    #[cfg(test)]
    pub const ALL: [Easing; 3] = [Easing::Pulse, Easing::EaseOut, Easing::Linear];

    /// `tail_ratio` only affects [`Easing::Pulse`].
    pub fn apply(self, t: f64, tail_ratio: f64) -> f64 {
        if t <= 0.0 {
            return 0.0;
        }
        if t >= 1.0 {
            return 1.0;
        }
        match self {
            Easing::Pulse => pulse(t, tail_ratio),
            Easing::EaseOut => 1.0 - (1.0 - t).powi(3),
            Easing::Linear => t,
        }
    }
}

/// Viscous-fluid "pulse" response (after Michael Herf's pulse function, also
/// used by the MIT-licensed smoothscroll.js). Time is scaled by `tail_ratio`:
/// the first `1 / tail_ratio` of the animation accelerates, the rest decays.
fn pulse(t: f64, tail_ratio: f64) -> f64 {
    let k = tail_ratio.max(0.1);
    raw_pulse(t * k) / raw_pulse(k)
}

fn raw_pulse(x: f64) -> f64 {
    if x < 1.0 {
        x - (1.0 - (-x).exp())
    } else {
        let knee = (-1.0f64).exp();
        knee + (1.0 - (-(x - 1.0)).exp()) * (1.0 - knee)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_curve_starts_at_zero_and_ends_at_one() {
        for easing in Easing::ALL {
            for ratio in [0.5, 1.0, 4.0, 10.0] {
                assert_eq!(easing.apply(0.0, ratio), 0.0, "{easing:?}");
                assert_eq!(easing.apply(1.0, ratio), 1.0, "{easing:?}");
                let near_end = easing.apply(0.999_999, ratio);
                assert!((near_end - 1.0).abs() < 1e-3, "{easing:?} ratio {ratio}: {near_end}");
            }
        }
    }

    #[test]
    fn every_curve_is_monotonic() {
        for easing in Easing::ALL {
            for ratio in [0.5, 1.0, 4.0, 10.0] {
                let mut prev = 0.0;
                for i in 1..=1000 {
                    let v = easing.apply(i as f64 / 1000.0, ratio);
                    assert!(v >= prev, "{easing:?} ratio {ratio} went backwards at {i}");
                    prev = v;
                }
            }
        }
    }

    #[test]
    fn bigger_tail_ratio_front_loads_the_motion() {
        // With a longer tail, more of the distance is covered early.
        assert!(Easing::Pulse.apply(0.3, 8.0) > Easing::Pulse.apply(0.3, 2.0));
    }
}
