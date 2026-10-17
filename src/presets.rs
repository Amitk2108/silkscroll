//! Ready-made scroll styles. Picking one fills in the five numbers of a
//! profile; every value stays editable afterwards.

use crate::config::Profile;
use crate::easing::Easing;

pub struct Preset {
    pub name: &'static str,
    pub description: &'static str,
    step_size_px: f64,
    animation_time_ms: f64,
    acceleration_delta_ms: f64,
    acceleration_max: f64,
    tail_to_head_ratio: f64,
}

pub const PRESETS: [Preset; 5] = [
    Preset {
        name: "Default",
        description: "Balanced smoothness for everyday use",
        step_size_px: 120.0,
        animation_time_ms: 360.0,
        acceleration_delta_ms: 70.0,
        acceleration_max: 3.0,
        tail_to_head_ratio: 3.0,
    },
    Preset {
        name: "Extra smooth",
        description: "A longer, silkier glide after every notch",
        step_size_px: 120.0,
        animation_time_ms: 600.0,
        acceleration_delta_ms: 70.0,
        acceleration_max: 3.0,
        tail_to_head_ratio: 5.0,
    },
    Preset {
        name: "Responsive",
        description: "Short and snappy; stops almost as soon as the wheel does",
        step_size_px: 120.0,
        animation_time_ms: 200.0,
        acceleration_delta_ms: 50.0,
        acceleration_max: 3.0,
        tail_to_head_ratio: 2.0,
    },
    Preset {
        name: "Precise",
        description: "Small steps for code, spreadsheets and design tools",
        step_size_px: 60.0,
        animation_time_ms: 250.0,
        acceleration_delta_ms: 50.0,
        acceleration_max: 2.0,
        tail_to_head_ratio: 3.0,
    },
    Preset {
        name: "Fast scroll",
        description: "Big steps and strong acceleration for long pages",
        step_size_px: 240.0,
        animation_time_ms: 400.0,
        acceleration_delta_ms: 80.0,
        acceleration_max: 6.0,
        tail_to_head_ratio: 3.0,
    },
];

impl Preset {
    /// Sets the feel of `profile` to this preset. Switches like *Enable*,
    /// *Horizontal smoothing* and *Reverse direction* are left as they are.
    pub fn apply(&self, profile: &mut Profile) {
        profile.step_size_px = self.step_size_px;
        profile.animation_time_ms = self.animation_time_ms;
        profile.acceleration_delta_ms = self.acceleration_delta_ms;
        profile.acceleration_max = self.acceleration_max;
        profile.tail_to_head_ratio = self.tail_to_head_ratio;
        profile.easing = Easing::Pulse;
    }

    fn matches(&self, profile: &Profile) -> bool {
        let mut applied = *profile;
        self.apply(&mut applied);
        applied == *profile
    }
}

/// The preset `profile` currently matches, or `None` for custom values.
pub fn matching(profile: &Profile) -> Option<usize> {
    PRESETS.iter().position(|preset| preset.matches(profile))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        LIMITS_ACCELERATION_DELTA, LIMITS_ACCELERATION_MAX, LIMITS_ANIMATION_TIME, LIMITS_STEP_SIZE, LIMITS_TAIL_RATIO,
    };

    #[test]
    fn default_preset_is_the_default_profile() {
        assert_eq!(PRESETS[0].name, "Default");
        assert_eq!(matching(&Profile::default()), Some(0));
    }

    #[test]
    fn every_preset_is_recognized_after_applying_it() {
        for (i, preset) in PRESETS.iter().enumerate() {
            let mut profile = Profile::default();
            preset.apply(&mut profile);
            assert_eq!(matching(&profile), Some(i), "{}", preset.name);
            assert_eq!(profile.sanitized(), profile, "{} is outside the allowed ranges", preset.name);
        }
    }

    #[test]
    fn presets_are_all_different() {
        for (i, a) in PRESETS.iter().enumerate() {
            for b in &PRESETS[i + 1..] {
                let mut profile = Profile::default();
                a.apply(&mut profile);
                assert!(!b.matches(&profile), "{} and {} are identical", a.name, b.name);
            }
        }
    }

    #[test]
    fn applying_a_preset_keeps_the_switches() {
        let mut profile = Profile {
            enabled: false,
            horizontal_smoothing: false,
            shift_for_horizontal: true,
            reverse_direction: true,
            ..Profile::default()
        };
        PRESETS[1].apply(&mut profile);
        assert!(!profile.enabled && !profile.horizontal_smoothing);
        assert!(profile.shift_for_horizontal && profile.reverse_direction);
        assert_eq!(profile.animation_time_ms, 600.0);
    }

    #[test]
    fn edited_values_count_as_custom() {
        let edited = Profile { step_size_px: 130.0, ..Profile::default() };
        assert_eq!(matching(&edited), None);
        let linear = Profile { easing: Easing::Linear, ..Profile::default() };
        assert_eq!(matching(&linear), None);
    }

    #[test]
    fn limits_cover_all_presets() {
        let within = |v: f64, (lo, hi): (f64, f64)| (lo..=hi).contains(&v);
        for p in &PRESETS {
            assert!(within(p.step_size_px, LIMITS_STEP_SIZE));
            assert!(within(p.animation_time_ms, LIMITS_ANIMATION_TIME));
            assert!(within(p.acceleration_delta_ms, LIMITS_ACCELERATION_DELTA));
            assert!(within(p.acceleration_max, LIMITS_ACCELERATION_MAX));
            assert!(within(p.tail_to_head_ratio, LIMITS_TAIL_RATIO));
        }
    }
}
