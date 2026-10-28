//! Reusable room authoring uses the same source, preview and history as hand edits.
mod prefabs;
mod styles;
mod variation;

use super::{
    App,
    actions::Popup,
    document::{Document, ObjectId, objects},
    ui::{self, Action, Hit},
};
use crate::engine::{
    level::{LevelDef, RoomDef},
    render::Vertex,
};
use sdl3::{
    event::Event,
    keyboard::{Keycode, Mod},
};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Command {
    Open,
    Tab(usize),
    Field(usize),
    Source,
    SourceBack,
    StyleChoice,
    Linked,
    Protect,
    Category(usize),
    Capture,
    Stamp,
    Update,
    Detach,
    Style,
    Vary,
    Reset,
    Rebase,
    Apply,
    Edit,
}

pub(super) struct Draft {
    expected: Value,
    selected: Option<ObjectId>,
    tab: usize,
    fields: [String; 8],
    active: Option<usize>,
    source: usize,
    linked: bool,
    protect: bool,
    categories: [bool; 5],
    operation: Option<Command>,
    candidate: Option<Value>,
    preview: bool,
    summary: String,
    pending_selection: Option<ObjectId>,
}

impl Draft {
    fn new(doc: &Document, selected: Option<ObjectId>) -> Self {
        let destination = match selected {
            Some(ObjectId::Room(i)) => doc.level().rooms.get(i).map(|r| [r.x + r.width + 2.0, r.z]),
            _ => None,
        }
        .unwrap_or([12.0, 0.0]);
        Self {
            expected: doc.raw().clone(),
            selected,
            tab: 0,
            fields: [
                "room_kit".into(),
                destination[0].to_string(),
                destination[1].to_string(),
                "0".into(),
                "12345".into(),
                "10".into(),
                "0.1".into(),
                "0.03".into(),
            ],
            active: None,
            source: 0,
            linked: true,
            protect: true,
            categories: [true; 5],
            operation: None,
            candidate: None,
            preview: false,
            pending_selection: None,
            summary: "Select a room for capture/styles, a room or prop for variation.".into(),
        }
    }
    fn number(&self, index: usize) -> Result<f32, String> {
        let n = self
            .fields
            .get(index)
            .ok_or("Missing field")?
            .parse::<f32>()
            .map_err(|_error| "Enter a finite number")?;
        if !n.is_finite() || n.abs() > 10_000.0 {
            return Err("Number must be finite and within 10000".into());
        }
        Ok(n)
    }
    fn room(&self) -> Result<usize, String> {
        match self.selected {
            Some(ObjectId::Room(i)) => Ok(i),
            _ => Err("Select one room in the scene, then reopen Reuse tools".into()),
        }
    }
    fn candidate(
        &self,
        doc: &Document,
        operation: Command,
        catalog: &crate::engine::assets::AssetCatalog,
    ) -> Result<(Value, String, Option<ObjectId>), String> {
        if doc.raw() != &self.expected {
            return Err(
                "Scene changed; close and reopen Reuse tools. Pending values were not applied."
                    .into(),
            );
        }
        match operation {
            Command::Capture => prefabs::capture(
                doc,
                self.room()?,
                self.fields.first().ok_or("Enter a source name")?,
            ),
            Command::Stamp => {
                let names = prefabs::names(doc.raw());
                let name = names
                    .get(self.source)
                    .ok_or("Capture a room source first")?;
                prefabs::stamp(doc, name, [self.number(1)?, self.number(2)?], self.linked)
            }
            Command::Update => prefabs::update(
                doc,
                self.selected.ok_or("Select any member of a linked stamp")?,
            ),
            Command::Detach => prefabs::detach(
                doc,
                self.selected.ok_or("Select any member of a linked stamp")?,
            ),
            Command::Style => styles::candidate(
                doc,
                self.room()?,
                self.fields
                    .get(3)
                    .ok_or("Choose a style")?
                    .parse::<usize>()
                    .map_err(|_error| "Style index must be 0, 1 or 2")?,
                self.categories,
                catalog,
            ),
            Command::Rebase => {
                variation::forget(doc, self.selected.ok_or("Select a room or fixed prop")?)
            }
            Command::Reset => variation::candidate(
                doc,
                self.selected.ok_or("Select a room or fixed prop")?,
                0,
                [0.0; 3],
                false,
                true,
            ),
            Command::Vary => variation::candidate(
                doc,
                self.selected.ok_or("Select a room or fixed prop first")?,
                self.fields
                    .get(4)
                    .ok_or("Seed")?
                    .parse::<u64>()
                    .map_err(|_error| "Seed must be a whole unsigned 64-bit number")?,
                [self.number(5)?, self.number(6)?, self.number(7)?],
                self.protect,
                false,
            ),
            Command::Open
            | Command::Tab(_)
            | Command::Field(_)
            | Command::Source
            | Command::SourceBack
            | Command::StyleChoice
            | Command::Linked
            | Command::Protect
            | Command::Category(_)
            | Command::Apply
            | Command::Edit => Err("Choose an operation to preview".into()),
        }
    }
}

pub(super) fn array<'a>(raw: &'a Value, key: &str) -> &'a [Value] {
    raw.get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}
pub(super) fn number(raw: &Value, key: &str, fallback: f32) -> f32 {
    raw.get(key)
        .and_then(|n| serde_json::from_value::<f32>(n.clone()).ok())
        .unwrap_or(fallback)
}
pub(super) fn inside(room: &RoomDef, x: f32, z: f32) -> bool {
    x >= room.x - 0.001
        && z >= room.z - 0.001
        && x <= room.x + room.width + 0.001
        && z <= room.z + room.depth + 0.001
}
pub(super) fn room_indices(
    doc: &Document,
    room_index: usize,
    key: &str,
) -> Result<Vec<usize>, String> {
    let room = doc
        .level()
        .rooms
        .get(room_index)
        .ok_or("Selected room no longer exists")?;
    let mut indices = Vec::new();
    for (i, record) in array(doc.raw(), key).iter().enumerate() {
        let x = number(record, "x", 0.0);
        let z = number(record, "z", 0.0);
        if !inside(room, x, z) {
            continue;
        }
        let floor = crate::engine::level::LevelSurfaces::new(doc.level())
            .floor_y_at(x, z)
            .unwrap_or(0.0);
        if (floor - room.floor_y).abs() > 0.01 {
            continue;
        }
        indices.push(i);
    }
    Ok(indices)
}

impl App {
    pub(super) fn reuse_action(&mut self, action: Action) -> Result<bool, String> {
        let Action::Reuse(command) = action else {
            return Ok(false);
        };
        if command == Command::Open {
            let doc = self.document.as_ref().ok_or("Open a project first")?;
            self.popup = Some(Popup::Reuse(Box::new(Draft::new(doc, self.selected))));
            return Ok(true);
        }
        let Some(Popup::Reuse(mut draft)) = self.popup.take() else {
            return Err("Open Reuse tools first".into());
        };
        let result = self.reuse_command(&mut draft, command);
        self.popup = Some(Popup::Reuse(draft));
        result?;
        Ok(true)
    }
    fn reuse_command(&mut self, draft: &mut Draft, command: Command) -> Result<(), String> {
        draft.active = None;
        if command == Command::Apply {
            return self.commit_reuse(draft);
        }

        if draft.preview {
            if let Some(doc) = &self.document {
                self.viewport.rebuild(doc.level());
            }
            draft.preview = false;
            draft.candidate = None;
        }
        match command {
            Command::Tab(tab) => {
                draft.tab = tab.min(2);
                draft.operation = None;
            }
            Command::Field(i) => draft.active = Some(i),
            Command::Source | Command::SourceBack => {
                let count = self
                    .document
                    .as_ref()
                    .map_or(0, |d| prefabs::names(d.raw()).len());
                draft.source = if command == Command::Source {
                    draft
                        .source
                        .saturating_add(1)
                        .checked_rem(count.max(1))
                        .unwrap_or(0)
                } else {
                    draft
                        .source
                        .checked_sub(1)
                        .unwrap_or_else(|| count.saturating_sub(1))
                };
                if let Some(name) = self
                    .document
                    .as_ref()
                    .and_then(|d| prefabs::names(d.raw()).get(draft.source).cloned())
                    && let Some(field) = draft.fields.first_mut()
                {
                    *field = name;
                }
            }
            Command::StyleChoice => {
                if let Some(field) = draft.fields.get_mut(3) {
                    let current = field.parse::<usize>().unwrap_or(0);
                    *field = (current.saturating_add(1) % 3).to_string();
                }
            }
            Command::Linked => draft.linked = !draft.linked,
            Command::Protect => draft.protect = !draft.protect,
            Command::Category(i) => {
                if let Some(value) = draft.categories.get_mut(i) {
                    *value = !*value;
                }
            }
            Command::Capture
            | Command::Stamp
            | Command::Update
            | Command::Detach
            | Command::Style
            | Command::Vary
            | Command::Reset
            | Command::Rebase => {
                self.preview_reuse(draft, command)?;
            }
            Command::Open | Command::Apply | Command::Edit => {}
        }
        Ok(())
    }
    fn commit_reuse(&mut self, draft: &mut Draft) -> Result<(), String> {
        if !draft.preview {
            return Err("Preview this operation before Apply".into());
        }
        let candidate = draft.candidate.as_ref().ok_or("Preview first")?.clone();
        self.check_focused_edit(&candidate)?;
        let doc = self.document.as_mut().ok_or("Open a project first")?;
        doc.apply_authoring_edit(&draft.expected, candidate)?;
        draft.expected = doc.raw().clone();
        draft.preview = false;
        draft.candidate = None;
        if let Some(id) = draft.pending_selection.take() {
            self.selected = Some(id);
        }
        draft.selected = self.selected;
        if draft.operation == Some(Command::Stamp)
            && let Some(ObjectId::Room(i)) = self.selected
            && let Some(room) = doc.level().rooms.get(i)
            && let Some(field) = draft.fields.get_mut(1)
        {
            *field = (room.x + room.width + 2.0).to_string();
        }
        self.changed();
        self.viewport.select(self.selected);
        self.status = format!("Applied in one undo step. {}", draft.summary);
        Ok(())
    }
    fn preview_reuse(&mut self, draft: &mut Draft, operation: Command) -> Result<(), String> {
        if self.compile.is_some() || self.play.is_some() {
            return Err("Finish Build/Play before using Reuse tools".into());
        }
        if self.focus.hidden.iter().any(|v| *v) || self.focus.isolated.is_some() {
            return Err("Show all layers and leave isolation before a room-wide preview".into());
        }
        let op = operation;
        let doc = self.document.as_ref().ok_or("Open a project first")?;
        let (candidate, summary, selection) = draft.candidate(doc, op, &self.catalog)?;
        self.check_focused_edit(&candidate)?;
        let level = doc.validate_candidate(&candidate)?;
        crate::engine::project::validate_level_assets(&level, &self.catalog)?;
        draft.candidate = Some(candidate);
        draft.summary = summary;
        draft.operation = Some(op);
        draft.preview = true;
        self.viewport.rebuild(&level);
        if matches!(op, Command::Stamp | Command::Update) {
            self.viewport.frame_next_preview();
            self.viewport.frame_all();
        }
        // Preview keeps the authored selection; commit chooses a stamped room.
        draft.pending_selection = selection;
        self.status = format!("PREVIEW: {} Apply commits; Esc cancels.", draft.summary);

        Ok(())
    }
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Only text entry events are consumed; ordinary modal mouse and navigation events retain their existing routing."
    )]
    pub(super) fn reuse_event(&mut self, event: &Event) -> bool {
        let Some(Popup::Reuse(draft)) = &mut self.popup else {
            return false;
        };
        if matches!(
            event,
            Event::KeyDown {
                keycode: Some(Keycode::Escape),
                timestamp: _,
                window_id: _,
                scancode: _,
                keymod: _,
                repeat: _,
                which: _,
                raw: _,
            }
        ) {
            self.close_popup();
            return true;
        }
        let Some(index) = draft.active else {
            return false;
        };
        let Some(value) = draft.fields.get_mut(index) else {
            return false;
        };
        match event {
            Event::TextInput {
                text,
                timestamp: _,
                window_id: _,
            } if value.len().saturating_add(text.len()) <= 64 => value.push_str(text),
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

pub(super) fn draw(app: &App, draft: &Draft, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    if draft.preview {
        ui::rect(v, [118.0, 190.0, 352.0, 48.0], [0.075, 0.11, 0.14, 1.0]);
        ui::text(
            v,
            "REUSE PREVIEW / source unchanged",
            126.0,
            194.0,
            ui::INK,
            80,
        );
        ui::text(v, &draft.summary, 126.0, 205.0, ui::MUTED, 80);
        for (x, label, action) in [
            (126.0, "Edit options", Action::Reuse(Command::Edit)),
            (238.0, "Apply", Action::Reuse(Command::Apply)),
            (350.0, "Cancel", Action::ClosePopup),
        ] {
            ui::popup_button(v, hits, [x, 219.0, 104.0, 13.0], label, action);
        }
        return;
    }
    ui::rect(v, [0.0, 0.0, 480.0, 272.0], [0.0, 0.0, 0.0, 0.6]);
    ui::rect(v, [116.0, 40.0, 354.0, 198.0], [0.075, 0.11, 0.14, 1.0]);
    ui::text(v, "REUSE / selected scope", 126.0, 46.0, ui::INK, 75);
    for (i, label) in ["Room prefabs", "Room styles", "Prop variation"]
        .into_iter()
        .enumerate()
    {
        let x = f32::from(u16::try_from(i).unwrap_or(0)).mul_add(112.0, 126.0);
        ui::popup_button(
            v,
            hits,
            [x, 58.0, 104.0, 13.0],
            label,
            Action::Reuse(Command::Tab(i)),
        );
    }
    ui::text(
        v,
        &format!(
            "Scope: {}",
            app.document
                .as_ref()
                .and_then(|d| d.objects().iter().find(|o| Some(o.id) == draft.selected))
                .map_or("select a room or fixed prop", |o| o.label.as_str())
        ),
        126.0,
        77.0,
        ui::MUTED,
        78,
    );
    match draft.tab {
        0 => draw_prefabs(app, draft, v, hits),
        1 => draw_styles(draft, v, hits),
        _ => draw_variation(draft, v, hits),
    }
    ui::popup_button(
        v,
        hits,
        [126.0, 225.0, 328.0, 10.0],
        "Close / discard preview (Esc)",
        Action::ClosePopup,
    );
}
fn field(v: &mut Vec<Vertex>, hits: &mut Vec<Hit>, draft: &Draft, i: usize, label: &str, y: f32) {
    ui::text(v, label, 126.0, y + 3.0, ui::INK, 49);
    let value = draft.fields.get(i).map_or("", String::as_str);
    ui::popup_button(
        v,
        hits,
        [332.0, y, 122.0, 13.0],
        &format!("{}{value}", if draft.active == Some(i) { "> " } else { "" }),
        Action::Reuse(Command::Field(i)),
    );
}

#[cfg(test)]
#[path = "tests/reuse.rs"]
mod tests;

fn draw_prefabs(app: &App, draft: &Draft, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    field(v, hits, draft, 0, "Source name / capture", 91.0);
    for (x, label, cmd) in [
        (126.0, "Capture room", Command::Capture),
        (294.0, "Update linked", Command::Update),
    ] {
        ui::popup_button(v, hits, [x, 108.0, 160.0, 13.0], label, Action::Reuse(cmd));
    }
    let names = app
        .document
        .as_ref()
        .map_or_else(Vec::new, |d| prefabs::names(d.raw()));
    let name = names
        .get(draft.source)
        .map_or("Capture a source first", String::as_str);
    ui::popup_button(
        v,
        hits,
        [148.0, 125.0, 194.0, 13.0],
        &format!("Stamp source > {name}"),
        Action::Reuse(Command::Source),
    );
    ui::popup_button(
        v,
        hits,
        [350.0, 125.0, 104.0, 13.0],
        if draft.linked {
            "Linked: on"
        } else {
            "Linked: off"
        },
        Action::Reuse(Command::Linked),
    );
    ui::popup_button(
        v,
        hits,
        [126.0, 125.0, 18.0, 13.0],
        "<",
        Action::Reuse(Command::SourceBack),
    );
    field(v, hits, draft, 1, "Destination X / minimum", 143.0);
    field(v, hits, draft, 2, "Destination Z / minimum", 160.0);
    ui::text(
        v,
        "Captures room, fitted walls, fixed props, doors and lights.",
        126.0,
        179.0,
        ui::MUTED,
        82,
    );
    ui::text(
        v,
        "Explicit updates keep field overrides and deleted members.",
        126.0,
        191.0,
        ui::MUTED,
        82,
    );
    ui::popup_button(
        v,
        hits,
        [126.0, 205.0, 160.0, 13.0],
        "Preview stamp",
        Action::Reuse(Command::Stamp),
    );
    ui::popup_button(
        v,
        hits,
        [294.0, 205.0, 160.0, 13.0],
        "Detach linked instance",
        Action::Reuse(Command::Detach),
    );
}

fn draw_styles(draft: &Draft, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    ui::popup_button(
        v,
        hits,
        [126.0, 94.0, 328.0, 13.0],
        &format!(
            "Preset > {}",
            match draft.fields.get(3).map(String::as_str) {
                Some("1") => "Worn",
                Some("2") => "Home",
                _ => "Office",
            }
        ),
        Action::Reuse(Command::StyleChoice),
    );
    for (i, label) in [
        "Walls / inside faces",
        "Floor",
        "Ceiling",
        "Existing trim",
        "Ceiling fixtures",
    ]
    .into_iter()
    .enumerate()
    {
        let y = f32::from(u16::try_from(i).unwrap_or(0)).mul_add(17.0, 113.0);
        ui::popup_button(
            v,
            hits,
            [126.0, y, 328.0, 13.0],
            &format!(
                "[{}] {label}",
                if draft.categories.get(i).copied().unwrap_or(false) {
                    "x"
                } else {
                    " "
                }
            ),
            Action::Reuse(Command::Category(i)),
        );
    }
    ui::text(
        v,
        "Geometry, openings, light power/circuits and props stay put.",
        126.0,
        199.0,
        ui::MUTED,
        82,
    );
    ui::popup_button(
        v,
        hits,
        [126.0, 212.0, 328.0, 13.0],
        "Preview room style",
        Action::Reuse(Command::Style),
    );
}

fn draw_variation(draft: &Draft, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    for (i, label, y) in [
        (4, "Seed / same baseline = same result", 94.0),
        (5, "Yaw variation +/- degrees / 0..180", 111.0),
        (6, "Uniform scale +/- fraction / 0..0.5", 128.0),
        (7, "Maximum sinking metres / 0..0.25", 145.0),
    ] {
        field(v, hits, draft, i, label, y);
    }
    ui::popup_button(
        v,
        hits,
        [126.0, 163.0, 328.0, 13.0],
        if draft.protect {
            "[x] Protect baseline standing clearance"
        } else {
            "[ ] Protect baseline standing clearance"
        },
        Action::Reuse(Command::Protect),
    );
    ui::text(
        v,
        "Uses real 0.4 m radius / 1.8 m hull at 0.2 m samples.",
        126.0,
        181.0,
        ui::MUTED,
        82,
    );
    ui::text(
        v,
        "Refused choices use baseline. Fixed scenery; Play review.",
        126.0,
        193.0,
        ui::MUTED,
        82,
    );
    ui::popup_button(
        v,
        hits,
        [126.0, 209.0, 104.0, 13.0],
        "Preview variation",
        Action::Reuse(Command::Vary),
    );
    ui::popup_button(
        v,
        hits,
        [238.0, 209.0, 104.0, 13.0],
        "Restore base",
        Action::Reuse(Command::Reset),
    );
    ui::popup_button(
        v,
        hits,
        [350.0, 209.0, 104.0, 13.0],
        "Keep current base",
        Action::Reuse(Command::Rebase),
    );
}
