//! Reusable, presentation-only weather configuration. Omission costs nothing.
use serde::{Deserialize, Serialize};

pub const MAX_SNOW_PARTICLES: usize = 2048;
pub const MAX_STORM_SHELTERS: usize = 32;
pub const DEFAULT_SNOW_MATERIAL: &str = "core:snowflake_01";

/// Optional session cycle. Its period includes two equal endpoint dwells and
/// two smooth transitions; enabling starts by easing from the current strength
/// to the minimum before starting the first dwell.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WeatherCycleDef {
    pub enabled: bool,
    pub min_strength: f32,
    pub max_strength: f32,
    pub period_seconds: f32,
    pub transition_seconds: f32,
}

impl Default for WeatherCycleDef {
    fn default() -> Self {
        Self {
            enabled: false,
            min_strength: 0.0,
            max_strength: 1.0,
            period_seconds: 60.0,
            transition_seconds: 4.0,
        }
    }
}

impl WeatherCycleDef {
    /// # Errors
    /// Rejects nonfinite values, inverted strength bounds and overlapping ramps.
    pub fn validate(&self) -> Result<(), String> {
        validate_strength(self.min_strength, self.transition_seconds)?;
        validate_strength(self.max_strength, self.transition_seconds)?;
        if self.min_strength >= self.max_strength {
            return Err("weather_cycle min_strength must be less than max_strength".to_string());
        }
        if !self.period_seconds.is_finite() || !(1.0..=3600.0).contains(&self.period_seconds) {
            return Err("weather_cycle period_seconds must be finite and 1..=3600".to_string());
        }
        if self.transition_seconds > self.period_seconds.mul_add(0.5, 0.0) {
            return Err("weather_cycle needs two transitions within its period".to_string());
        }
        Ok(())
    }
}

/// # Errors
/// Rejects a nonfinite or out-of-range manual strength or transition duration.
pub fn validate_strength(strength: f32, transition_seconds: f32) -> Result<(), String> {
    if !strength.is_finite() || !(0.0..=1.0).contains(&strength) {
        return Err("weather strength must be finite and 0..=1".to_string());
    }
    if !transition_seconds.is_finite() || !(0.0..=300.0).contains(&transition_seconds) {
        return Err("weather transition_seconds must be finite and 0..=300".to_string());
    }
    Ok(())
}

#[must_use]
pub fn strength_compatible(base: &WeatherDef, alternate: &WeatherDef) -> bool {
    let default_snow = base.snowfall();
    let alternate_snow = alternate.snowfall();
    default_snow.count == alternate_snow.count
        && default_snow.material.trim() == alternate_snow.material.trim()
}

/// Retained scalar playback state, advanced only by playing simulation time.
#[derive(Debug, Default)]
pub struct WeatherPlayback {
    strength: f32,
    target: f32,
    start: f32,
    elapsed: f32,
    duration: f32,
    cycle: Option<WeatherCycleDef>,
    cycle_enabled: bool,
    priming_cycle: bool,
    cycle_seconds: f32,
    control_seconds: f32,
}

impl WeatherPlayback {
    #[must_use]
    pub fn new(cycle: Option<WeatherCycleDef>) -> Self {
        let mut result = Self {
            cycle,
            ..Self::default()
        };
        if cycle.is_some_and(|configuration| configuration.enabled) {
            let _enabled = result.set_cycle(true);
        }
        result
    }

    #[must_use]
    pub const fn strength(&self) -> f32 {
        self.strength
    }

    #[must_use]
    pub const fn target(&self) -> f32 {
        self.target
    }

    #[must_use]
    pub const fn cycle_enabled(&self) -> bool {
        self.cycle_enabled
    }

    #[must_use]
    pub const fn control_seconds(&self) -> f32 {
        self.control_seconds
    }

    pub fn set_strength(&mut self, strength: f32, duration: f32) -> bool {
        if validate_strength(strength, duration).is_err() {
            return false;
        }
        self.cycle_enabled = false;
        self.priming_cycle = false;
        self.begin_transition(strength, duration);
        true
    }

    const fn begin_transition(&mut self, strength: f32, duration: f32) {
        self.start = self.strength;
        self.target = strength;
        self.elapsed = 0.0;
        self.duration = duration;
        if duration <= 0.0 {
            self.strength = strength;
        }
    }

    pub fn set_cycle(&mut self, enabled: bool) -> bool {
        let Some(cycle) = self.cycle.filter(|cycle| cycle.validate().is_ok()) else {
            return false;
        };
        self.cycle_enabled = enabled;
        self.cycle_seconds = 0.0;
        self.priming_cycle = enabled;
        if enabled {
            let duration = if self.strength.to_bits() == cycle.min_strength.to_bits() {
                0.0
            } else {
                cycle.transition_seconds
            };
            self.begin_transition(cycle.min_strength, duration);
            self.priming_cycle = duration > 0.0;
        } else {
            self.begin_transition(self.strength, 0.0);
        }
        true
    }

    /// Advances retained interpolation and timer state without allocating.
    pub fn advance(&mut self, delta: f32) {
        if !delta.is_finite() || delta <= 0.0 {
            return;
        }
        let mut remaining_delta = delta.min(1.0);
        self.control_seconds = (self.control_seconds + remaining_delta).min(1.0e6);
        if !self.cycle_enabled || self.priming_cycle {
            let consumed = remaining_delta.min((self.duration - self.elapsed).max(0.0));
            self.elapsed = (self.elapsed + consumed).min(self.duration);
            let fraction = if self.duration > 0.0 {
                self.elapsed / self.duration
            } else {
                1.0
            };
            self.strength = if self.elapsed >= self.duration {
                self.target
            } else {
                blend(self.start, self.target, smooth(fraction))
            };
            if !self.priming_cycle || self.elapsed < self.duration {
                return;
            }
            self.priming_cycle = false;
            remaining_delta -= consumed;
            if remaining_delta <= 0.0 {
                return;
            }
        }
        let Some(cycle) = self.cycle else {
            return;
        };
        self.cycle_seconds =
            (self.cycle_seconds + remaining_delta).rem_euclid(cycle.period_seconds);
        let dwell = cycle.period_seconds.mul_add(0.5, -cycle.transition_seconds);
        let falling_start = dwell.mul_add(2.0, cycle.transition_seconds);
        let age = self.cycle_seconds;
        let (target, fraction) = if age < dwell {
            (cycle.min_strength, 0.0)
        } else if age < dwell + cycle.transition_seconds {
            (
                cycle.max_strength,
                smooth((age - dwell) / cycle.transition_seconds),
            )
        } else if age < falling_start {
            (cycle.max_strength, 1.0)
        } else {
            (
                cycle.min_strength,
                1.0 - smooth((age - falling_start) / cycle.transition_seconds),
            )
        };
        self.target = target;
        self.strength = blend(cycle.min_strength, cycle.max_strength, fraction);
    }
}

fn blend(start: f32, end: f32, strength: f32) -> f32 {
    if strength <= 0.0 {
        start
    } else if strength >= 1.0 {
        end
    } else {
        (end - start).mul_add(strength, start)
    }
}

const fn smooth(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WeatherDef {
    Snow(SnowfallDef),
}

#[must_use]
pub fn shelter_count(level: &crate::level::LevelDef) -> usize {
    level
        .rooms
        .iter()
        .filter(|room| !room.ceiling.is_open())
        .count()
        .saturating_add(level.void_walls.iter().filter(|wall| wall.occludes).count())
}

impl WeatherDef {
    #[must_use]
    pub const fn snowfall(&self) -> &SnowfallDef {
        match self {
            Self::Snow(snow) => snow,
        }
    }
}

/// Light snow in a camera-centred volume; all distances are world metres.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SnowfallDef {
    pub count: u32,
    pub radius: f32,
    pub height: f32,
    /// Horizontal velocity along world X/Z, metres per second.
    pub wind: [f32; 2],
    pub size: [f32; 2],
    pub speed: [f32; 2],
    pub opacity: f32,
    pub material: String,
    /// Stable seed fraction, independent of storm extinction.
    pub intensity: f32,
    /// 0 preserves calm rendering; 1 enables severe whiteout.
    pub storm_severity: f32,
    /// Outdoor sightline at which a full storm removes 98.2% of contrast.
    pub visibility_m: f32,
    pub fog_color: [f32; 3],
}

impl Default for SnowfallDef {
    fn default() -> Self {
        Self {
            intensity: 1.0,
            storm_severity: 0.0,
            visibility_m: 5.0,
            fog_color: [0.68, 0.73, 0.79],
            count: 1400,
            radius: 16.0,
            height: 12.0,
            wind: [0.18, 0.06],
            size: [0.025, 0.075],
            speed: [0.45, 1.05],
            opacity: 0.85,
            material: DEFAULT_SNOW_MATERIAL.to_string(),
        }
    }
}

impl SnowfallDef {
    /// Updates only numeric configuration. Material identity and seed count stay
    /// authored, so continuous controls require compatible endpoint resources.
    pub fn blend_from(&mut self, base: &Self, alternate: &Self, strength: f32) {
        self.radius = blend(base.radius, alternate.radius, strength);
        self.height = blend(base.height, alternate.height, strength);
        self.opacity = blend(base.opacity, alternate.opacity, strength);
        self.intensity = blend(base.intensity, alternate.intensity, strength);
        self.storm_severity = blend(base.storm_severity, alternate.storm_severity, strength);
        self.visibility_m = blend(base.visibility_m, alternate.visibility_m, strength);
        for (out, (from, to)) in self
            .wind
            .iter_mut()
            .zip(base.wind.iter().zip(&alternate.wind))
        {
            *out = blend(*from, *to, strength);
        }
        for (out, (from, to)) in self
            .size
            .iter_mut()
            .zip(base.size.iter().zip(&alternate.size))
        {
            *out = blend(*from, *to, strength);
        }
        for (out, (from, to)) in self
            .speed
            .iter_mut()
            .zip(base.speed.iter().zip(&alternate.speed))
        {
            *out = blend(*from, *to, strength);
        }
        for (out, (from, to)) in self
            .fog_color
            .iter_mut()
            .zip(base.fog_color.iter().zip(&alternate.fog_color))
        {
            *out = blend(*from, *to, strength);
        }
    }
    /// A compact whiteout preset using the same seed budget as calm snow.
    #[must_use]
    pub fn blizzard() -> Self {
        Self {
            radius: 6.0,
            height: 6.0,
            wind: [8.0, 3.0],
            size: [0.025, 0.06],
            speed: [0.8, 1.8],
            storm_severity: 1.0,
            ..Self::default()
        }
    }

    /// Rejects unbounded or malformed authoring before building resources.
    ///
    /// # Errors
    /// Returns a named weather field error for an invalid parameter.
    pub fn validate(&self) -> Result<(), String> {
        if self.count == 0 || usize::try_from(self.count).unwrap_or(usize::MAX) > MAX_SNOW_PARTICLES
        {
            return Err(format!("weather count must be 1..={MAX_SNOW_PARTICLES}"));
        }
        for (name, value, min, max) in [
            ("radius", self.radius, 4.0, 32.0),
            ("height", self.height, 4.0, 24.0),
            ("opacity", self.opacity, 0.0, 1.0),
            ("intensity", self.intensity, 0.0, 1.0),
            ("storm_severity", self.storm_severity, 0.0, 1.0),
            ("visibility_m", self.visibility_m, 2.0, 100.0),
        ] {
            if !value.is_finite() || !(min..=max).contains(&value) {
                return Err(format!("weather {name} must be finite and {min}..={max}"));
            }
        }
        for (name, [min, max], low, high) in [
            ("size", self.size, 0.005, 0.15),
            ("speed", self.speed, 0.1, 2.0),
        ] {
            if !min.is_finite() || !max.is_finite() || min < low || max > high || min > max {
                return Err(format!(
                    "weather {name} must be an ordered range in {low}..={high}"
                ));
            }
        }
        let wind_limit = if self.storm_severity > 0.0 { 20.0 } else { 0.5 };
        if self
            .wind
            .iter()
            .any(|v| !v.is_finite() || v.abs() > wind_limit)
        {
            return Err(format!(
                "weather wind components must be finite and within ±{wind_limit}"
            ));
        }
        if self
            .fog_color
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err("weather fog_color must contain three finite 0..=1 components".to_string());
        }
        if !crate::assets::is_valid_asset_id(self.material.trim()) {
            return Err("weather material must be a logical asset id".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_near(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1.0e-6,
            "{actual} differs from {expected}"
        );
    }

    #[test]
    fn weather_strength_eases_retargets_continuously_and_pauses() {
        let mut playback = WeatherPlayback::new(None);
        assert_near(playback.strength(), 0.0);
        assert!(
            !playback.set_cycle(true),
            "omission creates no automatic timer"
        );
        assert!(playback.set_strength(0.35, 2.0));
        playback.advance(1.0);
        assert_near(playback.strength(), 0.175);
        playback.advance(0.0);
        playback.advance(f32::NAN);
        assert_near(playback.strength(), 0.175);
        assert_near(playback.control_seconds(), 1.0);
        assert!(playback.set_strength(1.0, 2.0));
        assert_near(playback.strength(), 0.175);
        playback.advance(1.0);
        assert_near(playback.strength(), 0.5875);
        playback.advance(1.0);
        assert_near(playback.strength(), 1.0);
        assert!(playback.set_strength(0.0, 2.0));
        playback.advance(1.0);
        assert_near(playback.strength(), 0.5);
        playback.advance(1.0);
        assert_eq!(
            playback.strength().to_bits(),
            0.0_f32.to_bits(),
            "calm completes exactly"
        );
    }

    #[test]
    fn weather_cycle_dwells_ramps_and_yields_to_manual_control() {
        let cycle = WeatherCycleDef {
            period_seconds: 8.0,
            transition_seconds: 2.0,
            max_strength: 0.6,
            ..WeatherCycleDef::default()
        };
        assert!(cycle.validate().is_ok());
        let mut playback = WeatherPlayback::new(Some(cycle));
        assert!(!playback.cycle_enabled(), "default cycle is opt-in");
        assert!(playback.set_cycle(true));
        for expected in [0.0, 0.0, 0.3, 0.6, 0.6, 0.6, 0.3, 0.0] {
            playback.advance(1.0);
            assert_near(playback.strength(), expected);
        }
        for _ in 0_u8..3 {
            playback.advance(1.0);
        }
        assert_near(playback.strength(), 0.3);
        assert!(playback.set_strength(0.15, 2.0));
        assert!(
            !playback.cycle_enabled(),
            "manual input owns the visit until resumed"
        );
        playback.advance(1.0);
        assert_near(playback.strength(), 0.225);
        for _ in 0_u8..32 {
            playback.advance(1.0);
        }
        assert_near(playback.strength(), 0.15);
        assert!(playback.set_cycle(true));
        playback.advance(1.0);
        assert_near(playback.strength(), 0.075);
        playback.advance(1.0);
        assert_near(playback.strength(), 0.0);
        assert!(playback.set_cycle(false));
        for _ in 0_u8..32 {
            playback.advance(1.0);
        }
        assert_near(playback.strength(), 0.0);
        assert!(!playback.cycle_enabled());
        assert_near(WeatherPlayback::new(Some(cycle)).strength(), 0.0);
    }

    #[test]
    fn weather_cycle_priming_carries_remaining_time_independent_of_frame_partition() {
        let cycle = WeatherCycleDef {
            period_seconds: 1.0,
            transition_seconds: 0.25,
            ..WeatherCycleDef::default()
        };
        let mut single_frame = WeatherPlayback::new(Some(cycle));
        let mut split_frames = WeatherPlayback::new(Some(cycle));
        for playback in [&mut single_frame, &mut split_frames] {
            assert!(playback.set_strength(1.0, 0.0));
            assert!(playback.set_cycle(true));
        }
        single_frame.advance(0.75);
        split_frames.advance(0.25);
        split_frames.advance(0.5);
        assert_near(single_frame.strength(), 1.0);
        assert_near(single_frame.target(), 1.0);
        assert_eq!(
            single_frame.strength().to_bits(),
            split_frames.strength().to_bits()
        );
        assert_eq!(
            single_frame.target().to_bits(),
            split_frames.target().to_bits()
        );
        assert_eq!(
            single_frame.control_seconds().to_bits(),
            split_frames.control_seconds().to_bits()
        );
        single_frame.advance(0.375);
        split_frames.advance(0.375);
        assert_near(single_frame.strength(), 0.5);
        assert_eq!(
            single_frame.strength().to_bits(),
            split_frames.strength().to_bits()
        );
    }

    #[test]
    fn weather_cycle_and_strength_validation_rejects_unbounded_playback()
    -> Result<(), serde_json::Error> {
        let cycle: WeatherCycleDef = serde_json::from_str("{}")?;
        assert!(!cycle.enabled);
        assert!(cycle.validate().is_ok());
        for invalid in [
            WeatherCycleDef {
                min_strength: 1.0,
                ..cycle
            },
            WeatherCycleDef {
                max_strength: -1.0,
                ..cycle
            },
            WeatherCycleDef {
                period_seconds: f32::NAN,
                ..cycle
            },
            WeatherCycleDef {
                transition_seconds: 31.0,
                ..cycle
            },
        ] {
            assert!(invalid.validate().is_err());
        }
        for (strength, duration) in [
            (f32::NAN, 0.0),
            (-0.1, 0.0),
            (1.1, 0.0),
            (0.5, f32::INFINITY),
            (0.5, -1.0),
            (0.5, 301.0),
        ] {
            assert!(validate_strength(strength, duration).is_err());
        }
        assert!(serde_json::from_str::<WeatherCycleDef>(r#"{"period":60}"#).is_err());
        Ok(())
    }

    #[test]
    fn defaults_are_opt_in_and_round_trip_in_semantics() -> Result<(), Box<dyn std::error::Error>> {
        let mut level: crate::level::LevelDef =
            serde_json::from_str(include_str!("../assets/levels/winter.json"))?;
        let snow = level
            .weather
            .as_ref()
            .ok_or("winter needs snow")?
            .snowfall();
        assert_eq!(snow.count, 1400, "light default budget");
        snow.validate()?;
        assert!(
            crate::materials::referenced_material_ids(&level)
                .iter()
                .any(|id| id == DEFAULT_SNOW_MATERIAL),
            "compiler must package the snow PNG dependency"
        );
        let encoded = serde_json::to_string(&level)?;
        let decoded: crate::level::LevelDef = serde_json::from_str(&encoded)?;
        assert_eq!(
            decoded
                .weather
                .as_ref()
                .ok_or("lost weather")?
                .snowfall()
                .count,
            snow.count,
            "package semantics retain weather"
        );
        level.weather = None;
        level.weather_alternate = None;
        level.weather_cycle = None;
        assert!(
            !serde_json::to_value(&level)?
                .as_object()
                .ok_or("not object")?
                .contains_key("weather"),
            "existing packages retain absent weather"
        );
        let demo: crate::level::LevelDef =
            serde_json::from_str(include_str!("../assets/levels/places_demo.json"))?;
        assert!(
            demo.weather.is_none(),
            "non-Winter defaults remain unchanged"
        );
        Ok(())
    }

    #[test]
    fn alternate_round_trip_preserves_default_and_packages_both_materials()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut level: crate::level::LevelDef =
            serde_json::from_str(include_str!("../assets/levels/winter.json"))?;
        let authored_default = serde_json::to_value(&level.weather)?;
        let alternate = WeatherDef::Snow(SnowfallDef {
            material: crate::level::DEFAULT_STEAM_MATERIAL.to_string(),
            ..SnowfallDef::blizzard()
        });
        level.weather_alternate = Some(alternate);
        level.weather_cycle = None;
        // Legacy binary selection permits independently authored materials;
        // continuous Winter controls require the compatible pair tested above.
        for prop in &mut level.props {
            for binding in &mut prop.bindings {
                binding.actions.retain(|action| {
                    !matches!(
                        action,
                        crate::level::ActionDef::SetWeatherCycle { enabled: _ }
                    )
                });
                for action in &mut binding.actions {
                    if matches!(
                        action,
                        crate::level::ActionDef::SetWeatherStrength {
                            strength: _,
                            transition_seconds: _,
                        }
                    ) {
                        *action = crate::level::ActionDef::ToggleWeather;
                    }
                }
            }
        }
        crate::loader::validate_level(&level)?;
        let materials = crate::materials::referenced_material_ids(&level);
        assert!(
            materials.iter().any(|id| id == DEFAULT_SNOW_MATERIAL),
            "default material packaged"
        );
        assert!(
            materials
                .iter()
                .any(|id| id == crate::level::DEFAULT_STEAM_MATERIAL),
            "alternate material packaged independently"
        );
        let encoded = serde_json::to_string(&level)?;
        let decoded: crate::level::LevelDef = serde_json::from_str(&encoded)?;
        assert_eq!(
            serde_json::to_value(decoded.weather)?,
            authored_default,
            "alternate never rewrites the authored calm configuration"
        );
        assert_eq!(
            serde_json::to_value(decoded.weather_alternate)?,
            serde_json::to_value(level.weather_alternate)?,
            "compiled semantics retain the entire alternate configuration"
        );
        Ok(())
    }

    #[test]
    fn invalid_configuration_is_rejected_by_loader() -> Result<(), Box<dyn std::error::Error>> {
        for patch in [
            serde_json::from_str::<serde_json::Value>(r#"{"count":0}"#)?,
            serde_json::from_str::<serde_json::Value>(r#"{"count":2049}"#)?,
            serde_json::from_str::<serde_json::Value>(r#"{"radius":100}"#)?,
            serde_json::from_str::<serde_json::Value>(r#"{"height":0}"#)?,
            serde_json::from_str::<serde_json::Value>(r#"{"wind":[0.6,0]}"#)?,
            serde_json::from_str::<serde_json::Value>(r#"{"size":[0.1,0.01]}"#)?,
            serde_json::from_str::<serde_json::Value>(r#"{"speed":[0.1,2.1]}"#)?,
            serde_json::from_str::<serde_json::Value>(r#"{"opacity":1.1}"#)?,
            serde_json::json!({"material":"../broken"}),
            serde_json::json!({"intensity":1.1_f64}),
            serde_json::json!({"storm_severity":-1.0_f64}),
            serde_json::json!({"visibility_m":1.9_f64}),
            serde_json::json!({"fog_color":[0.5_f64,0.5_f64,2.0_f64]}),
            serde_json::json!({"storm_severity":1.0_f64,"wind":[20.1_f64,0.0_f64]}),
        ] {
            let mut value = serde_json::json!({"kind":"snow"});
            value
                .as_object_mut()
                .ok_or("not object")?
                .extend(patch.as_object().ok_or("not patch")?.clone());
            let mut level: crate::level::LevelDef =
                serde_json::from_str(include_str!("../assets/levels/winter.json"))?;
            level.weather = Some(serde_json::from_value(value)?);
            assert!(
                crate::loader::validate_level(&level).is_err(),
                "invalid weather must fail before resource allocation: {patch}"
            );
        }
        assert!(
            serde_json::from_value::<WeatherDef>(serde_json::json!({"kind":"rain"})).is_err(),
            "unknown weather kind cannot silently disappear"
        );
        assert!(
            serde_json::from_value::<WeatherDef>(serde_json::from_str::<serde_json::Value>(
                r#"{"kind":"snow","cout":12}"#
            )?)
            .is_err(),
            "weather typo cannot silently take a default"
        );
        let mut config = SnowfallDef {
            radius: f32::NAN,
            ..SnowfallDef::default()
        };
        assert!(config.validate().is_err(), "non-finite dimension rejected");
        config.radius = 16.0;
        config.speed = [0.5, f32::INFINITY];
        assert!(config.validate().is_err(), "non-finite speed rejected");
        Ok(())
    }
}
