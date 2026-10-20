//! Focused source forms: complete records are validated before admission.
mod sheets;
use super::{
    App,
    actions::Popup,
    ui::{self, Action, Hit},
};
use crate::engine::render::Vertex;
use serde_json::Value;
pub(super) use sheets::draw;
use std::borrow::Cow;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Command {
    Hub,
    Open(Tool),
    Field(usize),
    Page(bool),
    Record(bool),
    New,
    Preview,
    Apply,
    Delete,
    Slice,
    RouteStep(&'static str),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tool {
    Ramp,
    Stairs,
    Rail,
    Water,
    Sky,
    Weather,
    Sun,
    Ceiling,
    LocalFloor,
    GlobalPatch,
    GlobalRegion,
    Route,
}
#[derive(Clone, Copy)]
enum Kind {
    Number,
    Text,
    Bool,
}
#[derive(Clone)]
struct Field {
    path: Cow<'static, str>,
    label: &'static str,
    kind: Kind,
    default: &'static str,
}
const fn number(path: &'static str, label: &'static str, default: &'static str) -> Field {
    Field {
        path: Cow::Borrowed(path),
        label,
        kind: Kind::Number,
        default,
    }
}
const fn text(path: &'static str, label: &'static str, default: &'static str) -> Field {
    Field {
        path: Cow::Borrowed(path),
        label,
        kind: Kind::Text,
        default,
    }
}
const fn flag(path: &'static str, label: &'static str, default: &'static str) -> Field {
    Field {
        path: Cow::Borrowed(path),
        label,
        kind: Kind::Bool,
        default,
    }
}
impl Tool {
    const fn title(self) -> &'static str {
        match self {
            Self::Ramp => "Ramp",
            Self::Stairs => "Staircase",
            Self::Rail => "Guardrail",
            Self::Water => "Water volume",
            Self::Sky => "Sky",
            Self::Weather => "Snow / weather",
            Self::Sun => "Directional light",
            Self::Ceiling => "Room ceiling",
            Self::LocalFloor => "Selected floor override",
            Self::GlobalPatch => "GLOBAL floor patch",
            Self::GlobalRegion => "GLOBAL floor region",
            Self::Route => "Legacy route",
        }
    }
    const fn collection(self) -> &'static str {
        match self {
            Self::Ramp => "ramps",
            Self::Stairs => "stairs",
            Self::Rail => "guardrails",
            Self::Water => "water",
            Self::Sky => "sky",
            Self::Weather => "weather",
            Self::Sun => "global_illuminators",
            Self::Ceiling | Self::LocalFloor => "rooms",
            Self::Route => "routes",
            Self::GlobalPatch => "floor_patches",
            Self::GlobalRegion => "floor_regions",
        }
    }
    const fn singleton(self) -> bool {
        matches!(self, Self::Sky | Self::Weather)
    }
    fn fields(self) -> Vec<Field> {
        match self {
            Self::Route => vec![
                text("/id", "Actor instance ID", ""),
                flag("/loop", "Loop", "false"),
            ],
            Self::Ramp => vec![
                number("/x", "X", "0"),
                number("/z", "Z", "0"),
                number("/width", "Width m", "3"),
                number("/depth", "Depth m", "1"),
                number("/offset_y", "Floor-relative Y", "0"),
                number("/rise", "Signed rise m", "0.5"),
                text("/material", "Surface material", ""),
                text("/edge_material", "Side material", ""),
            ],
            Self::Stairs => vec![
                number("/x", "X", "0"),
                number("/z", "Z", "0"),
                number("/width", "Width m", "4"),
                number("/depth", "Depth m", "1.5"),
                number("/offset_y", "Floor-relative Y", "0"),
                number("/rise", "Rise m", "1"),
                number("/steps", "Step count", "5"),
                text("/material", "Tread material", ""),
                text("/riser_material", "Riser material", ""),
                text("/side_material", "Side material", ""),
            ],
            Self::Rail => vec![
                number("/x", "Start X", "0"),
                number("/z", "Start Z", "0"),
                number("/y", "World base Y / auto", ""),
                number("/length", "Length m", "3"),
                number("/rotation_degrees", "Yaw degrees", "0"),
                number("/rise", "Rise m / follow floor", ""),
                number("/height", "Height m", "1"),
                number("/post_spacing", "Post spacing m", "1.2"),
                text("/material", "Rail material", ""),
                text("/post_material", "Post material", ""),
            ],
            Self::Water => vec![
                number("/x", "X", "0"),
                number("/z", "Z", "0"),
                number("/width", "Width m / blank circle", "2"),
                number("/depth", "Depth m / blank circle", "2"),
                number("/surface_y", "World surface Y", "-0.2"),
                number("/bottom_y", "World bottom Y / auto", ""),
                flag("/swimming", "Swimming", "true"),
                number("/opacity", "Opacity / material", ""),
                text("/material", "Surface material", ""),
                number("/attenuation_per_metre", "Absorption per m", "0"),
                text("/shape", "Shape: rect / circle", "rect"),
                number("/radius", "Circle radius / blank rect", ""),
                number("/motion/speed", "Ripple speed (0 holds)", "1"),
                number("/motion/strength", "Ripple strength 0..1", "0.35"),
                number("/motion/flow_speed", "Current m/s (0 still)", "0"),
                number(
                    "/motion/direction_degrees",
                    "Flow yaw: 0 north, 90 east",
                    "0",
                ),
            ],
            Self::Sky | Self::Weather | Self::Sun => self.environment_fields(),
            Self::Ceiling => vec![text("/ceiling/kind", "Ceiling: flat / open", "flat")],
            Self::GlobalPatch => vec![
                number("/x", "X", "0"),
                number("/z", "Z", "0"),
                number("/width", "Width m", "2"),
                number("/depth", "Depth m", "2"),
                text("/material", "Material ID", "core:carpet_beige_01"),
            ],
            Self::GlobalRegion => vec![
                number("/x", "X", "0"),
                number("/z", "Z", "0"),
                number("/width", "Width m", "2"),
                number("/depth", "Depth m", "2"),
                number("/offset_y", "Offset on ALL carriers", "0"),
                text("/material", "Floor material ID", ""),
                text("/edge_material", "Edge material ID", ""),
            ],
            Self::LocalFloor => vec![
                number("/x", "X", "0"),
                number("/z", "Z", "0"),
                number("/width", "Width m", "2"),
                number("/depth", "Depth m", "2"),
                number("/offset_y", "Offset from selected floor", "0"),
                text("/material", "Floor material ID", ""),
            ],
        }
    }
}
impl Tool {
    fn environment_fields(self) -> Vec<Field> {
        match self {
            Self::Sky => vec![
                text(
                    "/texture",
                    "Installed 2:1 texture ID",
                    "winter:tex_sky_aurora_01",
                ),
                number("/brightness", "Visual brightness", "1"),
                number("/ambient", "Baked ambient", "0"),
                number("/ambient_color/0", "Ambient red", "0.42"),
                number("/ambient_color/1", "Ambient green", "0.52"),
                number("/ambient_color/2", "Ambient blue", "0.72"),
            ],
            Self::Weather => vec![
                text("/kind", "Kind", "snow"),
                number("/intensity", "Intensity", "1"),
                number("/storm_severity", "Storm severity", "0"),
                number("/visibility_m", "Visibility m", "5"),
                number("/wind/0", "Wind X m/s", "0.18"),
                number("/wind/1", "Wind Z m/s", "0.06"),
                number("/count", "Seed count", "1400"),
                number("/radius", "Radius m", "16"),
                number("/height", "Height m", "12"),
                number("/opacity", "Opacity", "0.85"),
            ],
            Self::Sun => vec![
                text("/id", "Source ID", "editor_moon"),
                text("/kind", "directional / sun / moon", "moon"),
                number("/direction/0", "Direction X", "0.2"),
                number("/direction/1", "Direction Y", "-1"),
                number("/direction/2", "Direction Z", "0.3"),
                number("/intensity", "Intensity", "0.2"),
                number("/color/0", "Red", "0.7"),
                number("/color/1", "Green", "0.8"),
                number("/color/2", "Blue", "1"),
                flag("/enabled", "Enabled", "true"),
                flag("/cast_shadows", "Cast shadows", "true"),
                flag("/bake", "Bake", "true"),
                number("/angular_size_degrees", "Angular size degrees", "0.5"),
            ],
            Self::Ramp
            | Self::Stairs
            | Self::Rail
            | Self::Water
            | Self::Ceiling
            | Self::LocalFloor
            | Self::GlobalPatch
            | Self::GlobalRegion
            | Self::Route => Vec::new(),
        }
    }
}

pub(super) struct Draft {
    pub tool: Tool,
    index: usize,
    fields: Vec<(Field, String)>,
    active: Option<usize>,
    page: usize,
    preview: bool,
    slice_z: bool,
    pub(super) environment_before: bool,
    expected: Value,
    original: Option<Value>,
}
impl Draft {
    fn new(app: &App, tool: Tool, requested: Option<usize>) -> Result<Self, String> {
        let doc = app.document.as_ref().ok_or("Open a project first")?;
        let raw = doc.raw();
        let collection = raw.get(tool.collection());
        let index = if matches!(tool, Tool::Ceiling | Tool::LocalFloor) {
            let Some(super::document::ObjectId::Room(room_index)) = app.selected else {
                return Err("Select the owning room floor first".into());
            };
            room_index
        } else {
            requested.unwrap_or_else(|| collection.and_then(Value::as_array).map_or(0, Vec::len))
        };
        let mut original = if tool.singleton() {
            collection.filter(|value| !value.is_null())
        } else {
            collection.and_then(|values| values.get(index))
        }
        .cloned();
        if tool == Tool::Route && original.is_none() {
            let Some(super::document::ObjectId::Prop(prop_index)) = app.selected else {
                return Err("Select an actor before creating a route".into());
            };
            let prop = doc.level().props.get(prop_index).ok_or("Select an actor")?;
            let id = doc
                .level()
                .prop_instance_ids()
                .get(prop_index)
                .cloned()
                .ok_or("Missing actor ID")?;
            original = Some(
                serde_json::json!({"id":id,"loop":false,"steps":[{"step":"move_to","x":prop.x,"z":prop.z,"speed":0.5_f32}]}),
            );
        }
        let definitions = if tool == Tool::Route {
            route_fields(original.as_ref())
        } else {
            tool.fields()
        };
        let mut fields = Vec::new();
        for field in definitions {
            let source_value = if tool == Tool::LocalFloor && field.path.as_ref() == "/offset_y" {
                None
            } else {
                original
                    .as_ref()
                    .and_then(|record| record.pointer(&field.path))
            };
            let content = source_value.map_or_else(
                || field.default.into(),
                |value| {
                    value
                        .as_str()
                        .map_or_else(|| value.to_string(), str::to_owned)
                },
            );
            fields.push((field, content));
        }
        Ok(Self {
            tool,
            index,
            fields,
            active: None,
            page: 0,
            preview: false,
            slice_z: false,
            environment_before: match &app.popup {
                Some(Popup::AuthoringForm(existing)) => existing.environment_before,
                _ => app.viewport.environment_preview(),
            },
            expected: raw.clone(),
            original,
        })
    }
    fn record(&self) -> Result<Value, String> {
        let mut record = if self.tool == Tool::LocalFloor {
            serde_json::json!({})
        } else {
            self.original
                .clone()
                .unwrap_or_else(|| serde_json::json!({}))
        };
        for (field, content) in &self.fields {
            let value = if content.trim().is_empty() {
                None
            } else {
                Some(match field.kind {
                    Kind::Text => Value::String(content.trim().into()),
                    Kind::Number => {
                        let value: Value = serde_json::from_str(content)
                            .map_err(|_error| format!("{} needs a finite number", field.label))?;
                        if !value.is_number() {
                            return Err(format!("{} needs a number", field.label));
                        }
                        value
                    }
                    Kind::Bool => Value::Bool(
                        content
                            .parse::<bool>()
                            .map_err(|_error| format!("{} needs true or false", field.label))?,
                    ),
                })
            };
            set_path(&mut record, &field.path, value)?;
        }
        Ok(record)
    }
    fn candidate(&self, doc: &super::document::Document) -> Result<Value, String> {
        if doc.raw() != &self.expected {
            return Err("Scene changed; reopen the authoring form".into());
        }
        let record = self.record()?;
        if self.tool == Tool::LocalFloor {
            return doc.local_floor_candidate(self.index, &record);
        }
        let mut raw = self.expected.clone();
        if self.tool.singleton() {
            drop(
                raw.as_object_mut()
                    .ok_or("Invalid level")?
                    .insert(self.tool.collection().into(), record),
            );
        } else {
            let values = super::document::record_array_mut(&mut raw, self.tool.collection())?;
            if self.index == values.len() {
                values.push(record);
            } else {
                *values
                    .get_mut(self.index)
                    .ok_or("Record no longer exists")? = record;
            }
        }
        Ok(raw)
    }
}
fn set_path(record: &mut Value, path: &str, value: Option<Value>) -> Result<(), String> {
    let segments: Vec<_> = path.trim_start_matches('/').split('/').collect();
    let mut target = record;
    for (index, key) in segments.iter().enumerate() {
        let last = index.saturating_add(1) == segments.len();
        if let Ok(slot) = key.parse::<usize>() {
            let values = target.as_array_mut().ok_or("Invalid vector field")?;
            while values.len() <= slot {
                values.push(Value::Null);
            }
            if last {
                *values.get_mut(slot).ok_or("Invalid vector index")? = value.unwrap_or(Value::Null);
                return Ok(());
            }
            target = values.get_mut(slot).ok_or("Invalid vector index")?;
        } else {
            let object = target.as_object_mut().ok_or("Invalid record field")?;
            if last {
                if let Some(entry) = value {
                    drop(object.insert((*key).into(), entry));
                } else {
                    drop(object.remove(*key));
                }
                return Ok(());
            }
            let array = segments
                .get(index.saturating_add(1))
                .is_some_and(|next| next.parse::<usize>().is_ok());
            target = object.entry((*key).to_owned()).or_insert_with(|| {
                if array {
                    serde_json::json!([])
                } else {
                    serde_json::json!({})
                }
            });
        }
    }
    Ok(())
}
impl App {
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "This handler owns only the authoring forms."
    )]
    pub(super) fn authoring_action(&mut self, action: Action) -> Result<bool, String> {
        match action {
            Action::Author(Command::Hub) => self.popup = Some(Popup::Authoring),
            Action::Author(Command::Open(tool)) => {
                self.popup = Some(Popup::AuthoringForm(Box::new(Draft::new(
                    self,
                    tool,
                    Some(0),
                )?)));
            }
            Action::Author(Command::Slice) => {
                if let Some(Popup::AuthoringForm(draft)) = &mut self.popup {
                    draft.slice_z = !draft.slice_z;
                }
            }
            Action::Author(Command::Field(index)) => {
                if let Some(Popup::AuthoringForm(draft)) = &mut self.popup {
                    draft.active = Some(index);
                    draft.preview = false;
                }
            }
            Action::Author(Command::Page(next)) => {
                if let Some(Popup::AuthoringForm(draft)) = &mut self.popup {
                    draft.page = if next {
                        draft.page.saturating_add(1)
                    } else {
                        draft.page.saturating_sub(1)
                    };
                    draft.active = None;
                }
            }
            Action::Author(Command::Record(next)) => {
                let Some(Popup::AuthoringForm(draft)) = &self.popup else {
                    return Ok(true);
                };
                let maximum = self
                    .document
                    .as_ref()
                    .and_then(|doc| doc.raw().get(draft.tool.collection()))
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len);
                let index = if next {
                    draft.index.saturating_add(1).min(maximum)
                } else {
                    draft.index.saturating_sub(1)
                };
                self.popup = Some(Popup::AuthoringForm(Box::new(Draft::new(
                    self,
                    draft.tool,
                    Some(index),
                )?)));
            }
            Action::Author(Command::New) => {
                let Some(Popup::AuthoringForm(draft)) = &self.popup else {
                    return Ok(true);
                };
                self.popup = Some(Popup::AuthoringForm(Box::new(Draft::new(
                    self, draft.tool, None,
                )?)));
            }
            Action::Author(Command::Preview) => self.apply_authoring_form(false)?,
            Action::Author(Command::Apply) => self.apply_authoring_form(true)?,
            Action::Author(Command::RouteStep(kind)) => self.edit_authoring_route(kind)?,
            Action::Author(Command::Delete) => self.delete_authoring_record()?,
            _ => return Ok(false),
        }
        Ok(true)
    }
    fn apply_authoring_form(&mut self, apply: bool) -> Result<(), String> {
        let Some(Popup::AuthoringForm(draft)) = &self.popup else {
            return Ok(());
        };
        let doc = self.document.as_ref().ok_or("Open a project first")?;
        let raw = draft.candidate(doc)?;
        let level = doc.validate_candidate(&raw)?;
        let global_modifier = matches!(draft.tool, Tool::GlobalPatch | Tool::GlobalRegion);
        crate::engine::project::validate_level_assets(&level, &self.catalog)?;
        if apply {
            self.viewport
                .set_environment_preview(draft.environment_before);
            self.document
                .as_mut()
                .ok_or("Open a project first")?
                .apply_authoring_edit(&draft.expected, raw)?;
            self.popup = None;
            self.changed();
            self.status = "Authored record applied. Undo restores the entire edit.".into();
        } else {
            if matches!(draft.tool, Tool::Sky | Tool::Weather | Tool::Sun) {
                self.viewport.set_environment_preview(true);
            }
            self.viewport.rebuild(&level);
            if draft.tool == Tool::Route {
                let record = draft.record()?;
                let path = route_preview(
                    &level,
                    record.get("id").and_then(Value::as_str).unwrap_or_default(),
                );
                self.viewport
                    .navigation_guides(Vec::new(), &path, None, None);
            }
            if let Some(Popup::AuthoringForm(preview_draft)) = &mut self.popup {
                preview_draft.preview = true;
                preview_draft.active = None;
            }
            self.status = if global_modifier {
                "GLOBAL modifier preview: affects every overlapping storey. Use selected-floor override for local ownership.".into()
            } else {
                "Validated preview; Apply commits, Cancel restores the scene.".into()
            };
        }
        Ok(())
    }
    fn edit_authoring_route(&mut self, kind: &str) -> Result<(), String> {
        let Some(Popup::AuthoringForm(draft)) = &self.popup else {
            return Ok(());
        };
        let mut record = draft.record()?;
        let steps = record
            .get_mut("steps")
            .and_then(Value::as_array_mut)
            .ok_or("Open a route first")?;
        if kind == "remove" {
            if steps.len() <= 1 {
                return Err("A route needs at least one step".into());
            }
            let _removed = steps.pop();
        } else {
            if steps.len() >= crate::engine::level::MAX_ROUTE_STEPS {
                return Err("Route step limit reached".into());
            }
            steps.push(match kind {
                "move" => serde_json::json!({"step":"move_to","x":0_f32,"z":0_f32,"speed":0.5_f32}),
                "wait" => serde_json::json!({"step":"wait","seconds":1_f32}),
                _ => serde_json::json!({"step":"play","clip":"idle","seconds":1_f32,"loop":false}),
            });
        }
        if let Some(Popup::AuthoringForm(updated)) = &mut self.popup {
            updated.fields = route_fields(Some(&record))
                .into_iter()
                .map(|field| {
                    let content = record.pointer(&field.path).map_or_else(
                        || field.default.into(),
                        |v| v.as_str().map_or_else(|| v.to_string(), str::to_owned),
                    );
                    (field, content)
                })
                .collect();
            updated.original = Some(record);
            updated.page = 0;
            updated.preview = false;
            updated.active = None;
        }
        Ok(())
    }
    fn delete_authoring_record(&mut self) -> Result<(), String> {
        let Some(Popup::AuthoringForm(draft)) = &self.popup else {
            return Ok(());
        };
        if matches!(draft.tool, Tool::Ceiling | Tool::LocalFloor) {
            return Err("Use scene Delete for a room".into());
        }
        let mut raw = draft.expected.clone();
        if draft.tool.singleton() {
            drop(
                raw.as_object_mut()
                    .ok_or("Invalid level")?
                    .remove(draft.tool.collection()),
            );
        } else {
            let values = super::document::record_array_mut(&mut raw, draft.tool.collection())?;
            if draft.index >= values.len() {
                return Err("No saved record to delete".into());
            }
            drop(values.remove(draft.index));
        }
        self.document
            .as_mut()
            .ok_or("Open a project first")?
            .apply_authoring_edit(&draft.expected, raw)?;
        self.popup = None;
        self.changed();
        Ok(())
    }
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Only form text and keyboard input is consumed."
    )]
    pub(super) fn authoring_event(&mut self, event: &sdl3::event::Event) -> bool {
        use sdl3::{
            event::Event,
            keyboard::{Keycode, Mod},
        };
        let Some(Popup::AuthoringForm(draft)) = &mut self.popup else {
            return false;
        };
        let Some(index) = draft.active else {
            return false;
        };
        let Some((_, value)) = draft.fields.get_mut(index) else {
            return false;
        };
        match event {
            Event::TextInput {
                text,
                timestamp: _,
                window_id: _,
            } => {
                if value.len().saturating_add(text.len()) <= 256 {
                    value.push_str(text);
                }
            }
            Event::KeyDown {
                keycode: Some(Keycode::A),
                keymod,
                timestamp: _,
                window_id: _,
                scancode: _,
                repeat: _,
                which: _,
                raw: _,
            } if keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD | Mod::LGUIMOD | Mod::RGUIMOD) => {
                value.clear();
            }
            Event::KeyDown {
                keycode: Some(Keycode::Backspace),
                timestamp: _,
                window_id: _,
                scancode: _,
                keymod: _,
                repeat: _,
                which: _,
                raw: _,
            } => {
                let _removed = value.pop();
            }
            Event::KeyDown {
                keycode: Some(Keycode::Return | Keycode::Tab),
                timestamp: _,
                window_id: _,
                scancode: _,
                keymod: _,
                repeat: _,
                which: _,
                raw: _,
            } => draft.active = None,
            Event::KeyDown {
                keycode: Some(Keycode::Escape),
                timestamp: _,
                window_id: _,
                scancode: _,
                keymod: _,
                repeat: _,
                which: _,
                raw: _,
            } => self.close_popup(),
            Event::MouseButtonUp {
                timestamp: _,
                window_id: _,
                which: _,
                mouse_btn: _,
                clicks: _,
                x: _,
                y: _,
            } => return false,
            _ => {}
        }
        true
    }
}

fn route_fields(record: Option<&Value>) -> Vec<Field> {
    let mut fields = Tool::Route.fields();
    if let Some(steps) = record
        .and_then(|r| r.get("steps"))
        .and_then(Value::as_array)
    {
        for (index, step) in steps.iter().enumerate() {
            let definitions = match step.get("step").and_then(Value::as_str) {
                Some("move_to") => vec![
                    number("/x", "Move X", "0"),
                    number("/z", "Move Z", "0"),
                    number("/speed", "Speed m/s", "0.5"),
                ],
                Some("wait") => vec![number("/seconds", "Wait seconds", "1")],
                Some("face") => vec![number("/yaw_degrees", "Face degrees", "0")],
                Some("play") => vec![
                    text("/clip", "Pose clip", "idle"),
                    number("/seconds", "Pose seconds", "1"),
                    flag("/loop", "Loop pose", "false"),
                ],
                _ => Vec::new(),
            };
            for mut field in definitions {
                field.path = Cow::Owned(format!("/steps/{index}{}", field.path));
                fields.push(field);
            }
        }
    }
    fields
}

fn route_preview(level: &crate::engine::level::LevelDef, id: &str) -> Vec<glam::Vec3> {
    let routes = crate::engine::entity::EntityRoutes::from_level(level);
    let Some(route) = routes.get(id) else {
        return Vec::new();
    };
    let floor = crate::engine::level::WalkableFloor::from_level(level);
    let mut path = vec![route.spawn_position];
    let mut position = route.spawn_position;
    for step in &route.steps {
        if let crate::engine::level::RouteStepDef::MoveTo { x, z, speed: _ } = step {
            let start = position;
            for sample in 1_u16..=128 {
                let fraction = f32::from(sample) / 128.0;
                let sx = (*x - start.x).mul_add(fraction, start.x);
                let sz = (*z - start.z).mul_add(fraction, start.z);
                let Some(height) = floor.walk_height_near(sx, sz, position.y, route.step_height)
                else {
                    break;
                };
                position = glam::Vec3::new(sx, height, sz);
                path.push(position);
            }
        }
    }
    path
}
