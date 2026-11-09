//! Focused source forms: complete records are validated before admission.
mod creatures;
mod elevators;
mod generators;
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
    Create(Tool),
    Field(usize),
    Page(bool),
    ToggleWall(usize),
    Record(bool),
    New,
    Preview,
    Apply,
    Delete,
    Slice,
    RouteStep(&'static str),
    ElevatorStop(bool),
    EditElevator(usize),
    NativeList(&'static str),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tool {
    Wall,
    Light,
    Shaft,
    Feltling,
    TallGuest,
    Elevator,
    Recess,
    Surround,
    RailPath,
    Tunnel,
    Column,
    Pillar,
    ArcWall,
    WallTrim,
    Room,
    Ramp,
    Stairs,
    Rail,
    Water,
    Sky,
    Weather,
    Sun,
    Ceiling,
    AcousticMaterial,
    LocalFloor,
    GlobalPatch,
    GlobalRegion,
    Route,
}
#[derive(Clone, Copy)]
enum Kind {
    Json,
    Number,
    Text,
    Bool,
    Level,
    Arrival,
    InitialStop,
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
            Self::Wall => "Wall / numeric placement",
            Self::Light => "Light / numeric placement",
            Self::Shaft => "Stair shaft / numeric cut",
            Self::Feltling => "Native Feltling / feet coordinates",
            Self::TallGuest => "Native Tall Guest / feet coordinates",
            Self::Elevator => "Elevator / floor travel",
            Self::Recess => "Wall recess / TV niche",
            Self::Surround => "Door / window surround",
            Self::RailPath => "Railing / fence path",
            Self::Tunnel => "Crouch tunnel",
            Self::Column => "Rectangular pillar",
            Self::Pillar => "Circular pillar",
            Self::ArcWall => "Curved wall",
            Self::WallTrim => "Automatic wall trim",
            Self::Room => "New Room",
            Self::Ramp => "Ramp",
            Self::Stairs => "Staircase",
            Self::Rail => "Guardrail",
            Self::Water => "Water volume",
            Self::Sky => "Sky",
            Self::Weather => "Snow / weather",
            Self::Sun => "Directional light",
            Self::Ceiling => "Room ceiling / acoustics",
            Self::AcousticMaterial => "Acoustic material",
            Self::LocalFloor => "Selected room override",
            Self::GlobalPatch => "GLOBAL surface patch",
            Self::GlobalRegion => "GLOBAL surface region",
            Self::Route => "Legacy route",
        }
    }
    const fn collection(self) -> &'static str {
        match self {
            Self::Light => "ceiling_lights",
            Self::Elevator => "elevators",
            Self::Recess | Self::Surround => "openings",
            Self::Tunnel => "tunnels",
            Self::Column => "columns",
            Self::Pillar => "pillars",
            Self::ArcWall => "arc_walls",
            Self::Wall | Self::WallTrim => "walls",
            Self::Feltling | Self::TallGuest => "props",
            Self::Ramp => "ramps",
            Self::Stairs => "stairs",
            Self::Rail | Self::RailPath => "guardrails",
            Self::Water => "water",
            Self::AcousticMaterial => "acoustic_materials",
            Self::Sky => "sky",
            Self::Weather => "weather",
            Self::Sun => "global_illuminators",
            Self::Shaft | Self::Room | Self::Ceiling | Self::LocalFloor => "rooms",
            Self::Route => "routes",
            Self::GlobalPatch => "floor_patches",
            Self::GlobalRegion => "floor_regions",
        }
    }
    const fn singleton(self) -> bool {
        matches!(self, Self::Sky | Self::Weather)
    }
    fn pillar_fields(self) -> Vec<Field> {
        let mut fields = vec![
            number(
                "/x",
                if self == Self::Column {
                    "X / minimum corner"
                } else {
                    "Centre X"
                },
                "0",
            ),
            number(
                "/z",
                if self == Self::Column {
                    "Z / minimum corner"
                } else {
                    "Centre Z"
                },
                "0",
            ),
        ];
        if self == Self::Column {
            fields.extend([
                number("/width", "Width m", "0.6"),
                number("/depth", "Depth m", "0.6"),
            ]);
        } else {
            fields.extend([
                number("/radius", "Radius m", "0.35"),
                number("/segments", "Segments 3..128", "32"),
            ]);
        }
        fields.extend([
            number("/y", "Base Y / blank = floor", ""),
            number("/height", "Height / blank = ceiling", ""),
            text(
                "/material",
                "Wall material / catalog ID",
                "core:wallpaper_yellow_01",
            ),
        ]);
        if self == Self::ArcWall {
            fields.extend([
                number("/thickness", "Wall thickness m", "0.2"),
                number("/start_degrees", "Start degrees", "0"),
                number("/sweep_degrees", "Signed sweep degrees", "90"),
            ]);
        }
        fields.extend(trim_fields());
        fields
    }
    fn fields(self) -> Vec<Field> {
        match self {
            Self::Feltling | Self::TallGuest => Vec::new(),
            Self::Wall => wall_fields(),
            Self::Shaft => shaft_fields(),
            Self::Light => light_fields(),
            Self::Recess | Self::Surround | Self::RailPath | Self::Tunnel => {
                generators::fields(self)
            }
            Self::Column | Self::Pillar | Self::ArcWall => self.pillar_fields(),
            Self::WallTrim => {
                let mut fields = vec![text(
                    "/wall_selection",
                    "Wall numbers (comma separated)",
                    "1",
                )];
                fields.extend(trim_fields());
                if let Some(field) = fields
                    .iter_mut()
                    .find(|field| field.path.as_ref() == "/trim/placement")
                {
                    field.default = "bottom";
                }
                fields
            }
            Self::Room => room_fields(),
            Self::Elevator => elevators::fields(None),
            Self::Route => route_header_fields(),
            Self::Ramp => vec![
                number("/x", "X", "0"),
                number("/z", "Z", "0"),
                number("/width", "Width m", "3"),
                number("/depth", "Depth m", "1"),
                number("/offset_y", "Room-relative Y", "0"),
                number("/rise", "Signed rise m", "0.5"),
                text("/material", "Surface material", ""),
                text("/edge_material", "Side material", ""),
            ],
            Self::Stairs => stair_fields(),
            Self::Rail => vec![
                number("/x", "Start X", "0"),
                number("/z", "Start Z", "0"),
                number("/y", "World base Y / auto", ""),
                number("/length", "Length m", "3"),
                number("/rotation_degrees", "Yaw degrees", "0"),
                number("/rise", "Rise m / follow surface", ""),
                number("/height", "Height m", "1"),
                number("/post_spacing", "Post spacing m", "1.2"),
                text("/material", "Rail material", ""),
                text("/post_material", "Post material", ""),
            ],
            Self::Water => water_fields(),
            Self::Sky | Self::Weather | Self::Sun => self.environment_fields(),
            Self::Ceiling => vec![
                text("/ceiling/kind", "Ceiling: flat / open", "flat"),
                number("/acoustics/wet", "Room reflections 0..0.5", "0.18"),
                number("/acoustics/decay_scale", "Decay multiplier 0.25..2", "1"),
            ],
            Self::AcousticMaterial => vec![
                text("/material", "Existing material ID", "core:carpet_beige_01"),
                number("/absorption/0", "Low absorption 0..1", "0.2"),
                number("/absorption/1", "Mid absorption 0..1", "0.65"),
                number("/absorption/2", "High absorption 0..1", "0.9"),
                number("/transmission", "Solid transmission 0..1", "0.1"),
            ],
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
                text("/material", "Surface material ID", ""),
                text("/edge_material", "Edge material ID", ""),
            ],
            Self::LocalFloor => vec![
                number("/x", "X", "0"),
                number("/z", "Z", "0"),
                number("/width", "Width m", "2"),
                number("/depth", "Depth m", "2"),
                number("/offset_y", "Offset from room base", "0"),
                text("/material", "Surface material ID", ""),
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
                number("/azimuth_degrees", "Bearing degrees (0 = -Z)", "135"),
                number("/elevation_degrees", "Elevation above horizon", "45"),
                flag("/show_disc", "Show sun/moon texture", "true"),
                number(
                    "/visual/angular_size_degrees",
                    "Visual diameter degrees",
                    "6",
                ),
                number("/visual/brightness", "Visual brightness", "1"),
                number("/intensity", "Intensity", "0.2"),
                number("/color/0", "Red", "0.7"),
                number("/color/1", "Green", "0.8"),
                number("/color/2", "Blue", "1"),
                flag("/enabled", "Enabled", "true"),
                flag("/cast_shadows", "Cast shadows", "true"),
                flag("/bake", "Bake", "true"),
                number("/angular_size_degrees", "Angular size degrees", "0.5"),
            ],
            Self::Wall
            | Self::Light
            | Self::Shaft
            | Self::Feltling
            | Self::TallGuest
            | Self::Elevator
            | Self::Recess
            | Self::Surround
            | Self::RailPath
            | Self::Tunnel
            | Self::Column
            | Self::Pillar
            | Self::ArcWall
            | Self::WallTrim
            | Self::Ramp
            | Self::Stairs
            | Self::Rail
            | Self::Water
            | Self::Ceiling
            | Self::AcousticMaterial
            | Self::LocalFloor
            | Self::GlobalPatch
            | Self::GlobalRegion
            | Self::Route
            | Self::Room => Vec::new(),
        }
    }
}

fn water_fields() -> Vec<Field> {
    vec![
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
    ]
}

fn trim_fields() -> Vec<Field> {
    vec![
        text(
            "/trim/placement",
            "Trim: none / bottom / top / both",
            "none",
        ),
        number("/trim/height", "Trim height m", "0.09"),
        number("/trim/thickness", "Trim thickness m", "0.018"),
        text(
            "/trim/material",
            "Trim material / catalog ID",
            "home:baseboard_white_01",
        ),
    ]
}

fn form_index(
    app: &App,
    tool: Tool,
    requested: Option<usize>,
    collection: Option<&Value>,
) -> Result<usize, String> {
    Ok(if creatures::native(tool) {
        let Some(super::document::ObjectId::Prop(index)) = app.selected else {
            return Err("Select a native creature catalog prop first".into());
        };
        index
    } else if matches!(tool, Tool::Ceiling | Tool::LocalFloor) {
        let Some(super::document::ObjectId::Room(room_index)) = app.selected else {
            return Err("Select the owning room first".into());
        };
        room_index
    } else {
        requested.unwrap_or_else(|| collection.and_then(Value::as_array).map_or(0, Vec::len))
    })
}

pub(super) struct Draft {
    owner: Option<usize>,
    pub tool: Tool,
    native_cursor: usize,
    native_group: &'static str,
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
        let owner = generators::owner(app, tool)?;
        let collection = generators::collection(raw, tool, owner);
        let index = form_index(app, tool, requested, collection)?;
        let mut original = if tool.singleton() {
            collection.filter(|value| !value.is_null())
        } else {
            collection.and_then(|values| values.get(index))
        }
        .filter(|_| tool != Tool::Shaft)
        .cloned();
        generators::initialize(app, tool, owner, &mut original)?;
        creatures::initialize(app, tool, &mut original)?;
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
        if matches!(tool, Tool::Column | Tool::Pillar | Tool::ArcWall) && original.is_none() {
            let [x, _, z] = app.viewport.target();
            original = Some(serde_json::json!({"x":x,"z":z}));
        }
        if tool == Tool::WallTrim && original.is_none() {
            return Err("Select a wall before opening Automatic trim".into());
        }
        if tool == Tool::Elevator && original.is_none() {
            original = Some(elevators::new_record(app));
        }
        let definitions = form_fields(tool, original.as_ref());
        let original_light = original_light(original.as_ref());
        let mut fields = Vec::new();
        for field in definitions {
            let derived = if tool == Tool::Sun {
                match field.path.as_ref() {
                    "/azimuth_degrees" => original_light
                        .as_ref()
                        .map(|light| Value::from(light.celestial_angles()[0])),
                    "/elevation_degrees" => original_light
                        .as_ref()
                        .map(|light| Value::from(light.celestial_angles()[1])),
                    "/show_disc" => original.as_ref().map(|value| {
                        Value::Bool(value.get("visual").is_some_and(|visual| !visual.is_null()))
                    }),
                    _ => None,
                }
            } else {
                None
            };
            let source_value = if tool == Tool::LocalFloor && field.path.as_ref() == "/offset_y" {
                None
            } else {
                original
                    .as_ref()
                    .and_then(|record| record.pointer(&field.path))
            };
            let content = derived.as_ref().or(source_value).map_or_else(
                || field.default.into(),
                |value| {
                    value
                        .as_str()
                        .map_or_else(|| value.to_string(), str::to_owned)
                },
            );
            let field_content =
                if tool == Tool::WallTrim && field.path.as_ref() == "/wall_selection" {
                    index.saturating_add(1).to_string()
                } else {
                    content
                };
            fields.push((field, field_content));
        }
        Ok(Self {
            owner,
            tool,
            native_cursor: 0,
            native_group: "patrol",
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
    fn toggle_wall(&mut self, index: usize) {
        if self.tool != Tool::WallTrim {
            return;
        }
        if let Some((_, value)) = self
            .fields
            .iter_mut()
            .find(|(field, _)| field.path.as_ref() == "/wall_selection")
        {
            let number = index.saturating_add(1);
            let mut selected: std::collections::BTreeSet<usize> = value
                .split(',')
                .filter_map(|part| part.trim().parse().ok())
                .collect();
            if !selected.remove(&number) {
                let _inserted = selected.insert(number);
            }
            *value = selected
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(",");
            self.preview = false;
        }
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
                    Kind::Json => serde_json::from_str(content)
                        .map_err(|e| format!("{}: {e}", field.label))?,
                    Kind::Text | Kind::Level | Kind::Arrival => {
                        Value::String(content.trim().into())
                    }
                    Kind::Number | Kind::InitialStop => {
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
        if creatures::native(self.tool) {
            creatures::normalize(self.tool, &mut record)?;
        }
        if self.tool == Tool::Elevator {
            elevators::normalize(&record)?;
        }
        if self.tool == Tool::Sun {
            let object = record.as_object_mut().ok_or("Invalid directional source")?;
            let azimuth = serde_json::from_value::<f32>(
                object
                    .remove("azimuth_degrees")
                    .ok_or("Enter bearing degrees")?,
            )
            .map_err(|error| error.to_string())?;
            let elevation = serde_json::from_value::<f32>(
                object
                    .remove("elevation_degrees")
                    .ok_or("Enter elevation degrees")?,
            )
            .map_err(|error| error.to_string())?;
            let direction = crate::engine::level::GlobalIlluminatorDef::direction_from_angles(
                azimuth, elevation,
            )?;
            drop(object.insert("direction".into(), serde_json::json!(direction)));
            if object.remove("show_disc").and_then(|value| value.as_bool()) != Some(true) {
                drop(object.remove("visual"));
            }
        }
        if self.tool == Tool::Recess
            && record.pointer("/surround/width").is_none()
            && let Some(object) = record.as_object_mut()
        {
            drop(object.remove("surround"));
        }
        Ok(record)
    }
    fn candidate(&self, doc: &super::document::Document) -> Result<Value, String> {
        if doc.raw() != &self.expected {
            return Err("Scene changed; reopen the authoring form".into());
        }
        let mut record = self.record()?;
        if self.tool == Tool::Shaft {
            return super::stairs::numeric_shaft_candidate(doc, &record);
        }
        if self.tool == Tool::WallTrim {
            let selection = record
                .as_object_mut()
                .ok_or("Invalid trim draft")?
                .remove("wall_selection")
                .ok_or("Choose wall numbers")?;
            let indices: std::collections::BTreeSet<usize> = selection
                .as_str()
                .ok_or("Enter comma separated wall numbers")?
                .split(',')
                .map(|part| {
                    part.trim()
                        .parse::<usize>()
                        .ok()
                        .and_then(|i| i.checked_sub(1))
                        .ok_or_else(|| {
                            "Use wall numbers from the scene list, for example 1,3,4".to_string()
                        })
                })
                .collect::<Result<_, _>>()?;
            let trim = record.get("trim").cloned().ok_or("Choose trim placement")?;
            let mut raw = self.expected.clone();
            let walls = super::document::record_array_mut(&mut raw, "walls")?;
            for index in indices {
                let wall = walls
                    .get_mut(index)
                    .and_then(Value::as_object_mut)
                    .ok_or("A selected wall no longer exists")?;
                drop(wall.insert("trim".into(), trim.clone()));
            }
            return Ok(raw);
        }
        if self.tool == Tool::LocalFloor {
            return doc.local_floor_candidate(self.index, &record);
        }
        let mut raw = self.expected.clone();
        if self.tool == Tool::Elevator {
            elevators::link_destinations(doc, &record, &mut raw)?;
        }
        if self.tool.singleton() {
            drop(
                raw.as_object_mut()
                    .ok_or("Invalid level")?
                    .insert(self.tool.collection().into(), record),
            );
        } else {
            let values = generators::collection_mut(&mut raw, self.tool, self.owner)?;
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
            Action::Author(Command::EditElevator(index)) => {
                self.popup = Some(Popup::AuthoringForm(Box::new(Draft::new(
                    self,
                    Tool::Elevator,
                    Some(index),
                )?)));
            }
            Action::Author(Command::NativeList(operation)) => {
                if let Some(Popup::AuthoringForm(draft)) = &mut self.popup {
                    creatures::edit_list(
                        self.document.as_ref().ok_or("Open a project")?,
                        draft,
                        operation,
                    )?;
                }
            }
            Action::Author(Command::Hub) => self.popup = Some(Popup::Authoring),
            Action::Author(Command::Create(tool)) => {
                self.popup = Some(Popup::AuthoringForm(Box::new(Draft::new(
                    self, tool, None,
                )?)));
            }
            Action::Author(Command::Open(tool)) => {
                let index = generators::open_index(self, tool);
                self.popup = Some(Popup::AuthoringForm(Box::new(Draft::new(
                    self,
                    tool,
                    Some(index),
                )?)));
            }
            Action::Author(Command::Slice) => {
                if let Some(Popup::AuthoringForm(draft)) = &mut self.popup {
                    draft.slice_z = !draft.slice_z;
                }
            }
            Action::Author(Command::ElevatorStop(add)) => {
                if let Some(Popup::AuthoringForm(draft)) = &mut self.popup {
                    elevators::edit_stop(draft, add)?;
                }
            }
            Action::Author(Command::Field(index)) => self.select_authoring_field(index)?,
            Action::Author(Command::ToggleWall(index)) => {
                if let Some(Popup::AuthoringForm(draft)) = &mut self.popup {
                    draft.toggle_wall(index);
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
                    .and_then(|doc| generators::collection(doc.raw(), draft.tool, draft.owner))
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
            Action::Author(Command::Preview) => self.checked_authoring_form(false)?,
            Action::Author(Command::Apply) => self.checked_authoring_form(true)?,
            Action::Author(Command::RouteStep(kind)) => self.edit_authoring_route(kind)?,
            Action::Author(Command::Delete) => self.delete_authoring_record()?,
            _ => return Ok(false),
        }
        Ok(true)
    }
    fn select_authoring_field(&mut self, index: usize) -> Result<(), String> {
        if let Some(Popup::AuthoringForm(draft)) = &mut self.popup
            && let Some(doc) = &self.document
            && elevators::choose(doc, draft, index)?
        {
            return Ok(());
        }
        if let Some(Popup::AuthoringForm(draft)) = &mut self.popup {
            draft.native_cursor = index;
            if creatures::choose(
                self.document.as_ref().ok_or("Open a project")?,
                draft,
                index,
            )? {
                return Ok(());
            }
            if let Some((field, value)) = draft.fields.get_mut(index)
                && field.path.as_ref() == "/trim/placement"
            {
                *value = match value.as_str() {
                    "none" => "bottom",
                    "bottom" => "top",
                    "top" => "both",
                    _ => "none",
                }
                .into();
                draft.active = None;
            } else {
                draft.active = Some(index);
            }
            draft.preview = false;
        }
        Ok(())
    }
    fn checked_authoring_form(&mut self, apply: bool) -> Result<(), String> {
        let result = self.apply_authoring_form(apply);
        if result.is_err() {
            if let Some(doc) = &self.document {
                self.viewport.rebuild(doc.level());
            }
            if let Some(Popup::AuthoringForm(draft)) = &mut self.popup {
                draft.preview = false;
            }
        }
        result
    }
    fn apply_authoring_form(&mut self, apply: bool) -> Result<(), String> {
        let Some(Popup::AuthoringForm(draft)) = &self.popup else {
            return Ok(());
        };
        let doc = self.document.as_ref().ok_or("Open a project first")?;
        if draft.tool == Tool::Shaft {
            super::stairs::check_numeric_shaft(doc, &draft.record()?, &self.catalog)?;
        }
        let raw = draft.candidate(doc)?;
        let level = doc.validate_candidate(&raw)?;
        self.check_focused_edit(&raw)?;
        let global_modifier = matches!(draft.tool, Tool::GlobalPatch | Tool::GlobalRegion);
        crate::engine::project::validate_level_assets(&level, &self.catalog)?;
        if draft.tool == Tool::Room {
            validate_room_overlap(&level, draft.index)?;
        }
        if apply {
            self.viewport
                .set_environment_preview(draft.environment_before);
            self.document
                .as_mut()
                .ok_or("Open a project first")?
                .apply_authoring_edit(&draft.expected, raw)?;
            match draft.tool {
                Tool::Light => {
                    self.selected = Some(super::document::ObjectId::Light(draft.index));
                }
                Tool::Shaft => {
                    self.selected = None;
                }
                Tool::Wall => {
                    self.selected = Some(super::document::ObjectId::Wall(draft.index));
                }
                Tool::Feltling | Tool::TallGuest => {
                    self.selected = Some(super::document::ObjectId::Prop(draft.index));
                }
                Tool::Room => self.selected = Some(super::document::ObjectId::Room(draft.index)),
                Tool::Column => {
                    self.selected = Some(super::document::ObjectId::Column(draft.index));
                }
                Tool::Pillar => {
                    self.selected = Some(super::document::ObjectId::Pillar(draft.index));
                }
                Tool::ArcWall => {
                    self.selected = Some(super::document::ObjectId::ArcWall(draft.index));
                }
                Tool::Elevator
                | Tool::Recess
                | Tool::Surround
                | Tool::RailPath
                | Tool::Tunnel
                | Tool::WallTrim
                | Tool::Ramp
                | Tool::Stairs
                | Tool::Rail
                | Tool::Water
                | Tool::Sky
                | Tool::Weather
                | Tool::Sun
                | Tool::Ceiling
                | Tool::AcousticMaterial
                | Tool::LocalFloor
                | Tool::GlobalPatch
                | Tool::GlobalRegion
                | Tool::Route => {}
            }
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
                "GLOBAL modifier preview: affects every overlapping storey. Use selected-room override for local ownership.".into()
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
        if matches!(
            draft.tool,
            Tool::Ceiling | Tool::LocalFloor | Tool::WallTrim | Tool::Shaft
        ) {
            return Err("Use scene Delete for a room".into());
        }
        let mut raw = draft.expected.clone();
        if creatures::native(draft.tool) {
            let prop = raw
                .get_mut("props")
                .and_then(|v| v.get_mut(draft.index))
                .and_then(Value::as_object_mut)
                .ok_or("Missing creature")?;
            drop(prop.remove(creatures::key(draft.tool)));
        } else if draft.tool == Tool::Elevator {
            raw = crate::editor::elevators::remove_candidate(
                self.document.as_ref().ok_or("Open a project")?,
                draft.index,
            )?;
        } else if draft.tool == Tool::Surround {
            let values = generators::collection_mut(&mut raw, draft.tool, draft.owner)?;
            let opening = values
                .get_mut(draft.index)
                .and_then(Value::as_object_mut)
                .ok_or("Opening no longer exists")?;
            drop(opening.remove("surround"));
        } else if draft.tool.singleton() {
            drop(
                raw.as_object_mut()
                    .ok_or("Invalid level")?
                    .remove(draft.tool.collection()),
            );
        } else {
            let values = generators::collection_mut(&mut raw, draft.tool, draft.owner)?;
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

fn room_fields() -> Vec<Field> {
    vec![
        number("/x", "West X", "0"),
        number("/z", "North Z", "0"),
        number("/width", "Width m", "4"),
        number("/depth", "Depth m", "4"),
        number("/floor_y", "Floor elevation m", "0"),
        number("/height", "Room height m", "2.7"),
    ]
}

fn default_room(app: &App, doc: &super::document::Document) -> Value {
    let anchor = app.selected.and_then(|id| {
        if let super::document::ObjectId::Room(slot) = id {
            doc.level().rooms.get(slot)
        } else {
            None
        }
    });
    let x = anchor.map_or_else(
        || {
            doc.level()
                .rooms
                .iter()
                .map(|room| room.x + room.width)
                .fold(0.0_f32, f32::max)
        },
        |room| room.x + room.width,
    );

    serde_json::json!({"x":x,"z":anchor.map_or(0.0,|room|room.z),
                "floor_y":anchor.map_or(0.0,|room|room.floor_y),"width":4.0_f32,"depth":anchor.map_or(4.0,|room|room.depth),
                "height":anchor.map_or(2.7,|room|room.height)})
}

fn form_fields(tool: Tool, record: Option<&Value>) -> Vec<Field> {
    if creatures::native(tool) {
        creatures::fields(tool, record)
    } else if tool == Tool::Elevator {
        elevators::fields(record)
    } else if tool == Tool::Route {
        route_fields(record)
    } else {
        tool.fields()
    }
}

pub(super) fn creature_tool(model: &str) -> Option<Tool> {
    match model {
        "feltling" => Some(Tool::Feltling),
        "tall-guest" => Some(Tool::TallGuest),
        _ => None,
    }
}

#[cfg(test)]
#[path = "tests/native_creatures.rs"]
mod native_creature_tests;

fn wall_fields() -> Vec<Field> {
    vec![
        number("/x", "West X / minimum corner", "1"),
        number("/z", "North Z / minimum corner", "1"),
        number("/width", "X extent m", "4"),
        number("/depth", "Z extent m", "0.2"),
        number("/y", "World base Y", "0"),
        number("/height", "Height / blank = ceiling", ""),
        text("/material", "Wall material / blank = default", ""),
    ]
}

fn original_light(original: Option<&Value>) -> Option<crate::engine::level::GlobalIlluminatorDef> {
    original.and_then(|value| serde_json::from_value(value.clone()).ok())
}

fn route_header_fields() -> Vec<Field> {
    vec![
        text("/id", "Actor instance ID", ""),
        flag("/loop", "Loop", "false"),
    ]
}

#[cfg(test)]
#[path = "tests/stairs_lights.rs"]
mod stairs_lights_tests;

fn shaft_fields() -> Vec<Field> {
    vec![
        number("/lower", "Lower Room number", "1"),
        number("/upper", "Upper Room number", "2"),
        number("/x", "Shaft minimum X", "2"),
        number("/z", "Shaft minimum Z", "2"),
        number("/width", "Shaft width m", "3"),
        number("/depth", "Shaft depth m", "6"),
    ]
}

fn light_fields() -> Vec<Field> {
    vec![
        number("/x", "X / world metres", "2"),
        number("/z", "Z / world metres", "2"),
        number("/y", "World ceiling Y / blank = auto", ""),
        text(
            "/fixture",
            "Fixture / catalog ID",
            "core:fluorescent_panel_01",
        ),
        number("/brightness", "Power / blank = legacy default", ""),
        number("/range", "Range m / blank = default", ""),
        number("/rotation_degrees", "Yaw degrees", "0"),
        text("/align", "Alignment grid / none", "none"),
    ]
}

fn validate_room_overlap(
    level: &crate::engine::level::LevelDef,
    index: usize,
) -> Result<(), String> {
    let room = level.rooms.get(index).ok_or("Room draft is missing")?;
    if level.rooms.iter().enumerate().any(|(other_index, other)| {
        other_index != index
            && other.floor_y < room.eave_y() - 0.01
            && other.eave_y() > room.floor_y + 0.01
            && other.x < room.x + room.width - 0.01
            && other.x + other.width > room.x + 0.01
            && other.z < room.z + room.depth - 0.01
            && other.z + other.depth > room.z + 0.01
    }) {
        return Err("New Room overlaps an existing room on this storey. Adjust position or Floor elevation.".into());
    }
    Ok(())
}

fn stair_fields() -> Vec<Field> {
    vec![
        number("/x", "X", "0"),
        number("/z", "Z", "0"),
        number("/width", "Width m", "4"),
        number("/depth", "Depth m", "1.5"),
        number("/offset_y", "Room-relative Y", "0"),
        number("/rise", "Rise m", "1"),
        number("/steps", "Step count", "5"),
        flag("/reverse", "Ascend to minimum X/Z", "false"),
        text("/material", "Tread material", ""),
        text("/riser_material", "Riser material", ""),
        text("/side_material", "Side material", ""),
    ]
}

#[cfg(test)]
#[path = "tests/elevator_numeric.rs"]
mod elevator_numeric_tests;
