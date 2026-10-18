//! Focused capability forms mutate raw source, then use ordinary semantic validation.
//! No typed level serialization replaces unknown authoring fields.

mod bindings;
mod identity;
mod values;

use serde_json::{Value, json};

use super::objects::object_mut;
use super::{Document, ObjectId, objects};

/// One supported form location. Component and source indices stay local to a selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Identity,
    Map,
    Spawn,
    Defaults,
    Presentation,
    Fog,
    Object,
    Physics,
    Filter,
    Component(&'static str),
    State(usize),
    NewState,
    Light(usize),
    Binding(usize),
    Condition(usize, usize),
}

/// The field's explicit input type; absent optional strings remove the override.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Text,
    Number,
    Bool,
    Vector,
    State,
    Choice(&'static [&'static str]),
}

/// A named scalar/vector editor field, never an arbitrary JSON source editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Field {
    pub target: Target,
    pub key: &'static str,
    pub label: &'static str,
    pub kind: Kind,
    pub default: &'static str,
}

impl Field {
    pub(crate) const fn new(
        target: Target,
        key: &'static str,
        label: &'static str,
        kind: Kind,
        default: &'static str,
    ) -> Self {
        Self {
            target,
            key,
            label,
            kind,
            default,
        }
    }

    pub(crate) fn fallback(self) -> Value {
        serde_json::from_str(self.default).unwrap_or(Value::Null)
    }
}

impl Document {
    pub(crate) fn component_object(&self, id: ObjectId) -> Option<&Value> {
        let (collection, index) = id.collection();
        self.raw().get(collection)?.get(index)
    }

    pub(crate) fn component_value(&self, id: ObjectId, field: Field) -> Value {
        let object = match field.target {
            Target::Map => Some(self.raw()),
            Target::Spawn => self.raw().get("spawn"),
            Target::Defaults => self.raw().get("defaults"),
            Target::Presentation => self
                .raw()
                .get("environment")
                .and_then(|value| value.get("presentation")),
            Target::Fog => self
                .raw()
                .get("environment")
                .and_then(|value| value.get("fog")),
            Target::Identity
            | Target::Object
            | Target::Physics
            | Target::Filter
            | Target::Component(_)
            | Target::NewState
            | Target::State(_)
            | Target::Light(_)
            | Target::Binding(_)
            | Target::Condition(_, _) => self.component_object(id),
        };
        values::read(object, field)
    }

    /// One form edit is atomic and undoable, including all reference checks.
    pub(crate) fn set_component_field(
        &mut self,
        id: ObjectId,
        field: Field,
        text: &str,
    ) -> Result<(), String> {
        let value = values::parse(field, text)?;
        if field.target == Target::NewState {
            return self.add_state(id, text);
        }
        if let Target::State(index) = field.target
            && field.key == "name"
        {
            return self.rename_state(id, index, text);
        }
        if field.target == Target::Identity {
            return self.rename_instance(id, value.as_str().ok_or("Identity must be text")?);
        }
        let mut raw = self.raw().clone();
        if !matches!(
            field.target,
            Target::Map | Target::Spawn | Target::Defaults | Target::Presentation | Target::Fog
        ) {
            objects::pin_prop_ids(&mut raw, self.level())?;
            super::gameplay::pin_gameplay_ids(&mut raw, self.level())?;
        }
        let record = match field.target {
            Target::Map => raw.as_object_mut().ok_or("Invalid map")?,
            Target::Spawn | Target::Defaults => raw
                .as_object_mut()
                .ok_or("Invalid map")?
                .entry(if field.target == Target::Spawn {
                    "spawn"
                } else {
                    "defaults"
                })
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .ok_or("Invalid map settings")?,
            Target::Presentation | Target::Fog => raw
                .as_object_mut()
                .ok_or("Invalid map")?
                .entry("environment")
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .ok_or("Invalid environment")?
                .entry(if field.target == Target::Fog {
                    "fog"
                } else {
                    "presentation"
                })
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .ok_or("Invalid atmosphere")?,
            Target::Identity
            | Target::Object
            | Target::Physics
            | Target::Filter
            | Target::Component(_)
            | Target::NewState
            | Target::State(_)
            | Target::Light(_)
            | Target::Binding(_)
            | Target::Condition(_, _) => {
                values::record_mut(object_mut(&mut raw, id)?, field.target)?
            }
        };
        if (field.kind == Kind::Text && text.is_empty()) || value.is_null() {
            drop(record.remove(field.key));
        } else {
            drop(record.insert(field.key.into(), value));
        }
        self.accept_edit(raw)
    }

    pub(crate) fn add_component(&mut self, id: ObjectId, kind: &str) -> Result<(), String> {
        if !matches!(id, ObjectId::Prop(_) | ObjectId::Door(_)) {
            return Err("Capabilities belong to a prop or door".into());
        }
        let value = match kind {
            "interactable" => {
                json!({"component":"interactable","prompt":"Interact","enabled":true})
            }
            "light" => {
                json!({"component":"light","enabled":true,"switchable":false,"emission_scale":1.0_f32})
            }
            "light_response" => {
                json!({"component":"light_response","enter_intensity":0.035_f32,"exit_intensity":0.005_f32,"release_seconds":0.2_f32})
            }
            "audio" => json!({"component":"audio","sound":"switch_click","gain":1.0_f32}),
            _ => return Err("Choose a supported capability".into()),
        };
        let mut raw = self.raw().clone();
        objects::pin_prop_ids(&mut raw, self.level())?;
        let components = component_array(object_mut(&mut raw, id)?)?;
        if components
            .iter()
            .any(|entry| entry.get("component").and_then(Value::as_str) == Some(kind))
        {
            return Err("The capability already exists".into());
        }
        components.push(value);
        self.accept_edit(raw)
    }

    pub(crate) fn remove_component(&mut self, id: ObjectId, kind: &str) -> Result<(), String> {
        let mut raw = self.raw().clone();
        component_array(object_mut(&mut raw, id)?)?
            .retain(|entry| entry.get("component").and_then(Value::as_str) != Some(kind));
        self.accept_edit(raw)
    }

    pub(crate) fn add_state(&mut self, id: ObjectId, name: &str) -> Result<(), String> {
        if !matches!(id, ObjectId::Prop(_) | ObjectId::Door(_)) {
            return Err("State belongs to a prop or door".into());
        }
        let mut raw = self.raw().clone();
        objects::pin_prop_ids(&mut raw, self.level())?;
        component_array(object_mut(&mut raw, id)?)?
            .push(json!({"component":"state","name":name,"value":false}));
        self.accept_edit(raw)
    }

    pub(crate) fn remove_state(&mut self, id: ObjectId, index: usize) -> Result<(), String> {
        let mut raw = self.raw().clone();
        let components = component_array(object_mut(&mut raw, id)?)?;
        if components
            .get(index)
            .and_then(|entry| entry.get("component"))
            .and_then(Value::as_str)
            != Some("state")
        {
            return Err("State selection expired".into());
        }
        drop(components.remove(index));
        self.accept_edit(raw)
    }

    pub(crate) fn use_catalog_fixed_shape(
        &mut self,
        id: ObjectId,
        catalog: &crate::engine::assets::AssetCatalog,
    ) -> Result<(), String> {
        let ObjectId::Prop(index) = id else {
            return Err("Select a prop".into());
        };
        let prop = self
            .level()
            .props
            .get(index)
            .ok_or("Prop selection expired")?;
        let mut physics = catalog
            .placeable(&prop.model)
            .and_then(|entry| entry.physics.clone())
            .filter(|profile| profile.shape.is_some())
            .ok_or("This installed model has no fitted collision profile")?;
        physics.motion = crate::engine::physics::config::BodyMotion::Fixed;
        physics.pickup = false;
        physics.reset_group = None;
        physics.water = None;
        let mut raw = self.raw().clone();
        drop(object_mut(&mut raw, id)?.insert(
            "physics".into(),
            serde_json::to_value(physics).map_err(|error| error.to_string())?,
        ));
        self.accept_edit(raw)
    }

    pub(crate) fn add_attached_light(&mut self, id: ObjectId) -> Result<(), String> {
        if !matches!(id, ObjectId::Prop(_)) {
            return Err("Attached sources belong to fixed props".into());
        }
        let mut raw = self.raw().clone();
        let sources = object_mut(&mut raw, id)?
            .entry("lights")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or("Invalid sources")?;
        if sources.len() >= crate::engine::level::MAX_PROP_LIGHTS {
            return Err("At most 16 attached sources are supported".into());
        }
        sources.push(json!({"shape":"point","offset":[0.0_f32,1.0_f32,0.0_f32],"intensity":1.0_f32,"range":5.0_f32,"color":[1.0_f32,1.0_f32,1.0_f32]}));
        self.accept_edit(raw)
    }

    pub(crate) fn remove_attached_light(
        &mut self,
        id: ObjectId,
        index: usize,
    ) -> Result<(), String> {
        let mut raw = self.raw().clone();
        let sources = object_mut(&mut raw, id)?
            .get_mut("lights")
            .and_then(Value::as_array_mut)
            .ok_or("No attached sources")?;
        if index >= sources.len() {
            return Err("Source selection expired".into());
        }
        drop(sources.remove(index));
        self.accept_edit(raw)
    }
}

fn component_array(object: &mut serde_json::Map<String, Value>) -> Result<&mut Vec<Value>, String> {
    object
        .entry("components")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| "Invalid components".into())
}
