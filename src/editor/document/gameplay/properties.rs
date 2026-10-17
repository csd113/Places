//! Contextual properties and trigger wiring preserve raw source extensions.

use super::super::objects::{array_mut, object_mut};
use super::super::{Document, ObjectId, objects};
use super::triggers::set_trigger_policy;
use super::{GameplaySetting, unused_physics_group};
use crate::engine::assets::AssetCatalog;
use crate::engine::physics::config::{BodyMotion, PhysicsDef};
use serde_json::{Value, json};

impl Document {
    /// Changes the fixture carrier type while retaining manual coordinates.
    ///
    /// # Errors
    /// Rejects non-fixtures or invalid resulting source.
    pub fn toggle_light_mount(&mut self, id: ObjectId) -> Result<(), String> {
        let ObjectId::Light(index) = id else {
            return Err("Select a light fixture".into());
        };
        let light = self
            .level()
            .ceiling_lights
            .get(index)
            .ok_or("Fixture no longer exists")?;
        let mount = if light.mount == crate::engine::level::LightMount::Wall {
            "ceiling"
        } else {
            "wall"
        };
        let mut raw = self.raw().clone();
        super::pin_gameplay_ids(&mut raw, self.level())?;
        let object = object_mut(&mut raw, id)?;
        drop(object.insert("mount".into(), mount.into()));
        drop(
            object.insert(
                "fixture".into(),
                if mount == "wall" {
                    "core:pool_light_wall"
                } else {
                    "core:pool_light_round"
                }
                .into(),
            ),
        );
        drop(object.insert("align".into(), "none".into()));
        self.accept_edit(raw)
    }

    /// Reads the contextual property without replacing unsupported source data.
    #[must_use]
    pub fn gameplay_value(&self, id: ObjectId, setting: GameplaySetting) -> Value {
        if let Some(number) = resolved_light_value(self.level(), id, setting) {
            return json!(number);
        }
        let (collection, index) = id.collection();
        let Some(object) = self.raw().get(collection).and_then(|v| v.get(index)) else {
            return Value::Null;
        };
        if setting == GameplaySetting::Lifetime
            && let ObjectId::SpawnPoint(i) = id
        {
            return self
                .level()
                .spawn_points
                .get(i)
                .and_then(|p| {
                    self.level()
                        .spawn_templates
                        .iter()
                        .find(|t| t.id == p.template)
                })
                .and_then(|t| t.lifetime_seconds)
                .map_or_else(|| json!(0.0_f32), |v| json!(v));
        }
        let source = if matches!(
            setting,
            GameplaySetting::Once | GameplaySetting::Cooldown | GameplaySetting::ExitEvent
        ) {
            object
                .get("bindings")
                .and_then(|v| v.get(0))
                .or_else(|| object.get("editor_trigger_policy"))
                .unwrap_or(&Value::Null)
        } else if matches!(setting, GameplaySetting::Mass | GameplaySetting::Pickup) {
            object.get("physics").unwrap_or(&Value::Null)
        } else {
            object
        };
        if matches!(
            setting,
            GameplaySetting::Red | GameplaySetting::Green | GameplaySetting::Blue
        ) {
            let channel = if setting == GameplaySetting::Green {
                1
            } else if setting == GameplaySetting::Blue {
                2
            } else {
                0
            };
            return source
                .get("color")
                .and_then(|v| v.get(channel))
                .cloned()
                .unwrap_or_else(|| {
                    json!(
                        effective_color(self.level(), id)
                            .get(channel)
                            .copied()
                            .unwrap_or(1.0_f32)
                    )
                });
        }
        source
            .get(setting.key())
            .cloned()
            .unwrap_or_else(|| match setting {
                GameplaySetting::Enabled | GameplaySetting::Spatial => json!(true),
                GameplaySetting::Brightness
                | GameplaySetting::Emission
                | GameplaySetting::Gain
                | GameplaySetting::Mass => json!(1.0_f32),
                GameplaySetting::FogFalloff => json!(crate::engine::level::DEFAULT_FOG_FALLOFF_M),
                GameplaySetting::Range => json!(crate::engine::lighting::DEFAULT_LIGHT_RANGE_M),
                GameplaySetting::Swing => json!(90.0_f32),
                GameplaySetting::Speed => json!(120.0_f32),
                GameplaySetting::FogDensity => json!(0.06_f32),
                GameplaySetting::Cooldown => json!(0.0_f32),
                GameplaySetting::Aggregate => json!("per_actor"),
                GameplaySetting::ExitEvent => json!("enter_volume"),
                GameplaySetting::Red
                | GameplaySetting::Green
                | GameplaySetting::Blue
                | GameplaySetting::Switchable
                | GameplaySetting::Loop
                | GameplaySetting::Playing
                | GameplaySetting::Physics
                | GameplaySetting::Pickup
                | GameplaySetting::Solid
                | GameplaySetting::Bidirectional
                | GameplaySetting::Locked
                | GameplaySetting::Once
                | GameplaySetting::Lifetime => json!(false),
            })
    }

    /// Changes one supported property in one validated undo step.
    ///
    /// # Errors
    /// Invalid capabilities or out-of-range values leave the document unchanged.
    pub fn set_gameplay_value(
        &mut self,
        id: ObjectId,
        setting: GameplaySetting,
        value: Value,
        catalog: &AssetCatalog,
    ) -> Result<(), String> {
        let mut raw = self.raw().clone();
        if setting == GameplaySetting::Lifetime {
            let ObjectId::SpawnPoint(i) = id else {
                return Err("Lifetime belongs to a spawn point template".into());
            };
            let point = self
                .level()
                .spawn_points
                .get(i)
                .ok_or("Spawn point no longer exists")?;
            let template = self
                .level()
                .spawn_templates
                .iter()
                .position(|v| v.id == point.template)
                .ok_or("Spawn template no longer exists")?;
            let definition = array_mut(&mut raw, "spawn_templates")?
                .get_mut(template)
                .and_then(Value::as_object_mut)
                .ok_or("Invalid template")?;
            if value.as_f64() == Some(0.0_f64) {
                drop(definition.remove("lifetime_seconds"));
            } else {
                drop(definition.insert("lifetime_seconds".into(), value));
            }
        } else if setting == GameplaySetting::Physics {
            set_physics_enabled(
                &mut raw,
                self.level(),
                id,
                value.as_bool().unwrap_or(false),
                catalog,
            )?;
        } else if matches!(
            setting,
            GameplaySetting::Once | GameplaySetting::Cooldown | GameplaySetting::ExitEvent
        ) {
            set_trigger_policy(&mut raw, id, setting, value)?;
        } else if matches!(setting, GameplaySetting::Mass | GameplaySetting::Pickup) {
            let object = object_mut(&mut raw, id)?
                .get_mut("physics")
                .and_then(Value::as_object_mut)
                .ok_or("Enable physics first")?;
            drop(object.insert(setting.key().into(), value));
        } else if matches!(
            setting,
            GameplaySetting::Red | GameplaySetting::Green | GameplaySetting::Blue
        ) {
            let channel = if setting == GameplaySetting::Green {
                1
            } else if setting == GameplaySetting::Blue {
                2
            } else {
                0
            };
            let object = object_mut(&mut raw, id)?;
            let default_color = effective_color(self.level(), id);
            let color = object
                .entry("color")
                .or_insert_with(|| json!(default_color))
                .as_array_mut()
                .ok_or("Invalid color")?;
            *color.get_mut(channel).ok_or("Invalid color channel")? = value;
        } else {
            drop(object_mut(&mut raw, id)?.insert(setting.key().into(), value));
        }
        self.accept_edit(raw)
    }

    /// Assigns an installed built-in fixture, model or sound identity.
    ///
    /// # Errors
    /// Rejects invalid resulting scenes without changing history.
    pub fn set_gameplay_asset(
        &mut self,
        id: ObjectId,
        asset: &str,
        catalog: &AssetCatalog,
    ) -> Result<(), String> {
        let mut raw = self.raw().clone();
        objects::pin_prop_ids(&mut raw, self.level())?;
        super::pin_gameplay_ids(&mut raw, self.level())?;
        let key = match id {
            ObjectId::Light(_) => {
                let mount = if crate::engine::lighting::fixture_profile(asset).kind
                    == crate::engine::lighting::FixtureKind::WallSconce
                {
                    "wall"
                } else {
                    "ceiling"
                };
                drop(object_mut(&mut raw, id)?.insert("mount".into(), mount.into()));
                "fixture"
            }
            ObjectId::Audio(_) => "sound",
            ObjectId::SpawnPoint(i) => {
                if catalog.placeable(asset).is_none() {
                    return Err("Choose a built-in model".into());
                }
                let point = self
                    .level()
                    .spawn_points
                    .get(i)
                    .ok_or("Spawn point no longer exists")?;
                let template = self
                    .level()
                    .spawn_templates
                    .iter()
                    .position(|v| v.id == point.template)
                    .ok_or("Template no longer exists")?;
                drop(
                    array_mut(&mut raw, "spawn_templates")?
                        .get_mut(template)
                        .and_then(Value::as_object_mut)
                        .ok_or("Invalid template")?
                        .insert("model".into(), asset.into()),
                );
                return self.accept_edit(raw);
            }
            ObjectId::Prop(_) => {
                let entry = catalog.placeable(asset).ok_or("Choose a built-in model")?;
                if let Some(size) = entry.size {
                    drop(object_mut(&mut raw, id)?.insert("size".into(), json!(size)));
                }
                drop(object_mut(&mut raw, id)?.insert("solid".into(), json!(entry.solid)));
                "model"
            }
            ObjectId::Room(_)
            | ObjectId::Wall(_)
            | ObjectId::Ramp(_)
            | ObjectId::FloorRegion(_)
            | ObjectId::Door(_)
            | ObjectId::Trigger(_)
            | ObjectId::FogRegion(_)
            | ObjectId::GroundFog(_) => return Err("Choose a light, sound or entity".into()),
        };
        drop(object_mut(&mut raw, id)?.insert(key.into(), asset.into()));
        self.accept_edit(raw)
    }
}

fn set_physics_enabled(
    raw: &mut Value,
    level: &crate::engine::level::LevelDef,
    id: ObjectId,
    enabled: bool,
    catalog: &AssetCatalog,
) -> Result<(), String> {
    let ObjectId::Prop(index) = id else {
        return Err("Physics belongs to a prop".into());
    };
    objects::pin_prop_ids(raw, level)?;
    let object = object_mut(raw, id)?;
    if enabled {
        let prop = level.props.get(index).ok_or("Prop no longer exists")?;
        let mut profile = catalog
            .placeable(&prop.model)
            .and_then(|v| v.physics.clone())
            .unwrap_or_else(|| PhysicsDef {
                motion: BodyMotion::Dynamic,
                pickup: true,
                ..PhysicsDef::default()
            });
        if !profile.motion.is_movable() {
            profile.motion = BodyMotion::Dynamic;
        }
        if profile.reset_group.is_none() {
            profile.reset_group = Some(unused_physics_group(level)?);
        }
        drop(object.insert(
            "physics".into(),
            serde_json::to_value(profile).map_err(|e| e.to_string())?,
        ));
    } else {
        drop(object.remove("physics"));
    }
    Ok(())
}

fn effective_color(level: &crate::engine::level::LevelDef, id: ObjectId) -> [f32; 3] {
    let global = level.environment.unwrap_or_default().fog.color;
    match id {
        ObjectId::Light(index) => level
            .ceiling_lights
            .get(index)
            .map_or([1.0_f32; 3], |light| light.emitted_color().to_array()),
        ObjectId::FogRegion(index) => level
            .fog_regions
            .get(index)
            .and_then(|fog| fog.color)
            .unwrap_or(global),
        ObjectId::GroundFog(index) => level
            .ground_fog
            .get(index)
            .and_then(|fog| fog.color)
            .unwrap_or(global),
        ObjectId::Room(_)
        | ObjectId::Wall(_)
        | ObjectId::Prop(_)
        | ObjectId::Ramp(_)
        | ObjectId::FloorRegion(_)
        | ObjectId::Audio(_)
        | ObjectId::Door(_)
        | ObjectId::Trigger(_)
        | ObjectId::SpawnPoint(_) => [1.0_f32; 3],
    }
}

fn resolved_light_value(
    level: &crate::engine::level::LevelDef,
    id: ObjectId,
    setting: GameplaySetting,
) -> Option<f32> {
    if let ObjectId::Light(index) = id
        && let Some(light) = level.ceiling_lights.get(index)
    {
        let color = light.emitted_color();
        let resolved = match setting {
            GameplaySetting::Brightness => Some(light.intensity()),
            GameplaySetting::Range => Some(light.range()),
            GameplaySetting::Emission => Some(light.emission_intensity()),
            GameplaySetting::Red => Some(color.r),
            GameplaySetting::Green => Some(color.g),
            GameplaySetting::Blue => Some(color.b),
            GameplaySetting::Enabled
            | GameplaySetting::Switchable
            | GameplaySetting::Gain
            | GameplaySetting::Loop
            | GameplaySetting::Playing
            | GameplaySetting::Spatial
            | GameplaySetting::Physics
            | GameplaySetting::Pickup
            | GameplaySetting::Mass
            | GameplaySetting::Solid
            | GameplaySetting::Bidirectional
            | GameplaySetting::Locked
            | GameplaySetting::Swing
            | GameplaySetting::Speed
            | GameplaySetting::Once
            | GameplaySetting::Cooldown
            | GameplaySetting::Aggregate
            | GameplaySetting::ExitEvent
            | GameplaySetting::FogDensity
            | GameplaySetting::FogFalloff
            | GameplaySetting::Lifetime => None,
        };
        if let Some(number) = resolved {
            return Some(number);
        }
    }
    None
}
