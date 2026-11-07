//! Full-body planted capture with exactly one live prey owner. Acoustic evidence
//! authorizes the approach; exact nearby geometry can only admit/refuse contact.
use super::{Guest, Intent, prey};
use crate::engine::collision::{DoorCollider, WallAabb};
use crate::engine::entities::WorldContext;
use crate::engine::entity::JointPose;
use crate::engine::feltling::Feltling;
use glam::{Quat, Vec3};
mod rollback;

const ENTRY_BLEND_SECONDS: f32 = 0.3;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    Forward,
    CancelRequested,
    Reverse,
}

pub struct Capture {
    pub key: u64,
    pub prey_id: String,
    profile: &'static prey::CaptureProfile,
    start: (Vec3, f32),
    from: Vec<JointPose>,
    entry_phase: f32,
    entry_blend_seconds: f32,
    clock: f32,
    latched: bool,
    consumed: bool,
    direction: Direction,
    released: bool,
    blocked_seconds: f32,
}

impl Capture {
    fn substep(&self, left: f32) -> f32 {
        let next_boundary = [
            self.entry_phase + self.entry_blend_seconds,
            self.profile.contact_seconds,
            self.profile.capture_seconds,
            self.profile.capture_seconds + self.profile.inside_seconds,
            self.profile.capture_seconds + self.profile.consume_seconds,
            self.profile.capture_seconds + self.profile.swallow_seconds,
        ]
        .into_iter()
        .find(|boundary| *boundary > self.clock + 0.000_001)
        .unwrap_or(f32::MAX);
        left.min(1.0 / 120.0).min(next_boundary - self.clock)
    }

    pub(super) fn diagnostics(&self) -> serde_json::Value {
        serde_json::json!({"key":self.key,"prey":self.prey_id,"clock":self.clock,"entry_phase":self.entry_phase,"entry_blend_seconds":self.entry_blend_seconds,"elapsed":self.clock-self.entry_phase,"latched":self.latched,"consumed":self.consumed,"recovering":self.direction == Direction::Reverse,"released":self.released,"blocked_seconds":self.blocked_seconds})
    }
}

#[derive(Default)]
pub struct Tick {
    pub latched: bool,
    pub consumed: bool,
}

struct Step {
    clock: f32,
    clip: &'static str,
    time: f32,
    contacting: bool,
    root: Option<(Vec3, Quat)>,
}
enum Outcome {
    Continue,
    Complete,
}

impl Guest {
    pub(crate) const fn cancel_capture(&mut self) {
        if let Some(action) = &mut self.capture {
            action.direction = Direction::CancelRequested;
        }
    }

    #[must_use]
    pub const fn has_prey_capability(&self) -> bool {
        !self.definition.prey_targets.is_empty()
    }

    #[must_use]
    pub(crate) fn prey_key(&self) -> Option<u64> {
        self.capture.as_ref().map(|action| action.key).or_else(|| {
            self.state
                .observation
                .as_ref()
                .filter(|observation| observation.track != 1 && self.fresh())
                .map(|observation| observation.track)
        })
    }

    #[must_use]
    pub(crate) fn capture_target(&self) -> Option<&str> {
        self.capture
            .as_ref()
            .filter(|action| !action.released && !action.consumed)
            .map(|action| action.prey_id.as_str())
    }

    pub(crate) fn capture_owns_prey(&self) -> bool {
        self.capture
            .as_ref()
            .is_some_and(|action| !action.released && !action.consumed)
    }

    pub(crate) fn begin_capture(&mut self, target: &mut Feltling, key: u64) -> bool {
        if self.capture.is_some()
            || !self.enabled()
            || !self.fresh()
            || self.prey_key() != Some(key)
            || !target.capture_ready()
            || target.reserved_by.is_some()
            || self.capture_failure.is_some_and(|(failed, position)| {
                failed == key && position.distance(target.state.position) < 0.25
            })
            || self.state.transit.is_some()
            || !self.definition.prey_targets.contains(&target.state.id)
        {
            return false;
        }
        let Some(profile) = target.capture_profile() else {
            return false;
        };
        let expected = self.transform().transform_point3(profile.setup);
        if expected.distance(target.state.position) > 0.000_005
            || crate::engine::entity::angle_difference(self.state.yaw, target.state.yaw).abs()
                > 0.000_1
        {
            return false;
        }
        let from = self.animator.joint_pose();
        let Some((entry_phase, entry_blend_seconds)) = self.capture_entry_phase(profile, &from)
        else {
            return false;
        };
        self.capture = Some(Capture {
            key,
            profile,
            prey_id: target.state.id.clone(),
            start: (target.state.position, target.state.yaw),
            from,
            entry_phase,
            entry_blend_seconds,
            clock: entry_phase,
            latched: false,
            consumed: false,
            direction: Direction::Forward,
            released: false,
            blocked_seconds: 0.0,
        });
        target.reserved_by = Some(self.state.id.clone());
        target.release_pose = Some((target.state.position, target.state.yaw));
        self.state.plants = [const { None }; 3];
        self.state.speed = 0.0;
        self.voice(None, 0.0);
        self.log("capture_started", &serde_json::json!({"prey":target.state.id,"key":key,"setup_error_m":expected.distance(target.state.position),"scale":1_u8,"entry_phase":entry_phase,"entry_blend_seconds":entry_blend_seconds,"root_yaw":self.state.yaw}));
        true
    }

    /// Match the already visible head pose before contact, then overlap the
    /// complete outgoing pose with the advancing bow. No upright prelude is
    /// inserted. Every resulting substep still passes ordinary clearance.
    fn capture_entry_phase(
        &mut self,
        profile: &prey::CaptureProfile,
        from: &[JointPose],
    ) -> Option<(f32, f32)> {
        if !from.iter().all(JointPose::is_valid) {
            return None;
        }
        let (_, outgoing_rotation, outgoing_position) = self
            .animator
            .node_global("head")?
            .to_scale_rotation_translation();
        let blend_seconds = ENTRY_BLEND_SECONDS.min(profile.contact_seconds - 1.0 / 120.0);
        if blend_seconds <= 0.0 {
            return None;
        }
        let latest = (profile.contact_seconds - blend_seconds - 1.0 / 120.0).max(0.0);
        let mut best = (f32::MAX, 0.0);
        for step in 0_u8..=120 {
            let phase = latest * (f32::from(step) / 120.0);
            let _sampled = self
                .animator
                .sample_fraction(profile.capture_clip, phase / profile.capture_seconds);
            if let Some(head) = self.animator.node_global("head") {
                let (_, rotation, position) = head.to_scale_rotation_translation();
                let angle = outgoing_rotation.angle_between(rotation);
                let score =
                    0.04_f32.mul_add(angle * angle, outgoing_position.distance_squared(position));
                if score < best.0 {
                    best = (score, phase);
                }
            }
        }
        self.animator
            .restore_physical_pose(
                &self.state.clip,
                self.state.clip_time / super::duration(&self.state.clip),
                from,
            )
            .then_some((best.1, blend_seconds))
    }

    /// Before contact a moved prey invalidates the setup without being frozen.
    /// Refused candidates retain both physical poses and the action clock.
    pub(crate) fn tick_capture(
        &mut self,
        ctx: &WorldContext<'_>,
        leaves: &[DoorCollider],
        solids: &[WallAabb],
        mut target: Option<&mut Feltling>,
    ) -> Tick {
        self.state.clock += ctx.delta_seconds.clamp(0.0, 0.1);
        let mut events = Tick::default();
        let Some(mut action) = self.capture.take() else {
            return events;
        };
        self.capture_continues(&mut action, &mut target);
        if action.direction == Direction::Reverse {
            if !self.tick_capture_rollback(&mut action, ctx, leaves, solids, target.as_deref_mut())
            {
                self.capture = Some(action);
            }
            return events;
        }
        let mut left = ctx.delta_seconds.clamp(0.0, 0.1);
        for _ in 0_u8..24 {
            if left <= 0.000_001 {
                break;
            }
            let sub = action.substep(left);
            left -= sub;
            let Some(step) = self.capture_candidate(
                &action,
                action.clock + sub,
                target.as_deref_mut(),
                ctx,
                leaves,
                solids,
            ) else {
                action.blocked_seconds += ctx.delta_seconds.clamp(0.0, 0.1);
                if action.blocked_seconds >= 0.6 {
                    self.begin_capture_rollback(&mut action, "persistent physical obstruction");
                }
                break;
            };
            action.blocked_seconds = 0.0;
            match self.commit_capture_step(&mut action, &step, target.as_deref(), &mut events) {
                Outcome::Continue => {}
                Outcome::Complete => {
                    self.finish_capture();
                    return events;
                }
            }
        }
        self.capture = Some(action);
        events
    }

    fn capture_continues(&mut self, action: &mut Capture, target: &mut Option<&mut Feltling>) {
        if !action.consumed
            && !action.released
            && target.as_ref().is_none_or(|prey| !prey.enabled())
        {
            if target.is_none() {
                action.released = true;
            }
            self.begin_capture_rollback(action, "missing or disabled exact prey generation");
        }
        if !self.enabled() || action.direction == Direction::CancelRequested {
            self.begin_capture_rollback(action, "controller disabled or interrupted");
        }
        if action.direction != Direction::Reverse
            && !action.latched
            && target.as_ref().is_some_and(|prey| {
                !prey.capture_ready()
                    || prey.state.position.distance(action.start.0) > 0.000_1
                    || crate::engine::entity::angle_difference(prey.state.yaw, action.start.1).abs()
                        > 0.000_1
            })
        {
            self.log(
                "capture_missed",
                &serde_json::json!({"key":action.key,"reason":"prey moved before contact"}),
            );
            self.begin_capture_rollback(action, "prey moved before contact");
        }
        if action.direction == Direction::Reverse && !action.latched {
            if let Some(prey) = target.as_mut() {
                prey.reserved_by = None;
                prey.release_pose = None;
            }
            action.released = true;
        }
    }

    fn sample_capture(&mut self, action: &Capture, clock: f32) -> Step {
        let (clip, time) = if clock < action.profile.capture_seconds {
            (action.profile.capture_clip, clock)
        } else {
            (
                action.profile.swallow_clip,
                (clock - action.profile.capture_seconds).min(action.profile.swallow_seconds),
            )
        };
        let seconds = if clip == action.profile.capture_clip {
            action.profile.capture_seconds
        } else {
            action.profile.swallow_seconds
        };
        let _sampled = self.animator.sample_fraction(clip, time / seconds);
        if clock < action.entry_phase + action.entry_blend_seconds {
            let weight = prey::smooth((clock - action.entry_phase) / action.entry_blend_seconds);
            let mut pose = self.animator.joint_pose();
            for (joint, from) in pose.iter_mut().zip(&action.from) {
                joint.translation = from.translation.lerp(joint.translation, weight);
                joint.rotation = from.rotation.slerp(joint.rotation, weight);
                joint.scale = from.scale.lerp(joint.scale, weight);
            }
            let _restored = self
                .animator
                .restore_physical_pose(clip, time / seconds, &pose);
        }
        self.fill_vertices();
        let contacting =
            clip == action.profile.capture_clip && time >= action.profile.contact_seconds;
        let root = if !action.consumed && !action.released && (action.latched || contacting) {
            if action.direction == Direction::Reverse && clock <= action.profile.contact_seconds {
                Some((action.start.0, Quat::from_rotation_y(action.start.1)))
            } else {
                self.socket(action.profile.socket).map(|socket| {
                    action.profile.attached_pose(
                        socket,
                        (clip == action.profile.swallow_clip).then_some(time),
                    )
                })
            }
        } else {
            None
        };
        Step {
            clock,
            clip,
            time,
            contacting,
            root,
        }
    }

    fn capture_candidate(
        &mut self,
        action: &Capture,
        clock: f32,
        mut target: Option<&mut Feltling>,
        ctx: &WorldContext<'_>,
        leaves: &[DoorCollider],
        solids: &[WallAabb],
    ) -> Option<Step> {
        let physical = self.animator.joint_pose();
        let old_root = target.as_ref().map(|prey| prey.root_pose());
        let step = self.sample_capture(action, clock);
        let mut accepted = self.pose_clear(ctx, leaves, solids);
        if step.contacting && !action.latched {
            accepted &= target.as_ref().zip(step.root).is_some_and(|(prey, root)| {
                root.0.distance(prey.state.position) <= 0.000_02
                    && root.1.dot(Quat::from_rotation_y(prey.state.yaw)).abs() >= 0.999_999
                    && self.lip_contact(
                        action.profile,
                        &prey.physical_vertices(),
                        &prey.asset.model.indices,
                    )
            });
        }
        if let (Some(prey), Some(root)) = (target.as_mut(), step.root)
            && accepted
        {
            prey.attach(root);
            accepted = body_clear(
                &prey.physical_vertices(),
                &prey.asset.model.indices,
                ctx,
                leaves,
                solids,
                &self.ceiling,
                self.state.position.y,
            );
        }
        if accepted
            && step.clip == action.profile.swallow_clip
            && step.time >= action.profile.consume_seconds
            && !action.consumed
        {
            accepted = target
                .as_ref()
                .is_some_and(|prey| self.prey_inside(action.profile, &prey.physical_vertices()));
        }
        if accepted {
            return Some(step);
        }
        let _restored = self.animator.restore_physical_pose(
            &self.state.clip,
            self.state.clip_time / super::duration(&self.state.clip),
            &physical,
        );
        self.fill_vertices();
        if let (Some(prey), Some(root)) = (target.as_mut(), old_root)
            && !action.released
        {
            if action.latched {
                prey.attach(root);
            } else {
                prey.attachment = None;
                prey.state.position = root.0;
            }
        }
        self.log("capture_candidate_refused",&serde_json::json!({"clip":step.clip,"seconds":step.time,"contact":step.contacting,"latched":action.latched}));
        None
    }

    fn commit_capture_step(
        &mut self,
        action: &mut Capture,
        step: &Step,
        target: Option<&Feltling>,
        events: &mut Tick,
    ) -> Outcome {
        action.clock = step.clock;
        self.accept_capture_pose(step);
        if step.contacting && !action.latched {
            action.latched = true;
            events.latched = true;
            self.log("mouth_latched",&serde_json::json!({"prey":action.prey_id,"key":action.key,"clip_seconds":step.time,"root":step.root,"scale":1_u8}));
        }
        if step.clip == action.profile.swallow_clip
            && step.time >= action.profile.consume_seconds
            && !action.consumed
        {
            if target.is_some() {
                action.consumed = true;
                events.consumed = true;
                self.log("prey_consumed",&serde_json::json!({"prey":action.prey_id,"key":action.key,"clip_seconds":step.time,"all_vertices_inside":true}));
            } else {
                self.log(
                    "consume_refused",
                    &serde_json::json!({"key":action.key,"reason":"whole-prey containment failed"}),
                );
                return Outcome::Continue;
            }
        }
        if step.clip == action.profile.swallow_clip && step.time >= action.profile.swallow_seconds {
            self.log(
                "capture_recovered",
                &serde_json::json!({"key":action.key,"consumed":action.consumed}),
            );
            Outcome::Complete
        } else {
            Outcome::Continue
        }
    }

    fn accept_capture_pose(&mut self, step: &Step) {
        self.state.clip = step.clip.into();
        self.state.clip_time = step.time;
        self.state.previous = step.clip.into();
        self.state.previous_time = step.time;
        self.state.blend_time = super::BLEND;
        self.state.aim_weight = 0.0;
        self.state.base_posture = 0.0;
    }

    fn finish_capture(&mut self) {
        self.state.observation = None;
        self.state.pending.clear();
        self.state.goal = None;
        self.state.speed = 0.0;
        self.state.aim_weight = 0.0;
        self.state.base_posture = 0.0;
        if self.enabled() {
            self.set_intent(Intent::Listen, "capture completed or interrupted");
            self.set_clip("idle_listen");
        }
        self.state.next_pulse = self.state.clock + 0.7;
    }

    /// The author preserves a ~0.315 mm surface gap. A 0.5 mm contact slop
    /// tests both full triangle surfaces, never a vertex-only approximation.
    fn lip_contact(
        &self,
        profile: &prey::CaptureProfile,
        prey_vertices: &[Vec3],
        prey_indices: &[u16],
    ) -> bool {
        let Some(socket) = self.socket(profile.socket) else {
            return false;
        };
        let inverse = socket.inverse();
        let Some(submesh) = self
            .asset
            .model
            .submeshes
            .iter()
            .find(|mesh| mesh.material == profile.lumen_material)
        else {
            return false;
        };
        let start = usize::try_from(submesh.first_index).unwrap_or(usize::MAX);
        let end = start.saturating_add(usize::try_from(submesh.index_count).unwrap_or(0));
        let crown = prey_vertices
            .iter()
            .map(|p| p.y)
            .fold(f32::NEG_INFINITY, f32::max);
        self.asset
            .model
            .indices
            .get(start..end)
            .unwrap_or(&[])
            .as_chunks::<3>()
            .0
            .iter()
            .any(|face| {
                let points = face.map(|i| {
                    self.vertices
                        .get(usize::from(i))
                        .copied()
                        .unwrap_or(Vec3::ZERO)
                });
                if points
                    .iter()
                    .all(|point| inverse.transform_point3(*point).z.abs() > 0.055)
                {
                    return false;
                }
                prey_indices.as_chunks::<3>().0.iter().any(|prey_face| {
                    let skin = prey_face.map(|i| {
                        prey_vertices
                            .get(usize::from(i))
                            .copied()
                            .unwrap_or(Vec3::ZERO)
                    });
                    skin.iter()
                        .any(|point| point.y >= crown - profile.crown_depth)
                        && triangles_touch(points, skin, 0.000_5)
                })
            })
    }

    /// Actual lining triangles plus the current opening cap, not root/bounds
    /// containment. Every full-size posed prey vertex must have odd winding.
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Bounded exact mesh ray parity and quantized source-seam welding."
    )]
    fn prey_inside(&self, profile: &prey::CaptureProfile, prey: &[Vec3]) -> bool {
        use std::collections::BTreeMap;
        let Some(submesh) = self
            .asset
            .model
            .submeshes
            .iter()
            .find(|mesh| mesh.material == profile.lumen_material)
        else {
            return false;
        };
        let start = usize::try_from(submesh.first_index).unwrap_or(usize::MAX);
        let end = start.saturating_add(usize::try_from(submesh.index_count).unwrap_or(0));
        let mut welded = BTreeMap::new();
        let mut edges: BTreeMap<(usize, usize), usize> = BTreeMap::new();
        let mut faces = Vec::new();
        for face in self
            .asset
            .model
            .indices
            .get(start..end)
            .unwrap_or(&[])
            .as_chunks::<3>()
            .0
        {
            // The certified lumen starts at pink UV row 2. Rows 0/1 form
            // the folded lip exterior and are not a closed cavity boundary.
            if face.iter().any(|index| {
                self.asset
                    .model
                    .vertices
                    .get(usize::from(*index))
                    .is_none_or(|v| {
                        ((profile.uv_row_origin - v.uv[1]) / profile.uv_row_step).round()
                            < profile.lumen_first_row
                    })
            }) {
                continue;
            }
            let mut slots = [0; 3];
            for (slot, index) in slots.iter_mut().zip(face) {
                let vertex_index = usize::from(*index);
                let Some(vertex) = self
                    .asset
                    .model
                    .skin
                    .as_ref()
                    .and_then(|skin| skin.source_vertices.get(vertex_index))
                else {
                    return false;
                };
                let key = vertex.position.map(|v| {
                    let value = (f64::from(v) * 1.0e6).round_ties_even();
                    if value == 0.0_f64 {
                        0.0_f64.to_bits()
                    } else {
                        value.to_bits()
                    }
                });
                *slot = *welded.entry(key).or_insert(vertex_index);
            }
            for (a, b) in [
                (slots[0], slots[1]),
                (slots[1], slots[2]),
                (slots[2], slots[0]),
            ] {
                let key = if a < b { (a, b) } else { (b, a) };
                *edges.entry(key).or_default() += 1;
            }
            faces.push(face.map(|i| {
                self.vertices
                    .get(usize::from(i))
                    .copied()
                    .unwrap_or(Vec3::ZERO)
            }));
        }
        if !close_lumen(&mut faces, &edges, &self.vertices) {
            return false;
        }
        let direction = Vec3::new(0.371_391, 0.219_173, 0.902_139).normalize();
        prey.iter().all(|point| {
            let mut hits: Vec<f32> = faces
                .iter()
                .filter_map(|face| ray_triangle(*point, direction, *face))
                .collect();
            hits.sort_by(f32::total_cmp);
            hits.dedup_by(|a, b| (*a - *b).abs() < 0.000_002);
            hits.len() % 2 == 1
        })
    }
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "Finite triangle closest-point reference using clamped barycentric coordinates."
)]
fn point_triangle_distance_squared(point: Vec3, [a, b, c]: [Vec3; 3]) -> f32 {
    let ab = b - a;
    let ac = c - a;
    let ap = point - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return ap.length_squared();
    }
    let bp = point - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return bp.length_squared();
    }
    let vc = d3.mul_add(-d2, d1 * d4);
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return point.distance_squared(a + ab * (d1 / (d1 - d3)));
    }
    let cp = point - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return cp.length_squared();
    }
    let vb = d1.mul_add(-d6, d5 * d2);
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return point.distance_squared(a + ac * (d2 / (d2 - d6)));
    }
    let va = d5.mul_add(-d4, d3 * d6);
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        return point.distance_squared(b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6))));
    }
    let denominator = (va + vb + vc).recip();
    point.distance_squared(a + ab * (vb * denominator) + ac * (vc * denominator))
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "Bounded Moller-Trumbore ray intersection; singular and behind-origin intersections are rejected."
)]
fn ray_triangle(origin: Vec3, direction: Vec3, [first, second, third]: [Vec3; 3]) -> Option<f32> {
    let edge_one = second - first;
    let edge_two = third - first;
    let perpendicular = direction.cross(edge_two);
    let determinant = edge_one.dot(perpendicular);
    if determinant.abs() < 1.0e-10 {
        return None;
    }
    let inverse = determinant.recip();
    let displacement = origin - first;
    let coordinate_u = inverse * displacement.dot(perpendicular);
    if !(0.0..=1.0).contains(&coordinate_u) {
        return None;
    }
    let crossed = displacement.cross(edge_one);
    let coordinate_v = inverse * direction.dot(crossed);
    if coordinate_v < 0.0 || coordinate_u + coordinate_v > 1.0 {
        return None;
    }
    let time = inverse * edge_two.dot(crossed);
    (time > 1.0e-7).then_some(time)
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "Finite whole-prey triangle/box SAT in world and oriented leaf coordinates."
)]
fn body_clear(
    vertices: &[Vec3],
    indices: &[u16],
    ctx: &WorldContext<'_>,
    leaves: &[DoorCollider],
    solids: &[WallAabb],
    ceiling: &crate::engine::level::WalkableCeiling,
    floor_height: f32,
) -> bool {
    if vertices.iter().any(|point| {
        ceiling
            .ceiling_above_disc(point.x, point.z, 0.002, floor_height)
            .is_some_and(|height| point.y + 0.03 >= height)
            || ctx
                .floor
                .lowest_room_underside(glam::Vec2::new(point.x, point.z), 0.002, floor_height)
                .is_some_and(|height| point.y + 0.03 >= height)
    }) {
        return false;
    }
    if vertices.iter().any(|point| {
        ctx.floor
            .height_below(point.x, point.z, point.y + 0.005)
            .is_some_and(|floor| point.y + 0.002 < floor)
    }) {
        return false;
    }
    let body_min = vertices
        .iter()
        .copied()
        .fold(Vec3::splat(f32::INFINITY), Vec3::min);
    let body_max = vertices
        .iter()
        .copied()
        .fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
    let player = WallAabb::with_y(
        ctx.feet.x - ctx.body_half,
        ctx.feet.y,
        ctx.feet.z - ctx.body_half,
        ctx.body_half * 2.0,
        ctx.body_height,
        ctx.body_half * 2.0,
    );
    for wall in ctx
        .walls
        .iter()
        .chain(solids)
        .chain(std::iter::once(&player))
    {
        let lo = Vec3::new(wall.min_x, wall.min_y, wall.min_z);
        let hi = Vec3::new(wall.max_x, wall.max_y, wall.max_z);
        if body_min.cmpgt(hi).any() || body_max.cmplt(lo).any() {
            continue;
        }
        if indices.as_chunks::<3>().0.iter().any(|face| {
            super::triangle_box(
                face.map(|i| vertices.get(usize::from(i)).copied().unwrap_or(Vec3::ZERO)),
                lo,
                hi,
            )
        }) {
            return false;
        }
    }
    for leaf in leaves {
        let origin = Vec3::new(leaf.hinge_x, leaf.hinge_y, leaf.hinge_z);
        let x = Vec3::new(leaf.dir_x, 0.0, leaf.dir_z);
        let z = Vec3::new(-leaf.dir_z, 0.0, leaf.dir_x);
        if indices.as_chunks::<3>().0.iter().any(|face| {
            let points = face.map(|i| {
                let p = vertices.get(usize::from(i)).copied().unwrap_or(Vec3::ZERO) - origin;
                Vec3::new(p.dot(x), p.y, p.dot(z))
            });
            super::triangle_box(
                points,
                Vec3::new(0.0, 0.0, -leaf.thickness * 0.5),
                Vec3::new(leaf.width, leaf.height, leaf.thickness * 0.5),
            )
        }) {
            return false;
        }
    }
    true
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "Bounded finite whole-triangle contact and segment-distance arithmetic."
)]
fn triangles_touch(first: [Vec3; 3], second: [Vec3; 3], tolerance: f32) -> bool {
    let first_min = first
        .into_iter()
        .fold(Vec3::splat(f32::INFINITY), Vec3::min)
        - Vec3::splat(tolerance);
    let first_max = first
        .into_iter()
        .fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max)
        + Vec3::splat(tolerance);
    let second_min = second
        .into_iter()
        .fold(Vec3::splat(f32::INFINITY), Vec3::min);
    let second_max = second
        .into_iter()
        .fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
    if first_min.cmpgt(second_max).any() || first_max.cmplt(second_min).any() {
        return false;
    }
    let limit = tolerance * tolerance;
    if first
        .iter()
        .any(|point| point_triangle_distance_squared(*point, second) <= limit)
        || second
            .iter()
            .any(|point| point_triangle_distance_squared(*point, first) <= limit)
    {
        return true;
    }
    let [one, two, three] = first;
    let [four, five, six] = second;
    [[one, two], [two, three], [three, one]].iter().any(|edge| {
        [[four, five], [five, six], [six, four]]
            .iter()
            .any(|other| segment_distance_squared(*edge, *other) <= limit)
    })
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "Finite segment projections handle short/parallel edges before division and clamp both parameters."
)]
fn segment_distance_squared([first, second]: [Vec3; 2], [third, fourth]: [Vec3; 2]) -> f32 {
    let edge_one = second - first;
    let edge_two = fourth - third;
    let offset = first - third;
    let length_one = edge_one.length_squared();
    let length_two = edge_two.length_squared();
    if length_one < 1.0e-20 || length_two < 1.0e-20 {
        return first.distance_squared(third);
    }
    let product = edge_one.dot(edge_two);
    let along_one = edge_one.dot(offset);
    let along_two = edge_two.dot(offset);
    let denominator = length_one.mul_add(length_two, -product * product);
    let mut fraction_one = if denominator > 1.0e-20 {
        product.mul_add(along_two, -length_two * along_one) / denominator
    } else {
        0.0
    }
    .clamp(0.0, 1.0);
    let mut fraction_two = product.mul_add(fraction_one, along_two) / length_two;
    if fraction_two < 0.0 {
        fraction_two = 0.0;
        fraction_one = (-along_one / length_one).clamp(0.0, 1.0);
    } else if fraction_two > 1.0 {
        fraction_two = 1.0;
        fraction_one = ((product - along_one) / length_one).clamp(0.0, 1.0);
    }
    (first + edge_one * fraction_one).distance_squared(third + edge_two * fraction_two)
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "Finite certified lumen opening centroid and manifold topology counts."
)]
fn close_lumen(
    faces: &mut Vec<[Vec3; 3]>,
    edges: &std::collections::BTreeMap<(usize, usize), usize>,
    vertices: &[Vec3],
) -> bool {
    use std::collections::{BTreeMap, BTreeSet};
    let boundary: Vec<_> = edges
        .iter()
        .filter(|(_, count)| **count == 1)
        .map(|(edge, _)| *edge)
        .collect();
    let mut neighbours: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for &(first, second) in &boundary {
        neighbours.entry(first).or_default().push(second);
        neighbours.entry(second).or_default().push(first);
    }
    if neighbours.is_empty()
        || neighbours.values().any(|n| n.len() != 2)
        || edges.values().any(|n| *n > 2)
    {
        return false;
    }
    let start_vertex = *neighbours.keys().next().unwrap_or(&usize::MAX);
    let mut visited = BTreeSet::new();
    let mut pending = vec![start_vertex];
    while let Some(vertex) = pending.pop() {
        if visited.insert(vertex) {
            pending.extend(neighbours.get(&vertex).into_iter().flatten().copied());
        }
    }
    if visited.len() != neighbours.len() {
        return false;
    }
    let centre = neighbours
        .keys()
        .filter_map(|i| vertices.get(*i))
        .copied()
        .sum::<Vec3>()
        / f32::from(u16::try_from(neighbours.len()).unwrap_or(u16::MAX));
    for (first, second) in boundary {
        let (Some(a), Some(b)) = (vertices.get(first), vertices.get(second)) else {
            return false;
        };
        faces.push([*a, *b, centre]);
    }
    true
}
