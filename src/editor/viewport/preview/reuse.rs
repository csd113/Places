//! One bounded previous preview, keyed by each prepared part's dependencies.
use crate::engine::{
    level::{LevelDef, LevelSurfaces},
    loader::PropCatalog,
    materials::MaterialTable,
    render::LevelBuild,
};
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Default)]
pub(super) struct Reuse {
    previous: Option<Entry>,
    pub(super) inputs: String,
    pub(super) metrics: Value,
}
struct Entry {
    scene: Value,
    props: Value,
    build: BuildOwner,
    complete_props: bool,
    dependencies: crate::engine::render::PreviewDependencies,
    architecture: Value,
    inputs: String,
    environment: bool,
}

enum BuildOwner {
    Optional(Arc<LevelBuild>),
    Displayed(std::sync::Weak<LevelBuild>),
}
impl BuildOwner {
    fn get(&self) -> Option<Arc<LevelBuild>> {
        match self {
            Self::Optional(build) => Some(Arc::clone(build)),
            Self::Displayed(build) => build.upgrade(),
        }
    }
}

impl Reuse {
    #[cfg(test)]
    pub(super) fn display_owner_only(&mut self) -> Option<Arc<LevelBuild>> {
        let entry = self.previous.as_mut()?;
        let displayed = entry.build.get()?;
        entry.build = BuildOwner::Displayed(Arc::downgrade(&displayed));
        Some(displayed)
    }

    pub fn build(
        &mut self,
        level: &LevelDef,
        catalog: &PropCatalog,
        assets: &mut crate::engine::props::PropAssets,
        materials: &MaterialTable,
        environment: bool,
        inputs: &str,
    ) -> Result<(Arc<LevelBuild>, bool, bool), String> {
        crate::engine::resources::checkpoint()?;
        if self.previous.as_ref().is_some_and(|old| {
            old.scene
                .get("level")
                .and_then(|value| value.get("id"))
                .and_then(Value::as_str)
                != Some(level.id.as_str())
        }) {
            self.previous = None;
        }
        if self.inputs != inputs {
            crate::engine::lighting::set_preparation_assets(catalog.clone(), assets.clone());
            inputs.clone_into(&mut self.inputs);
        }
        let Identity {
            scene,
            props,
            architecture,
        } = identities(level, materials, environment, inputs)?;
        if let Some(old) = &self.previous
            && old.scene == scene
            && let Some(build) = old.build.get()
        {
            self.metrics = json!({"whole_build":true});
            return Ok((build, true, true));
        }
        let reusable = self
            .previous
            .as_ref()
            .filter(|old| !environment && old.complete_props && old.props == props)
            .and_then(|old| old.build.get());
        let previous = self
            .previous
            .as_ref()
            .filter(|old| old.inputs == inputs && old.environment == environment)
            .and_then(|old| old.build.get().map(|build| (old, build)));
        let (prepared, complete_props, dependencies) =
            crate::engine::render::build_authoring_preview_reusing(
                level,
                catalog,
                assets,
                materials,
                !environment,
                crate::engine::render::PreviewReuse {
                    inspect: reusable.as_ref().map(|build| build.batches.as_slice()),
                    previous: previous
                        .as_ref()
                        .map(|(old, build)| (build.as_ref(), &old.dependencies)),
                    same_architecture: previous.as_ref().is_some_and(|(old, _)| {
                        old.complete_props && old.architecture == architecture
                    }),
                },
            );
        crate::engine::resources::checkpoint()?;
        self.metrics = json!({"whole_build":false,"reused_prop_batches":dependencies.reused_batches,"prop_batches":dependencies.batches.len(),"reused_mesh":dependencies.reused_mesh,"equivalent_lighting":dependencies.equivalent_lighting,"lighting_record_bytes":dependencies.lighting.len()});
        let dependency_bytes = dependencies.lighting.capacity().saturating_add(
            dependencies
                .batches
                .iter()
                .fold(0_usize, |bytes, key| bytes.saturating_add(key.capacity())),
        );
        let build = Arc::new(prepared);
        let reused_props = reusable.is_some();
        // One previous identity, with weak reuse for oversized displayed builds.
        // A cancelled undisplayed result expires without retaining its geometry.
        let optional_limit = usize::try_from(crate::engine::resources::cpu_budget() / 4)
            .unwrap_or(usize::MAX)
            .min(512 * 1024 * 1024);
        self.previous = Some(Entry {
            scene,
            props,
            build: if build.retained_bytes().saturating_add(dependency_bytes) <= optional_limit {
                BuildOwner::Optional(Arc::clone(&build))
            } else {
                BuildOwner::Displayed(Arc::downgrade(&build))
            },
            complete_props,
            dependencies,
            architecture,
            inputs: inputs.to_owned(),
            environment,
        });
        Ok((build, false, reused_props))
    }
}

struct Identity {
    scene: Value,
    props: Value,
    architecture: Value,
}
fn identities(
    level: &LevelDef,
    materials: &MaterialTable,
    environment: bool,
    inputs: &str,
) -> Result<Identity, String> {
    let mut source = serde_json::to_value(level).map_err(|error| error.to_string())?;
    // Installation/character creation consume these fields; static geometry,
    // prop instancing and LevelLighting::bake do not. Unknown fields stay
    // in the key, conservatively invalidating reuse.
    if let Some(fields) = source.as_object_mut() {
        for field in [
            "name",
            "author",
            "description",
            "crosshair",
            "requires_full_lighting",
            "transitions",
            "scripts",
            "script_sources",
            "environment",
            "weather",
            "weather_alternate",
            "weather_cycle",
            "fog_regions",
            "ground_fog",
            "sound_emitters",
        ] {
            drop(fields.remove(field));
        }
    }
    let material_key: Vec<_> = materials
        .entries()
        .iter()
        .map(|entry| {
            let mut identity = entry.clone();
            identity.image = None;
            format!("{identity:?}")
        })
        .collect();
    let texture_keys: Vec<_> = materials
        .textures()
        .iter()
        .map(|texture| &texture.key)
        .collect();
    let scene = json!({"level":source,"environment":environment,"materials":material_key,"textures":texture_keys,"inputs":inputs});
    let surfaces = LevelSurfaces::new(level);
    // Inspect replaces prop RGB with source albedo/studio shading. Its
    // transformed geometry depends on support height and batching grid,
    // rather than unrelated walls or discarded light samples.
    let floors: Vec<_> = level
        .props
        .iter()
        .map(|prop| surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0).to_bits())
        .collect();
    let props = json!({"neutral":!environment,"props":level.props,"templates":level.spawn_templates,"fixtures":level.ceiling_lights,"rooms":level.room_iter().map(|room| (room.x,room.z,room.width,room.depth,room.floor_y,room.height, &room.ceiling)).collect::<Vec<_>>(),"floors":floors,"grid":format!("{:?}",crate::engine::render::spatial_cell_grid(level)),"materials":material_key,"textures":texture_keys,"inputs":inputs});
    let mut architecture_source = source.clone();
    if let Some(fields) = architecture_source.as_object_mut() {
        drop(fields.remove("props"));
    }
    let architecture = json!({"source":architecture_source,"grid":format!("{:?}",crate::engine::render::spatial_cell_grid(level)),"materials":material_key,"textures":texture_keys});
    Ok(Identity {
        scene,
        props,
        architecture,
    })
}
