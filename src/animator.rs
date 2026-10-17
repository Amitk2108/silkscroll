//! Pure scroll math, kept free of Win32 so it can be unit tested.
//!
//! Each real wheel notch becomes a [`Glide`]: a distance that is released
//! bit by bit over `animation_time_ms`, following an easing curve. Glides
//! overlap, so spinning the wheel quickly stacks them up. On every frame the
//! animator sums what all glides moved since the last frame and hands back the
//! whole wheel units to inject, carrying the fraction over to the next frame.

use crate::config::Profile;
use crate::easing::Easing;

/// Wheel units Windows uses for one standard notch.
pub const WHEEL_DELTA: f64 = 120.0;
/// Step size is expressed in "browser pixels": one 120-unit notch scrolls a
/// browser page by 100 px.
pub const UNITS_PER_PX: f64 = WHEEL_DELTA / 100.0;

#[derive(Clone, Copy, Debug)]
struct Glide {
    distance: f64,
    released: f64,
    start_ms: f64,
    duration_ms: f64,
    easing: Easing,
    tail_ratio: f64,
}

/// Animation state for one scroll axis.
#[derive(Default, Debug)]
pub struct AxisAnimator {
    glides: Vec<Glide>,
    carry: f64,
    last_notch_ms: Option<f64>,
    direction: f64,
}

/// How much faster to scroll when notches arrive `gap_ms` apart.
pub fn acceleration_factor(gap_ms: f64, profile: &Profile) -> f64 {
    let window = profile.acceleration_delta_ms;
    if window <= 0.0 || gap_ms >= window {
        return 1.0;
    }
    // Halfway between "no change" and "as much faster as the gap is shorter
    // than the window"; reaches 2x when notches come at half the window.
    let gap = gap_ms.max(1.0);
    ((1.0 + window / gap) / 2.0).clamp(1.0, profile.acceleration_max.max(1.0))
}

impl AxisAnimator {
    /// Registers one real wheel event of `units` wheel units (signed) at
    /// time `now_ms`, already scaled by the step size.
    pub fn push(&mut self, now_ms: f64, units: f64, profile: &Profile) {
        if units == 0.0 {
            return;
        }
        let direction = units.signum();
        if direction != self.direction {
            // Turning the wheel the other way stops the current motion at
            // once instead of fighting it.
            self.glides.clear();
            self.carry = 0.0;
            self.last_notch_ms = None;
            self.direction = direction;
        }

        let factor = match self.last_notch_ms {
            Some(prev) => acceleration_factor(now_ms - prev, profile),
            None => 1.0,
        };
        self.last_notch_ms = Some(now_ms);

        self.glides.push(Glide {
            distance: units * factor,
            released: 0.0,
            start_ms: now_ms,
            duration_ms: profile.animation_time_ms.max(1.0),
            easing: profile.easing,
            tail_ratio: profile.tail_to_head_ratio,
        });
    }

    /// Advances every glide to `now_ms` and returns the whole wheel units to
    /// inject for this frame.
    pub fn tick(&mut self, now_ms: f64) -> i32 {
        let mut moved = self.carry;
        self.glides.retain_mut(|g| {
            let t = ((now_ms - g.start_ms) / g.duration_ms).clamp(0.0, 1.0);
            let position = g.distance * g.easing.apply(t, g.tail_ratio);
            moved += position - g.released;
            g.released = position;
            t < 1.0
        });

        // Once everything has landed, round the leftover fraction instead of
        // dropping it, so a notch always travels its full distance.
        let whole = if self.glides.is_empty() { moved.round() } else { moved.trunc() };
        self.carry = if self.glides.is_empty() { 0.0 } else { moved - whole };
        whole as i32
    }

    pub fn is_idle(&self) -> bool {
        self.glides.is_empty()
    }

    pub fn stop(&mut self) {
        *self = AxisAnimator::default();
    }
}

/// Wheel units one real event should travel under `profile`.
pub fn units_for(delta: i32, profile: &Profile) -> f64 {
    let notches = delta as f64 / WHEEL_DELTA;
    notches * profile.step_size_px * UNITS_PER_PX
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> Profile {
        Profile { acceleration_delta_ms: 0.0, ..Profile::default() }
    }

    /// Runs the animator at `fps` until it goes idle, returning every frame.
    fn run(anim: &mut AxisAnimator, start_ms: f64, fps: f64) -> Vec<i32> {
        let mut frames = Vec::new();
        let mut now = start_ms;
        while !anim.is_idle() {
            now += 1000.0 / fps;
            frames.push(anim.tick(now));
            assert!(frames.len() < 100_000, "animation never finished");
        }
        frames
    }

    #[test]
    fn one_notch_travels_exactly_its_distance() {
        for easing in Easing::ALL {
            let p = Profile { easing, ..profile() };
            let mut anim = AxisAnimator::default();
            anim.push(0.0, units_for(120, &p), &p);
            let frames = run(&mut anim, 0.0, 144.0);
            assert_eq!(frames.iter().sum::<i32>(), 120, "{easing:?}");
        }
    }

    #[test]
    fn motion_is_split_across_many_frames() {
        let p = profile();
        let mut anim = AxisAnimator::default();
        anim.push(0.0, units_for(120, &p), &p);
        let frames = run(&mut anim, 0.0, 144.0);
        let nonzero = frames.iter().filter(|&&f| f != 0).count();
        assert!(nonzero > 20, "only {nonzero} frames moved");
        assert!(frames.iter().all(|&f| f.abs() < 120));
    }

    #[test]
    fn animation_lasts_about_animation_time() {
        let p = Profile { animation_time_ms: 300.0, ..profile() };
        let mut anim = AxisAnimator::default();
        anim.push(0.0, units_for(120, &p), &p);
        let frames = run(&mut anim, 0.0, 100.0);
        assert_eq!(frames.len(), 30);
    }

    #[test]
    fn downward_scroll_stays_negative() {
        let p = profile();
        let mut anim = AxisAnimator::default();
        anim.push(0.0, units_for(-240, &p), &p);
        let frames = run(&mut anim, 0.0, 60.0);
        assert!(frames.iter().all(|&f| f <= 0));
        assert_eq!(frames.iter().sum::<i32>(), -240);
    }

    #[test]
    fn step_size_scales_distance() {
        let p = Profile { step_size_px: 50.0, ..profile() };
        let mut anim = AxisAnimator::default();
        anim.push(0.0, units_for(120, &p), &p);
        assert_eq!(run(&mut anim, 0.0, 60.0).iter().sum::<i32>(), 60);
    }

    #[test]
    fn reversing_direction_cancels_the_glide_in_flight() {
        let p = profile();
        let mut anim = AxisAnimator::default();
        anim.push(0.0, 120.0, &p);
        let early: i32 = (1..=5).map(|i| anim.tick(i as f64 * 10.0)).sum();
        anim.push(50.0, -120.0, &p);
        let rest = run(&mut anim, 50.0, 100.0);
        assert!(early > 0);
        assert!(rest.iter().all(|&f| f <= 0), "old upward motion leaked through");
        assert_eq!(rest.iter().sum::<i32>(), -120);
    }

    #[test]
    fn fast_notches_accelerate_up_to_the_limit() {
        let p = Profile { acceleration_delta_ms: 50.0, acceleration_max: 3.0, ..Profile::default() };
        assert_eq!(acceleration_factor(80.0, &p), 1.0);
        assert_eq!(acceleration_factor(50.0, &p), 1.0);
        assert!((acceleration_factor(25.0, &p) - 1.5).abs() < 1e-9);
        assert_eq!(acceleration_factor(1.0, &p), 3.0);

        let mut anim = AxisAnimator::default();
        anim.push(0.0, 120.0, &p);
        anim.push(10.0, 120.0, &p);
        let total: i32 = run(&mut anim, 10.0, 120.0).iter().sum();
        assert_eq!(total, 120 + 360);
    }

    #[test]
    fn acceleration_can_be_turned_off() {
        let p = Profile { acceleration_max: 1.0, ..Profile::default() };
        assert_eq!(acceleration_factor(1.0, &p), 1.0);
    }
}
