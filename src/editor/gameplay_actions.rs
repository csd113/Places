//! Contextual authoring commands share engine definitions and one undo history.

use serde_json::{Value, json};

use super::App;
use super::actions::Popup;
use super::document::{GameplaySetting, ObjectId, ObjectKind};
use super::ui::Action;
use crate::engine::assets::AssetType;
use crate::engine::level::LevelSurfaces;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GameplayChoice {
    Fixture,
    Sound,
    EntityModel,
    EntityBehavior,
    SpawnTemplate,
    Filter,
    SoundAction,
    DoorAction,
    SpawnAction,
    PhysicsAction,
}

pub(super) const NUMBER_SETTINGS: [GameplaySetting; 14] = [
    GameplaySetting::Brightness,
    GameplaySetting::Range,
    GameplaySetting::Red,
    GameplaySetting::Green,
    GameplaySetting::Blue,
    GameplaySetting::Emission,
    GameplaySetting::Gain,
    GameplaySetting::Mass,
    GameplaySetting::Swing,
    GameplaySetting::Speed,
    GameplaySetting::Cooldown,
    GameplaySetting::FogDensity,
    GameplaySetting::FogFalloff,
    GameplaySetting::Lifetime,
];

pub(super) fn number_field(setting: GameplaySetting) -> Option<usize> {
    NUMBER_SETTINGS
        .iter()
        .position(|v| *v == setting)
        .map(|v| v.saturating_add(100))
}

impl App {
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Contextual gameplay commands are routed before the exhaustive application dispatcher."
    )]
    pub(super) fn gameplay_action(&mut self, action: Action) -> Result<bool, String> {
        match action {
            Action::MountSurface => {
                self.document
                    .as_mut()
                    .ok_or("Open a project first")?
                    .mount_to_surface(
                        self.selected.ok_or("Select a prop or fixture")?,
                        self.view.snap.then_some(self.step),
                    )?;
                self.changed();
                self.status = "Mounted to the carrier surface. Cmd/Ctrl Z to undo.".into();
            }
            Action::ToggleLightMount => {
                self.document
                    .as_mut()
                    .ok_or("Open a project first")?
                    .toggle_light_mount(self.selected.ok_or("Select a fixture")?)?;
                self.changed();
                self.status =
                    "Carrier type changed. Use Mount surface to align with geometry.".into();
            }
            Action::Create => self.popup = Some(Popup::Create),
            Action::Gameplay => self.popup = Some(Popup::Gameplay { page: 0 }),
            Action::GameplayPage(page) => self.popup = Some(Popup::Gameplay { page }),
            Action::GameplayActions => self.popup = Some(Popup::GameplayActions),
            Action::GamePick(choice) => {
                self.popup = Some(Popup::GameplayChoices { choice, page: 0 });
            }
            Action::GameChoose(choice, index) => self.choose_gameplay(choice, index)?,
            Action::GameSetting(setting) => {
                if let Some(field) = number_field(setting) {
                    self.popup = Some(Popup::Number {
                        field,
                        value: format!("{:.3}", self.field(field)?),
                    });
                } else {
                    let id = self.selected.ok_or("Select an object first")?;
                    let doc = self.document.as_mut().ok_or("Open a project first")?;
                    let current = doc.gameplay_value(id, setting);
                    let value = match setting {
                        GameplaySetting::Physics => json!(
                            matches!(id,ObjectId::Prop(i) if doc.level().props.get(i).is_some_and(|v|v.physics.is_none()))
                        ),
                        GameplaySetting::Aggregate => {
                            json!(if current.as_str() == Some("aggregate") {
                                "per_actor"
                            } else {
                                "aggregate"
                            })
                        }
                        GameplaySetting::ExitEvent => {
                            json!(if current.as_str() == Some("exit_volume") {
                                "enter_volume"
                            } else {
                                "exit_volume"
                            })
                        }
                        _ => json!(!current.as_bool().unwrap_or(false)),
                    };
                    doc.set_gameplay_value(id, setting, value, &self.catalog)?;
                    self.changed();
                    self.status = "Gameplay property updated. Cmd/Ctrl Z to undo.".into();
                }
            }
            Action::RemoveTriggerStep(index) => {
                self.document
                    .as_mut()
                    .ok_or("Open a project first")?
                    .remove_trigger_action(self.selected.ok_or("Select a trigger")?, index)?;
                self.changed();
            }
            Action::EnvironmentPreview => {
                self.viewport
                    .set_environment_preview(!self.viewport.environment_preview());
                self.changed();
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    pub(super) fn gameplay_choices(&self, choice: GameplayChoice) -> Vec<String> {
        let Some(doc) = self.document.as_ref() else {
            return Vec::new();
        };
        match choice {
            GameplayChoice::Fixture => self
                .catalog
                .entries()
                .into_iter()
                .filter(|e| e.asset_type.as_str() == AssetType::LIGHT)
                .map(|e| e.id.clone())
                .collect(),
            GameplayChoice::EntityModel => self
                .catalog
                .entries()
                .into_iter()
                .filter(|e| e.asset_type.as_str() == AssetType::ENTITY)
                .map(|e| e.id.clone())
                .collect(),
            GameplayChoice::Sound => self.sound_cues.clone(),
            GameplayChoice::EntityBehavior => [
                "Decorative / no AI",
                "Idle and observe",
                "Wander",
                "Follow player",
                "Flee player",
            ]
            .map(str::to_string)
            .to_vec(),
            GameplayChoice::SpawnTemplate => doc
                .level()
                .spawn_templates
                .iter()
                .map(|template| template.id.clone())
                .collect(),
            GameplayChoice::SoundAction => doc
                .level()
                .sound_emitters
                .iter()
                .map(|v| v.id.clone())
                .collect(),
            GameplayChoice::DoorAction => doc.level().doors.iter().map(|v| v.id.clone()).collect(),
            GameplayChoice::SpawnAction => doc
                .level()
                .spawn_points
                .iter()
                .map(|v| v.id.clone())
                .collect(),
            GameplayChoice::PhysicsAction => doc
                .level()
                .props
                .iter()
                .filter_map(|prop| prop.physics.as_ref()?.reset_group.clone())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect(),
            GameplayChoice::Filter => {
                let mut result = vec![
                    "Player".into(),
                    "Any player or item".into(),
                    "Any physical item".into(),
                    "Held items".into(),
                    "Unheld items".into(),
                ];
                let ids = doc.level().prop_instance_ids();
                for (i, prop) in doc.level().props.iter().enumerate() {
                    if prop.physics.is_some() {
                        result.push(format!("Item: {}", ids.get(i).cloned().unwrap_or_default()));
                    }
                }
                for id in ids {
                    result.push(format!("Entity: {id}"));
                }
                result.extend(
                    doc.level()
                        .doors
                        .iter()
                        .map(|v| format!("Entity: {}", v.id)),
                );
                result
            }
        }
    }

    fn choose_gameplay(&mut self, choice: GameplayChoice, index: usize) -> Result<(), String> {
        let value = self
            .gameplay_choices(choice)
            .get(index)
            .cloned()
            .ok_or("Choice expired")?;
        let id = self.selected.ok_or("Select an object")?;
        let doc = self.document.as_mut().ok_or("Open a project first")?;
        match choice {
            GameplayChoice::Fixture | GameplayChoice::Sound | GameplayChoice::EntityModel => {
                doc.set_gameplay_asset(id, &value, &self.catalog)?;
            }
            GameplayChoice::EntityBehavior => doc.set_entity_behavior(id, index, &self.catalog)?,
            GameplayChoice::SpawnTemplate => doc.set_spawn_template(id, &value)?,
            GameplayChoice::Filter => {
                let filter = match index {
                    0 => json!({"kind":"player"}),
                    1 => json!({"kind":"any"}),
                    2 => json!({"kind":"item"}),
                    3 => json!({"kind":"item","held":true}),
                    4 => json!({"kind":"item","held":false}),
                    _ => {
                        if let Some(instance) = value.strip_prefix("Item: ") {
                            json!({"kind":"item","instance":instance})
                        } else {
                            json!({"kind":"entity","instance":value.strip_prefix("Entity: ").ok_or("Invalid entity filter")?})
                        }
                    }
                };
                doc.set_trigger_filter(id, filter)?;
            }
            GameplayChoice::SoundAction => {
                let looping = doc
                    .level()
                    .sound_emitters
                    .iter()
                    .find(|emitter| emitter.id == value)
                    .is_some_and(|emitter| emitter.looped);
                doc.add_trigger_action(
                    id,
                    json!({"action":"play_sound","target":value,"loop":looping}),
                )?;
            }
            GameplayChoice::DoorAction => {
                doc.add_trigger_action(id, json!({"action":"open","target":value}))?;
            }
            GameplayChoice::SpawnAction => {
                doc.add_trigger_action(id, json!({"action":"spawn_entity","point":value}))?;
            }
            GameplayChoice::PhysicsAction => doc
                .add_trigger_action(id, json!({"action":"activate_physics_group","group":value}))?,
        }
        self.changed();
        self.popup = Some(Popup::Gameplay {
            page: usize::from(matches!(
                choice,
                GameplayChoice::SoundAction
                    | GameplayChoice::DoorAction
                    | GameplayChoice::SpawnAction
                    | GameplayChoice::PhysicsAction
            )),
        });
        self.status = "Updated gameplay configuration. Cmd/Ctrl Z to undo.".into();
        Ok(())
    }

    pub(super) fn add_gameplay(&mut self, kind: ObjectKind) -> Result<(), String> {
        let doc = self.document.as_mut().ok_or("Open a project first")?;
        let mut position = self.viewport.target();
        position[1] = LevelSurfaces::new(doc.level())
            .floor_y_at(position[0], position[2])
            .unwrap_or(0.0);
        if self.view.snap {
            position = position.map(|v| (v / self.step).round() * self.step);
        }
        let asset = match kind {
            ObjectKind::Light => Some("core:fluorescent_panel_01"),
            ObjectKind::Audio => Some("ambience_hvac"),
            ObjectKind::Entity | ObjectKind::SpawnPoint => Some("mannequin"),
            ObjectKind::Room
            | ObjectKind::Wall
            | ObjectKind::Prop
            | ObjectKind::Ramp
            | ObjectKind::FloorRegion
            | ObjectKind::Door
            | ObjectKind::Trigger
            | ObjectKind::FogRegion
            | ObjectKind::GroundFog => None,
        };
        self.selected = Some(doc.add_with_catalog(kind, position, asset, &self.catalog)?);
        let mount_error =
            if self.view.snap && matches!(kind, ObjectKind::Light | ObjectKind::Entity) {
                self.selected
                    .and_then(|id| doc.mount_to_surface(id, Some(self.step)).err())
            } else {
                None
            };
        self.changed();
        self.popup = Some(Popup::Gameplay { page: 0 });
        self.status =
            "Added object. Configure here; close to place and resize in the viewport.".into();
        if let Some(error) = mount_error {
            self.status = format!("Added object; adjust its mount: {error}");
        }
        Ok(())
    }

    pub(super) fn gameplay_number(&self, field: usize) -> Result<f32, String> {
        let setting = NUMBER_SETTINGS
            .get(field.saturating_sub(100))
            .copied()
            .ok_or("Unknown gameplay value")?;
        let value = self
            .document
            .as_ref()
            .ok_or("Open a project first")?
            .gameplay_value(self.selected.ok_or("Select an object")?, setting);
        serde_json::from_value::<f32>(value).map_err(|e| e.to_string())
    }

    pub(super) fn apply_gameplay_number(&mut self, field: usize, value: f32) -> Result<(), String> {
        let setting = NUMBER_SETTINGS
            .get(field.saturating_sub(100))
            .copied()
            .ok_or("Unknown gameplay value")?;
        self.document
            .as_mut()
            .ok_or("Open a project first")?
            .set_gameplay_value(
                self.selected.ok_or("Select an object")?,
                setting,
                Value::from(value),
                &self.catalog,
            )?;
        self.changed();
        Ok(())
    }
}
