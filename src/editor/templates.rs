//! Event forms produce ordinary editable Luau and existing event bindings.
use super::{
    App,
    actions::Popup,
    document::{Document, ObjectId},
    ui::{self, Action, Hit},
};
use crate::engine::render::Vertex;
use sdl3::{event::Event, keyboard::Keycode};
use serde_json::{Value, json};

const EVENTS: [&str; 4] = ["start", "interact", "enter_volume", "exit_volume"];
const KINDS: [&str; 9] = [
    "Message",
    "Door toggle",
    "Sound on",
    "Investigate",
    "Fetch / deliver",
    "Follow player",
    "Use environment",
    "Cancel AI task",
    "Pause AI task",
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Command {
    Open,
    Event,
    Kind,
    Actor,
    Item,
    Target,
    Location,
    Pick(usize),
    Page(bool),
    Field(usize),
    Once,
    Create,
    Preview,
    Contacts,
}
pub(super) struct Draft {
    pub root: std::path::PathBuf,
    pub level: String,
    pub owner: Option<ObjectId>,
    pub event: usize,
    pub kind: usize,
    actor: String,
    item: String,
    target: String,
    location: usize,
    values: [String; 11],
    contacts: bool,
    once: bool,
    active: Option<usize>,
    picker: Option<(Command, Vec<String>)>,
    page: usize,
    preview: Option<String>,
}
impl Draft {
    fn new(doc: &Document, owner: Option<ObjectId>) -> Self {
        Self {
            root: doc.root().to_path_buf(),
            level: doc.level_id().into(),
            owner,
            event: 0,
            kind: 0,
            actor: String::new(),
            item: String::new(),
            target: String::new(),
            location: 0,
            values: [
                "Welcome to this place".into(),
                "8".into(),
                "3".into(),
                "event_task".into(),
                "0".into(),
                "0".into(),
                "0".into(),
                "0".into(),
                "0.8".into(),
                "0".into(),
                "0".into(),
            ],
            once: true,
            contacts: false,
            active: None,
            picker: None,
            page: 0,
            preview: None,
        }
    }
    fn destination(&self, doc: &Document) -> Result<Value, String> {
        let mut value = destination(doc, &self.target)?;
        let mut coordinates = Vec::new();
        for (index, text) in self.values.iter().enumerate().skip(4).take(6) {
            let number = text
                .parse::<f32>()
                .map_err(|error| format!("Contact option {}: {error}", index.saturating_sub(3)))?;
            if !number.is_finite() {
                return Err("Approach/contact coordinates must be finite".into());
            }
            coordinates.push(number);
        }
        drop(value.as_object_mut().ok_or("Expected JSON object")?.insert(
            "approach".into(),
            json!(coordinates.get(..3).ok_or("Missing approach")?),
        ));
        drop(value.as_object_mut().ok_or("Expected JSON object")?.insert(
            "contact".into(),
            json!(coordinates.get(3..).ok_or("Missing contact")?),
        ));
        let yaw = self
            .values
            .get(10)
            .ok_or("Missing facing yaw")?
            .parse::<f32>()
            .map_err(|error| format!("Facing yaw: {error}"))?;
        if !yaw.is_finite() || yaw.abs() > 3600.0 {
            return Err("Facing yaw must be finite and within 3600 degrees".into());
        }
        let object = value.as_object_mut().ok_or("Expected contact object")?;
        drop(object.insert("yaw_degrees".into(), json!(yaw)));
        if self.kind == 4 {
            drop(object.insert("kind".into(), json!("shelf")));
        }
        Ok(value)
    }
    fn pick(&mut self, doc: &Document, index: usize) -> Result<(), String> {
        let (choice, values) = self.picker.as_ref().ok_or("Open a picker")?;
        let value = values.get(index).ok_or("Choice expired")?.clone();
        match choice {
            Command::Actor => self.actor = value,
            Command::Item => self.item = value,
            Command::Target => {
                self.target = value;
                if let Ok(point) = destination(doc, &self.target) {
                    for (coordinate_index, coordinate) in point
                        .get("approach")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .chain(
                            point
                                .get("contact")
                                .and_then(Value::as_array)
                                .into_iter()
                                .flatten(),
                        )
                        .enumerate()
                    {
                        if let Some(text) = self.values.get_mut(coordinate_index.saturating_add(4))
                        {
                            *text = coordinate.to_string();
                        }
                    }
                }
            }
            Command::Location => self.location = index,
            Command::Open
            | Command::Event
            | Command::Kind
            | Command::Pick(_)
            | Command::Page(_)
            | Command::Field(_)
            | Command::Once
            | Command::Create
            | Command::Preview
            | Command::Contacts => return Err("Invalid picker".into()),
        }
        self.picker = None;

        Ok(())
    }
    fn source(&self, doc: &Document) -> Result<String, String> {
        use crate::editor::document::behavior::luau;
        let number = |index: usize| -> Result<f32, String> {
            let text = self.values.get(index).ok_or("Missing option")?;
            let n = text
                .parse::<f32>()
                .map_err(|_error| "Duration/radius needs a number")?;
            if !n.is_finite() {
                return Err("Duration/radius must be finite".into());
            }
            Ok(n)
        };
        let identity = |id: &str| luau(&json!(id), 0);
        let actor = identity(&self.actor);
        let target = identity(&self.target);
        let task_key = self.values.get(3).map_or("", String::as_str);
        if self.kind >= 3 && !crate::engine::ai::authoring::name_valid(task_key) {
            return Err("Task key must be nonempty, printable and at most 96 bytes".into());
        }
        let key = identity(task_key);
        let task = match self.kind {
            3 => {
                let room = doc
                    .level()
                    .rooms
                    .get(self.location)
                    .ok_or("Choose an existing room location")?;
                Some(
                    json!({"task":"investigate","point":[f32::mul_add(room.width, 0.5_f32, room.x),room.floor_y,f32::mul_add(room.depth, 0.5_f32, room.z)],"seconds":number(1)?,"radius":number(2)?}),
                )
            }
            4 => Some(
                json!({"task":"fetch","item":{"match":"id","value":self.item},"destination":self.destination(doc)?,"search_radius":30.0_f32}),
            ),
            5 => Some(
                json!({"task":"follow","target":"player","distance":1.6_f32,"memory_seconds":number(1)?}),
            ),
            6 => Some(json!({"task":"use","point":self.destination(doc)?})),
            _ => None,
        };
        if self.kind >= 3 {
            let ids = doc.level().prop_instance_ids();
            let index = ids
                .iter()
                .position(|id| id == &self.actor)
                .ok_or("Choose an existing NPC")?;
            if doc.level().props.get(index).is_none_or(|prop| {
                !prop
                    .components
                    .iter()
                    .any(|component| matches!(component, crate::engine::level::ComponentDef::Ai(_)))
            }) {
                return Err("Selected actor has no shared AI component".into());
            }
        }
        let body = if let Some(definition) = task {
            if self.kind == 4 {
                let ids = doc.level().prop_instance_ids();
                let index = ids
                    .iter()
                    .position(|id| id == &self.item)
                    .ok_or("Choose an existing movable item")?;
                if doc
                    .level()
                    .props
                    .get(index)
                    .is_none_or(|prop| prop.physics.is_none())
                {
                    return Err("Fetch item needs an authored physics body".into());
                }
            }
            let request = json!({"key":self.values.get(3).ok_or("Missing task key")?,"priority":60_u8,"definition":definition});
            let typed: crate::engine::ai::authoring::TaskRequest =
                serde_json::from_value(request.clone()).map_err(|e| e.to_string())?;
            if !typed.is_valid() {
                return Err("Task parameters exceed shared runtime limits".into());
            }
            format!(
                "        if not api.exists({actor}) then return end\n        api.ai_task({actor}, {key}, {})",
                luau(&request, 2)
            )
        } else {
            self.simple_body(doc, &actor, &target, &key)?
        };
        let handler = if self.event == 0 {
            "start"
        } else {
            "handle_event"
        };
        Ok(format!(
            "--!strict\n-- Generated event template. Edit freely; another generation creates a new file.\n-- Host effects are atomic per call. Shared AI retains physical interaction and budgets.\nreturn {{\n    {handler} = function(api, event)\n{body}\n    end,\n}}\n"
        ))
    }
    fn simple_body(
        &self,
        doc: &Document,
        actor: &str,
        target: &str,
        key: &str,
    ) -> Result<String, String> {
        use crate::editor::document::behavior::luau;
        let identity = |id: &str| luau(&json!(id), 0);
        Ok(match self.kind {
            0 => {
                let message = self.values.first().ok_or("Enter a message")?;
                if message.is_empty() || message.len() > 72 || message.chars().any(char::is_control)
                {
                    return Err("Message must be nonempty, printable and at most 72 bytes".into());
                }
                format!(
                    "        api.message(\"Event message\", {{{}}})",
                    identity(message)
                )
            }
            1 => {
                if !doc.level().door_instance_ids().contains(&self.target) {
                    return Err("Choose an existing door".into());
                }
                format!("        if api.exists({target}) then api.door({target}, \"toggle\") end")
            }
            2 => {
                if !doc
                    .level()
                    .sound_emitters
                    .iter()
                    .any(|sound| sound.id == self.target)
                {
                    return Err("Choose an existing sound emitter".into());
                }
                format!("        if api.exists({target}) then api.sound({target}, true) end")
            }
            7 => {
                format!("        if api.exists({actor}) then api.ai_task({actor}, {key}, nil) end")
            }
            8 => format!(
                "        if api.exists({actor}) then api.ai_pause({actor}, {key}, true) end"
            ),
            _ => return Err("Choose a template action".into()),
        })
    }
}
fn destination(doc: &Document, id: &str) -> Result<Value, String> {
    let surfaces = crate::engine::level::LevelSurfaces::new(doc.level());
    let ids = doc.level().prop_instance_ids();
    let (contact, approach, kind) = if let Some(prop) = ids
        .iter()
        .position(|known| known == id)
        .and_then(|index| doc.level().props.get(index))
    {
        let floor = surfaces
            .floor_y_at(prop.x, prop.z)
            .ok_or("Target has no supporting floor")?;
        let (sin, cos) = prop.rotation_degrees.to_radians().sin_cos();
        let size = prop
            .size
            .unwrap_or(crate::engine::level::PROP_FALLBACK_SIZE);
        let front = size[2] * prop.scale * 0.5;
        let contact = [
            sin.mul_add(front, prop.x),
            floor + prop.y + (size[1] * prop.scale * 0.5).min(1.0),
            cos.mul_add(front, prop.z),
        ];
        (
            contact,
            [
                sin.mul_add(0.8, contact[0]),
                floor,
                cos.mul_add(0.8, contact[2]),
            ],
            "interact",
        )
    } else if let Some(door) = doc.level().doors.iter().find(|door| door.id == id) {
        let floor = surfaces
            .floor_y_at(door.x, door.z)
            .ok_or("Door has no supporting floor")?;
        let (sin, cos) = door.rotation_degrees.to_radians().sin_cos();
        let contact = [
            (door.width * 0.75).mul_add(cos, door.x),
            floor + door.y + 0.9,
            (-door.width * 0.75).mul_add(sin, door.z),
        ];
        (
            contact,
            [
                sin.mul_add(0.9, contact[0]),
                floor,
                cos.mul_add(0.9, contact[2]),
            ],
            "door",
        )
    } else {
        return Err("Choose an existing interaction/delivery target".into());
    };
    Ok(
        json!({"target":id,"approach":approach,"contact":contact,"reach":1.5_f32,"kind":kind,"contact_seconds":0.35_f32,"hold_seconds":0.0_f32}),
    )
}

impl App {
    pub(super) fn template_action(&mut self, action: Action) -> Result<bool, String> {
        let Action::Template(command) = action else {
            return Ok(false);
        };
        let doc = self.document.as_mut().ok_or("Open a project first")?;
        if command == Command::Open {
            if self
                .template
                .as_ref()
                .is_none_or(|draft| draft.root != doc.root() || draft.level != doc.level_id())
            {
                self.template = Some(Draft::new(doc, self.selected));
            }
            if let Some(draft) = &mut self.template {
                draft.owner = self.selected;
            }
            self.popup = Some(Popup::Template);
            return Ok(true);
        }
        let draft = self.template.as_mut().ok_or("Open Event templates first")?;
        draft.active = None;
        draft.preview = None;
        match command {
            Command::Open => {}
            Command::Event => {
                draft.event = draft
                    .event
                    .saturating_add(1)
                    .checked_rem(EVENTS.len())
                    .unwrap_or(0);
            }
            Command::Kind => {
                draft.kind = draft
                    .kind
                    .saturating_add(1)
                    .checked_rem(KINDS.len())
                    .unwrap_or(0);
            }
            Command::Field(index) => draft.active = Some(index),
            Command::Once => draft.once = !draft.once,
            Command::Contacts => draft.contacts = !draft.contacts,
            Command::Actor | Command::Item | Command::Target | Command::Location => {
                let choices = picker_choices(doc, command);
                draft.picker = Some((command, choices));
                draft.page = 0;
            }
            Command::Pick(index) => {
                draft.pick(doc, index)?;
            }
            Command::Page(forward) => {
                let maximum = draft
                    .picker
                    .as_ref()
                    .map_or(0, |(_, values)| values.len().saturating_sub(1) / 8);
                draft.page = if forward {
                    draft.page.saturating_add(1).min(maximum)
                } else {
                    draft.page.saturating_sub(1)
                };
            }
            Command::Preview => draft.preview = Some(draft.source(doc)?),
            Command::Create => {
                if self.compile.is_some() || self.play.is_some() {
                    return Err("Finish Build/Play before creating a script".into());
                }
                let source = draft.source(doc)?;
                let event = EVENTS.get(draft.event).ok_or("Missing event")?;
                let path = doc.create_event_template(&source, draft.owner, event, draft.once)?;
                self.popup = Some(Popup::ScriptEdit {
                    path,
                    cursor: 0,
                    scroll: 0,
                    all: false,
                });
                self.status="New event module created and linked. Edit source, Check, then Build+Play. Existing modules remain intact.".into();
                self.changed();
            }
        }
        Ok(true)
    }
    #[expect(
        clippy::wildcard_enum_match_arm,
        clippy::rest_pattern_accessible_field,
        reason = "Template input handles only focused text and cancellation."
    )]
    pub(super) fn template_event(&mut self, event: &Event) -> bool {
        if !matches!(self.popup, Some(Popup::Template)) {
            return false;
        }
        let Some(draft) = &mut self.template else {
            return false;
        };
        if let Event::KeyDown {
            keycode: Some(Keycode::Escape),
            ..
        } = event
        {
            if draft.picker.take().is_some()
                || draft.preview.take().is_some()
                || draft.active.take().is_some()
            {
                return true;
            }
            return false;
        }
        let Some(index) = draft.active else {
            return false;
        };
        let Some(value) = draft.values.get_mut(index) else {
            return false;
        };
        match event {
            Event::TextInput { text, .. } => {
                if value.len().saturating_add(text.len()) <= 256 {
                    value.push_str(text);
                }
            }
            Event::KeyDown {
                keycode: Some(Keycode::A),
                keymod,
                ..
            } if super::input::command(*keymod) => value.clear(),
            Event::KeyDown {
                keycode: Some(Keycode::Backspace),
                ..
            } => {
                let _removed = value.pop();
            }
            Event::KeyDown {
                keycode: Some(Keycode::Return | Keycode::Tab),
                ..
            } => draft.active = None,
            Event::MouseButtonUp { .. } => return false,
            _ => {}
        }
        true
    }
}
pub(super) fn draw(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    ui::rect(
        v,
        [0.0_f32, 38.0_f32, 480.0_f32, 204.0_f32],
        [0.055_f32, 0.08_f32, 0.11_f32, 1.0_f32],
    );
    ui::text(
        v,
        "EVENT TEMPLATE / creates a new editable module",
        8.0_f32,
        43.0_f32,
        ui::INK,
        100,
    );
    ui::popup_button(
        v,
        hits,
        [422.0_f32, 41.0_f32, 50.0_f32, 11.0_f32],
        "Close",
        Action::ClosePopup,
    );
    let Some(draft) = &app.template else {
        return;
    };
    if let Some(source) = &draft.preview {
        for (index, line) in source.lines().take(22).enumerate() {
            ui::text(
                v,
                line,
                8.0_f32,
                f32::from(u16::try_from(index).unwrap_or(0)).mul_add(7.5_f32, 60.0_f32),
                ui::INK,
                130,
            );
        }
        ui::text(
            v,
            "Esc returns to the form. Creation never replaces an existing source.",
            8.0_f32,
            229.0_f32,
            ui::MUTED,
            125,
        );
        return;
    }
    if let Some((_, choices)) = &draft.picker {
        for (index, value) in choices
            .iter()
            .enumerate()
            .skip(draft.page.saturating_mul(8))
            .take(8)
        {
            ui::popup_button(
                v,
                hits,
                [
                    8.0_f32,
                    f32::from(u16::try_from(index % 8).unwrap_or(0)).mul_add(18.0_f32, 62.0_f32),
                    460.0_f32,
                    15.0_f32,
                ],
                value,
                Action::Template(Command::Pick(index)),
            );
        }
        for (x, label, next) in [(8.0_f32, "Previous", false), (90.0_f32, "Next", true)] {
            ui::popup_button(
                v,
                hits,
                [x, 216.0_f32, 76.0_f32, 12.0_f32],
                label,
                Action::Template(Command::Page(next)),
            );
        }
        return;
    }
    draw_form(draft, v, hits);
}
fn draw_form(draft: &Draft, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    let event = EVENTS.get(draft.event).copied().unwrap_or("?");
    let kind = KINDS.get(draft.kind).copied().unwrap_or("?");
    for (index, (label, value, command)) in [
        ("Event", event.to_string(), Command::Event),
        ("Response", kind.to_string(), Command::Kind),
        ("NPC", draft.actor.clone(), Command::Actor),
        ("Item", draft.item.clone(), Command::Item),
        ("Target", draft.target.clone(), Command::Target),
        (
            "Location",
            format!("Room{} center", draft.location.saturating_add(1)),
            Command::Location,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let y = f32::from(u16::try_from(index).unwrap_or(0)).mul_add(23.0_f32, 64.0_f32);
        ui::text(v, label, 8.0_f32, y + 4.0_f32, ui::MUTED, 20);
        ui::popup_button(
            v,
            hits,
            [52.0_f32, y, 185.0_f32, 18.0_f32],
            &value,
            Action::Template(command),
        );
    }
    draw_options(draft, v, hits);
    ui::popup_button(
        v,
        hits,
        [250.0_f32, 187.0_f32, 220.0_f32, 13.0_f32],
        if draft.once {
            "Event: once per run"
        } else {
            "Event: repeat (0.5s cooldown)"
        },
        Action::Template(Command::Once),
    );
    for (x, label, command) in [
        (8.0_f32, "Preview source", Command::Preview),
        (250.0_f32, "Create new + link event", Command::Create),
    ] {
        ui::popup_button(
            v,
            hits,
            [x, 209.0_f32, 220.0_f32, 15.0_f32],
            label,
            Action::Template(command),
        );
    }
    ui::text(
        v,
        &format!(
            "Event owner: {:?}. Non-start events use the selected object.",
            draft.owner
        ),
        8.0_f32,
        231.0_f32,
        ui::MUTED,
        130,
    );
}

fn picker_choices(doc: &Document, command: Command) -> Vec<String> {
    let mut choices = doc.level().prop_instance_ids();
    if command == Command::Actor {
        choices.retain(|id| {
            doc.level()
                .prop_instance_ids()
                .iter()
                .position(|p| p == id)
                .and_then(|i| doc.level().props.get(i))
                .is_some_and(|p| {
                    p.components
                        .iter()
                        .any(|c| matches!(c, crate::engine::level::ComponentDef::Ai(_)))
                })
        });
    }
    if command == Command::Target {
        choices.extend(doc.level().door_instance_ids());
        choices.extend(
            doc.level()
                .sound_emitters
                .iter()
                .map(|sound| sound.id.clone()),
        );
    }
    if command == Command::Location {
        choices = doc
            .level()
            .rooms
            .iter()
            .enumerate()
            .map(|(i, r)| format!("Room{} / floor Y {:.2}", i.saturating_add(1), r.floor_y))
            .collect();
    }

    choices
}

fn draw_options(draft: &Draft, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    let labels: &[&str] = if draft.contacts {
        &[
            "Approach X",
            "Approach floor Y",
            "Approach Z",
            "Contact X",
            "Contact Y",
            "Contact Z",
        ]
    } else {
        &[
            "Message",
            "Duration / memory seconds",
            "Search radius m",
            "Task key (same key cancels)",
            "Facing yaw (degrees)",
        ]
    };
    for (slot, label) in labels.iter().enumerate() {
        let index = if draft.contacts {
            slot.saturating_add(4)
        } else if slot == 4 {
            10
        } else {
            slot
        };
        let y = f32::from(u16::try_from(slot).unwrap_or(0))
            .mul_add(if draft.contacts { 18.0 } else { 22.0 }, 64.0);
        ui::text(v, label, 250.0, y, ui::MUTED, 62);
        ui::popup_button(
            v,
            hits,
            [250.0, y + 8.0, 220.0, 11.0],
            draft.values.get(index).map_or("", String::as_str),
            Action::Template(Command::Field(index)),
        );
    }
    ui::popup_button(
        v,
        hits,
        [250.0, 175.0, 220.0, 10.0],
        if draft.contacts {
            "< General options"
        } else {
            "Approach / contact coordinates >"
        },
        Action::Template(Command::Contacts),
    );
}
