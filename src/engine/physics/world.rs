//! Bounded solver ownership, snapshots and interpolated rendering poses.

use std::collections::{BTreeMap, VecDeque};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;
use std::time::Instant;

use glam::{Quat, Vec3};
use rapier3d::parry::query::contact as shape_contact;
use rapier3d::parry::utils::PoseOpt;
use rapier3d::prelude::{
    ColliderBuilder, ColliderHandle, ContactPair, IntegrationParameters, PhysicsWorld,
    RigidBodyBuilder, RigidBodyHandle, SharedShape, SpringCoefficients, Vector,
};
use serde::{Serialize, Serializer};

use crate::engine::collision::{DoorCollider, PLAYER_RADIUS, WallAabb};
use crate::engine::level::{LevelDef, WalkableCeiling, WalkableFloor, WaterVolumes};
use crate::engine::materials::MaterialTable;

use super::activation::{WakeSweep, WakeTarget};
use super::collision::{
    add, door_pose, install_static, mass_properties, model_shape, point, pose, quaternion,
    query_filter, refresh_collider, scaled, subtract, vector,
};
use super::config::{BodyMotion, PhysicsDef, validate_reset_group};
use super::contacts::ContactOnsets;
use super::hold::HeldItem;
use super::recovery::{RecoverySpace, RecoveryState, RecoveryTracker};
use super::surfaces::{FloorContactHooks, FloorSurfaces};
use super::water::{WaterBody, WaterWork};
use super::{FIXED_STEP, GRAVITY, MAX_BODIES};

const MAX_SUBSTEPS: usize = 12;
const MAX_FRAME_SECONDS: f32 = 0.1;
const MAX_IMPACTS: usize = 32;
const MAX_CONTACT_PAIRS: usize = 256;
const MAX_FIXED_PROP_SHAPES: usize = 1024;
#[cfg(all(feature = "parallel-physics", not(target_arch = "wasm32")))]
const MIN_PARALLEL_SOLVER_BODIES: usize = 128;

#[cfg(all(feature = "parallel-physics", not(target_arch = "wasm32")))]
#[derive(Default)]
struct SolverPoolPolicy {
    parallel: bool,
    quiet_seconds: f32,
}

#[cfg(all(feature = "parallel-physics", not(target_arch = "wasm32")))]
impl SolverPoolPolicy {
    fn update(&mut self, active: usize, delta: f32) -> bool {
        if active >= MIN_PARALLEL_SOLVER_BODIES {
            self.quiet_seconds = 0.0;
            self.parallel = true;
        } else if active < 112 {
            self.quiet_seconds = (self.quiet_seconds + delta.min(MAX_FRAME_SECONDS)).min(0.25);
            if self.quiet_seconds >= 0.25 {
                self.parallel = false;
            }
        } else {
            self.quiet_seconds = 0.0;
        }
        self.parallel
    }
}

struct FixedPropShape {
    shape: SharedShape,
    position: rapier3d::prelude::Pose,
    friction: f32,
    restitution: f32,
}

impl FixedPropShape {
    fn install(&self, world: &mut PhysicsWorld) {
        let handle = world.insert_collider(
            ColliderBuilder::new(self.shape.clone())
                .position(self.position)
                .friction(self.friction)
                .restitution(self.restitution),
            None,
        );
        refresh_collider(world, handle);
    }
}

/// World transform of the original model origin, with unrestricted orientation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ItemPose {
    #[serde(serialize_with = "serialize_vector")]
    pub position: Vec3,
    #[serde(serialize_with = "serialize_quaternion")]
    pub rotation: Quat,
}

impl ItemPose {
    pub(super) fn interpolate(self, next: Self, alpha: f32, center: Vec3) -> Self {
        let previous_center = add(self.position, self.rotation.mul_vec3(center));
        let next_center = add(next.position, next.rotation.mul_vec3(center));
        let rotation = self.rotation.slerp(next.rotation, alpha);
        Self {
            position: subtract(
                previous_center.lerp(next_center, alpha),
                rotation.mul_vec3(center),
            ),
            rotation,
        }
    }
}

/// Controller output mirrored to physics; the solver never changes the player.
#[derive(Debug, Clone, Copy)]
pub struct PlayerContext {
    pub eye: Vec3,
    pub feet: Vec3,
    pub height: f32,
    pub view: Vec3,
    pub yaw: f32,
    /// False for a noclip player; rigid items continue simulating.
    pub active: bool,
}

/// A diagnostic snapshot suitable for developer traces and native acceptance.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyState {
    Fixed,
    Dormant,
    Held,
    Sleeping,
    Active,
    Kinematic,
    Suspended,
}

/// A diagnostic snapshot suitable for developer traces and native acceptance.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct BodySnapshot {
    pub key: u64,
    pub pose: ItemPose,
    /// Exact interpolated model transform published to the renderer this frame.
    pub render_pose: ItemPose,
    #[serde(serialize_with = "serialize_vector")]
    pub linear_velocity: Vec3,
    #[serde(serialize_with = "serialize_vector")]
    pub angular_velocity: Vec3,
    pub sleeping: bool,
    /// Authored pose retained immobile until an explicit or physical activation.
    pub dormant: bool,
    pub state: BodyState,
    pub held: bool,
    pub motion: BodyMotion,
    pub mass_kg: f32,
    pub solver_mass_kg: f32,
    #[serde(serialize_with = "serialize_vector")]
    pub effective_mass_kg: Vec3,
    #[serde(serialize_with = "serialize_vector")]
    pub local_center_of_mass: Vec3,
    #[serde(serialize_with = "serialize_vector")]
    pub world_center_of_mass: Vec3,
    #[serde(serialize_with = "serialize_vector")]
    pub principal_inertia: Vec3,
    pub friction: f32,
    pub restitution: f32,
    /// Full-submersion physical displacement, including a scaled profile override.
    pub displaced_volume_m3: f32,
    /// Most recently integrated fraction inside enabled swimming water.
    pub submerged_fraction: f32,
}

/// Most recent frame's bounded solver work.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct PhysicsStats {
    pub fixed_step_seconds: f32,
    pub max_substeps: usize,
    pub body_count: usize,
    pub active_bodies: usize,
    pub sleeping_bodies: usize,
    pub dormant_bodies: usize,
    pub substeps: usize,
    pub clamped_seconds: f32,
    pub step_millis: f64,
    pub static_colliders: usize,
    pub recovered_bodies: usize,
    pub suspended_bodies: usize,
}

impl Default for PhysicsStats {
    fn default() -> Self {
        Self {
            fixed_step_seconds: FIXED_STEP,
            max_substeps: MAX_SUBSTEPS,
            body_count: 0,
            active_bodies: 0,
            sleeping_bodies: 0,
            dormant_bodies: 0,
            substeps: 0,
            clamped_seconds: 0.0,
            step_millis: 0.0,
            static_colliders: 0,
            recovered_bodies: 0,
            suspended_bodies: 0,
        }
    }
}

/// Contact onset seam for a later sound layer. The queue holds at most 32 events.
#[derive(Debug, Clone, Copy)]
pub struct ImpactEvent {
    pub key: u64,
    pub other_key: Option<u64>,
    pub position: Vec3,
    pub impulse: f32,
}

/// Positive solved item-pair impulse, maximized across this frame's substeps.
/// Speculative geometry with zero solver impulse is excluded.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct ContactImpulse {
    pub pair: [u64; 2],
    pub impulse: f32,
}

/// Result of an atomic reset of existing group members; no bodies are spawned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ResetReport {
    pub reset_count: usize,
    pub released_held: Option<u64>,
}

pub(super) struct BodyEntry {
    pub body: RigidBodyHandle,
    pub collider: ColliderHandle,
    pub motion: BodyMotion,
    pub pickup: bool,
    pub authored: bool,
    pub dormant: bool,
    pub center: Vec3,
    pub interpolation_center: Vec3,
    pub radius: f32,
    pub shape: SharedShape,
    pub authored_pose: ItemPose,
    pub reset_group: Option<Box<str>>,
    pub previous: ItemPose,
    pub current: ItemPose,
    pub recovery: RecoveryTracker,
    pub water: Option<WaterBody>,
}

impl BodyEntry {
    fn render_pose(&self, alpha: f32, held: bool) -> ItemPose {
        if self.dormant {
            self.authored_pose
        } else if held {
            self.current
        } else {
            self.previous
                .interpolate(self.current, alpha, self.interpolation_center)
        }
    }
}

pub(super) struct Runtime {
    pub world: PhysicsWorld,
    floor_contacts: FloorContactHooks,
    pub entries: BTreeMap<u64, BodyEntry>,
    pub held: Option<HeldItem>,
    pub player: RigidBodyHandle,
    pub player_collider: ColliderHandle,
    player_height: f32,
    doors: Vec<RigidBodyHandle>,
    pub player_velocity: Vec3,
    previous_player: Option<Vec3>,
    pub last_player: Option<PlayerContext>,
    static_count: usize,
    impacts: VecDeque<ImpactEvent>,
    contact_onsets: ContactOnsets,
    frame_contacts: Vec<[u64; 2]>,
    frame_contact_impulses: Vec<ContactImpulse>,
    pub wake_sweeps: Vec<WakeSweep>,
    pub wake_targets: Vec<WakeTarget>,
    pub recovery: RecoveryState,
    pub water_work: Vec<WaterWork>,
    #[cfg(not(target_arch = "wasm32"))]
    pub worker_pool: Option<Arc<rayon::ThreadPool>>,
    #[cfg(all(feature = "parallel-physics", not(target_arch = "wasm32")))]
    serial_solver_pool: Arc<rayon::ThreadPool>,
    #[cfg(all(feature = "parallel-physics", not(target_arch = "wasm32")))]
    solver_pool_policy: SolverPoolPolicy,
}

/// A level's reusable item simulation. Rapier and static shapes are allocated
/// on the first spawn, so ordinary levels retain an empty runtime.
pub struct ItemWorld {
    walls: Vec<WallAabb>,
    floor: WalkableFloor,
    surfaces: FloorSurfaces,
    ceiling: WalkableCeiling,
    water: WaterVolumes,
    fixed_props: Vec<FixedPropShape>,
    pub(super) recovery_space: RecoverySpace,
    pub(super) runtime: Option<Box<Runtime>>,
    pending_player: Option<PlayerContext>,
    pending_doors: Vec<DoorCollider>,
    accumulator: f32,
    pub(super) stats: PhysicsStats,
    #[cfg(not(target_arch = "wasm32"))]
    worker_pool: Option<Arc<rayon::ThreadPool>>,
}

impl Default for ItemWorld {
    fn default() -> Self {
        Self {
            walls: Vec::new(),
            floor: WalkableFloor::default(),
            surfaces: FloorSurfaces::default(),
            ceiling: WalkableCeiling::default(),
            water: WaterVolumes::default(),
            fixed_props: Vec::new(),
            recovery_space: RecoverySpace::default(),
            runtime: None,
            pending_player: None,
            pending_doors: Vec::new(),
            accumulator: 0.0,
            stats: PhysicsStats::default(),
            #[cfg(not(target_arch = "wasm32"))]
            worker_pool: None,
        }
    }
}

fn serialize_vector<S: Serializer>(value: &Vec3, serializer: S) -> Result<S::Ok, S::Error> {
    value.to_array().serialize(serializer)
}

fn serialize_quaternion<S: Serializer>(value: &Quat, serializer: S) -> Result<S::Ok, S::Error> {
    value.to_array().serialize(serializer)
}

impl ItemWorld {
    /// Retains the existing collision descriptors; no static mesh is baked.
    #[must_use]
    pub fn from_level(level: &LevelDef, walls: &[WallAabb]) -> Self {
        Self {
            walls: if level
                .props
                .iter()
                .any(crate::engine::level::PropDef::has_fixed_physics_shape)
            {
                level.item_collision_aabbs()
            } else {
                walls.to_vec()
            },
            floor: WalkableFloor::from_level(level),
            surfaces: FloorSurfaces::default(),
            ceiling: WalkableCeiling::from_level(level),
            water: WaterVolumes::from_level(level),
            fixed_props: Vec::new(),
            recovery_space: RecoverySpace::from_level(level, walls),
            runtime: None,
            pending_player: None,
            pending_doors: Vec::new(),
            accumulator: 0.0,
            stats: PhysicsStats::default(),
            #[cfg(not(target_arch = "wasm32"))]
            worker_pool: None,
        }
    }

    /// Copies resolved physical material coefficients once. No images or
    /// material tables are consulted by the solver's per-frame work.
    #[must_use]
    pub fn from_level_with_materials(
        level: &LevelDef,
        walls: &[WallAabb],
        materials: &MaterialTable,
    ) -> Self {
        Self {
            surfaces: FloorSurfaces::from_level(level, materials),
            ..Self::from_level(level, walls)
        }
    }

    /// Selects an explicit bounded pool for developer comparisons. It can be
    /// configured before the first spawn and does not reset solver history.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_worker_pool(&mut self, pool: Arc<rayon::ThreadPool>) {
        self.worker_pool = Some(Arc::clone(&pool));
        if let Some(runtime) = self.runtime.as_mut() {
            #[cfg(feature = "parallel-physics")]
            runtime.world.set_thread_pool(Some(Arc::clone(&pool)));
            runtime.worker_pool = Some(pool);
        }
    }

    fn initialize(
        &mut self,
        #[cfg(all(feature = "parallel-physics", not(target_arch = "wasm32")))]
        serial_solver_pool: Arc<rayon::ThreadPool>,
    ) -> &mut Runtime {
        self.runtime.get_or_insert_with(|| {
            let mut world = PhysicsWorld {
                gravity: Vector::new(0.0, -GRAVITY, 0.0),
                integration_parameters: IntegrationParameters {
                    dt: FIXED_STEP,
                    // A fast small item can meet two surfaces in one interval.
                    max_ccd_substeps: 2,
                    // Five-centimetre contact reuse and five-millimetre CCD
                    // slop are too coarse for cups and thin door leaves.
                    contact_recycling: false,
                    normalized_allowed_linear_error: 0.001,
                    // Rigid terrain needs a stiffer constraint than Rapier's
                    // human-size default: the native 5 kg / 25 g collision
                    // otherwise compresses its floor contact by centimetres.
                    static_contact_softness: SpringCoefficients::new(240.0, 1.0),
                    ..IntegrationParameters::default()
                },
                ..PhysicsWorld::default()
            };
            // Clearing a configured Rapier pool selects Rayon's global pool.
            // Always bind a bounded pool, including the one-worker reference.
            #[cfg(all(feature = "parallel-physics", not(target_arch = "wasm32")))]
            world.set_thread_pool(Some(
                self.worker_pool
                    .clone()
                    .unwrap_or_else(|| Arc::clone(&serial_solver_pool)),
            ));
            let floor_contacts = install_static(
                &mut world,
                &self.walls,
                &self.floor,
                &self.ceiling,
                &self.surfaces,
            );
            for fixed in &self.fixed_props {
                fixed.install(&mut world);
            }
            let static_count = world.colliders.len();
            let (player, player_collider) = world.insert(
                RigidBodyBuilder::kinematic_position_based()
                    .translation(Vector::new(0.0, -1000.0, 0.0)),
                ColliderBuilder::capsule_y(0.6, PLAYER_RADIUS).friction(0.7),
            );
            let mut initialized = Runtime {
                world,
                floor_contacts,
                entries: BTreeMap::new(),
                held: None,
                player,
                player_collider,
                player_height: 1.8,
                doors: Vec::new(),
                player_velocity: Vec3::ZERO,
                previous_player: None,
                last_player: None,
                static_count,
                impacts: VecDeque::with_capacity(MAX_IMPACTS),
                contact_onsets: ContactOnsets::default(),
                frame_contacts: Vec::with_capacity(MAX_CONTACT_PAIRS),
                frame_contact_impulses: Vec::with_capacity(MAX_CONTACT_PAIRS),
                wake_sweeps: Vec::with_capacity(MAX_BODIES),
                wake_targets: Vec::with_capacity(MAX_BODIES),
                recovery: RecoveryState::default(),
                water_work: Vec::new(),
                #[cfg(not(target_arch = "wasm32"))]
                worker_pool: self.worker_pool.clone(),
                #[cfg(all(feature = "parallel-physics", not(target_arch = "wasm32")))]
                serial_solver_pool,
                #[cfg(all(feature = "parallel-physics", not(target_arch = "wasm32")))]
                solver_pool_policy: SolverPoolPolicy::default(),
            };
            if let Some(context) = self.pending_player {
                initialized.previous_player = Some(context.feet);
                initialized.last_player = Some(context);
                initialized.update_player(context);
                initialized.update_doors(&self.pending_doors, true);
            }
            Box::new(initialized)
        })
    }

    /// Adds a body from unscaled model bounds and an original-model-origin pose.
    ///
    /// # Errors
    /// Returns an error for invalid dimensions/parameters, duplicate keys or
    /// the 128-body limit. Rejection leaves the world unchanged.
    pub fn spawn(
        &mut self,
        key: u64,
        def: &PhysicsDef,
        bounds: (Vec3, Vec3),
        position: Vec3,
        yaw: f32,
        scale: f32,
    ) -> Result<(), String> {
        def.validate()?;
        if !position.is_finite() || !yaw.is_finite() || position.abs().max_element() > 100_000.0 {
            return Err("physics position and yaw must be finite and within world bounds".into());
        }
        if self
            .runtime
            .as_ref()
            .is_some_and(|runtime| runtime.entries.contains_key(&key))
        {
            return Err("physics key already exists".into());
        }
        if self
            .runtime
            .as_ref()
            .is_some_and(|runtime| runtime.entries.len() >= MAX_BODIES)
        {
            return Err("physics body limit reached (128)".into());
        }
        let (shape, center, radius) = model_shape(def, bounds, scale)?;
        let physical_mass = mass_properties(&shape, def.mass_kg)?;
        let water = (def.motion == BodyMotion::Dynamic && !self.water.is_empty())
            .then(|| WaterBody::new(&shape, def.water.unwrap_or_default(), scale, def.mass_kg))
            .transpose()?;
        let rotation = Quat::from_rotation_y(yaw);
        let initial = ItemPose { position, rotation };
        let builder = match def.motion {
            BodyMotion::Fixed => RigidBodyBuilder::fixed(),
            BodyMotion::Dynamic => RigidBodyBuilder::dynamic(),
            BodyMotion::Kinematic => RigidBodyBuilder::kinematic_position_based(),
        }
        .pose(pose(add(position, rotation.mul_vec3(center)), rotation))
        .linear_damping(def.linear_damping)
        .angular_damping(def.angular_damping)
        .ccd_enabled(def.motion == BodyMotion::Dynamic)
        .can_sleep(true);
        let collider = ColliderBuilder::new(shape.clone())
            .mass_properties(physical_mass)
            // A small, scale-aware solver skin leaves the core shape clear of
            // resting contacts, so CCD can detect the next floor impact after
            // a heavy body strikes a light sleeper. This is Rapier's contact
            // margin; render/model transforms are never lifted independently.
            .contact_skin((shape.compute_local_aabb().extents().min_element() * 0.1).min(0.002))
            .friction(def.friction)
            .restitution(def.restitution)
            .user_data(u128::from(key).saturating_add(1));
        #[cfg(all(feature = "parallel-physics", not(target_arch = "wasm32")))]
        let serial_solver_pool = crate::engine::runtime_work::serial_pool()?;
        let runtime = self.initialize(
            #[cfg(all(feature = "parallel-physics", not(target_arch = "wasm32")))]
            serial_solver_pool,
        );
        let (body, collider_handle) = runtime.world.insert(builder, collider);
        // Collider attachment computes mass properties immediately. The body
        // pose is a reference frame, so interpolate its actual mass center for
        // compounds whose balance point differs from the model bounds center.
        let interpolation_center = runtime.world.bodies.get(body).map_or(center, |rigid_body| {
            add(center, point(rigid_body.local_center_of_mass()))
        });
        let _previous = runtime.entries.insert(
            key,
            BodyEntry {
                body,
                collider: collider_handle,
                motion: def.motion,
                pickup: def.pickup,
                authored: false,
                dormant: false,
                center,
                interpolation_center,
                radius,
                shape,
                authored_pose: initial,
                reset_group: def.reset_group.as_deref().map(Into::into),
                previous: initial,
                current: initial,
                recovery: RecoveryTracker::new(initial),
                water,
            },
        );
        runtime.refresh_queries();
        Ok(())
    }

    /// Caches a bounded explicit fixed collider independently of movable-body
    /// capacity. Scenery remains in its static render and lighting batches.
    ///
    /// # Errors
    /// Rejects non-fixed/implicit profiles, invalid shapes or transforms and
    /// more than 1024 explicit fixed placements. No partial insertion occurs.
    pub fn install_fixed_shape(
        &mut self,
        def: &PhysicsDef,
        bounds: (Vec3, Vec3),
        position: Vec3,
        yaw: f32,
        scale: f32,
    ) -> Result<(), String> {
        def.validate()?;
        if def.motion != BodyMotion::Fixed || def.shape.is_none() {
            return Err("fixed scenery requires an explicit fixed shape".into());
        }
        if self.fixed_props.len() >= MAX_FIXED_PROP_SHAPES {
            return Err("explicit fixed physics shape limit reached (1024)".into());
        }
        if !position.is_finite() || position.abs().max_element() > 100_000.0 || !yaw.is_finite() {
            return Err("fixed physics pose must be finite and within world bounds".into());
        }
        let (shape, center, _) = model_shape(def, bounds, scale)?;
        let rotation = Quat::from_rotation_y(yaw);
        let fixed = FixedPropShape {
            shape,
            position: pose(add(position, rotation.mul_vec3(center)), rotation),
            friction: def.friction,
            restitution: def.restitution,
        };
        if let Some(runtime) = self.runtime.as_mut() {
            fixed.install(&mut runtime.world);
            runtime.static_count = runtime.static_count.saturating_add(1);
        }
        self.fixed_props.push(fixed);
        Ok(())
    }

    /// Retains an authored movable item exactly at its saved pose until pickup,
    /// a gameplay activation or an active dynamic body's meaningful impact.
    /// Fixed definitions remain permanently fixed; no map opt-in is required.
    ///
    /// # Errors
    /// Returns the same bounded validation, duplicate and capacity errors as spawn.
    pub fn spawn_authored(
        &mut self,
        key: u64,
        def: &PhysicsDef,
        bounds: (Vec3, Vec3),
        position: Vec3,
        yaw: f32,
        scale: f32,
    ) -> Result<(), String> {
        self.spawn(key, def, bounds, position, yaw, scale)?;
        if let Some(runtime) = self.runtime.as_mut() {
            if let Some(entry) = runtime.entries.get_mut(&key) {
                entry.authored = true;
                entry.dormant = entry.motion.is_movable();
            }
            runtime.restore_authored_body(key);
        }
        Ok(())
    }

    /// Adds an item only when its actual physical shape has clear space. This
    /// is the bounded `/give` path; authored placements use `spawn_authored`.
    ///
    /// # Errors
    /// Returns the regular spawn errors or a blocked-placement error. An
    /// overlapping candidate is removed before returning, preserving capacity.
    pub fn spawn_if_clear(
        &mut self,
        key: u64,
        def: &PhysicsDef,
        bounds: (Vec3, Vec3),
        position: Vec3,
        yaw: f32,
        scale: f32,
    ) -> Result<(), String> {
        self.spawn(key, def, bounds, position, yaw, scale)?;
        let blocked = self.runtime.as_ref().is_some_and(|runtime| {
            let Some(entry) = runtime.entries.get(&key) else {
                return true;
            };
            let Some(body) = runtime.world.bodies.get(entry.body) else {
                return true;
            };
            let filter = query_filter().exclude_collider(entry.collider);
            runtime
                .world
                .intersect_shape(*body.position(), entry.shape.as_ref(), filter)
                .next()
                .is_some()
        });
        if blocked {
            let _removed = self.remove(key);
            return Err("physics spawn is blocked by another body or world geometry".into());
        }
        Ok(())
    }

    /// Removes one body, including a held body, without leaving solver handles.
    pub fn remove(&mut self, key: u64) -> bool {
        let Some(runtime) = self.runtime.as_mut() else {
            return false;
        };
        let Some(entry) = runtime.entries.remove(&key) else {
            return false;
        };
        let _removed = runtime.world.remove_body(entry.body);
        runtime.contact_onsets.remove_collider(entry.collider);
        runtime.frame_contacts.retain(|pair| !pair.contains(&key));
        runtime
            .frame_contact_impulses
            .retain(|contact| !contact.pair.contains(&key));
        runtime
            .impacts
            .retain(|impact| impact.key != key && impact.other_key != Some(key));
        if runtime.held.as_ref().is_some_and(|held| held.key == key) {
            runtime.held = None;
        }
        true
    }

    /// Restores existing group members to their authored model-origin poses.
    /// All destinations are checked before any pose, held state or velocity is
    /// changed. Unrelated bodies and their solver history remain untouched.
    ///
    /// # Errors
    /// Rejects malformed/unknown groups or a destination blocked by the live
    /// player, a door or an unrelated body. Authored members retain their
    /// original static-world placement; explicit runtime spawns also check
    /// world geometry. Group scattering is ignored.
    pub fn reset_group(&mut self, group: &str) -> Result<ResetReport, String> {
        validate_reset_group(group)?;
        let runtime = self
            .runtime
            .as_mut()
            .ok_or("physics reset group has no bodies")?;
        let keys: Vec<u64> = runtime
            .entries
            .iter()
            .filter_map(|(&key, entry)| {
                (entry.reset_group.as_deref() == Some(group)).then_some(key)
            })
            .collect();
        if keys.is_empty() {
            return Err(format!("physics reset group '{group}' has no bodies"));
        }
        runtime.refresh_queries();
        runtime.validate_reset_destinations(&keys)?;
        let released_held = runtime
            .held
            .as_ref()
            .map(|held| held.key)
            .filter(|key| keys.binary_search(key).is_ok());
        for &key in &keys {
            runtime.restore_authored_body(key);
        }
        if released_held.is_some() {
            runtime.held = None;
        }
        for &key in &keys {
            runtime.clear_item_contacts(key);
        }
        runtime.refresh_queries();
        runtime.populate_counts(&mut self.stats);
        Ok(ResetReport {
            reset_count: keys.len(),
            released_held,
        })
    }

    /// Prescribes a kinematic body's next model-origin pose. Dynamic and fixed
    /// bodies cannot be teleported through this external-motion seam.
    pub fn set_kinematic_pose(&mut self, key: u64, target: ItemPose) -> bool {
        let quaternion_length = target.rotation.length_squared();
        if !target.position.is_finite()
            || target.position.abs().max_element() > 100_000.0
            || !target.rotation.is_finite()
            || !quaternion_length.is_finite()
            || quaternion_length < 1.0e-10
        {
            return false;
        }
        let Some(runtime) = self.runtime.as_mut() else {
            return false;
        };
        let Some(entry) = runtime.entries.get(&key) else {
            return false;
        };
        if entry.motion != BodyMotion::Kinematic {
            return false;
        }
        let handle = entry.body;
        let center = entry.center;
        if !runtime.activate_key(key) {
            return false;
        }
        let Some(body) = runtime.world.bodies.get_mut(handle) else {
            return false;
        };
        let rotation = target.rotation.normalize();
        body.set_next_kinematic_position(pose(
            add(target.position, rotation.mul_vec3(center)),
            rotation,
        ));
        true
    }

    /// Repositions a dynamic, unheld body for explicit developer acceptance
    /// tooling. Normal gameplay uses contacts, carrying and release instead.
    /// Invalid transforms and held/fixed/kinematic bodies are rejected.
    pub fn set_fixture_pose(&mut self, key: u64, target: ItemPose) -> bool {
        let quaternion_length = target.rotation.length_squared();
        if !target.position.is_finite()
            || target.position.abs().max_element() > 100_000.0
            || !target.rotation.is_finite()
            || !quaternion_length.is_finite()
            || quaternion_length < 1.0e-10
        {
            return false;
        }
        let Some(runtime) = self.runtime.as_mut() else {
            return false;
        };
        if runtime.held.as_ref().is_some_and(|held| held.key == key) {
            return false;
        }
        let Some(entry) = runtime.entries.get(&key) else {
            return false;
        };
        if entry.motion != BodyMotion::Dynamic {
            return false;
        }
        let handle = entry.body;
        let center = entry.center;
        let rotation = target.rotation.normalize();
        if !runtime.activate_key(key) {
            return false;
        }
        let Some(body) = runtime.world.bodies.get_mut(handle) else {
            return false;
        };
        body.set_position(
            pose(add(target.position, rotation.mul_vec3(center)), rotation),
            true,
        );
        body.set_linvel(Vector::ZERO, true);
        body.set_angvel(Vector::ZERO, true);
        runtime.world.wake_up(handle, true);
        runtime.refresh_queries();
        runtime.update_poses();
        if let Some(updated_entry) = runtime.entries.get_mut(&key) {
            updated_entry.previous = updated_entry.current;
        }
        true
    }

    /// Sets a dynamic, unheld body's motion for explicit developer comparisons.
    /// Both finite velocity magnitudes are bounded at 20 in SI units.
    pub fn set_fixture_velocity(&mut self, key: u64, linear: Vec3, angular: Vec3) -> bool {
        if !linear.is_finite()
            || !angular.is_finite()
            || linear.length() > 20.0
            || angular.length() > 20.0
        {
            return false;
        }
        let Some(runtime) = self.runtime.as_mut() else {
            return false;
        };
        if runtime.held.as_ref().is_some_and(|held| held.key == key) {
            return false;
        }
        let Some(entry) = runtime.entries.get(&key) else {
            return false;
        };
        if entry.motion != BodyMotion::Dynamic {
            return false;
        }
        let handle = entry.body;
        if !runtime.activate_key(key) {
            return false;
        }
        let Some(body) = runtime.world.bodies.get_mut(handle) else {
            return false;
        };
        body.set_linvel(vector(linear), true);
        body.set_angvel(vector(angular), true);
        runtime.world.wake_up(handle, true);
        true
    }

    /// Mirrors a controller/door frame and advances at most twelve 120 Hz steps.
    /// Invalid or negative frame intervals perform no work.
    pub fn step(&mut self, delta: f32, player: PlayerContext, doors: &[DoorCollider]) {
        self.stats = PhysicsStats::default();
        let Some(runtime) = self.runtime.as_mut() else {
            return;
        };
        runtime.frame_contacts.clear();
        runtime.frame_contact_impulses.clear();
        self.stats.body_count = runtime.entries.len();
        self.stats.static_colliders = runtime.static_count;
        runtime.populate_counts(&mut self.stats);
        if runtime.entries.is_empty() {
            return;
        }
        if !delta.is_finite() || delta <= 0.0 || !player.valid() {
            return;
        }
        #[cfg(all(feature = "parallel-physics", not(target_arch = "wasm32")))]
        runtime.configure_solver_pool(delta);
        let started = Instant::now();
        let interval = delta.min(MAX_FRAME_SECONDS);
        self.stats.clamped_seconds = delta - interval;
        runtime.player_velocity = runtime
            .previous_player
            .map_or(Vec3::ZERO, |previous| {
                scaled(subtract(player.feet, previous), 1.0 / delta)
            })
            .clamp_length_max(2.0);
        runtime.previous_player = Some(player.feet);
        runtime.last_player = Some(player);
        runtime.update_player(player);
        runtime.update_doors(doors, false);
        if !runtime
            .entries
            .values()
            .any(|entry| entry.motion.is_movable() && !entry.dormant && !entry.recovery.suspended)
        {
            self.stats.step_millis = started.elapsed().as_secs_f64() * 1_000.0_f64;
            return;
        }
        // Preserve the prior fractional step. The clamped interval plus the
        // twelve-step limit already bounds work without silently losing it.
        self.accumulator += interval;
        while self.accumulator + 1.0e-7 >= FIXED_STEP && self.stats.substeps < MAX_SUBSTEPS {
            self.stats.recovered_bodies = self
                .stats
                .recovered_bodies
                .saturating_add(runtime.recover_escaped(&self.recovery_space, 0.0));
            for entry in runtime.entries.values_mut() {
                entry.previous = entry.current;
            }
            runtime.advance_held(player);
            runtime.apply_water(&self.water);
            runtime.activate_impacts();
            runtime.world.step_with_events(&runtime.floor_contacts, &());
            runtime.update_poses();
            self.stats.recovered_bodies = self
                .stats
                .recovered_bodies
                .saturating_add(runtime.recover_escaped(&self.recovery_space, FIXED_STEP));
            runtime.collect_impacts();
            self.accumulator = (self.accumulator - FIXED_STEP).max(0.0);
            self.stats.substeps = self.stats.substeps.saturating_add(1);
        }
        runtime.populate_counts(&mut self.stats);
        self.stats.step_millis = started.elapsed().as_secs_f64() * 1_000.0_f64;
    }

    /// Publishes live door/player poses for an interaction query without
    /// advancing solver time or deriving velocity from the synchronization.
    pub fn sync_context(&mut self, player: PlayerContext, doors: &[DoorCollider]) {
        if !player.valid() {
            return;
        }
        let Some(runtime) = self.runtime.as_mut() else {
            self.pending_player = Some(player);
            self.pending_doors.clear();
            self.pending_doors.extend_from_slice(doors);
            return;
        };
        runtime.last_player = Some(player);
        runtime.update_player(player);
        runtime.update_doors(doors, true);
        runtime.refresh_queries();
    }

    /// Mirrors gameplay enable/disable actions onto the resolved water used by
    /// rigid items. Unchanged flags allocate nothing and do not disturb sleep;
    /// authored dormant bodies remain dormant when a volume changes.
    pub fn sync_water(&mut self, water: &WaterVolumes) {
        let changed = self
            .water
            .volumes()
            .iter()
            .zip(water.volumes())
            .any(|(current, incoming)| current.enabled != incoming.enabled);
        if !changed {
            return;
        }
        for (index, incoming) in water.volumes().iter().enumerate() {
            let _changed = self.water.set_enabled(index, incoming.enabled);
        }
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.wake_changed_water();
        }
    }

    /// Emits interpolated model-origin poses in stable key order.
    pub fn for_each_pose(&self, mut visit: impl FnMut(u64, ItemPose)) {
        let Some(runtime) = self.runtime.as_ref() else {
            return;
        };
        let alpha = (self.accumulator / FIXED_STEP).clamp(0.0, 1.0);
        for (&key, entry) in &runtime.entries {
            if entry.recovery.suspended {
                continue;
            }
            let interpolated = entry.render_pose(
                alpha,
                runtime.held.as_ref().is_some_and(|held| held.key == key),
            );
            visit(key, interpolated);
        }
    }

    /// Current physical shape center for targeting and developer inspection.
    #[must_use]
    pub fn body_center(&self, key: u64) -> Option<Vec3> {
        let runtime = self.runtime.as_ref()?;
        let entry = runtime.entries.get(&key)?;
        runtime
            .world
            .bodies
            .get(entry.body)
            .map(|body| point(body.translation()))
    }

    /// Current solver poses and velocities; these do not include render interpolation.
    #[must_use]
    pub fn snapshots(&self) -> Vec<BodySnapshot> {
        let Some(runtime) = self.runtime.as_ref() else {
            return Vec::new();
        };
        let alpha = (self.accumulator / FIXED_STEP).clamp(0.0, 1.0);
        runtime
            .entries
            .iter()
            .filter_map(|(&key, entry)| {
                let body = runtime.world.bodies.get(entry.body)?;
                let collider = runtime.world.colliders.get(entry.collider)?;
                Some(BodySnapshot {
                    key,
                    pose: entry.current,
                    render_pose: entry.render_pose(
                        alpha,
                        runtime.held.as_ref().is_some_and(|held| held.key == key),
                    ),
                    linear_velocity: point(body.linvel()),
                    angular_velocity: point(body.angvel()),
                    sleeping: !entry.dormant && body.is_sleeping(),
                    dormant: entry.dormant,
                    state: if entry.recovery.suspended {
                        BodyState::Suspended
                    } else if entry.dormant {
                        BodyState::Dormant
                    } else if runtime.held.as_ref().is_some_and(|held| held.key == key) {
                        BodyState::Held
                    } else if entry.motion == BodyMotion::Fixed {
                        BodyState::Fixed
                    } else if entry.motion == BodyMotion::Kinematic {
                        BodyState::Kinematic
                    } else if body.is_sleeping() {
                        BodyState::Sleeping
                    } else {
                        BodyState::Active
                    },
                    held: runtime.held.as_ref().is_some_and(|held| held.key == key),
                    motion: entry.motion,
                    mass_kg: collider.mass(),
                    solver_mass_kg: body.mass(),
                    effective_mass_kg: point(body.mass_properties().effective_mass()),
                    local_center_of_mass: add(entry.center, point(body.local_center_of_mass())),
                    world_center_of_mass: point(body.center_of_mass()),
                    principal_inertia: point(
                        body.mass_properties().local_mprops.principal_inertia(),
                    ),
                    friction: collider.friction(),
                    restitution: collider.restitution(),
                    displaced_volume_m3: entry.water.as_ref().map_or(0.0, |water| water.volume_m3),
                    submerged_fraction: entry
                        .water
                        .as_ref()
                        .map_or(0.0, |water| water.submerged_fraction),
                })
            })
            .collect()
    }

    #[must_use]
    pub const fn stats(&self) -> PhysicsStats {
        self.stats
    }

    /// Drains the bounded contact-onset seam for a later presentation layer.
    #[must_use]
    pub fn take_impacts(&mut self) -> Vec<ImpactEvent> {
        self.runtime
            .as_mut()
            .map_or_else(Vec::new, |runtime| runtime.impacts.drain(..).collect())
    }

    /// Item-only solver contacts accumulated across the latest frame's fixed
    /// steps, in stable key order. The diagnostic list contains at most 256 pairs.
    #[must_use]
    pub fn contact_pairs(&self) -> Vec<[u64; 2]> {
        self.runtime.as_ref().map_or_else(Vec::new, |runtime| {
            let mut pairs = runtime.frame_contacts.clone();
            pairs.sort_unstable();
            pairs
        })
    }
    /// At most 256 positive item-pair impulses; allocated only when requested.
    #[must_use]
    pub fn contact_impulses(&self) -> Vec<ContactImpulse> {
        self.runtime.as_ref().map_or_else(Vec::new, |runtime| {
            let mut contacts = runtime.frame_contact_impulses.clone();
            contacts.sort_unstable_by_key(|contact| contact.pair);
            contacts
        })
    }
}

impl PlayerContext {
    fn valid(self) -> bool {
        self.eye.is_finite()
            && self.feet.is_finite()
            && self.view.is_finite()
            && self.yaw.is_finite()
            && self.height.is_finite()
            && self.height > PLAYER_RADIUS * 2.0
    }
}

impl Runtime {
    #[cfg(all(feature = "parallel-physics", not(target_arch = "wasm32")))]
    fn configure_solver_pool(&mut self, delta: f32) {
        if self.worker_pool.is_some() {
            return;
        }
        let active = self
            .entries
            .values()
            .filter(|entry| !entry.dormant && !entry.recovery.suspended)
            .filter_map(|entry| self.world.bodies.get(entry.body))
            .filter(|body| body.is_enabled() && body.is_dynamic() && !body.is_sleeping())
            .count();
        let parallel = self.solver_pool_policy.update(active, delta);
        let selected = if parallel {
            crate::engine::runtime_work::pool().filter(|pool| pool.current_num_threads() > 1)
        } else {
            None
        }
        .unwrap_or_else(|| Arc::clone(&self.serial_solver_pool));
        if self
            .world
            .thread_pool()
            .as_ref()
            .is_none_or(|current| !Arc::ptr_eq(current, &selected))
        {
            self.world.set_thread_pool(Some(selected));
        }
    }

    pub(super) fn clear_item_contacts(&mut self, key: u64) {
        if let Some(entry) = self.entries.get(&key) {
            self.contact_onsets.remove_collider(entry.collider);
        }
        self.frame_contacts.retain(|pair| !pair.contains(&key));
        self.frame_contact_impulses
            .retain(|contact| !contact.pair.contains(&key));
        self.impacts
            .retain(|impact| impact.key != key && impact.other_key != Some(key));
    }
    fn validate_reset_destinations(&self, keys: &[u64]) -> Result<(), String> {
        let outside_group = |handle, collider: &rapier3d::prelude::Collider| {
            collider.is_enabled()
                && self
                    .key_for_collider(handle)
                    .is_none_or(|key| keys.binary_search(&key).is_err())
        };
        let filter = query_filter().predicate(&outside_group);
        for key in keys {
            let entry = self
                .entries
                .get(key)
                .ok_or("physics reset body is missing")?;
            if self.world.bodies.get(entry.body).is_none() {
                return Err("physics reset solver body is missing".into());
            }
            let target = pose(
                add(
                    entry.authored_pose.position,
                    entry.authored_pose.rotation.mul_vec3(entry.center),
                ),
                entry.authored_pose.rotation,
            );
            for (_, collider) in self
                .world
                .intersect_shape(target, entry.shape.as_ref(), filter)
            {
                // Authored dormancy deliberately retains imperfect placement
                // against immutable scenery, including its original floor.
                // Live actors and unrelated runtime items must still be clear.
                if entry.authored && collider.parent().is_none() {
                    continue;
                }
                let tolerable_contact = shape_contact(
                    &target,
                    entry.shape.as_ref(),
                    collider.position(),
                    collider.shape(),
                    0.0,
                )
                .is_ok_and(|contact| {
                    let tolerance = if collider.parent().is_none() {
                        0.002
                    } else {
                        1.0e-5
                    };
                    contact.is_none_or(|point| point.dist >= -tolerance)
                });
                if !tolerable_contact {
                    return Err(format!(
                        "physics reset destination for body {key} is blocked"
                    ));
                }
            }
        }
        Ok(())
    }

    pub(super) fn restore_authored_body(&mut self, key: u64) {
        let Some(entry) = self.entries.get_mut(&key) else {
            return;
        };
        let target = pose(
            add(
                entry.authored_pose.position,
                entry.authored_pose.rotation.mul_vec3(entry.center),
            ),
            entry.authored_pose.rotation,
        );
        entry.dormant = entry.authored && entry.motion.is_movable();
        entry.recovery.reset(entry.authored_pose);
        if let Some(water) = entry.water.as_mut() {
            water.submerged_fraction = 0.0;
        }
        if let Some(collider) = self.world.colliders.get_mut(entry.collider) {
            collider.set_enabled(true);
        }
        if let Some(body) = self.world.bodies.get_mut(entry.body) {
            body.set_enabled(true);
            let motion = if entry.dormant {
                rapier3d::prelude::RigidBodyType::Fixed
            } else {
                match entry.motion {
                    BodyMotion::Fixed => rapier3d::prelude::RigidBodyType::Fixed,
                    BodyMotion::Dynamic => rapier3d::prelude::RigidBodyType::Dynamic,
                    BodyMotion::Kinematic => {
                        rapier3d::prelude::RigidBodyType::KinematicPositionBased
                    }
                }
            };
            body.set_body_type(motion, true);
            body.set_position(target, true);
            body.set_next_kinematic_position(target);
            body.set_linvel(Vector::ZERO, true);
            body.set_angvel(Vector::ZERO, true);
            body.reset_forces(true);
            body.reset_torques(true);
            // Position-based kinematics ignore velocity setters. Sleeping
            // clears their stored velocity as well, before any wake below.
            body.sleep();
        }
        if !entry.dormant {
            self.world.wake_up(entry.body, true);
        }
        entry.previous = entry.authored_pose;
        entry.current = entry.authored_pose;
    }

    fn impact_position(&self, contact: &ContactPair) -> Option<Vec3> {
        let first = self.world.colliders.get(contact.collider1)?;
        let second = self.world.colliders.get(contact.collider2)?;
        contact
            .manifolds()
            .iter()
            .flat_map(|manifold| {
                let first_pose = manifold.subshape_pos1().prepend_to(first.position());
                let second_pose = manifold.subshape_pos2().prepend_to(second.position());
                manifold.points.iter().map(move |contact_point| {
                    let first_point = first_pose.transform_point(contact_point.local_p1);
                    let second_point = second_pose.transform_point(contact_point.local_p2);
                    (
                        contact_point.data.impulse,
                        point(first_point.midpoint(second_point)),
                    )
                })
            })
            .filter(|(impulse, position)| impulse.is_finite() && position.is_finite())
            .max_by(|left, right| left.0.total_cmp(&right.0))
            .map(|(_, position)| position)
    }

    pub(super) fn refresh_queries(&mut self) {
        self.world
            .bodies
            .propagate_modified_body_positions_to_colliders(&mut self.world.colliders);
        refresh_collider(&mut self.world, self.player_collider);
        for entry in self.entries.values() {
            refresh_collider(&mut self.world, entry.collider);
        }
        for &handle in &self.doors {
            let collider = self
                .world
                .bodies
                .get(handle)
                .and_then(|body| body.colliders().first())
                .copied();
            if let Some(collider_handle) = collider {
                refresh_collider(&mut self.world, collider_handle);
            }
        }
    }

    pub(super) fn populate_counts(&self, stats: &mut PhysicsStats) {
        stats.active_bodies = 0;
        stats.sleeping_bodies = 0;
        stats.dormant_bodies = 0;
        stats.suspended_bodies = 0;
        for entry in self.entries.values() {
            if entry.recovery.suspended {
                stats.suspended_bodies = stats.suspended_bodies.saturating_add(1);
                continue;
            }
            if entry.dormant {
                stats.dormant_bodies = stats.dormant_bodies.saturating_add(1);
                continue;
            }
            if entry.motion != BodyMotion::Dynamic {
                continue;
            }
            if self
                .world
                .bodies
                .get(entry.body)
                .is_some_and(rapier3d::prelude::RigidBody::is_sleeping)
            {
                stats.sleeping_bodies = stats.sleeping_bodies.saturating_add(1);
            } else {
                stats.active_bodies = stats.active_bodies.saturating_add(1);
            }
        }
    }

    pub(super) fn key_for_collider(&self, handle: ColliderHandle) -> Option<u64> {
        let encoded = self.world.colliders.get(handle)?.user_data.checked_sub(1)?;
        u64::try_from(encoded).ok()
    }

    fn update_player(&mut self, player: PlayerContext) {
        if let Some(collider) = self.world.colliders.get_mut(self.player_collider) {
            collider.set_enabled(player.active);
        }
        if (player.height - self.player_height).abs() > 1.0e-4 {
            if let Some(collider) = self.world.colliders.get_mut(self.player_collider) {
                collider.set_shape(SharedShape::capsule_y(
                    player.height.mul_add(0.5, -PLAYER_RADIUS),
                    PLAYER_RADIUS,
                ));
            }
            self.player_height = player.height;
        }
        if let Some(body) = self.world.bodies.get_mut(self.player) {
            let center = Vec3::new(
                player.feet.x,
                player.height.mul_add(0.5, player.feet.y),
                player.feet.z,
            );
            let target = pose(center, Quat::IDENTITY);
            // Player motion is prescribed externally: no solver correction is
            // fed back into the controller, including during stance changes.
            body.set_position(target, false);
            body.set_next_kinematic_position(target);
        }
        self.refresh_queries();
    }

    fn update_doors(&mut self, doors: &[DoorCollider], immediate: bool) {
        if self.doors.len() != doors.len() {
            for handle in self.doors.drain(..) {
                let _removed = self.world.remove_body(handle);
            }
            for door in doors {
                let (handle, _collider) = self.world.insert(
                    RigidBodyBuilder::kinematic_position_based().pose(door_pose(door)),
                    ColliderBuilder::cuboid(
                        door.width * 0.5,
                        door.height * 0.5,
                        door.thickness * 0.5,
                    )
                    .friction(0.7),
                );
                self.doors.push(handle);
            }
        }
        for (&handle, door) in self.doors.iter().zip(doors) {
            if let Some(body) = self.world.bodies.get_mut(handle) {
                let target = door_pose(door);
                if immediate {
                    body.set_position(target, false);
                }
                body.set_next_kinematic_position(target);
            }
        }
    }

    pub(super) fn update_poses(&mut self) {
        for entry in self.entries.values_mut() {
            if entry.dormant {
                entry.previous = entry.authored_pose;
                entry.current = entry.authored_pose;
                continue;
            }
            if let Some(body) = self.world.bodies.get(entry.body) {
                let rotation = quaternion(*body.rotation());
                entry.current = ItemPose {
                    position: subtract(point(body.translation()), rotation.mul_vec3(entry.center)),
                    rotation,
                };
            }
        }
    }

    fn collect_impacts(&mut self) {
        self.contact_onsets.begin_step();
        // Split borrows retain zero-allocation traversal of the solver graph.
        for contact in self.world.contact_pairs() {
            if !contact.has_any_active_contact() {
                continue;
            }
            let first = self.key_for_collider(contact.collider1);
            let second = self.key_for_collider(contact.collider2);
            let impulse = contact.total_impulse_magnitude();
            let item_pair = first
                .zip(second)
                .filter(|(a, b)| a != b)
                .map(|(a, b)| [a.min(b), a.max(b)]);
            if let Some(pair) = item_pair {
                if !self.frame_contacts.contains(&pair)
                    && self.frame_contacts.len() < MAX_CONTACT_PAIRS
                {
                    self.frame_contacts.push(pair);
                }
                if impulse.is_finite() && impulse > 0.0 {
                    if let Some(row) = self
                        .frame_contact_impulses
                        .iter_mut()
                        .find(|row| row.pair == pair)
                    {
                        row.impulse = row.impulse.max(impulse);
                    } else if self.frame_contact_impulses.len() < MAX_CONTACT_PAIRS {
                        self.frame_contact_impulses
                            .push(ContactImpulse { pair, impulse });
                    }
                }
            }
            let active_key = [first, second].into_iter().flatten().find(|key| {
                self.entries
                    .get(key)
                    .is_some_and(|entry| entry.motion == BodyMotion::Dynamic && !entry.dormant)
                    && self.held.as_ref().is_none_or(|held| held.key != *key)
            });
            let Some(key) = active_key else {
                continue;
            };
            if !self
                .contact_onsets
                .observe(contact.collider1, contact.collider2, impulse)
            {
                continue;
            }
            let contact_position = self
                .impact_position(contact)
                .or_else(|| self.entries.get(&key).map(|entry| entry.current.position));
            let Some(position) = contact_position else {
                continue;
            };
            if self.impacts.len() == MAX_IMPACTS {
                let _oldest = self.impacts.pop_front();
            }
            self.impacts.push_back(ImpactEvent {
                key,
                other_key: if first == Some(key) { second } else { first },
                position,
                impulse,
            });
        }
        self.contact_onsets.finish_step();
    }
}

#[cfg(all(test, feature = "parallel-physics", not(target_arch = "wasm32")))]
mod pool_policy_tests {
    use super::SolverPoolPolicy;

    // Eight bytes of policy state protect hysteresis without physics/map work.
    #[test]
    fn brief_active_count_changes_do_not_chatter_between_solver_pools() {
        let mut policy = SolverPoolPolicy::default();
        assert!(!policy.update(0, 0.1), "idle stays serial");
        assert!(policy.update(128, 0.01), "measured heavy workload enters");
        assert!(policy.update(127, 0.1), "one sleeper does not switch pools");
        assert!(
            policy.update(0, 0.1),
            "brief quiet interval retains the pool"
        );
        assert!(
            policy.update(0, 0.1),
            "quarter-second exit delay is respected"
        );
        assert!(!policy.update(0, 0.1), "sustained quiet returns to serial");
    }
}
