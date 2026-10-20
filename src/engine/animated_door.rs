//! Reversible rigid mechanisms sampled from their delivered animation clip.
//! Gameplay publishes exact clip time and oriented boxes from the same rig.

use glam::{Mat4, Vec3};

use crate::engine::collision::DoorCollider;
use crate::engine::entity::PoseCue;
use crate::engine::gltf::PropModel;
use crate::engine::level::{AnimatedDoorDef, PropDef};
use crate::engine::render::CharacterAnimator;

/// Resolve only explicitly authored mechanisms; ordinary levels perform no
/// model I/O here. Package dependency validation owns the installed bytes.
/// # Errors
/// Rejects missing assets, clips, nodes or non-upright rigid transforms.
pub fn load(level: &crate::engine::level::LevelDef) -> Result<Vec<AnimatedDoor>, String> {
    if !level.props.iter().any(|prop| prop.animated_door.is_some()) {
        return Ok(Vec::new());
    }
    let catalog = crate::engine::loader::PropCatalog::load_default();
    let mut assets = crate::engine::props::PropAssets::load_default();
    let surfaces = crate::engine::level::LevelSurfaces::new(level);
    let mut doors = Vec::new();
    for (prop, id) in level.props.iter().zip(level.prop_instance_ids()) {
        if prop.animated_door.is_none() {
            continue;
        }
        let path = catalog
            .get(&prop.model)
            .model
            .ok_or_else(|| format!("{id}: missing animated door model"))?;
        let asset = assets.resolve(&path)?;
        let base_y = surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0) + prop.y;
        doors.push(AnimatedDoor::new(id, prop, base_y, &asset.model)?);
    }
    Ok(doors)
}

pub struct AnimatedDoor {
    pub id: String,
    def: AnimatedDoorDef,
    animator: CharacterAnimator,
    transform: Mat4,
    duration: f32,
    progress: f32,
    target: f32,
    pub obstructed: bool,
    colliders: Vec<DoorCollider>,
}

impl AnimatedDoor {
    /// Resolve every named box and prove the clip stays upright at bounded
    /// samples. Invalid rigs cannot silently become a visual-only door.
    /// # Errors
    /// Rejects a missing rigid clip, named box or upright rigid transform.
    pub fn new(id: String, prop: &PropDef, base_y: f32, model: &PropModel) -> Result<Self, String> {
        let def = prop.animated_door.clone().ok_or("missing animated door")?;
        if model.skin.is_some() {
            return Err("animated doors require rigid nodes".into());
        }
        let animator = CharacterAnimator::new(model).ok_or("model has no rigid animation")?;
        let duration = animator
            .clip_duration(&def.clip)
            .filter(|value| *value > 0.0)
            .ok_or("animated door clip is missing or has zero duration")?;
        let progress = if def.initial_open { 1.0 } else { 0.0 };
        let mut door = Self {
            id,
            def,
            animator,
            duration,
            progress,
            target: progress,
            transform: Mat4::from_scale_rotation_translation(
                Vec3::splat(prop.scale),
                glam::Quat::from_rotation_y(prop.rotation_degrees.to_radians()),
                Vec3::new(prop.x, base_y, prop.z),
            ),
            obstructed: false,
            colliders: Vec::new(),
        };
        for sample in 0_u16..=280 {
            let _changed = door
                .animator
                .sample_fraction(&door.def.clip, f32::from(sample) / 280.0);
            for shape in &door.def.boxes {
                let pose = door
                    .animator
                    .node_global(&shape.node)
                    .ok_or_else(|| format!("missing collider node {}", shape.node))?;
                let up = pose.transform_vector3(Vec3::Y);
                let x = pose.transform_vector3(Vec3::X);
                let z = pose.transform_vector3(Vec3::Z);
                if Vec3::new(up.x, up.y - 1.0, up.z).length() > 1.0e-4
                    || x.y.abs() > 1.0e-4
                    || z.y.abs() > 1.0e-4
                    || (x.length() - 1.0).abs() > 1.0e-4
                    || (z.length() - 1.0).abs() > 1.0e-4
                {
                    return Err("animated door boxes require rigid upright node transforms".into());
                }
            }
        }
        door.colliders = door.sample(progress);
        Ok(door)
    }

    fn sample(&mut self, progress: f32) -> Vec<DoorCollider> {
        let _changed = self.animator.sample_fraction(&self.def.clip, progress);
        self.def
            .boxes
            .iter()
            .filter_map(|shape| {
                let matrix = self
                    .transform
                    .mul_mat4(&self.animator.node_global(&shape.node)?);
                let x = matrix.transform_vector3(Vec3::X);
                let scale = x.length();
                let direction = Vec3::new(x.x / scale, x.y / scale, x.z / scale);
                let center = matrix.transform_point3(Vec3::from(shape.center));
                let [sx, sy, sz] = shape.size;
                let size = Vec3::new(sx * scale, sy * scale, sz * scale);
                let hinge = Vec3::new(
                    (-direction.x).mul_add(size.x * 0.5, center.x),
                    size.y.mul_add(-0.5, center.y),
                    (-direction.z).mul_add(size.x * 0.5, center.z),
                );
                Some(DoorCollider::from_pose(
                    hinge.into(),
                    [direction.x, direction.z],
                    size.x,
                    size.z,
                    size.y,
                ))
            })
            .collect()
    }

    pub fn request(&mut self, clip: &str, toggle: bool) -> bool {
        if clip != self.def.clip {
            return false;
        }
        self.target = if toggle && self.target > 0.5 {
            0.0
        } else {
            1.0
        };
        true
    }

    pub fn reset(&mut self) {
        self.progress = if self.def.initial_open { 1.0 } else { 0.0 };
        self.target = self.progress;
        self.obstructed = false;
        self.colliders = self.sample(self.progress);
    }

    #[must_use]
    pub fn cue(&self) -> PoseCue {
        PoseCue::Sample {
            name: self.def.clip.clone(),
            progress: self.progress,
        }
    }

    #[must_use]
    pub fn colliders(&self) -> &[DoorCollider] {
        &self.colliders
    }

    #[must_use]
    pub fn is_moving(&self) -> bool {
        (self.target - self.progress).abs() > 1.0e-6
    }

    /// Bounded explicit diagnostic, never emitted by ordinary frame updates.
    #[must_use]
    pub fn diagnostics(&self) -> serde_json::Value {
        serde_json::json!({"id":self.id,"clip":self.def.clip,"progress":self.progress,
            "target":self.target,"obstructed":self.obstructed,
            "boxes":self.colliders.iter().map(|leaf| serde_json::json!({
                "hinge":[leaf.hinge_x,leaf.hinge_y,leaf.hinge_z],
                "direction":[leaf.dir_x,leaf.dir_z],"size":[leaf.width,leaf.height,leaf.thickness]
            })).collect::<Vec<_>>()})
    }

    /// At most 5 ms of clip time per prospective probe. A hitch consumes at
    /// most 100 ms rather than skipping solid poses. Stop at an obstruction;
    /// retargeting always starts at the last accepted fraction.
    pub fn advance(&mut self, delta: f32, mut blocked: impl FnMut(&DoorCollider) -> bool) -> bool {
        if !delta.is_finite() || delta <= 0.0 {
            return false;
        }
        let intended = (self.target - self.progress)
            .abs()
            .min(delta.min(0.1) / self.duration);
        let direction = (self.target - self.progress).signum();
        let mut remaining = intended;
        let mut moved = false;
        for _ in 0_u8..32 {
            if remaining <= 1.0e-7 {
                break;
            }
            let step = remaining.min(0.005 / self.duration);
            let candidate = direction.mul_add(step, self.progress).clamp(0.0, 1.0);
            let shapes = self.sample(candidate);
            if shapes
                .iter()
                .zip(&self.colliders)
                .any(|(next, current)| next != current && blocked(next))
            {
                self.obstructed = true;
                // The sampler is scratch; the accepted collider/visual fraction
                // remains unchanged until the entire mechanism can move.
                return moved;
            }
            self.progress = candidate;
            self.colliders = shapes;
            self.obstructed = false;
            moved = true;
            remaining -= step;
        }
        moved
    }
}
