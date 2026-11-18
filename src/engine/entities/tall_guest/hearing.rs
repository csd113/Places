//! Device-independent evidence; quiet-room calibration and masking use linear
//! amplitude, never output volume, voice admission or a hidden target handle.
use glam::Vec3;
use serde::{Deserialize, Serialize};

pub struct Request {
    pub listener: Vec3,
    pub origin: Vec3,
    pub source: Vec3,
    pub radius: f32,
    pub loudness: f32,
    pub seed: u64,
    pub target_key: Option<u64>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Heard {
    pub estimated: Vec3,
    pub goal: Vec3,
    pub quality: f32,
    pub uncertainty: f32,
    pub snr_db: f32,
    pub portals: Vec<u32>,
    pub transmission: f32,
}

/// A nominal signal is calibrated to a quiet simulation noise floor. Relative
/// ambient levels come from enabled, playing world emitters and cue gain.
/// These are gameplay tuning, not measured physical sound-pressure levels.
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Finite acoustic amplitude ratios are clamped before logarithms; vector error is bounded."
)]
pub fn resolve(request: &Request, distance: f32, noise: f32, transmission: f32) -> Option<Heard> {
    let signal = 0.1 * request.loudness.max(0.0) * transmission / distance.max(1.0);
    let snr_db = 20.0 * (signal / noise.max(0.001)).max(1.0e-6).log10();
    // A 0.0001dB guard covers f32 conversion at the inclusive 3dB boundary.
    if snr_db <= 3.0001 {
        return None;
    }
    let quality = ((snr_db - 3.0) / 12.0).clamp(0.0, 1.0);
    let uncertainty = (1.0 - quality).mul_add(2.5, 0.15);
    let phase = f32::from(u16::try_from(request.seed.wrapping_mul(40_503) & 65_535).unwrap_or(0))
        * (std::f32::consts::TAU / 65_536.0);
    let error = Vec3::new(phase.sin(), 0.0, phase.cos()) * (uncertainty - 0.15);
    Some(Heard {
        estimated: request.source + error,
        goal: request.source + error,
        quality,
        uncertainty,
        snr_db,
        portals: Vec::new(),
        transmission,
    })
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Landing {
    impact: crate::engine::physics::ImpactEvent,
    sampled: f32,
    evidence: Heard,
}

impl super::Guest {
    /// Capture physical audibility now even while fresh prey has priority.
    /// The bounded memory can survive the four-second safe frustration action;
    /// its original sample is never presented as a new or fresh sound.
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Finite source coordinates are raised above the contact floor for acoustic rays."
    )]
    pub(super) fn capture_landings(&mut self, scene: &dyn super::prop_track::Scene) {
        self.state
            .impacts
            .retain(|(_, at)| self.state.clock - at <= 3.0);
        self.state
            .heard_impacts
            .retain(|landing| self.state.clock - landing.sampled <= 6.0);
        let (head, _) = self.head();
        for _ in 0_u8..8 {
            let Some((impact, at)) = self.state.impacts.pop_front() else {
                break;
            };
            if self.state.incidents.iter().any(|(key, generation)| {
                *key == impact.causal_source && *generation >= impact.incident
            }) || self
                .state
                .source_cooldowns
                .iter()
                .any(|(key, until)| *key == impact.causal_source && *until > self.state.clock)
            {
                continue;
            }
            let radius = if impact.release == crate::engine::physics::ImpactRelease::Dropped {
                1.0
            } else {
                impact.strength * 8.0
            };
            let event = crate::engine::ai::perception::Stimulus {
                position: impact.position,
                radius,
                loudness: 1.0,
                category: "impact".into(),
                source: None,
                age: self.state.clock - at,
                player_movement: None,
                movement_serial: None,
            };
            if !event.heard_at(self.state.position, 22.0) {
                continue;
            }
            let request = Request {
                listener: self.state.position,
                origin: head,
                source: impact.position + Vec3::Y * 0.05,
                radius: event.audible_radius(0.0),
                loudness: if impact.release == crate::engine::physics::ImpactRelease::Dropped {
                    0.35 * impact.strength
                } else {
                    impact.strength
                },
                seed: impact.incident,
                target_key: Some(impact.key),
            };
            let Some(evidence) = scene.hear(&request) else {
                continue;
            };
            if let Some(old) = self.state.heard_impacts.iter_mut().find(|landing| {
                landing.impact.causal_source == impact.causal_source
                    && landing.impact.incident == impact.incident
            }) {
                if old.impact.strength < impact.strength {
                    *old = Landing {
                        impact,
                        sampled: at,
                        evidence,
                    };
                }
            } else {
                if self.state.heard_impacts.len() == 32 {
                    let _old = self.state.heard_impacts.pop_front();
                }
                self.log("landing_retained", &serde_json::json!({"source":impact.causal_source,"incident":impact.incident,"sampled":at,"prey_priority":self.fresh(),"evidence":evidence}));
                self.state.heard_impacts.push_back(Landing {
                    impact,
                    sampled: at,
                    evidence,
                });
            }
        }
    }

    pub(super) fn admit_landing(&mut self) {
        if self.fresh()
            || self.state.transit.is_some()
            || matches!(
                self.state.intent,
                super::Intent::Chase | super::Intent::EscapeFrustration | super::Intent::Disabled
            )
            || (self.state.intent == super::Intent::Inspect
                && self.state.clock - self.state.entered < 1.2)
        {
            return;
        }
        let Some(index) = self
            .state
            .heard_impacts
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| {
                (a.impact.strength * a.evidence.quality)
                    .total_cmp(&(b.impact.strength * b.evidence.quality))
            })
            .map(|(index, _)| index)
        else {
            return;
        };
        let Some(mut landing) = self.state.heard_impacts.remove(index) else {
            return;
        };
        if self.state.incidents.iter().any(|(key, generation)| {
            *key == landing.impact.causal_source && *generation >= landing.impact.incident
        }) {
            return;
        }
        landing.evidence.uncertainty = 0.9_f32.mul_add(
            self.state.clock - landing.sampled,
            landing.evidence.uncertainty,
        );
        self.accept_landing(landing.impact, landing.sampled, &landing.evidence);
    }
}

impl Landing {
    pub(super) fn validate(&self, now: f32) -> Result<(), String> {
        use super::checkpoint::{between, point, sample_time};
        super::checkpoint::impact(&self.impact)?;
        sample_time(self.sampled, now)?;
        point(self.evidence.estimated)?;
        point(self.evidence.goal)?;
        between(self.evidence.quality, 0.0, 1.0)?;
        between(self.evidence.uncertainty, 0.0, 100_000.0)?;
        between(self.evidence.snr_db, -200.0, 200.0)?;
        between(self.evidence.transmission, 0.0, 1.0)?;
        if self.evidence.portals.len() > 2 {
            return Err("Saved hearing path exceeds two portals".into());
        }
        Ok(())
    }
}
