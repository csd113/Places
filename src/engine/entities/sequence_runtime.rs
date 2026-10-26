//! Sequence scheduling and bounded movement on the world's live entity table.
//!
//! Runs remain visible while actions execute. A per-run revision prevents an
//! old tick snapshot from advancing, restoring or completing its replacement.

use glam::Vec3;

use super::components::SequenceCtl;
use super::sequences::{MAX_ACTIVE_SEQUENCES, SequenceRuntime, SequenceStepDef};
use super::{
    DispatchReport, EntityHandle, EntityWorld, EventKind, WorldCommand, WorldContext, WorldTick,
    warn_once,
};
use crate::engine::collision::resolve_player_collision_for_body_indexed;
use crate::engine::entity::{
    ENTITY_FACE_EPS_RAD, ENTITY_MAX_STEP_S, ENTITY_STEP_HEIGHT_M,
    ENTITY_TURN_RATE_DEGREES_PER_SECOND, angle_difference, turn_toward,
};

impl EntityWorld {
    pub(super) fn start_sequence(&mut self, id: &str, sequence: &str) -> bool {
        let Some(handle) = self.handle_of(id) else {
            return false;
        };
        let Some(def) = self.sequences.get(sequence) else {
            return false;
        };
        if self.sequence_runs.len() >= MAX_ACTIVE_SEQUENCES
            && !self.sequence_runs.iter().any(|run| run.owner == handle)
        {
            warn_once(
                "sequence-budget",
                format!(
                    "[entities] sequence `{sequence}` refused: {MAX_ACTIVE_SEQUENCES} sequences \
                     are already running"
                ),
            );
            return false;
        }
        let Some(next_revision) = self.next_sequence_revision.checked_add(1) else {
            warn_once(
                "sequence-revision-budget",
                "[entities] sequence run identities exhausted".to_string(),
            );
            return false;
        };
        let mut run = SequenceRuntime::new(def, handle);
        run.revision = self.next_sequence_revision;
        self.next_sequence_revision = next_revision;
        let sequence_id = def.id.clone();
        self.sequence_runs.retain(|active| active.owner != handle);
        self.sequence_runs.push(run);
        if let Some(control) = self.components.sequences.get_mut(handle) {
            control.sequence = sequence_id;
            control.running = true;
        } else {
            drop(self.components.sequences.insert(
                handle,
                SequenceCtl {
                    sequence: sequence_id,
                    running: true,
                },
            ));
        }
        true
    }

    pub(super) fn stop_sequence(&mut self, id: &str) -> bool {
        let Some(handle) = self.handle_of(id) else {
            return false;
        };
        let previous = self.sequence_runs.len();
        self.sequence_runs.retain(|run| run.owner != handle);
        if let Some(control) = self.components.sequences.get_mut(handle) {
            control.running = false;
        }
        self.sequence_runs.len() != previous
    }

    fn sequence_run_index(&self, owner: EntityHandle, revision: u64) -> Option<usize> {
        self.sequence_runs
            .iter()
            .position(|run| run.owner == owner && run.revision == revision)
    }

    /// Advances only the runs present at the start of this simulation tick.
    /// Actions may stop or replace any of them; newly started runs wait until
    /// the following tick, independent of their owner or insertion order.
    pub(super) fn tick_sequences(&mut self, ctx: &WorldContext<'_>, tick: &mut WorldTick) {
        if self.sequence_runs.is_empty()
            || !ctx.delta_seconds.is_finite()
            || ctx.delta_seconds <= 0.0
        {
            return;
        }
        let scheduled: Vec<(EntityHandle, u64)> = self
            .sequence_runs
            .iter()
            .map(|run| (run.owner, run.revision))
            .collect();
        for (owner, revision) in scheduled {
            let Some(mut run) = self
                .sequence_run_index(owner, revision)
                .and_then(|index| self.sequence_runs.get(index))
                .cloned()
            else {
                continue;
            };
            if !self.store.contains(owner) {
                run.stop();
            }
            if self.ai.sequence_held(owner)
                || self
                    .physics_actors
                    .contains_key(self.id_of(owner).unwrap_or_default())
            {
                continue;
            }
            if !run.finished && !run.stopped {
                run.step_time += ctx.delta_seconds;
                self.tick_sequence_steps(&mut run, ctx, tick);
            }
            self.finish_sequence_tick(run, tick);
            if tick.player_reset {
                return;
            }
        }
    }

    /// Processes the immediate tail without copying an entire authored sequence.
    fn tick_sequence_steps(
        &mut self,
        run: &mut SequenceRuntime,
        ctx: &WorldContext<'_>,
        tick: &mut WorldTick,
    ) {
        let Some(definition_index) = self
            .sequences
            .defs()
            .iter()
            .position(|def| def.id == run.sequence)
        else {
            run.finished = true;
            return;
        };
        for _ in 0_u8..64 {
            let Some(step) = self
                .sequences
                .defs()
                .get(definition_index)
                .and_then(|def| run.current(def))
                .cloned()
            else {
                run.finished = true;
                break;
            };
            let advance = match step {
                SequenceStepDef::Action { action } => {
                    if run.step_ran {
                        break;
                    }
                    run.step_ran = true;
                    let mut report = DispatchReport::default();
                    let origin = self
                        .components
                        .transforms
                        .get(run.owner)
                        .map(|pose| pose.position);
                    self.run_action(&action, Some(run.owner), origin, &mut report, 1);
                    merge_action_report(tick, &report);
                    true
                }
                SequenceStepDef::Wait { seconds } => run.step_time >= seconds,
                SequenceStepDef::Move { x, y, z, speed } => {
                    let target = Vec3::new(x, y.unwrap_or(f32::NAN), z);
                    self.sequence_move_step(run, target, speed, ctx, tick)
                }
                SequenceStepDef::Face { yaw_degrees } => {
                    self.sequence_face_step(run, yaw_degrees, ctx.delta_seconds, tick)
                }
                SequenceStepDef::WaitAnimation { clip, timeout } => {
                    run.animation_complete
                        || (timeout > 0.0 && run.step_time >= timeout)
                        || clip.as_deref().is_some_and(|clip_name| {
                            self.components
                                .animations
                                .get(run.owner)
                                .is_some_and(|animation| {
                                    !animation.playing
                                        && animation.clip == clip_name
                                        && animation.progress >= 1.0
                                })
                        })
                }
                SequenceStepDef::Emit { on, key } => {
                    let _event_queued = self.emit_depth(
                        EventKind::parse(on),
                        run.owner,
                        key.as_deref().unwrap_or_default(),
                        Some(run.owner),
                        1,
                    );
                    true
                }
                SequenceStepDef::SetState { name, value } => {
                    if let Some(id) = self.id_of(run.owner).map(str::to_string) {
                        let _event_queued = self.emit_state(&id, &name, value, Some(run.owner), 1);
                    }
                    true
                }
                SequenceStepDef::Stop => {
                    run.finished = true;
                    break;
                }
            };
            if self.sequence_run_index(run.owner, run.revision).is_none() || run.stopped {
                break;
            }
            if advance {
                if let Some(def) = self.sequences.defs().get(definition_index) {
                    let _advanced = run.advance(def);
                }
            } else {
                break;
            }
            if run.finished || tick.player_reset {
                break;
            }
        }
    }

    fn finish_sequence_tick(&mut self, run: SequenceRuntime, tick: &mut WorldTick) {
        let Some(index) = self.sequence_run_index(run.owner, run.revision) else {
            // A sequence action stopped, despawned or replaced this exact run.
            return;
        };
        if !run.finished && !run.stopped {
            if let Some(active) = self.sequence_runs.get_mut(index) {
                *active = run;
            }
            return;
        }
        let _removed = self.sequence_runs.remove(index);
        if let Some(control) = self.components.sequences.get_mut(run.owner) {
            control.running = false;
        }
        if run.finished && !run.stopped {
            let _event_queued = self.emit_depth(
                EventKind::SequenceComplete,
                run.owner,
                &run.sequence,
                Some(run.owner),
                1,
            );
            tick.sequences_completed = tick.sequences_completed.saturating_add(1);
        }
    }

    /// One `move` step of a running sequence. Returns true when it moved on.
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "bounded flat movement arithmetic"
    )] // bounded flat movement arithmetic
    fn sequence_move_step(
        &mut self,
        run: &mut SequenceRuntime,
        target: Vec3,
        speed: f32,
        ctx: &WorldContext<'_>,
        tick: &mut WorldTick,
    ) -> bool {
        if !target.x.is_finite() || !target.z.is_finite() || !speed.is_finite() || speed <= 0.0 {
            return true;
        }
        let Some(transform) = self.components.transforms.get(run.owner).copied() else {
            return true;
        };
        let here = transform.position;
        let flat = glam::Vec2::new(target.x - here.x, target.z - here.z);
        let distance = flat.length();
        if !distance.is_finite() {
            return true;
        }
        if distance <= super::sequences::SEQUENCE_ARRIVE_EPS_M {
            return true;
        }
        let direction = flat / distance;
        let travel = (speed * ctx.delta_seconds).min(distance);
        let candidate = glam::Vec2::new(here.x, here.z) + direction * travel;
        let (radius, height) = self.body_of(run.owner);
        let resolved = resolve_player_collision_for_body_indexed(
            ctx.index, candidate, radius, here.y, height, ctx.walls,
        );
        if (resolved - candidate).length() > crate::engine::entity::ENTITY_BLOCK_EPS_M {
            warn_once(
                "sequence-move-blocked",
                format!(
                    "[entities] sequence `{}` is blocked by a wall",
                    run.sequence
                ),
            );
            run.stop();
            return true;
        }
        let Some(floor_y) = ctx.floor.walk_height_at(resolved.x, resolved.y) else {
            warn_once(
                "sequence-move-void",
                format!(
                    "[entities] sequence `{}` leaves every walkable floor",
                    run.sequence
                ),
            );
            run.stop();
            return true;
        };
        if (floor_y - here.y).abs() > ENTITY_STEP_HEIGHT_M + crate::engine::collision::STEP_EPS {
            warn_once(
                "sequence-move-step",
                format!(
                    "[entities] sequence `{}` cannot climb the step on its path",
                    run.sequence
                ),
            );
            run.stop();
            return true;
        }
        let next = Vec3::new(resolved.x, floor_y, resolved.y);
        if let Some(entry) = self.components.transforms.get_mut(run.owner) {
            entry.position = next;
        }
        let _agent_updated =
            self.ai
                .apply_sequence_transform(run.owner, next, transform.yaw_degrees);
        if let Some(key) = self.dynamic_key(run.owner) {
            self.commands.push(WorldCommand::SetDynamicTransform {
                entity: key,
                position: next,
                yaw_degrees: transform.yaw_degrees,
            });
        }
        tick.frames_dirty = true;
        false
    }

    /// One `face` step. Returns true when the entity reached the yaw.
    ///
    /// The turn is delta-scaled, exactly like a route's `face` step, so the
    /// authored degrees-per-second rate holds at any frame rate.
    fn sequence_face_step(
        &mut self,
        run: &SequenceRuntime,
        yaw_degrees: f32,
        delta_seconds: f32,
        tick: &mut WorldTick,
    ) -> bool {
        if !yaw_degrees.is_finite() {
            return true;
        }
        let Some(transform) = self.components.transforms.get(run.owner).copied() else {
            return true;
        };
        let target = yaw_degrees.to_radians();
        let current = transform.yaw_degrees.to_radians();
        if angle_difference(current, target).abs() <= ENTITY_FACE_EPS_RAD {
            return true;
        }
        let delta = if delta_seconds.is_finite() {
            delta_seconds.clamp(0.0, ENTITY_MAX_STEP_S)
        } else {
            0.0
        };
        let step = ENTITY_TURN_RATE_DEGREES_PER_SECOND.to_radians() * delta;
        let next = turn_toward(current, target, step);
        if let Some(entry) = self.components.transforms.get_mut(run.owner) {
            entry.yaw_degrees = next.to_degrees();
        }
        let _agent_updated =
            self.ai
                .apply_sequence_transform(run.owner, transform.position, next.to_degrees());
        if let Some(key) = self.dynamic_key(run.owner) {
            self.commands.push(WorldCommand::SetDynamicTransform {
                entity: key,
                position: transform.position,
                yaw_degrees: next.to_degrees(),
            });
        }
        tick.frames_dirty = true;
        false
    }
}

const fn merge_action_report(tick: &mut WorldTick, report: &DispatchReport) {
    tick.actions_run = tick.actions_run.saturating_add(report.actions_run);
    tick.doors_acted = tick.doors_acted.saturating_add(report.doors_acted);
    tick.lights_toggled = tick.lights_toggled.saturating_add(report.lights_toggled);
    tick.spawned = tick.spawned.saturating_add(report.spawned);
    tick.despawned = tick.despawned.saturating_add(report.despawned);
    tick.missing_targets = tick.missing_targets.saturating_add(report.missing_targets);
    tick.unsupported = tick.unsupported.saturating_add(report.unsupported);
    tick.labels_shown = tick.labels_shown.saturating_add(report.labels_shown);
    tick.labels_hidden = tick.labels_hidden.saturating_add(report.labels_hidden);
    tick.player_reset |= report.player_reset;
    tick.frames_dirty |=
        report.animations_started > 0 || report.spawned > 0 || report.despawned > 0;
}
