//! Native ordered behavior blocks over shared serialized AI tasks and senses.
mod model;
mod sheet;
use super::{
    App,
    actions::Popup,
    document::ObjectId,
    ui::{Action, Hit},
};
pub(super) use model::{Block, Draft};
use sdl3::{
    event::{Event, WindowEvent},
    keyboard::Keycode,
    mouse::MouseButton,
    video::Window,
};
use serde_json::{Value, json};
pub(super) use sheet::draw;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Command {
    Open,
    Setup,
    Reload,
    Add(Block),
    Row(usize),
    Remove,
    Field(usize),
    Choice(usize),
    Fields(bool),
    Rows(bool),
    Apply,
    Check,
    GenerateCopy,
    EditSource,
    UseVisual,
    Response(u8),
    Location(usize),
    Locations,
    Alerts,
    Runtime,
    RuntimeBack,
    RuntimePage(bool),
}
impl App {
    pub(super) fn behavior_action(&mut self, action: Action) -> Result<bool, String> {
        let Action::Behavior(command) = action else {
            return Ok(false);
        };
        if self.behavior_runtime(command)? {
            return Ok(true);
        }
        if self.behavior_open(command)? {
            return Ok(true);
        }
        let doc = self.document.as_mut().ok_or("Open a project first")?;
        let draft = self.behavior.as_mut().ok_or("Open Behaviors first")?;
        if draft.root != doc.root() || draft.level != doc.level_id() {
            return Err("Behavior belongs to another level; reopen it".into());
        }
        draft.finish_field()?;
        match command {
            Command::Open
            | Command::Reload
            | Command::Setup
            | Command::Runtime
            | Command::RuntimeBack
            | Command::RuntimePage(_) => {}
            Command::Add(kind) => draft.add(kind, None)?,
            Command::Row(index) => {
                draft.select(index)?;
            }
            Command::Remove => draft.remove()?,
            Command::Field(index) => draft.open_field(doc, index)?,
            Command::Choice(index) => draft.choose(index)?,
            Command::Fields(forward) => {
                draft.page_fields(doc, forward);
            }
            Command::Rows(forward) => {
                draft.row_page = if forward {
                    draft
                        .row_page
                        .saturating_add(1)
                        .min(draft.rows().len().saturating_sub(1) / 8)
                } else {
                    draft.row_page.saturating_sub(1)
                }
            }
            Command::Response(kind) => draft.response(kind)?,
            Command::Locations => draft.locations = true,
            Command::Location(index) => {
                draft.location(doc, index)?;
                draft.locations = false;
            }
            Command::Alerts => draft.toggle_alerts()?,
            Command::EditSource => {
                let path = doc
                    .behavior_script(draft.id)
                    .ok_or("Generate a script copy first")?
                    .to_owned();
                doc.open_script(&path)?;
                self.popup = Some(Popup::ScriptEdit {
                    path,
                    cursor: 0,
                    scroll: 0,
                    all: false,
                });
            }
            Command::Apply | Command::UseVisual | Command::Check | Command::GenerateCopy => {
                if doc.raw() != &draft.expected {
                    return Err("Scene changed while editing. Copy any pending values, then choose Reload scene; nothing was applied.".into());
                }
                if doc.behavior_script(draft.id).is_some() && command == Command::Apply {
                    return Err("Luau owns this NPC. Choose 'Use visual' explicitly to suspend that module and keep its source, or generate a new script copy.".into());
                }
                let _candidate = doc.behavior_candidate(draft.id, &draft.plan)?;
                if command == Command::Check {
                    self.status="Behavior valid. Shared runtime enforces LOS, contact, priorities and execution budgets; Play verifies reachability.".into();
                } else {
                    if command == Command::GenerateCopy {
                        if self.compile.is_some() || self.play.is_some() {
                            return Err("Finish Build/Play before creating a module".into());
                        }
                        let path = doc.generate_behavior_script(draft.id, &draft.plan)?;
                        self.status = format!(
                            "Created {path}; Luau now owns execution. Previous source files are retained."
                        );
                    } else {
                        doc.apply_behavior(draft.id, &draft.plan)?;
                        self.status="Visual behavior applied. Any former Luau source is retained and unlinked.".into();
                    }
                    draft.expected = doc.raw().clone();
                    draft.original_plan = draft.plan.clone();
                    self.changed();
                }
            }
        }
        Ok(true)
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Only runtime snapshot commands are consumed by this focused dispatcher."
    )]
    fn behavior_runtime(&mut self, command: Command) -> Result<bool, String> {
        if !matches!(
            command,
            Command::Runtime | Command::RuntimeBack | Command::RuntimePage(_)
        ) {
            return Ok(false);
        }
        let draft = self.behavior.as_mut().ok_or("Open Behaviors first")?;
        match command {
            Command::Runtime => {
                let path = self
                    .last_play_trace
                    .as_ref()
                    .ok_or("Build+Play first; runtime state is captured from the real player")?;
                let bytes = crate::engine::project::read_bounded_file(path, 262_144)
                    .map_err(|error| format!("Runtime trace is not ready yet: {error}"))?;
                let source = String::from_utf8(bytes).map_err(|error| error.to_string())?;
                let mut lines = Vec::new();
                for raw_line in source.lines().take(1024) {
                    let line = runtime_summary(raw_line);
                    let chars: Vec<char> = line.chars().take(2048).collect();
                    for chunk in chars.chunks(110) {
                        lines.push(chunk.iter().collect());
                    }
                }
                draft.runtime = Some(lines);
                draft.runtime_page = 0;
            }
            Command::RuntimeBack => draft.runtime = None,
            Command::RuntimePage(forward) => {
                let last = draft
                    .runtime
                    .as_ref()
                    .map_or(0, |lines| lines.len().saturating_sub(1) / 20);
                draft.runtime_page = if forward {
                    draft.runtime_page.saturating_add(1).min(last)
                } else {
                    draft.runtime_page.saturating_sub(1)
                };
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn behavior_open(&mut self, command: Command) -> Result<bool, String> {
        if command == Command::Open {
            let doc = self.document.as_ref().ok_or("Open a project first")?;
            let id = self.selected.ok_or("Select an NPC first")?;
            if self.behavior.as_ref().is_none_or(|draft| {
                draft.id != id || draft.root != doc.root() || draft.level != doc.level_id()
            }) {
                if self
                    .behavior
                    .as_ref()
                    .is_some_and(|draft| draft.plan != draft.original_plan || draft.edit.is_some())
                {
                    self.popup = Some(Popup::Behavior);
                    return Err("Unapplied behavior retained. Apply it before changing NPC, or choose Reload scene to explicitly discard that draft.".into());
                }
                self.behavior = Some(Draft::new(doc, id)?);
            }
            self.popup = Some(Popup::Behavior);
            return Ok(true);
        }
        if command == Command::Reload || command == Command::Setup {
            let id = self.selected.ok_or("Select an NPC first")?;
            let doc = self.document.as_mut().ok_or("Open a project first")?;
            if command == Command::Setup {
                if doc
                    .component_object(id)
                    .and_then(|v| v.get("components"))
                    .and_then(Value::as_array)
                    .is_some_and(|values| {
                        values.iter().any(|c| {
                            matches!(
                                c.get("component").and_then(Value::as_str),
                                Some("ai" | "nav_agent")
                            )
                        })
                    })
                {
                    return Err("Existing AI/body preserved. Configure its body explicitly; Enable AI only initializes a new NPC.".into());
                }
                doc.set_entity_behavior(id, 1, &self.catalog)?;
            }
            self.behavior = Some(Draft::new(doc, id)?);
            self.changed();
            return Ok(true);
        }
        Ok(false)
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        clippy::rest_pattern_accessible_field,
        reason = "The behavior sheet consumes only its text, drag, and cancellation events."
    )]
    pub(super) fn behavior_event(&mut self, event: &Event, window: &Window, hits: &[Hit]) -> bool {
        if !matches!(self.popup, Some(Popup::Behavior)) {
            return false;
        }
        let size = window.size_in_pixels();
        let point = |x, y| {
            crate::engine::render::DrawableSize::new(size.0, size.1)
                .window_to_ui(window.size(), [x, y])
        };
        let Some(draft) = &mut self.behavior else {
            return false;
        };
        if draft.runtime.is_some()
            && matches!(
                event,
                Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                }
            )
        {
            draft.runtime = None;
            return true;
        }
        if draft.edit.is_some() {
            return behavior_text(draft, event, &mut self.status);
        }
        match event {
            Event::MouseButtonDown {
                mouse_btn: MouseButton::Left,
                x,
                y,
                ..
            } => {
                if let Some(position) = point(*x, *y)
                    && let Some(hit) = hits.iter().find(|h| h.contains(position))
                    && let Action::Behavior(command @ (Command::Add(_) | Command::Row(_))) =
                        hit.action
                {
                    draft.drag = Some((command, position));
                    return true;
                }
            }
            Event::MouseButtonUp {
                mouse_btn: MouseButton::Left,
                x,
                y,
                ..
            } => {
                if let Some((command, start)) = draft.drag.take()
                    && let Some(position) = point(*x, *y)
                {
                    let result = drop_block(draft, hits, command, start, position);
                    let consumed = result.as_ref().copied().unwrap_or(true);
                    self.report(result.map(|_| ()));
                    if consumed {
                        return true;
                    }
                }
            }
            Event::KeyDown {
                keycode: Some(Keycode::Escape),
                ..
            } => {
                if draft.drag.take().is_some() || draft.choices.take().is_some() || draft.locations
                {
                    draft.locations = false;
                    return true;
                }
            }
            Event::Window {
                win_event: WindowEvent::FocusLost,
                ..
            } => {
                draft.drag = None;
                return true;
            }
            Event::MouseWheel {
                y,
                mouse_x,
                mouse_y,
                ..
            } => {
                if let Some(position) = point(*mouse_x, *mouse_y) {
                    let command = if draft.runtime.is_some() {
                        Command::RuntimePage(*y < 0.0)
                    } else if position[0] < 270.0 {
                        Command::Rows(*y < 0.0)
                    } else {
                        Command::Fields(*y < 0.0)
                    };
                    let result = self.behavior_action(Action::Behavior(command)).map(|_| ());
                    self.report(result);
                    return true;
                }
            }
            _ => {}
        }
        false
    }
}

#[expect(
    clippy::wildcard_enum_match_arm,
    clippy::rest_pattern_accessible_field,
    reason = "Only text-field editing events are consumed here."
)]
fn behavior_text(draft: &mut Draft, event: &Event, status: &mut String) -> bool {
    let mut finish = false;
    match event {
        Event::TextInput { text, .. } => {
            if let Some((_, value)) = &mut draft.edit
                && value.len().saturating_add(text.len()) <= 256
            {
                value.push_str(text);
            }
        }
        Event::KeyDown {
            keycode: Some(Keycode::A),
            keymod,
            ..
        } if super::input::command(*keymod) => {
            if let Some((_, value)) = &mut draft.edit {
                value.clear();
            }
        }
        Event::KeyDown {
            keycode: Some(Keycode::Backspace),
            ..
        } => {
            if let Some((_, value)) = &mut draft.edit {
                let _removed = value.pop();
            }
        }
        Event::KeyDown {
            keycode: Some(Keycode::Return | Keycode::Tab),
            ..
        } => finish = true,
        Event::KeyDown {
            keycode: Some(Keycode::Escape),
            ..
        } => {
            draft.edit = None;
        }
        Event::MouseButtonUp { .. } => return false,
        _ => return true,
    }
    if finish {
        let result = draft.finish_field();
        if let Err(error) = result {
            *status = error;
        }
    }

    true
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "Only draggable palette/row commands reach this drop target."
)]
fn drop_block(
    draft: &mut Draft,
    hits: &[Hit],
    command: Command,
    start: [f32; 2],
    position: [f32; 2],
) -> Result<bool, String> {
    let moved = (position[0] - start[0]).abs() + (position[1] - start[1]).abs() > 4.0;
    if moved {
        let target = hits.iter().find(|h| h.contains(position)).and_then(|h| {
            if let Action::Behavior(Command::Row(index)) = h.action {
                Some(index)
            } else {
                None
            }
        });
        let result = if (100.0..270.0).contains(&position[0]) {
            match command {
                Command::Add(kind) => draft.add(kind, target),
                Command::Row(index) => draft.reorder(index, target),
                _ => Ok(()),
            }
        } else {
            Ok(())
        };
        return result.map(|()| true);
    }

    Ok(false)
}

fn runtime_summary(line: &str) -> String {
    let field = |key| {
        line.split_once(key)
            .and_then(|(_, value)| value.split_whitespace().next())
            .unwrap_or("-")
    };
    if let Some((actor, _)) = line.split_once(" owner=") {
        let result = field(" result=")
            .trim_start_matches("Some(\"")
            .trim_end_matches("\")");
        format!(
            "{actor} | owner: {} | state: {} | result: {} | position: {}",
            field(" owner="),
            field(" state="),
            if result == "None" {
                "pending / none"
            } else {
                result
            },
            field(" pos=")
        )
    } else if line.trim_start().starts_with("key=") {
        format!(
            "  Task {} | active: {} | paused: {} | priority: {} | phase: {} | step: {} | {}",
            field("key="),
            field("active="),
            field("paused="),
            field("priority="),
            field("phase="),
            field("step="),
            field("status=")
        )
    } else {
        line.to_owned()
    }
}
