//! A known thrown prop is followed only through validated, delayed echoes.
use super::{Guest, Intent};
use crate::engine::physics::ReleaseIdentity;
use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

pub struct Reflection {
    pub point: Vec3,
    pub sleeping: bool,
}

/// The trusted world owns physical transforms. The hunter can request only an
/// already heard release, and receives a coordinate only after the beam fits.
pub trait Scene {
    fn reflect_prey(
        &self,
        _ids: &[String],
        _origin: Vec3,
        _axis: Vec3,
    ) -> Option<super::prey::Echo> {
        None
    }
    fn prey_return_clear(&self, _key: u64, from: Vec3, to: Vec3) -> bool {
        self.clear(from, to)
    }
    fn clear(&self, from: Vec3, to: Vec3) -> bool;
    fn hear(&self, request: &super::hearing::Request) -> Option<super::hearing::Heard>;
    fn echo_quality(&self, origin: Vec3, hit: Vec3) -> f32;
    fn valid(&self, identity: ReleaseIdentity) -> bool;
    fn reflect(&self, identity: ReleaseIdentity, origin: Vec3, axis: Vec3) -> Option<Reflection>;
    fn return_clear(&self, identity: ReleaseIdentity, from: Vec3, sampled: Vec3) -> bool;
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Echo {
    point: Vec3,
    sleeping: bool,
    sampled: f32,
    deliver: f32,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Track {
    identity: ReleaseIdentity,
    point: Vec3,
    sampled: f32,
    stable_since: f32,
    rest_confirmed: bool,
    next_probe: f32,
    initial_until: f32,
    click_probe: bool,
    pending: VecDeque<Echo>,
}

impl Guest {
    pub(super) fn begin_prop_track(
        &mut self,
        identity: ReleaseIdentity,
        point: Vec3,
        sampled: f32,
    ) {
        self.state.prop_track = Some(Track {
            identity,
            point,
            sampled,
            stable_since: self.state.clock,
            rest_confirmed: false,
            next_probe: self.state.clock,
            initial_until: self.state.clock + 3.5,
            click_probe: false,
            pending: VecDeque::new(),
        });
    }

    pub(super) fn sense_prop(
        &mut self,
        scene: &dyn Scene,
        ctx: &crate::engine::entities::WorldContext<'_>,
    ) {
        let Some(mut track) = self.state.prop_track.take() else {
            return;
        };
        if !scene.valid(track.identity) {
            self.log(
                "prop_track_lost",
                &serde_json::json!({"reason":"release identity changed, held or absent"}),
            );
            return;
        }
        let now = self.state.clock;
        let (origin, axis) = self.head();
        while track
            .pending
            .front()
            .is_some_and(|echo| echo.deliver <= now)
        {
            let Some(echo) = track.pending.pop_front() else {
                break;
            };
            if now - echo.sampled > 0.35 || !scene.return_clear(track.identity, origin, echo.point)
            {
                continue;
            }
            let moved = track.point.distance(echo.point) > 0.015;
            if moved || !echo.sleeping {
                self.state.noise_deadline = now + 8.0;
                track.stable_since = echo.sampled;
                track.rest_confirmed = false;
            } else if echo.sampled - track.stable_since >= 0.5 {
                track.rest_confirmed = true;
            }
            track.point = echo.point;
            track.sampled = echo.sampled;
            self.log("prop_echo_return", &serde_json::json!({"release":track.identity,"sampled":echo.sampled,"point":echo.point,"sleeping":echo.sleeping,"rest_confirmed":track.rest_confirmed}));
            if !self.fresh()
                && matches!(
                    self.state.intent,
                    Intent::InvestigateNoise | Intent::Inspect | Intent::Listen
                )
            {
                // Only a validated known-source echo supplies this point. The
                // authored floor below it determines its storey, not ours.
                let goal = echoed_goal(ctx, echo.point);
                if self.state.goal.is_none_or(|old| old.distance(goal) > 0.08) {
                    self.next_plan = 0.0;
                    self.path.clear();
                }
                self.state.goal = Some(goal);
                self.state.goal_time = echo.sampled;
                self.state.noise_portal = false;
                if (moved && self.state.intent == Intent::Inspect)
                    || self.state.intent == Intent::Listen
                {
                    self.set_intent(
                        Intent::InvestigateNoise,
                        "known thrown prop moved during inspection",
                    );
                }
            }
        }
        let may_probe = matches!(self.state.intent, Intent::InvestigateNoise | Intent::Listen)
            || (self.state.intent == Intent::Inspect && track.click_probe);
        if may_probe
            && now >= track.next_probe
            && track.pending.len() < 4
            && !matches!(
                self.state.intent,
                Intent::Disabled | Intent::EscapeFrustration
            )
        {
            track.next_probe = super::cadence(track.next_probe, now, 0.25);
            track.click_probe = false;
            if let Some(reflection) = scene.reflect(track.identity, origin, axis) {
                let distance = origin.distance(reflection.point);
                track.pending.push_back(Echo {
                    point: reflection.point,
                    sleeping: reflection.sleeping,
                    sampled: now,
                    deliver: now + (distance * 2.0 / 343.0).max(0.05),
                });
                self.log("prop_echo_pulse", &serde_json::json!({"release":track.identity,"origin":origin,"axis":axis,"range":14_u8,"cone":110_u8}));
            }
        }
        if now - track.sampled > 3.0
            && now > track.initial_until
            && self.state.intent == Intent::InvestigateNoise
        {
            self.set_intent(
                Intent::Search,
                "known prop echo expired; last observed region only",
            );
            self.state.search_until = now + 6.0;
        }
        self.state.prop_track = Some(track);
    }

    pub(super) const fn request_prop_click(&mut self) {
        if let Some(track) = self.state.prop_track.as_mut() {
            track.next_probe = self.state.clock;
            track.click_probe = true;
        }
    }

    pub(super) fn prop_ready(&self) -> bool {
        self.state
            .prop_track
            .as_ref()
            .is_none_or(|track| track.rest_confirmed)
    }

    pub(super) fn inspection_standoff(&self) -> f32 {
        if self.state.noise_portal {
            0.1
        } else if self.prop_ready() {
            2.001_614_6
        } else {
            3.6
        }
    }
}

impl Track {
    pub(super) fn validate(&self, now: f32) -> Result<(), String> {
        use super::checkpoint::{between, point, sample_time};
        if self.identity.key == 0
            || self.identity.generation == 0
            || self.identity.kind != crate::engine::physics::ImpactRelease::Thrown
            || self.pending.len() > 4
        {
            return Err("Invalid saved thrown-source identity or return count".into());
        }
        point(self.point)?;
        sample_time(self.sampled, now)?;
        sample_time(self.stable_since, now)?;
        between(self.next_probe, 0.0, 4.0_f32.mul_add(1.0, now))?;
        between(self.initial_until, 0.0, 3.5_f32.mul_add(1.0, now))?;
        let mut previous = 0.0;
        for echo in &self.pending {
            point(echo.point)?;
            sample_time(echo.sampled, now)?;
            between(
                echo.deliver,
                echo.sampled,
                0.36_f32.mul_add(1.0, echo.sampled),
            )?;
            if echo.deliver < previous {
                return Err("Unordered saved prop echoes".into());
            }
            previous = echo.deliver;
        }
        Ok(())
    }
}

fn echoed_goal(ctx: &crate::engine::entities::WorldContext<'_>, point: Vec3) -> Vec3 {
    let y = ctx
        .floor
        .height_below(point.x, point.z, 0.01_f32.mul_add(1.0, point.y))
        .and_then(|floor| ctx.floor.walk_height_near(point.x, point.z, floor, 0.4))
        .unwrap_or(point.y);
    Vec3::new(point.x, y, point.z)
}
