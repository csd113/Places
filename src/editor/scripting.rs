//! Focused native source sheet; source buffers survive sheet/level changes and
//! join the existing unsaved/save/build guard. No external editor is required.

use super::{App, actions::Popup, ui::Action};
use sdl3::event::Event;
use sdl3::keyboard::Keycode;
use sdl3::video::Window;

impl App {
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "This focused dispatcher consumes only script sheet controls"
    )]
    pub(super) fn script_action(&mut self, action: Action) -> Result<bool, String> {
        match action {
            Action::Scripts => self.popup = Some(Popup::Scripts),
            Action::ScriptNew => {
                if self.compile.is_some() || self.play.is_some() {
                    return Err("Finish Build/Play before adding scripts".into());
                }
                let path = self
                    .document
                    .as_mut()
                    .ok_or("Open project first")?
                    .create_script()?;
                self.popup = Some(Popup::ScriptEdit {
                    path,
                    cursor: 0,
                    scroll: 0,
                    all: false,
                });
                self.status = "New module linked to this level. Edit, Save, Check, then link a handler to a trigger.".into();
            }
            Action::ScriptOpen(index) => {
                let doc = self.document.as_mut().ok_or("Open project first")?;
                let path = doc
                    .script_paths()
                    .get(index)
                    .cloned()
                    .ok_or("Missing script")?;
                doc.open_script(&path)?;
                self.popup = Some(Popup::ScriptEdit {
                    path,
                    cursor: 0,
                    scroll: 0,
                    all: false,
                });
            }
            Action::ScriptDiagnostics => self.popup = Some(Popup::ScriptDiagnostics),
            Action::ScriptCheck => {
                if let Err(problem) = self
                    .document
                    .as_ref()
                    .ok_or("Open project first")?
                    .check_scripts()
                {
                    self.script_feedback = problem.chars().take(4096).collect();
                    self.status =
                        "Luau check failed; review diagnostics, then return to the source module."
                            .into();
                    self.popup = Some(Popup::ScriptDiagnostics);
                    return Ok(true);
                }
                self.script_feedback = "Luau initialization and linked export checks passed. Play exercises runtime arguments and effects.".into();
                self.status = "Luau check passed: bounded initialization and exports. Play verifies handler behavior.".into();
            }
            Action::ScriptLink => {
                if !matches!(self.selected, Some(super::document::ObjectId::Trigger(_))) {
                    return Err("Select a trigger before linking a handler".into());
                }
                if let Some(Popup::ScriptEdit {
                    path,
                    cursor: _,
                    scroll: _,
                    all: _,
                }) = &self.popup
                {
                    let names = self
                        .document
                        .as_ref()
                        .ok_or("Open project first")?
                        .script_handlers(path)?;
                    self.popup = Some(Popup::ScriptHandlers {
                        path: path.clone(),
                        names,
                        page: 0,
                    });
                }
            }
            Action::ScriptHandler(index) => self.link_script_handler(index)?,
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn link_script_handler(&mut self, index: usize) -> Result<(), String> {
        if let Some(Popup::ScriptHandlers {
            path,
            names,
            page: _,
        }) = &self.popup
        {
            let doc = self.document.as_mut().ok_or("Open project first")?;
            let handler = names.get(index).cloned().ok_or("Missing handler")?;
            if let Some(super::document::ObjectId::Trigger(trigger_index)) = self.selected
                && doc
                    .level()
                    .volumes
                    .get(trigger_index)
                    .and_then(|volume| volume.bindings.first())
                    .is_some_and(|binding| binding.actions.len() >= 8)
            {
                return Err("The native trigger sheet supports eight actions; remove an action before linking another handler".into());
            }
            doc.add_trigger_action(
                self.selected.ok_or("Select trigger first")?,
                serde_json::json!({"action":"run_script","script":path,"handler":handler}),
            )?;
            self.status = format!(
                "Linked {path} / {handler}; trigger enter/exit, cooldown and once policy apply."
            );
            self.changed();
            self.popup = Some(Popup::Gameplay { page: 0 });
        }
        Ok(())
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        clippy::too_many_lines,
        reason = "SDL source editing consumes text/navigation keys and leaves unrelated events to ordinary modal routing"
    )]
    pub(super) fn script_event(&mut self, event: &Event, window: &Window) -> bool {
        let Some(Popup::ScriptEdit {
            path,
            cursor,
            scroll,
            all,
        }) = &mut self.popup
        else {
            return false;
        };
        let Some(doc) = &mut self.document else {
            return false;
        };
        let Some(buffer) = doc.script_text(path) else {
            return false;
        };
        let mut source = buffer.to_string();
        if let Event::KeyDown {
            keycode: Some(key),
            keymod,
            repeat: _,
            timestamp: _,
            window_id: _,
            scancode: _,
            which: _,
            raw: _,
        } = event
            && super::input::command(*keymod)
            && matches!(*key, Keycode::Z | Keycode::Y)
        {
            let redo = *key == Keycode::Y
                || keymod
                    .intersects(sdl3::keyboard::Mod::LSHIFTMOD | sdl3::keyboard::Mod::RSHIFTMOD);
            let _restored = doc.script_history(path, redo);
            *cursor = doc.script_text(path).map_or(0, str::len);
            *all = false;
            return true;
        }
        let mut insertion = None;
        let mut handled = true;
        let mut save = false;
        let mut check = false;
        match event {
            Event::TextInput {
                text,
                timestamp: _,
                window_id: _,
            } => insertion = Some(text.clone()),
            Event::KeyDown {
                keycode: Some(key),
                keymod,
                repeat: _,
                timestamp: _,
                window_id: _,
                scancode: _,
                which: _,
                raw: _,
            } => {
                let command = super::input::command(*keymod);
                match *key {
                    Keycode::S if command => save = true,
                    Keycode::B if command => check = true,
                    Keycode::A if command => *all = true,
                    Keycode::C if command && *all => {
                        let _copied = window.subsystem().clipboard().set_clipboard_text(&source);
                    }
                    Keycode::V if command => {
                        match window.subsystem().clipboard().clipboard_text() {
                            Ok(value) => insertion = Some(value),
                            Err(problem) => self.status = problem.to_string(),
                        }
                    }
                    Keycode::Return => insertion = Some("\n".into()),
                    Keycode::Tab => insertion = Some("    ".into()),
                    Keycode::Backspace => {
                        if *all {
                            source.clear();
                            *cursor = 0;
                            *all = false;
                        } else if let Some(previous) = source.get(..*cursor).and_then(|prefix| {
                            prefix.char_indices().next_back().map(|(index, _)| index)
                        }) {
                            drop(source.drain(previous..*cursor));
                            *cursor = previous;
                        }
                    }
                    Keycode::Delete => {
                        if *all {
                            source.clear();
                            *cursor = 0;
                            *all = false;
                        } else if let Some(next) = source.get(*cursor..).and_then(|suffix| {
                            suffix
                                .chars()
                                .next()
                                .map(|c| cursor.saturating_add(c.len_utf8()))
                        }) {
                            drop(source.drain(*cursor..next));
                        }
                    }
                    Keycode::Left => {
                        *all = false;
                        *cursor = source
                            .get(..*cursor)
                            .and_then(|prefix| {
                                prefix.char_indices().next_back().map(|(index, _)| index)
                            })
                            .unwrap_or(0);
                    }
                    Keycode::Right => {
                        *all = false;
                        *cursor = source
                            .get(*cursor..)
                            .and_then(|suffix| {
                                suffix
                                    .chars()
                                    .next()
                                    .map(|c| cursor.saturating_add(c.len_utf8()))
                            })
                            .unwrap_or(source.len());
                    }
                    Keycode::Home => {
                        *all = false;
                        *cursor = source
                            .get(..*cursor)
                            .and_then(|prefix| {
                                prefix.rfind('\n').map(|index| index.saturating_add(1))
                            })
                            .unwrap_or(0);
                    }
                    Keycode::End => {
                        *all = false;
                        *cursor = source
                            .get(*cursor..)
                            .and_then(|suffix| {
                                suffix.find('\n').map(|index| cursor.saturating_add(index))
                            })
                            .unwrap_or(source.len());
                    }
                    Keycode::Up | Keycode::Down => {
                        *all = false;
                        *cursor = vertical_cursor(&source, *cursor, *key == Keycode::Down);
                    }
                    Keycode::Escape => handled = false,
                    _ => {}
                }
            }
            Event::MouseWheel {
                y,
                timestamp: _,
                window_id: _,
                which: _,
                x: _,
                direction: _,
                integer_x: _,
                integer_y: _,
                mouse_x: _,
                mouse_y: _,
            } => {
                *scroll = if *y > 0.0 {
                    scroll.saturating_sub(3)
                } else {
                    scroll
                        .saturating_add(3)
                        .min(source.lines().count().saturating_sub(20))
                };
            }
            _ => handled = false,
        }
        if let Some(value) = insertion {
            let normalized = value.replace('\r', "");
            let retained = if *all { 0 } else { source.len() };
            if retained.saturating_add(normalized.len()) <= 65_536 {
                if *all {
                    source.clear();
                    *cursor = 0;
                    *all = false;
                }
                source.insert_str(*cursor, &normalized);
                *cursor = cursor.saturating_add(normalized.len());
            } else {
                self.status = "Script exceeds 64 KiB; insertion refused".into();
            }
        }
        if handled {
            let line = source
                .get(..*cursor)
                .map_or(0, |prefix| prefix.bytes().filter(|c| *c == b'\n').count());
            if !matches!(
                event,
                Event::MouseWheel {
                    y: _,
                    timestamp: _,
                    window_id: _,
                    which: _,
                    x: _,
                    direction: _,
                    integer_x: _,
                    integer_y: _,
                    mouse_x: _,
                    mouse_y: _
                }
            ) {
                if line < *scroll {
                    *scroll = line;
                } else if line >= scroll.saturating_add(20) {
                    *scroll = line.saturating_sub(19);
                }
            }
            let result = doc.set_script_text(path, source);
            self.report(result);
            if save {
                let saved = self.save();
                self.report(saved);
            }
            if check {
                let checked = self.script_action(Action::ScriptCheck).map(|_| ());
                self.report(checked);
            }
        }
        handled
    }
}

fn vertical_cursor(source: &str, cursor: usize, down: bool) -> usize {
    let prefix = source.get(..cursor).unwrap_or_default();
    let start = prefix.rfind('\n').map_or(0, |i| i.saturating_add(1));
    let column = source
        .get(start..cursor)
        .unwrap_or_default()
        .chars()
        .count();
    let next = if down {
        source
            .get(cursor..)
            .and_then(|tail| tail.find('\n'))
            .map(|i| cursor.saturating_add(i).saturating_add(1))
    } else {
        source
            .get(..start.saturating_sub(1))
            .map(|before| before.rfind('\n').map_or(0, |i| i.saturating_add(1)))
    };
    next.map_or(cursor, |offset| {
        source
            .get(offset..)
            .unwrap_or_default()
            .split('\n')
            .next()
            .unwrap_or_default()
            .char_indices()
            .nth(column)
            .map_or_else(
                || {
                    offset.saturating_add(
                        source
                            .get(offset..)
                            .unwrap_or_default()
                            .split('\n')
                            .next()
                            .unwrap_or_default()
                            .len(),
                    )
                },
                |(i, _)| offset.saturating_add(i),
            )
    })
}
