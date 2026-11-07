//! Bounded out-of-body shots owned by the ordinary sequence scheduler.
use super::sequences::{SequenceDef, SequenceRuntime, SequenceStepDef};
use super::{EntityHandle, EntityWorld};
use glam::Vec3;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
/// Opt-in out-of-body ownership for an ordinary sequence.
pub struct CinematicDef {
    pub start: CameraPoseDef,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
/// Authored world-space start; yaw zero faces -Z and positive pitch looks up.
pub struct CameraPoseDef {
    pub position: [f32; 3],
    pub yaw_degrees: f32,
    pub pitch_degrees: f32,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Bounded interpolation without spline overshoot.
pub enum CameraEasing {
    #[default]
    Linear,
    Smooth,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
/// Point or authored entity plus a world-axis offset.
pub enum CameraTargetDef {
    Point {
        position: [f32; 3],
    },
    Entity {
        id: String,
        #[serde(default)]
        offset: [f32; 3],
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
/// Free camera motion: no physical player relocation or collision sweep.
pub enum CameraMotionDef {
    Move {
        position: [f32; 3],
        #[serde(default)]
        easing: CameraEasing,
    },
    Follow {
        target: CameraTargetDef,
        offset: [f32; 3],
    },
    Orbit {
        target: CameraTargetDef,
        radius: f32,
        height: f32,
        start_degrees: f32,
        sweep_degrees: f32,
    },
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
/// Orientation held, interpolated, or aimed at a live target.
pub enum CameraAimDef {
    #[default]
    Keep,
    Angles {
        yaw_degrees: f32,
        pitch_degrees: f32,
    },
    LookAt {
        target: CameraTargetDef,
    },
}
#[derive(Debug, Clone, Copy, PartialEq)]
/// Effective view position and orientation, in radians.
pub struct CinematicView {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Exact lease identity; stale frames cannot own a replacement camera.
pub struct CinematicToken {
    pub generation: u64,
    pub owner: EntityHandle,
    pub revision: u64,
}
#[derive(Debug, Clone, PartialEq)]
pub(super) struct CinematicRun {
    pub token: CinematicToken,
    pub view: CinematicView,
    pub elapsed: f32,
    shot: Option<Shot>,
}
#[derive(Debug, Clone, PartialEq)]
struct Shot {
    step: usize,
    start: CinematicView,
    motion_target: Option<ResolvedTarget>,
    aim_target: Option<ResolvedTarget>,
}
#[derive(Debug, Clone, Copy, PartialEq)]
enum ResolvedTarget {
    Point(Vec3),
    Entity(EntityHandle, Vec3),
}
impl CinematicRun {
    pub fn new(start: CameraPoseDef, token: CinematicToken) -> Self {
        Self {
            token,
            view: CinematicView {
                position: start.position.into(),
                yaw: start.yaw_degrees.to_radians(),
                pitch: start.pitch_degrees.to_radians(),
            },
            elapsed: 0.0,
            shot: None,
        }
    }
}

impl EntityWorld {
    /// Pure frame snapshot; call refresh after live entity transforms change.
    #[must_use]
    pub fn cinematic_view(&self) -> Option<CinematicView> {
        self.sequence_runs
            .iter()
            .filter(|run| !run.finished && !run.stopped)
            .find_map(|run| run.cinematic.as_ref().map(|camera| camera.view))
    }
    #[must_use]
    pub fn cinematic_token(&self) -> Option<CinematicToken> {
        self.sequence_runs
            .iter()
            .filter(|run| !run.finished && !run.stopped)
            .find_map(|run| run.cinematic.as_ref().map(|camera| camera.token))
    }
    /// Cancellation never turns a previously committed successful finish into failure.
    pub fn cancel_cinematic(&mut self) {
        let owners: Vec<_> = self
            .sequence_runs
            .iter()
            .filter(|run| run.cinematic.is_some() && !run.finished)
            .map(|run| run.owner)
            .collect();
        self.sequence_runs
            .retain(|run| run.cinematic.is_none() || run.finished);
        for owner in owners {
            if let Some(control) = self.components.sequences.get_mut(owner) {
                control.running = false;
            }
        }
    }
    fn resolve_camera_target(&self, target: &CameraTargetDef) -> Option<ResolvedTarget> {
        match target {
            CameraTargetDef::Point { position } => Some(ResolvedTarget::Point((*position).into())),
            CameraTargetDef::Entity { id, offset } => {
                let handle = self.handle_of(id)?;
                let _pose = self.components.transforms.get(handle)?;
                Some(ResolvedTarget::Entity(handle, (*offset).into()))
            }
        }
    }
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "world-space addition is immediately checked for a finite result"
    )]
    fn camera_target_position(&self, target: ResolvedTarget) -> Option<Vec3> {
        match target {
            ResolvedTarget::Point(position) => Some(position),
            ResolvedTarget::Entity(handle, offset) => self
                .components
                .transforms
                .get(handle)
                .map(|pose| pose.position + offset),
        }
        .filter(|position| position.is_finite())
    }
    /// Resamples active live targets without advancing simulation or replaying effects.
    pub fn refresh_cinematic_view(&mut self) {
        let Some(index) = self
            .sequence_runs
            .iter()
            .position(|run| run.cinematic.is_some() && !run.finished && !run.stopped)
        else {
            return;
        };
        let Some(mut run) = self.sequence_runs.get(index).cloned() else {
            return;
        };
        if !self.store.contains(run.owner)
            || self.ai.sequence_held(run.owner)
            || self
                .physics_actors
                .contains_key(self.id_of(run.owner).unwrap_or_default())
        {
            self.cancel_cinematic();
            return;
        }
        let step = self
            .sequences
            .get(&run.sequence)
            .and_then(|def| run.current(def))
            .cloned();
        if let Some(SequenceStepDef::Camera {
            seconds,
            motion,
            aim,
        }) = step
        {
            let _done = self.cinematic_step(&mut run, seconds, &motion, &aim);
        }
        if run.stopped {
            self.cancel_cinematic();
        } else if let Some(active) = self.sequence_runs.get_mut(index) {
            *active = run;
        }
    }
    fn begin_camera_shot(
        &self,
        step: usize,
        start: CinematicView,
        motion: &CameraMotionDef,
        aim: &CameraAimDef,
    ) -> Option<Shot> {
        let motion_target = match motion {
            CameraMotionDef::Move {
                position: _,
                easing: _,
            } => None,
            CameraMotionDef::Follow { target, offset: _ }
            | CameraMotionDef::Orbit {
                target,
                radius: _,
                height: _,
                start_degrees: _,
                sweep_degrees: _,
            } => Some(self.resolve_camera_target(target)?),
        };
        let aim_target = match aim {
            CameraAimDef::LookAt { target } => Some(self.resolve_camera_target(target)?),
            CameraAimDef::Keep
            | CameraAimDef::Angles {
                yaw_degrees: _,
                pitch_degrees: _,
            } => None,
        };
        Some(Shot {
            step,
            start,
            motion_target,
            aim_target,
        })
    }
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "validated bounded camera interpolation and finite checked target transforms"
    )]
    pub(super) fn cinematic_step(
        &self,
        run: &mut SequenceRuntime,
        seconds: f32,
        motion: &CameraMotionDef,
        aim: &CameraAimDef,
    ) -> bool {
        let Some(camera) = run.cinematic.as_mut() else {
            run.stop();
            return false;
        };
        if camera
            .shot
            .as_ref()
            .is_none_or(|shot| shot.step != run.step)
        {
            let Some(shot) = self.begin_camera_shot(run.step, camera.view, motion, aim) else {
                run.stop();
                return false;
            };
            camera.shot = Some(shot);
        }
        let Some(shot) = camera.shot.as_ref() else {
            return false;
        };
        let t = (run.step_time / seconds).clamp(0.0, 1.0);
        let motion_position = match shot.motion_target {
            Some(target) => {
                let Some(position) = self.camera_target_position(target) else {
                    run.stop();
                    return false;
                };
                position
            }
            None => Vec3::ZERO,
        };
        camera.view.position = match motion {
            CameraMotionDef::Move { position, easing } => shot.start.position.lerp(
                (*position).into(),
                match easing {
                    CameraEasing::Linear => t,
                    CameraEasing::Smooth => t * t * 2.0_f32.mul_add(-t, 3.0),
                },
            ),
            CameraMotionDef::Follow { target: _, offset } => motion_position + Vec3::from(*offset),
            CameraMotionDef::Orbit {
                radius,
                height,
                start_degrees,
                sweep_degrees,
                target: _,
            } => {
                let angle = sweep_degrees.mul_add(t, *start_degrees).to_radians();
                motion_position + Vec3::new(angle.sin() * radius, *height, -angle.cos() * radius)
            }
        };
        if !self.aim_camera(&mut camera.view, shot, aim, t) {
            run.stop();
            return false;
        }
        if !camera.view.position.is_finite()
            || !camera.view.yaw.is_finite()
            || !camera.view.pitch.is_finite()
        {
            run.stop();
            return false;
        }
        run.step_time >= seconds
    }
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "finite bounded camera vectors and clamped aim interpolation"
    )]
    fn aim_camera(
        &self,
        view: &mut CinematicView,
        shot: &Shot,
        aim: &CameraAimDef,
        t: f32,
    ) -> bool {
        match aim {
            CameraAimDef::Keep => {
                view.yaw = shot.start.yaw;
                view.pitch = shot.start.pitch;
            }
            CameraAimDef::Angles {
                yaw_degrees,
                pitch_degrees,
            } => {
                view.yaw = crate::engine::entity::angle_difference(
                    shot.start.yaw,
                    yaw_degrees.to_radians(),
                )
                .mul_add(t, shot.start.yaw);
                view.pitch =
                    (pitch_degrees.to_radians() - shot.start.pitch).mul_add(t, shot.start.pitch);
            }
            CameraAimDef::LookAt { target: _ } => {
                let Some(aim_position) = shot
                    .aim_target
                    .and_then(|reference| self.camera_target_position(reference))
                else {
                    return false;
                };
                let delta = aim_position - view.position;
                let flat = delta.x.hypot(delta.z);
                if flat > 0.0001 {
                    view.yaw = delta.x.atan2(-delta.z);
                }
                if flat > 0.0001 || delta.y.abs() > 0.0001 {
                    view.pitch = delta
                        .y
                        .atan2(flat)
                        .clamp(-89_f32.to_radians(), 89_f32.to_radians());
                }
            }
        }
        true
    }
}

/// Shared editable/runtime admission; target names are checked by the loader separately.
///
/// # Errors
/// Returns a diagnostic for an unbounded or malformed cinematic.
pub fn validate_cinematic(def: &SequenceDef) -> Result<(), String> {
    let camera_count = def
        .steps
        .iter()
        .filter(|step| {
            matches!(
                step,
                SequenceStepDef::Camera {
                    seconds: _,
                    motion: _,
                    aim: _
                }
            )
        })
        .count();
    let Some(camera) = &def.cinematic else {
        return if camera_count == 0 {
            Ok(())
        } else {
            Err("camera steps require cinematic start metadata".into())
        };
    };
    if def.looped
        || camera_count == 0
        || !vector_valid(camera.start.position)
        || !angles_valid(camera.start.yaw_degrees, camera.start.pitch_degrees)
    {
        return Err("cinematics require a finite start pose, camera steps and no looping".into());
    }
    let mut duration = 0.0;
    for step in &def.steps {
        match step {
            SequenceStepDef::Camera {
                seconds,
                motion,
                aim,
            } => {
                if !(0.05..=120.0).contains(seconds) || !motion_valid(motion) || !aim_valid(aim) {
                    return Err("camera duration, motion or aim is outside finite bounds".into());
                }
                duration += seconds;
            }
            SequenceStepDef::Wait { seconds } => {
                if !(0.0..=600.0).contains(seconds) {
                    return Err("cinematic wait is outside finite bounds".into());
                }
                duration += seconds;
            }
            SequenceStepDef::WaitAnimation { timeout, clip: _ } => {
                if !(0.0..=120.0).contains(timeout) {
                    return Err("cinematic animation timeout is outside finite bounds".into());
                }
                duration += timeout;
            }
            SequenceStepDef::Action { action: _ }
            | SequenceStepDef::Move {
                x: _,
                y: _,
                z: _,
                speed: _,
            }
            | SequenceStepDef::Face { yaw_degrees: _ }
            | SequenceStepDef::Emit { on: _, key: _ }
            | SequenceStepDef::SetState { name: _, value: _ }
            | SequenceStepDef::Stop => {}
        }
    }
    if !duration.is_finite() || duration > 600.0 {
        return Err("cinematic declared duration exceeds 600 seconds".into());
    }
    Ok(())
}
fn vector_valid(position: [f32; 3]) -> bool {
    position
        .iter()
        .all(|value| value.is_finite() && value.abs() <= 100_000.0)
}
fn angles_valid(yaw: f32, pitch: f32) -> bool {
    yaw.is_finite() && yaw.abs() <= 360_000.0 && pitch.is_finite() && pitch.abs() <= 89.0
}
fn target_valid(target: &CameraTargetDef) -> bool {
    match target {
        CameraTargetDef::Point { position } => vector_valid(*position),
        CameraTargetDef::Entity { id, offset } => !id.trim().is_empty() && vector_valid(*offset),
    }
}
fn motion_valid(motion: &CameraMotionDef) -> bool {
    match motion {
        CameraMotionDef::Move {
            position,
            easing: _,
        } => vector_valid(*position),
        CameraMotionDef::Follow { target, offset } => target_valid(target) && vector_valid(*offset),
        CameraMotionDef::Orbit {
            target,
            radius,
            height,
            start_degrees,
            sweep_degrees,
        } => {
            target_valid(target)
                && (0.05..=1000.0).contains(radius)
                && height.is_finite()
                && height.abs() <= 100_000.0
                && start_degrees.is_finite()
                && start_degrees.abs() <= 360_000.0
                && sweep_degrees.is_finite()
                && sweep_degrees.abs() <= 720.0
        }
    }
}
fn aim_valid(aim: &CameraAimDef) -> bool {
    match aim {
        CameraAimDef::Keep => true,
        CameraAimDef::Angles {
            yaw_degrees,
            pitch_degrees,
        } => angles_valid(*yaw_degrees, *pitch_degrees),
        CameraAimDef::LookAt { target } => target_valid(target),
    }
}
/// Authored target references, for validation without allocating a second registry.
pub fn camera_targets<'a>(
    motion: &'a CameraMotionDef,
    aim: &'a CameraAimDef,
) -> impl Iterator<Item = &'a CameraTargetDef> {
    let motion_target = match motion {
        CameraMotionDef::Move {
            position: _,
            easing: _,
        } => None,
        CameraMotionDef::Follow { target, offset: _ }
        | CameraMotionDef::Orbit {
            target,
            radius: _,
            height: _,
            start_degrees: _,
            sweep_degrees: _,
        } => Some(target),
    };
    let aim_target = match aim {
        CameraAimDef::LookAt { target } => Some(target),
        CameraAimDef::Keep
        | CameraAimDef::Angles {
            yaw_degrees: _,
            pitch_degrees: _,
        } => None,
    };
    motion_target.into_iter().chain(aim_target)
}
