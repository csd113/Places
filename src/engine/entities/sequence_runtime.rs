//! Sequence scheduling and bounded movement on the world's live entity table.
//!
//! Runs remain visible while actions execute. A per-run revision prevents an
//! old tick snapshot from advancing, restoring or completing its replacement.

use glam::{Vec2, Vec3};

use super::components::SequenceCtl;
use super::sequences::{AfterMove, MAX_ACTIVE_SEQUENCES, SequenceRuntime, SequenceStepDef};
use super::{
    DispatchReport, EntityHandle, EntityWorld, EventKind, WorldCommand, WorldContext, WorldTick,
    warn_once,
};
use crate::engine::collision::{
    disc_segment_blocked, door_segment_blocked, dynamic_disc_segment_blocked,
};
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
        if super::cinematics::validate_cinematic(def).is_err()
            || self.sequence_runs.iter().any(|run| {
                run.cinematic.is_some()
                    && (def.cinematic.is_some() || (run.owner == handle && run.finished))
            })
        {
            return false;
        }
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
        run.cinematic = def.cinematic.as_ref().map(|camera| {
            super::cinematics::CinematicRun::new(
                camera.start,
                super::cinematics::CinematicToken {
                    generation: self.generation,
                    owner: handle,
                    revision: run.revision,
                },
            )
        });
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
        self.sequence_runs
            .retain(|run| run.owner != handle || (run.cinematic.is_some() && run.finished));
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
                if run.cinematic.is_some() && !run.finished {
                    run.stop();
                } else if !run.finished {
                    continue;
                }
            }
            if let Some(camera) = run.cinematic.as_mut() {
                camera.elapsed += ctx.delta_seconds;
                if !run.finished && camera.elapsed > 600.0 {
                    run.stop();
                }
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
        let Some((definition_index, definition)) = self
            .sequences
            .defs()
            .iter()
            .enumerate()
            .find(|(_, def)| def.id == run.sequence)
        else {
            run.finished = true;
            return;
        };
        let after_move = definition.after_move;
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
                SequenceStepDef::Camera {
                    seconds,
                    motion,
                    aim,
                } => self.cinematic_step(run, seconds, &motion, &aim),
                SequenceStepDef::Wait { seconds } => run.step_time >= seconds,
                SequenceStepDef::Move { x, y, z, speed } => {
                    let target = Vec3::new(x, y.unwrap_or(f32::NAN), z);
                    self.sequence_move_step(run, target, speed, after_move, ctx, tick)
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
        if run.cinematic.is_some() && run.finished && !run.stopped {
            if let Some(control) = self.components.sequences.get_mut(run.owner) {
                control.running = false;
            }
            if self.events.remaining_capacity() == 0 {
                if let Some(active) = self.sequence_runs.get_mut(index) {
                    *active = run;
                }
                return;
            }
        }
        let _removed = self.sequence_runs.remove(index);
        if let Some(control) = self.components.sequences.get_mut(run.owner) {
            control.running = false;
        }
        if run.finished && !run.stopped {
            if run.moved_owner
                && let Some(def) = self.sequences.get(&run.sequence)
            {
                self.ai.complete_script_move(run.owner, def.after_move);
            }
            let _event_queued = self.emit_terminal(
                EventKind::SequenceComplete,
                run.owner,
                &run.sequence,
                Some(run.owner),
            );
            tick.sequences_completed = tick.sequences_completed.saturating_add(1);
        }
    }

    /// One straight scripted step. Sample the current storey and sweep each
    /// supported segment before publishing any part of the pose.
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "finite scripted movement and support arithmetic"
    )]
    pub(super) fn scripted_move_position(
        &self,
        owner: EntityHandle,
        here: Vec3,
        target: Vec3,
        speed: f32,
        ctx: &WorldContext<'_>,
    ) -> Result<Option<Vec3>, &'static str> {
        let from = Vec2::new(here.x, here.z);
        let remaining = Vec2::new(target.x, target.z) - from;
        let distance = remaining.length();
        if !distance.is_finite() || !speed.is_finite() || speed <= 0.0 {
            return Err("the requested movement is invalid");
        }
        if distance <= super::sequences::SEQUENCE_ARRIVE_EPS_M {
            return Ok(None);
        }
        let (radius, height) = self.body_of(owner);
        let leaves = self
            .door_colliders
            .get(..self.guest_collider_start)
            .unwrap_or_default();
        let step_m = crate::engine::nav::NAV_MOVE_SUBSTEP_M;
        let travel = (speed * ctx.delta_seconds.min(ENTITY_MAX_STEP_S))
            .min(distance)
            .min(step_m * 64.0);
        let direction = remaining / distance;
        let mut position = from;
        let mut feet = here.y;
        let mut covered = 0.0_f32;
        for _ in 0_u8..64 {
            if covered >= travel {
                break;
            }
            covered = (covered + step_m).min(travel);
            let candidate = from + direction * covered;
            let next_floor = ctx
                .floor
                .walk_height_near(candidate.x, candidate.y, feet, ENTITY_STEP_HEIGHT_M)
                .ok_or("the path has no reachable floor")?;
            let lower = feet.min(next_floor);
            let swept_height = height + (feet - next_floor).abs();
            if self
                .ceiling
                .ceiling_above_disc(candidate.x, candidate.y, radius, next_floor)
                .is_some_and(|ceiling| {
                    next_floor + height > ceiling + crate::engine::collision::CONTACT_EPS
                })
                || disc_segment_blocked(
                    position,
                    candidate,
                    radius,
                    lower,
                    swept_height,
                    ENTITY_STEP_HEIGHT_M,
                    ctx.index,
                    ctx.walls,
                )
                || door_segment_blocked(position, candidate, radius, lower, swept_height, leaves)
                || dynamic_disc_segment_blocked(
                    position,
                    candidate,
                    radius,
                    lower,
                    swept_height,
                    &self.navigation_obstacles,
                )
            {
                return Err("a solid blocks the path");
            }
            position = candidate;
            feet = next_floor;
        }
        Ok(Some(Vec3::new(position.x, feet, position.y)))
    }

    /// One sequence movement step. Returns true when the entity reaches the waypoint.
    fn sequence_move_step(
        &mut self,
        run: &mut SequenceRuntime,
        target: Vec3,
        speed: f32,
        after_move: AfterMove,
        ctx: &WorldContext<'_>,
        tick: &mut WorldTick,
    ) -> bool {
        let Some(transform) = self.components.transforms.get(run.owner).copied() else {
            return true;
        };
        let next =
            match self.scripted_move_position(run.owner, transform.position, target, speed, ctx) {
                Ok(Some(next)) => next,
                Ok(None) => {
                    run.moved_owner |= after_move == AfterMove::Stay;
                    return true;
                }
                Err(reason) => {
                    warn_once(
                        "sequence-move-blocked",
                        format!(
                            "[entities] cannot move in sequence {}: {reason}",
                            run.sequence
                        ),
                    );
                    run.stop();
                    return true;
                }
            };
        if let Some(entry) = self.components.transforms.get_mut(run.owner) {
            entry.position = next;
        }
        let _agent_updated = self.ai.apply_sequence_transform(
            run.owner,
            next,
            transform.yaw_degrees,
            Some(after_move),
        );
        run.moved_owner |= next != transform.position;
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
        let _agent_updated = self.ai.apply_sequence_transform(
            run.owner,
            transform.position,
            next.to_degrees(),
            None,
        );
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
