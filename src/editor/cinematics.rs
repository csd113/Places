//! Bounded ordered cinematic authoring; playback remains the ordinary Test runtime.
mod sheet;
use super::{
    App,
    actions::Popup,
    document::{Document, ObjectId},
    ui::{Action, Hit},
};
use crate::editor::document::cinematics::{value_at, value_at_mut};
use sdl3::{event::Event, keyboard::Keycode, video::Window};
use serde_json::{Value, json};
pub(super) use sheet::draw;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Command {
    Open,
    New,
    Sequence(usize),
    Row(usize),
    Add(u8),
    RemoveRow,
    Up,
    Down,
    Field(usize),
    CaptureStart,
    CaptureEnd,
    Owner,
    Destination,
    Once,
    Aim,
    Target,
    Easing,
    Script,
    Handler,
    Page(bool),
    ListPage(bool),
    Apply,
    Remove,
}
pub(super) struct Draft {
    root: PathBuf,
    level: String,
    expected: Value,
    original: Option<String>,
    sequence: Value,
    owner: Option<ObjectId>,
    destination: Option<String>,
    once: bool,
    row: usize,
    page: usize,
    list_page: usize,
    edit: Option<(String, String)>,
}
impl Draft {
    fn new(
        doc: &Document,
        source_sequence: Option<Value>,
        selected_owner: Option<ObjectId>,
        pose: crate::game::play_spawn::Pose,
    ) -> Self {
        let original = source_sequence
            .as_ref()
            .and_then(|v| value_at(v, "/id").as_str())
            .map(str::to_owned);
        let mut number = 1_usize;
        while value_at(doc.raw(), "/sequences")
            .as_array()
            .is_some_and(|v| {
                v.iter().any(|s| {
                    value_at(s, "/id").as_str() == Some(format!("cinematic_{number}").as_str())
                })
            })
        {
            number = number.saturating_add(1);
        }
        let sequence = source_sequence.unwrap_or_else(|| json!({"id":format!("cinematic_{number}"),"looped":false,"cinematic":{"start":pose_value(pose)},"steps":[camera(0_u8,pose)]}));
        let meta = value_at(&sequence, "/editor_cinematic");
        let owner = value_at(meta, "/owner")
            .as_str()
            .and_then(|id| {
                doc.level()
                    .volume_instance_ids()
                    .iter()
                    .position(|value| value == id)
            })
            .map(ObjectId::Trigger)
            .or(selected_owner);
        Self {
            root: doc.root().into(),
            level: doc.level_id().into(),
            expected: doc.raw().clone(),
            original,
            owner,
            destination: value_at(meta, "/destination").as_str().map(str::to_owned),
            once: value_at(meta, "/once").as_bool().unwrap_or(true),
            sequence,
            row: 0,
            page: 0,
            list_page: 0,
            edit: None,
        }
    }
    fn fields(&self) -> Vec<(String, String)> {
        fn walk(value: &Value, path: &str, out: &mut Vec<(String, String)>) {
            match value {
                Value::Object(map) => {
                    for (key, v) in map {
                        if !matches!(
                            key.as_str(),
                            "kind" | "step" | "action" | "easing" | "script" | "handler"
                        ) {
                            walk(v, &format!("{path}/{key}"), out);
                        }
                    }
                }
                Value::Array(values) => {
                    for (index, v) in values.iter().enumerate() {
                        walk(v, &format!("{path}/{index}"), out);
                    }
                }
                Value::String(s) => out.push((path.into(), s.clone())),
                Value::Number(n) => out.push((path.into(), n.to_string())),
                Value::Null | Value::Bool(_) => {}
            }
        }
        let mut fields = Vec::new();
        if self.original.is_none() {
            fields.push((
                "/id".into(),
                value_at(&self.sequence, "/id")
                    .as_str()
                    .unwrap_or_default()
                    .into(),
            ));
        }
        walk(
            value_at(&self.sequence, "/cinematic/start"),
            "/cinematic/start",
            &mut fields,
        );
        walk(
            value_at(&self.sequence, &format!("/steps/{}", self.row)),
            &format!("/steps/{}", self.row),
            &mut fields,
        );
        fields
    }
    fn finish(&mut self) -> Result<(), String> {
        let Some((path, text)) = &self.edit else {
            return Ok(());
        };
        let old = self.sequence.pointer(path).ok_or("Field expired")?;
        let value = if old.is_number() {
            let n = text
                .parse::<f32>()
                .map_err(|error| format!("Enter a finite number: {error}"))?;
            if !n.is_finite() {
                return Err("Enter a finite number".into());
            }
            json!(n)
        } else {
            json!(text)
        };
        *self.sequence.pointer_mut(path).ok_or("Field expired")? = value;
        self.edit = None;
        Ok(())
    }
}
fn pose_value(p: crate::game::play_spawn::Pose) -> Value {
    json!({"position":p.eye.to_array(),"yaw_degrees":p.yaw.to_degrees(),"pitch_degrees":p.pitch.to_degrees()})
}
fn camera(kind: u8, p: crate::game::play_spawn::Pose) -> Value {
    let target = json!({"kind":"point","position":(glam::Vec3::new(p.eye.x, p.eye.y, p.eye.z + 3.0)).to_array()});
    let motion = match kind {
        1 => json!({"kind":"follow","target":target,"offset":[0_i32,0_i32,-3_i32]}),
        2 => {
            json!({"kind":"orbit","target":target,"radius":3_i32,"height":0_i32,"start_degrees":0_i32,"sweep_degrees":90_i32})
        }
        _ => json!({"kind":"move","position":p.eye.to_array(),"easing":"smooth"}),
    };
    json!({"step":"camera","seconds":3_i32,"motion":motion,"aim":{"kind":"keep"}})
}
impl App {
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Draft commands are dispatched to focused handlers below."
    )]
    pub(super) fn cinematic_action(&mut self, action: Action) -> Result<bool, String> {
        let Action::Cinematic(command) = action else {
            return Ok(false);
        };
        let pose = self.viewport.play_camera()?;
        if command == Command::Apply
            && let Some(owner) = self.cinematics.as_ref().and_then(|d| d.owner)
            && (!self.editable(owner)
                || self
                    .document
                    .as_ref()
                    .is_some_and(|doc| !self.focus.included(doc.level(), owner)))
        {
            return Err("Show and unlock the controller before editing".into());
        }
        if self.cinematic_open(command, pose)? {
            return Ok(true);
        }
        let doc = self.document.as_mut().ok_or("Open a project first")?;
        let draft = self.cinematics.as_mut().ok_or("Open Cinematics first")?;
        if draft.root != doc.root() || draft.level != doc.level_id() {
            return Err("Cinematic belongs to another level; reopen it".into());
        }
        draft.finish()?;
        match command {
            Command::Open | Command::New | Command::Sequence(_) => {}
            Command::Apply => {
                doc.apply_cinematic(&draft.expected,draft.original.as_deref(),&draft.sequence,draft.owner.ok_or("Create/select a player Trigger in ordinary authoring, then choose Controller")?,draft.destination.as_deref(),draft.once)?;
                draft.expected = doc.raw().clone();
                draft.original = value_at(&draft.sequence, "/id").as_str().map(str::to_owned);
                self.changed();
                self.status="Cinematic applied atomically. Use Test/Stop; next-level completion requires project Build + Play.".into();
            }
            Command::Remove => {
                doc.remove_cinematic(
                    &draft.expected,
                    draft
                        .original
                        .as_deref()
                        .ok_or("Unsaved draft; close to discard")?,
                )?;
                self.cinematics = None;
                self.popup = None;
                self.changed();
            }
            _ => {
                let _handled = Draft::navigation(draft, command, doc)?
                    || Draft::camera_capture(draft, command, pose)?
                    || Draft::rows(draft, command, pose)?
                    || Draft::controller(draft, command, doc)?
                    || Draft::camera_options(draft, command)?
                    || Draft::scripts(draft, command, doc)?;
            }
        }
        Ok(true)
    }
    fn cinematic_open(
        &mut self,
        command: Command,
        pose: crate::game::play_spawn::Pose,
    ) -> Result<bool, String> {
        let doc = self.document.as_ref().ok_or("Open a project first")?;
        if command == Command::Open
            && self
                .cinematics
                .as_ref()
                .is_some_and(|d| d.root == doc.root() && d.level == doc.level_id())
        {
            self.popup = Some(Popup::Cinematics);
            return Ok(true);
        }
        if matches!(command, Command::Open | Command::New | Command::Sequence(_)) {
            let sequence = if let Command::Sequence(index) = command {
                Some(
                    value_at(doc.raw(), "/sequences")
                        .get(index)
                        .filter(|v| !value_at(v, "/cinematic").is_null())
                        .ok_or("Cinematic no longer exists")?
                        .clone(),
                )
            } else {
                None
            };
            self.cinematics = Some(Draft::new(
                doc,
                sequence,
                self.selected
                    .filter(|id| matches!(id, ObjectId::Trigger(_))),
                pose,
            ));
            self.popup = Some(Popup::Cinematics);
            return Ok(true);
        }
        Ok(false)
    }
    #[expect(
        clippy::wildcard_enum_match_arm,
        clippy::rest_pattern_accessible_field,
        reason = "Only cinematic text-field events are consumed here."
    )]
    pub(super) fn cinematic_event(
        &mut self,
        event: &Event,
        _window: &Window,
        _hits: &[Hit],
    ) -> bool {
        if !matches!(self.popup, Some(Popup::Cinematics)) {
            return false;
        }
        let Some(draft) = &mut self.cinematics else {
            return false;
        };
        let Some((_, text)) = &mut draft.edit else {
            return false;
        };
        match event {
            Event::TextInput { text: input, .. } => {
                if text.len().saturating_add(input.len()) <= 256 {
                    text.push_str(input);
                }
            }
            Event::KeyDown {
                keycode: Some(Keycode::A),
                keymod,
                ..
            } if super::input::command(*keymod) => text.clear(),
            Event::KeyDown {
                keycode: Some(Keycode::Backspace),
                ..
            } => {
                let _last = text.pop();
            }
            Event::KeyDown {
                keycode: Some(Keycode::Escape),
                ..
            } => draft.edit = None,
            Event::KeyDown {
                keycode: Some(Keycode::Return | Keycode::Tab),
                ..
            } => {
                if let Err(error) = draft.finish() {
                    self.status = error;
                }
            }
            Event::MouseButtonUp { .. } => return false,
            _ => {}
        }
        true
    }
}

impl Draft {
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "This focused handler consumes only its own cinematic commands."
    )]
    fn navigation(draft: &mut Self, command: Command, doc: &Document) -> Result<bool, String> {
        match command {
            Command::ListPage(forward) => {
                let count = value_at(doc.raw(), "/sequences")
                    .as_array()
                    .map_or(0, |rows| {
                        rows.iter()
                            .filter(|row| !value_at(row, "/cinematic").is_null())
                            .count()
                    });
                draft.list_page = if forward {
                    draft
                        .list_page
                        .saturating_add(1)
                        .min(count.saturating_sub(1) / 6)
                } else {
                    draft.list_page.saturating_sub(1)
                };
            }
            Command::Row(row) => {
                draft.row = row;
                draft.page = 0;
            }
            Command::Field(index) => {
                let (path, value) = draft.fields().get(index).cloned().ok_or("Field expired")?;
                draft.edit = Some((path, value));
            }
            Command::Page(forward) => {
                draft.page = if forward {
                    draft
                        .page
                        .saturating_add(1)
                        .min(draft.fields().len().saturating_sub(1) / 7)
                } else {
                    draft.page.saturating_sub(1)
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "This focused handler consumes only its own cinematic commands."
    )]
    fn camera_capture(
        draft: &mut Self,
        command: Command,
        pose: crate::game::play_spawn::Pose,
    ) -> Result<bool, String> {
        match command {
            Command::CaptureStart => {
                *value_at_mut(&mut draft.sequence, "/cinematic/start")? = pose_value(pose);
            }
            Command::CaptureEnd => {
                let row = value_at_mut(&mut draft.sequence, &format!("/steps/{}", draft.row))?;
                if value_at(row, "/motion/kind") != "move" {
                    return Err("Capture endpoint is for Move rows; use world-axis offsets for Follow/Orbit".into());
                }
                *value_at_mut(row, "/motion/position")? = json!(pose.eye.to_array());
                *value_at_mut(row, "/aim")? = json!({"kind":"angles","yaw_degrees":pose.yaw.to_degrees(),"pitch_degrees":pose.pitch.to_degrees()});
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "This focused handler consumes only its own cinematic commands."
    )]
    fn rows(
        draft: &mut Self,
        command: Command,
        pose: crate::game::play_spawn::Pose,
    ) -> Result<bool, String> {
        match command {
            Command::Add(kind) => {
                let step = match kind {
                    3 => json!({"step":"wait","seconds":1_i32}),
                    4 => {
                        json!({"step":"action","action":{"action":"run_script","script":"","handler":""}})
                    }
                    _ => camera(kind, pose),
                };
                let rows = value_at_mut(&mut draft.sequence, "/steps")?
                    .as_array_mut()
                    .ok_or("Invalid steps")?;
                if rows.len() >= 128 {
                    return Err("At most 128 steps".into());
                }
                rows.push(step);
                draft.row = rows.len().saturating_sub(1);
                draft.page = 0;
            }
            Command::RemoveRow | Command::Up | Command::Down => {
                let rows = value_at_mut(&mut draft.sequence, "/steps")?
                    .as_array_mut()
                    .ok_or("Invalid steps")?;
                if draft.row >= rows.len() {
                    return Err("Select a step".into());
                }
                if command == Command::RemoveRow {
                    if rows.len() == 1 {
                        return Err(
                            "Keep at least one step; add a replacement before removing this row"
                                .into(),
                        );
                    }
                    drop(rows.remove(draft.row));
                    draft.row = draft.row.min(rows.len().saturating_sub(1));
                } else {
                    let other = if command == Command::Up {
                        draft.row.saturating_sub(1)
                    } else {
                        draft
                            .row
                            .saturating_add(1)
                            .min(rows.len().saturating_sub(1))
                    };
                    rows.swap(draft.row, other);
                    draft.row = other;
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "This focused handler consumes only its own cinematic commands."
    )]
    fn controller(draft: &mut Self, command: Command, doc: &Document) -> Result<bool, String> {
        match command {
            Command::Owner => {
                if value_at(&draft.sequence, "/editor_cinematic/owner").is_string() {
                    return Err(
                        "Existing controller is fixed; edit its ordinary Events explicitly".into(),
                    );
                }
                let owners: Vec<_> = doc
                    .objects()
                    .iter()
                    .filter(|o| matches!(o.id, ObjectId::Trigger(_)))
                    .map(|o| o.id)
                    .collect();
                draft.owner = owners
                    .iter()
                    .position(|id| Some(*id) == draft.owner)
                    .and_then(|n| owners.get(n.saturating_add(1)))
                    .or_else(|| owners.first())
                    .copied();
            }
            Command::Destination => {
                let ids: Vec<_> = doc
                    .level_ids()
                    .filter(|id| *id != doc.level_id())
                    .map(str::to_owned)
                    .collect();
                draft.destination = draft.destination.as_ref().map_or_else(
                    || ids.first().cloned(),
                    |current| {
                        ids.iter()
                            .position(|id| id == current)
                            .and_then(|i| ids.get(i.saturating_add(1)))
                            .cloned()
                    },
                );
            }
            Command::Once => draft.once = !draft.once,
            _ => return Ok(false),
        }
        Ok(true)
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "This focused handler consumes only its own cinematic commands."
    )]
    fn camera_options(draft: &mut Self, command: Command) -> Result<bool, String> {
        match command {
            Command::Aim => {
                let row = value_at_mut(&mut draft.sequence, &format!("/steps/{}", draft.row))?;
                if value_at(row, "/step") != "camera" {
                    return Err("Choose a camera row".into());
                }
                *value_at_mut(row, "/aim")? = match value_at(row, "/aim/kind").as_str() {
                    Some("keep") => {
                        json!({"kind":"angles","yaw_degrees":0_i32,"pitch_degrees":0_i32})
                    }
                    Some("angles") => {
                        json!({"kind":"look_at","target":{"kind":"point","position":[0_i32,0_i32,0_i32]}})
                    }
                    _ => json!({"kind":"keep"}),
                };
            }
            Command::Target => {
                let row = value_at_mut(&mut draft.sequence, &format!("/steps/{}", draft.row))?;
                let target = if value_at(row, "/aim/kind") == "look_at" {
                    value_at_mut(row, "/aim/target")?
                } else if matches!(
                    value_at(row, "/motion/kind").as_str(),
                    Some("follow" | "orbit")
                ) {
                    value_at_mut(row, "/motion/target")?
                } else {
                    return Err("Choose Follow/Orbit or Look at aim".into());
                };
                *target = if value_at(target, "/kind") == "point" {
                    json!({"kind":"entity","id":"","offset":[0_i32,0_i32,0_i32]})
                } else {
                    json!({"kind":"point","position":[0_i32,0_i32,0_i32]})
                };
            }
            Command::Easing => {
                let row = value_at_mut(&mut draft.sequence, &format!("/steps/{}", draft.row))?;
                if value_at(row, "/motion/kind") != "move" {
                    return Err("Easing applies to Move".into());
                }
                *value_at_mut(row, "/motion/easing")? =
                    json!(if value_at(row, "/motion/easing") == "smooth" {
                        "linear"
                    } else {
                        "smooth"
                    });
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "This focused handler consumes only its own cinematic commands."
    )]
    fn scripts(draft: &mut Self, command: Command, doc: &mut Document) -> Result<bool, String> {
        match command {
            Command::Script | Command::Handler => {
                let row = value_at_mut(&mut draft.sequence, &format!("/steps/{}", draft.row))?;
                if value_at(row, "/action/action") != "run_script" {
                    return Err("Choose a Script row".into());
                }
                let key = if command == Command::Script {
                    "script"
                } else {
                    "handler"
                };
                let choices = if command == Command::Script {
                    doc.script_paths()
                } else {
                    let path = value_at(row, "/action/script")
                        .as_str()
                        .ok_or("Choose a script")?;
                    doc.open_script(path)?;
                    doc.script_handlers(path)?
                };
                let current = value_at(row, &format!("/action/{key}"))
                    .as_str()
                    .unwrap_or_default();
                let index = choices.iter().position(|v| v == current).map_or(0, |i| {
                    i.saturating_add(1)
                        .checked_rem(choices.len().max(1))
                        .unwrap_or(0)
                });
                *value_at_mut(row, &format!("/action/{key}"))? = json!(choices.get(index).ok_or(
                    "No declared scripts/exported handlers; author a module in Scripts first"
                )?);
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}
