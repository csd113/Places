//! Exact dependency keys and dirty-region certificates for actor and prop lighting.

use glam::Mat4;

use super::LevelLighting;
use super::entity_receivers::{self, EntityStaticRayCache, MovableOccluders};
use crate::engine::{entity::DynamicId, spatial::Aabb};

/// Center samples depend on the field, authored lighting and exact position,
/// independently of movable-caster visibility. The caller's revision covers
/// resource contents, including in-place mutations and fixture switches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::engine) struct EntityCenterLightKey {
    revision: u64,
    lighting: usize,
    field: usize,
    position: [u32; 3],
}

impl EntityCenterLightKey {
    pub(in crate::engine) fn new(
        revision: u64,
        lighting: &LevelLighting,
        field: Option<&crate::engine::lighting::probes::ProbeField>,
        position: [f32; 3],
    ) -> Self {
        Self {
            revision,
            lighting: std::ptr::from_ref(lighting).addr(),
            field: field.map_or(0, |value| std::ptr::from_ref(value).addr()),
            position: position.map(f32::to_bits),
        }
    }
}

/// Exact revisioned receiver dependencies, checked before emitter enumeration
/// or worker dispatch. Bounds/center also cover animated balloon lighting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::engine) struct EntityLightingRefreshKey {
    center: EntityCenterLightKey,
    model: [[u32; 4]; 4],
    bounds: [[u32; 3]; 2],
    scene: usize,
    dynamic: usize,
    dynamic_revision: u64,
    observed_dynamic_revision: u64,
    receiver: Option<DynamicId>,
}

impl EntityLightingRefreshKey {
    pub(in crate::engine) const fn center_key(self) -> EntityCenterLightKey {
        self.center
    }

    /// A moving center diagnostic can stay inside unchanged spatial bounds.
    /// Only resources, bounds, placement and source visibility invalidate the
    /// anchor payload; the center position has its independent sample key.
    pub(in crate::engine) fn spatial_changed(self, previous: Option<Self>) -> bool {
        previous.is_none_or(|cached| {
            self.center.revision != cached.center.revision
                || self.center.lighting != cached.center.lighting
                || self.center.field != cached.center.field
                || self.model != cached.model
                || self.bounds != cached.bounds
                || self.scene != cached.scene
                || self.dynamic != cached.dynamic
                || self.dynamic_revision != cached.dynamic_revision
                || self.receiver != cached.receiver
        })
    }
}

/// Prepared once before worker dispatch, consumed by the same synchronous batch.
/// The receiver publishes its dependency key in preparation; only these two
/// independent payload updates remain for its worker.
#[derive(Debug, Default)]
pub(in crate::engine) struct EntityLightingRefreshPlan {
    pub(in crate::engine) center: bool,
    pub(in crate::engine) spatial: bool,
}

impl EntityLightingRefreshPlan {
    pub(in crate::engine) fn new(
        key: Option<EntityLightingRefreshKey>,
        previous: Option<EntityLightingRefreshKey>,
        center: Option<EntityCenterLightKey>,
    ) -> Self {
        Self {
            center: key.is_none_or(|current| Some(current.center) != center),
            spatial: key.is_none_or(|current| current.spatial_changed(previous)),
        }
    }

    pub(in crate::engine) const fn needed(&self) -> bool {
        self.center || self.spatial
    }
}

#[derive(Clone, Copy)]
pub(in crate::engine) struct EntityLightingRefreshContext<'a> {
    lighting: &'a LevelLighting,
    field: Option<&'a crate::engine::lighting::probes::ProbeField>,
    scene: Option<&'a crate::engine::lighting::transport::TransportScene>,
    visibility: Option<&'a dyn MovableOccluders>,
    revision: Option<u64>,
}

impl<'a> EntityLightingRefreshContext<'a> {
    pub(in crate::engine) const fn new(
        lighting: &'a LevelLighting,
        field: Option<&'a crate::engine::lighting::probes::ProbeField>,
        scene: Option<&'a crate::engine::lighting::transport::TransportScene>,
        visibility: Option<&'a dyn MovableOccluders>,
        revision: Option<u64>,
    ) -> Self {
        Self {
            lighting,
            field,
            scene,
            visibility,
            revision,
        }
    }

    pub(in crate::engine) fn key(
        self,
        model: Mat4,
        bounds: Aabb,
        position: [f32; 3],
        receiver: Option<DynamicId>,
        previous: Option<EntityLightingRefreshKey>,
        rays: Option<&EntityStaticRayCache>,
    ) -> Option<EntityLightingRefreshKey> {
        let observed = self.visibility.map_or(0, MovableOccluders::revision);
        let mut key = EntityLightingRefreshKey {
            center: EntityCenterLightKey::new(self.revision?, self.lighting, self.field, position),
            model: model
                .to_cols_array_2d()
                .map(|column| column.map(f32::to_bits)),
            bounds: [bounds.min.map(f32::to_bits), bounds.max.map(f32::to_bits)],
            scene: self
                .scene
                .map_or(0, |value| std::ptr::from_ref(value).addr()),
            dynamic: self
                .visibility
                .map_or(0, |value| std::ptr::from_ref(value).addr()),
            dynamic_revision: observed,
            observed_dynamic_revision: observed,
            receiver,
        };
        if let Some(cached) = previous {
            let mut unchanged = key;
            unchanged.dynamic_revision = cached.dynamic_revision;
            // A moving receiver or changed resource already needs new samples;
            // it must not first scan an obsolete dependency set to learn that.
            if !unchanged.spatial_changed(Some(cached))
                && self.visibility.is_some_and(|visibility| {
                    visibility
                        .changed_regions_since(cached.observed_dynamic_revision)
                        .is_some_and(|regions| {
                            regions.iter().all(|dirty| {
                                !entity_receivers::visibility_affects(
                                    self.lighting,
                                    self.field,
                                    crate::engine::spatial::transform_bounds(&bounds, &model),
                                    *dirty,
                                ) || rays.is_some_and(|cache| !cache.visibility_affected(*dirty))
                            })
                        })
                })
            {
                key.dynamic_revision = cached.dynamic_revision;
            }
        }
        Some(key)
    }
}
