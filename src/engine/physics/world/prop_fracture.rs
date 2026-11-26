//! Opt-in Christmas props fracture on their own first qualifying solved drop contact.
use glam::Vec3;

use crate::engine::level::PropDef;
use crate::engine::physics::{BodyMotion, ImpactRelease, PhysicsDef};

use super::{BodyEntry, ItemPose, ItemWorld};

const HOUSE: &str = "christmas:gingerbread_house";
const SPRUCE: &str = "christmas:snow_globe_spruce";
const VILLAGE: &str = "christmas:snow_globe_village";
const AUTHORED_ITEM_KEY_BASE: u64 = 1_u64 << 63;
const FRAGMENT_BASE: u64 = 3_u64 << 62;
const MIN_SPEED: f32 = 2.3;
const MIN_IMPULSE: f32 = 0.06;

#[derive(Clone, Copy)]
pub struct Panel {
    pub path: &'static str,
    pub offset: [f32; 3],
    pub min: [f32; 3],
    pub max: [f32; 3],
}
#[derive(Clone, Copy)]
pub enum Breakable {
    House,
    SpruceGlobe,
    VillageGlobe,
}

impl Breakable {
    pub const fn panels(self) -> &'static [Panel] {
        match self {
            Self::House => &HOUSE_PARTS,
            Self::SpruceGlobe => &SPRUCE_PARTS,
            Self::VillageGlobe => &VILLAGE_PARTS,
        }
    }

    const fn thresholds(self) -> (f32, f32) {
        match self {
            Self::House => (MIN_SPEED, MIN_IMPULSE),
            Self::SpruceGlobe | Self::VillageGlobe => (2.15, 0.035),
        }
    }
}

pub(super) struct BreakableBody {
    kind: Breakable,
    scale: f32,
    tested: Option<u64>,
    pub(super) pending: bool,
    pub(super) pieces: Vec<u64>,
}

pub fn globe(model: &str) -> bool {
    matches!(model, SPRUCE | VILLAGE)
}

pub fn authored_item_key(index: usize) -> u64 {
    AUTHORED_ITEM_KEY_BASE.saturating_add(u64::try_from(index).unwrap_or(u64::MAX))
}

pub fn eligible(prop: &PropDef) -> Option<Breakable> {
    if !prop
        .physics
        .as_ref()
        .is_some_and(|p| p.pickup && p.motion == BodyMotion::Dynamic)
    {
        return None;
    }
    match prop.model.as_str() {
        HOUSE => Some(Breakable::House),
        SPRUCE => Some(Breakable::SpruceGlobe),
        VILLAGE => Some(Breakable::VillageGlobe),
        _ => None,
    }
}

/// Companion GLBs are exact dependencies even though they are not catalog placements.
pub fn dependency_paths(
    level: &crate::engine::level::LevelDef,
) -> impl Iterator<Item = &'static str> + '_ {
    level
        .props
        .iter()
        .filter_map(eligible)
        .flat_map(|family| family.panels().iter().map(|panel| panel.path))
}

#[derive(Clone, Copy)]
pub struct BreakPlan {
    pub key: u64,
    pub pose: ItemPose,
    pub scale: f32,
    pub kind: Breakable,
}

impl BreakableBody {
    pub(super) fn contact_tested(&self, serial: u64) -> bool {
        self.tested == Some(serial)
    }

    pub(super) fn restore_contact(&mut self, release: ImpactRelease, serial: u64, tested: bool) {
        self.tested = (release == ImpactRelease::Dropped && tested).then_some(serial);
        self.pending = false;
    }

    /// Called ahead of the audio-budget gate. Zero/speculative solver contacts
    /// cannot spend a release; held bodies and inherited release causes cannot
    /// trigger it. The first actual contact decides, even if it was gentle.
    pub(super) fn witness_contact(
        &mut self,
        entry: &BodyEntry,
        held: bool,
        speed: f32,
        impulse: f32,
    ) {
        if entry.originating_release != ImpactRelease::Dropped
            || self.tested == Some(entry.release_serial)
            || entry.recovery.suspended
            || held
            || !impulse.is_finite()
            || impulse <= 0.001
        {
            return;
        }
        self.tested = Some(entry.release_serial);
        let (min_speed, min_impulse) = self.kind.thresholds();
        self.pending = speed >= min_speed && impulse >= min_impulse;
    }
}

impl ItemWorld {
    pub(crate) fn install_breakable(&mut self, key: u64, prop: &PropDef) {
        if let Some(kind) = eligible(prop) {
            self.register_breakable(key, kind, prop.scale);
        }
    }

    pub(crate) fn register_breakable(&mut self, key: u64, kind: Breakable, scale: f32) {
        if let Some(runtime) = self.runtime.as_mut() {
            drop(runtime.breakables.insert(
                key,
                BreakableBody {
                    kind,
                    scale,
                    tested: None,
                    pending: false,
                    pieces: Vec::new(),
                },
            ));
        }
    }

    pub(crate) fn drain_break_plans(&mut self) -> Vec<BreakPlan> {
        let Some(runtime) = self.runtime.as_mut() else {
            return Vec::new();
        };
        runtime
            .breakables
            .iter_mut()
            .filter_map(|(&key, house)| {
                if !std::mem::take(&mut house.pending) || !house.pieces.is_empty() {
                    return None;
                }
                let entry = runtime.entries.get(&key)?;
                if entry.recovery.suspended || runtime.held.as_ref().is_some_and(|h| h.key == key) {
                    return None;
                }
                Some(BreakPlan {
                    key,
                    pose: entry.current,
                    scale: house.scale,
                    kind: house.kind,
                })
            })
            .collect()
    }

    pub(crate) fn next_fragment_key(&mut self) -> Option<u64> {
        let runtime = self.runtime.as_mut()?;
        let key = runtime.next_break_fragment;
        runtime.next_break_fragment = key.checked_add(1)?;
        (!runtime.entries.contains_key(&key)).then_some(key)
    }

    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Shard offsets, masses and emission velocities derive from bounded reviewed assets."
    )]
    pub(crate) fn spawn_break_fragment(
        &mut self,
        key: u64,
        plan: BreakPlan,
        panel: Panel,
    ) -> Result<ItemPose, String> {
        let mass = self
            .runtime
            .as_ref()
            .and_then(|r| {
                r.entries
                    .get(&plan.key)
                    .and_then(|e| r.world.bodies.get(e.body))
            })
            .map_or(0.3, rapier3d::prelude::RigidBody::mass);
        let velocity = self
            .body_pose_velocity(plan.key)
            .map_or(Vec3::ZERO, |(_, v)| v);
        let offset = Vec3::from_array(panel.offset);
        let mut direction = plan
            .pose
            .rotation
            .mul_vec3(Vec3::new(offset.x, 0.0, offset.z));
        direction.y = 0.0;
        direction = direction.normalize_or_zero();
        let position = plan.pose.position
            + plan.pose.rotation.mul_vec3(offset * plan.scale)
            + direction * (0.014 * plan.scale)
            + Vec3::Y * (0.035 * plan.scale);
        let model_min = Vec3::from_array(panel.min);
        let model_max = Vec3::from_array(panel.max);
        let center = (model_min + model_max) * 0.5;
        let half = ((model_max - model_min) * Vec3::new(0.47, 0.31, 0.47)).max(Vec3::splat(0.006));
        let settings = PhysicsDef {
            motion: BodyMotion::Dynamic,
            mass_kg: (mass / f32::from(u16::try_from(plan.kind.panels().len()).unwrap_or(1)))
                .max(0.001),
            friction: 0.75,
            restitution: 0.08,
            ..PhysicsDef::default()
        };
        self.spawn_oriented(
            key,
            &settings,
            (center - half, center + half),
            position,
            plan.pose.rotation,
            plan.scale,
        )?;
        let scatter = velocity.clamp_length_max(1.1) * 0.25 + direction * 0.37 + Vec3::Y * 0.34;
        let spin = direction.cross(Vec3::Y) * 1.8;
        if !self.set_fixture_velocity(key, scatter, spin) {
            let _retired = self.remove(key);
            return Err("Fracture piece failed velocity setup".into());
        }
        Ok(ItemPose {
            position,
            rotation: plan.pose.rotation,
        })
    }

    /// Keeps a dormant ownership record for ordinary group reset, but no
    /// enabled original body, pickup, trigger occupant or rendered geometry.
    pub(crate) fn commit_break(&mut self, key: u64, pieces: Vec<u64>) -> bool {
        let Some(runtime) = self.runtime.as_mut() else {
            return false;
        };
        let Some(entry) = runtime.entries.get_mut(&key) else {
            return false;
        };
        let Some(house) = runtime.breakables.get_mut(&key) else {
            return false;
        };
        let Some(body) = runtime.world.bodies.get_mut(entry.body) else {
            return false;
        };
        let Some(collider) = runtime.world.colliders.get_mut(entry.collider) else {
            return false;
        };
        body.set_enabled(false);
        collider.set_enabled(false);
        entry.recovery.suspended = true;
        house.pieces = pieces;
        runtime.refresh_queries();
        runtime.populate_counts(&mut self.stats);
        true
    }

    pub(crate) fn drain_fragment_retirements(&mut self) -> Vec<u64> {
        self.runtime
            .as_mut()
            .map_or_else(Vec::new, |r| std::mem::take(&mut r.retired_break_parts))
    }
}

pub(super) const fn first_fragment_key() -> u64 {
    FRAGMENT_BASE
}

const HOUSE_PARTS: [Panel; 7] = [
    Panel {
        path: "environments/christmas/props/gingerbread_house_piece_base.glb",
        offset: [0.0, 0.0, 0.0],
        min: [-0.176_5, 0.0, -0.142],
        max: [0.176_5, 0.038_982_5, 0.142],
    },
    Panel {
        path: "environments/christmas/props/gingerbread_house_piece_front.glb",
        offset: [0.0, 0.033, 0.115_201],
        min: [-0.154_211_9, 0.0, -0.020_201_3],
        max: [0.154_211_9, 0.281, 0.020_201_3],
    },
    Panel {
        path: "environments/christmas/props/gingerbread_house_piece_back.glb",
        offset: [0.0, 0.033, -0.114_482],
        min: [-0.154_211_9, 0.0, -0.019_481_6],
        max: [0.154_211_9, 0.281, 0.019_481_6],
    },
    Panel {
        path: "environments/christmas/props/gingerbread_house_piece_side_left.glb",
        offset: [-0.145, 0.033_5, 0.0],
        min: [-0.013_5, 0.0, -0.099_5],
        max: [0.013_5, 0.147, 0.099_5],
    },
    Panel {
        path: "environments/christmas/props/gingerbread_house_piece_side_right.glb",
        offset: [0.145, 0.033_5, 0.0],
        min: [-0.013_5, 0.0, -0.099_5],
        max: [0.013_5, 0.147, 0.099_5],
    },
    Panel {
        path: "environments/christmas/props/gingerbread_house_piece_roof_left.glb",
        offset: [-0.082_67, 0.183, 0.0],
        min: [-0.088_330_2, 0.0, -0.133_743_5],
        max: [0.088_330_2, 0.141_103_7, 0.133_743_5],
    },
    Panel {
        path: "environments/christmas/props/gingerbread_house_piece_roof_right.glb",
        offset: [0.082_67, 0.183, 0.0],
        min: [-0.088_330_2, 0.0, -0.133_743_5],
        max: [0.088_330_2, 0.159, 0.133_743_5],
    },
];
const SHARD_FRONT: Panel = Panel {
    path: "environments/christmas/props/snow_globe_shard_front.glb",
    offset: [-0.022, 0.071, 0.07],
    min: [-0.035, 0.0, -0.006_783],
    max: [0.035, 0.073, 0.006_783],
};
const SHARD_BACK: Panel = Panel {
    path: "environments/christmas/props/snow_globe_shard_back.glb",
    offset: [0.02, 0.082, -0.066],
    min: [-0.031_5, 0.0, -0.010_143],
    max: [0.031_5, 0.063, 0.010_143],
};
const SHARD_RIGHT: Panel = Panel {
    path: "environments/christmas/props/snow_globe_shard_right.glb",
    offset: [0.069, 0.081, 0.008],
    min: [-0.031, 0.0, -0.005_346],
    max: [0.031, 0.049, 0.005_346],
};
const SHARD_LEFT: Panel = Panel {
    path: "environments/christmas/props/snow_globe_shard_left.glb",
    offset: [-0.075, 0.087, -0.011],
    min: [-0.028, 0.0, -0.005_381],
    max: [0.028, 0.051, 0.005_381],
};
const SPRUCE_PARTS: [Panel; 5] = [
    Panel {
        path: "environments/christmas/props/snow_globe_spruce_broken_base.glb",
        offset: [0.0, 0.0, 0.0],
        min: [-0.101, 0.0, -0.101_851],
        max: [0.101, 0.161_01, 0.101_851],
    },
    SHARD_FRONT,
    SHARD_BACK,
    SHARD_RIGHT,
    SHARD_LEFT,
];
const VILLAGE_PARTS: [Panel; 5] = [
    Panel {
        path: "environments/christmas/props/snow_globe_village_broken_base.glb",
        offset: [0.0, 0.0, 0.0],
        min: [-0.101, 0.0, -0.101_851],
        max: [0.101, 0.157, 0.101_851],
    },
    SHARD_FRONT,
    SHARD_BACK,
    SHARD_RIGHT,
    SHARD_LEFT,
];
