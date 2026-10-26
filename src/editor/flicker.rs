//! A shared native form for fixture/circuit switching and asset-wide glow.
use super::{
    App,
    actions::Popup,
    document::{Document, ObjectId, flicker::Flicker},
    ui::{self, Action, Hit},
};
use crate::engine::render::Vertex;
use sdl3::{event::Event, keyboard::Keycode};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Command {
    Open,
    Tab,
    Enabled,
    Scope,
    Auto,
    Preset,
    Field(usize),
    Apply,
    Replace,
    Edit,
    Reload,
}
pub(super) struct Draft {
    root: std::path::PathBuf,
    level: String,
    label: String,
    id: ObjectId,
    expected: Value,
    config: Flicker,
    glow: bool,
    glow_enabled: bool,
    values: [String; 9],
    active: Option<usize>,
    dirty: [bool; 2],
    preset: usize,
}
impl Draft {
    fn new(doc: &Document, id: ObjectId) -> Result<Self, String> {
        let ObjectId::Light(index) = id else {
            return Err("Select a light fixture to author flicker".into());
        };
        let light = doc
            .level()
            .ceiling_lights
            .get(index)
            .ok_or("Missing fixture")?;
        let config = doc.flicker_config(id)?;
        let emission = doc
            .level()
            .animated_emissions
            .iter()
            .find(|e| e.material == light.fixture);
        Ok(Self {
            root: doc.root().into(),
            level: doc.level_id().into(),
            label: doc
                .level()
                .light_instance_ids()
                .get(index)
                .cloned()
                .ok_or("Missing fixture identity")?,
            id,
            expected: doc.raw().clone(),
            values: [
                config.steady.to_string(),
                config.dark.to_string(),
                config.pulses.to_string(),
                config.gap.to_string(),
                config.delay.to_string(),
                light.fixture.clone(),
                emission.and_then(|e| e.hz).unwrap_or(9.5).to_string(),
                emission.and_then(|e| e.depth).unwrap_or(0.55).to_string(),
                emission.and_then(|e| e.phase).unwrap_or(0.0).to_string(),
            ],
            config,
            glow: false,
            glow_enabled: emission.is_some(),
            active: None,
            dirty: [false; 2],
            preset: 0,
        })
    }
    const fn set_dirty(&mut self, value: bool) {
        if self.glow {
            self.dirty[1] = value;
        } else {
            self.dirty[0] = value;
        }
    }
    fn cycle_preset(&mut self) {
        self.preset = self.preset.saturating_add(1) % 3;
        if self.glow {
            let (hz, depth): (f32, f32) = match self.preset {
                1 => (7.5, 0.6),
                2 => (2.0, 0.8),
                _ => (9.5, 0.55),
            };
            self.values[6] = hz.to_string();
            self.values[7] = depth.to_string();
            self.glow_enabled = true;
        } else {
            let (steady, dark, pulses, gap): (f32, f32, u8, f32) = match self.preset {
                1 => (2.0, 0.08, 3, 0.1),
                2 => (8.0, 1.2, 1, 0.1),
                _ => (3.0, 0.12, 1, 0.1),
            };
            self.values[0] = steady.to_string();
            self.values[1] = dark.to_string();
            self.values[2] = pulses.to_string();
            self.values[3] = gap.to_string();
            self.config.enabled = true;
        }
        self.set_dirty(true);
    }
    fn number(&self, index: usize, label: &str) -> Result<f32, String> {
        self.values
            .get(index)
            .ok_or("Missing form field")?
            .parse::<f32>()
            .map_err(|_e| format!("{label} needs a number"))
    }
    fn settings(&self) -> Result<Flicker, String> {
        Ok(Flicker {
            steady: self.number(0, "Steady seconds")?,
            dark: self.number(1, "Dark seconds")?,
            pulses: self
                .values
                .get(2)
                .ok_or("Missing burst count")?
                .parse::<u8>()
                .map_err(|_e| "Burst count must be an integer from 1 to 8")?,
            gap: self.number(3, "Burst gap seconds")?,
            delay: self.number(4, "Start delay seconds")?,
            ..self.config.clone()
        })
    }
}
impl App {
    pub(super) fn flicker_action(&mut self, action: Action) -> Result<bool, String> {
        let Action::Flicker(command) = action else {
            return Ok(false);
        };
        let doc = self.document.as_mut().ok_or("Open a project first")?;
        let selected = self.selected.ok_or("Select a light fixture")?;
        if matches!(command, Command::Open | Command::Reload) {
            self.open_flicker(command, selected)?;
            return Ok(true);
        }
        let draft = self.flicker.as_mut().ok_or("Open Light flicker first")?;
        draft.active = None;
        match command {
            Command::Open | Command::Reload => {}
            Command::Tab => draft.glow = !draft.glow,
            Command::Field(index) => draft.active = Some(index),
            Command::Enabled => {
                if draft.glow {
                    draft.glow_enabled = !draft.glow_enabled;
                } else {
                    draft.config.enabled = !draft.config.enabled;
                }
                draft.set_dirty(true);
            }
            Command::Scope => {
                draft.config.circuit = !draft.config.circuit;
                draft.set_dirty(true);
            }
            Command::Auto => {
                draft.config.autostart = !draft.config.autostart;
                draft.set_dirty(true);
            }
            Command::Preset => draft.cycle_preset(),
            Command::Edit => {
                if draft.dirty.iter().any(|v| *v) {
                    return Err(
                        "Apply pending flicker fields or Reload before editing its script".into(),
                    );
                }
                let path = doc
                    .flicker_script(draft.id)?
                    .ok_or("Apply an enabled illumination preset first")?;
                doc.open_script(&path)?;
                self.popup = Some(Popup::ScriptEdit {
                    path,
                    cursor: 0,
                    scroll: 0,
                    all: false,
                });
            }
            Command::Apply | Command::Replace => {
                if self.compile.is_some() || self.play.is_some() {
                    return Err("Finish Build/Play before changing flicker".into());
                }
                if draft.root != doc.root()
                    || draft.level != doc.level_id()
                    || draft.expected != *doc.raw()
                {
                    return Err("Scene changed; pending flicker values retained. Reload before applying to current references.".into());
                }
                if draft.glow {
                    let asset = &draft.values[5];
                    if !self
                        .catalog
                        .get(asset)
                        .is_some_and(|entry| entry.is_material() || entry.is_light())
                    {
                        return Err(
                            "Glow target must be a catalog material or fixture asset ID".into()
                        );
                    }
                    doc.apply_glow(
                        asset,
                        draft.glow_enabled,
                        draft.number(6, "Frequency")?,
                        draft.number(7, "Depth")?,
                        draft.number(8, "Phase")?,
                    )?;
                    self.status="Asset-wide glow saved in the document. Every matching surface/fixture shares it; room illumination stays unchanged.".into();
                } else {
                    let settings = draft.settings()?;
                    doc.apply_flicker(draft.id, &settings, command == Command::Replace)?;
                    draft.config = settings;
                    self.status="Flicker applied as one undo step. Save/Build+Play in Full. Existing source files retained; use generated mute/pause/resume event handlers.".into();
                }
                draft.expected = doc.raw().clone();
                draft.set_dirty(false);
                self.changed();
            }
        }
        Ok(true)
    }
    fn open_flicker(&mut self, command: Command, selected: ObjectId) -> Result<(), String> {
        let doc = self.document.as_ref().ok_or("Open a project first")?;
        if command == Command::Reload {
            self.flicker = Some(Draft::new(doc, selected)?);
        } else {
            let same = self.flicker.as_ref().is_some_and(|d| {
                d.root == doc.root() && d.level == doc.level_id() && d.id == selected
            });
            if !same
                && self
                    .flicker
                    .as_ref()
                    .is_some_and(|d| d.dirty.iter().any(|v| *v))
            {
                self.popup = Some(Popup::Flicker);
                self.status = "Pending fields belong to the previous fixture. Apply there, or Reload to discard and use this selection.".into();
                return Ok(());
            }
            if !same
                || self
                    .flicker
                    .as_ref()
                    .is_some_and(|d| !d.dirty.iter().any(|v| *v) && d.expected != *doc.raw())
            {
                self.flicker = Some(Draft::new(doc, selected)?);
            }
        }
        self.popup = Some(Popup::Flicker);
        Ok(())
    }
    #[expect(
        clippy::wildcard_enum_match_arm,
        clippy::rest_pattern_accessible_field,
        reason = "The modal text editor consumes focused editing events only."
    )]
    pub(super) fn flicker_event(&mut self, event: &Event) -> bool {
        if !matches!(self.popup, Some(Popup::Flicker)) {
            return false;
        }
        let Some(draft) = &mut self.flicker else {
            return false;
        };
        if matches!(
            event,
            Event::KeyDown {
                keycode: Some(Keycode::Escape),
                ..
            }
        ) {
            return draft.active.take().is_some();
        }
        let Some(index) = draft.active else {
            return false;
        };
        let Some(value) = draft.values.get_mut(index) else {
            return false;
        };
        match event {
            Event::TextInput { text, .. } => {
                if value.len().saturating_add(text.len()) <= 96 {
                    value.push_str(text);
                    draft.set_dirty(true);
                }
            }
            Event::KeyDown {
                keycode: Some(Keycode::A),
                keymod,
                ..
            } if super::input::command(*keymod) => {
                value.clear();
                draft.set_dirty(true);
            }
            Event::KeyDown {
                keycode: Some(Keycode::Backspace),
                ..
            } => {
                let _removed = value.pop();
                draft.set_dirty(true);
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
fn button(v: &mut Vec<Vertex>, hits: &mut Vec<Hit>, rect: [f32; 4], label: &str, command: Command) {
    ui::popup_button(v, hits, rect, label, Action::Flicker(command));
}
fn field(
    v: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    draft: &Draft,
    index: usize,
    rect: [f32; 3],
    label: &str,
) {
    button(
        v,
        hits,
        [rect[0], rect[1], rect[2], 13.0],
        &format!(
            "{}{}: {}",
            if draft.active == Some(index) {
                "> "
            } else {
                ""
            },
            label,
            draft.values.get(index).map_or("", String::as_str)
        ),
        Command::Field(index),
    );
}
pub(super) fn draw(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    ui::rect(v, [0.0, 38.0, 480.0, 204.0], [0.055, 0.08, 0.11, 1.0]);
    ui::text(
        v,
        &format!(
            "LIGHT FLICKER: {}",
            app.flicker
                .as_ref()
                .map_or("Select fixture", |d| d.label.as_str())
        ),
        8.0,
        43.0,
        ui::INK,
        105,
    );
    ui::popup_button(
        v,
        hits,
        [422.0, 41.0, 50.0, 11.0],
        "Close",
        Action::ClosePopup,
    );
    let Some(d) = &app.flicker else {
        return;
    };
    let Some(doc) = &app.document else {
        return;
    };
    button(
        v,
        hits,
        [8.0, 58.0, 290.0, 14.0],
        if d.glow {
            "Effect: GLOW ONLY / all matching assets >"
        } else {
            "Effect: ACTUAL ILLUMINATION >"
        },
        Command::Tab,
    );
    button(
        v,
        hits,
        [310.0, 58.0, 162.0, 14.0],
        if if d.glow {
            d.glow_enabled
        } else {
            d.config.enabled
        } {
            "Flicker enabled: Yes"
        } else {
            "Flicker enabled: No"
        },
        Command::Enabled,
    );
    if d.glow {
        draw_glow(d, v, hits);
    } else {
        draw_illumination(d, doc, v, hits);
    }
    draw_footer(d, v, hits);
}
fn draw_glow(d: &Draft, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    ui::text(
        v,
        "GLOBAL ASSET SCOPE: every matching material or fixture face.",
        8.0,
        79.0,
        ui::INK,
        125,
    );
    ui::text(
        v,
        "The room's baked illumination does not flicker. Existing actual controls stay intact.",
        8.0,
        89.0,
        ui::MUTED,
        125,
    );
    field(
        v,
        hits,
        d,
        5,
        [8.0, 104.0, 464.0],
        "Material / fixture asset",
    );
    field(v, hits, d, 6, [8.0, 124.0, 222.0], "Frequency Hz");
    field(
        v,
        hits,
        d,
        7,
        [242.0, 124.0, 230.0],
        "Depth (drop fraction)",
    );
    field(v, hits, d, 8, [8.0, 144.0, 222.0], "Phase cycles");
    ui::text(
        v,
        "Depth 0.6 falls to 40% face glow. Switches still gate the face.",
        8.0,
        166.0,
        ui::MUTED,
        125,
    );
    ui::text(
        v,
        "Apply replaces only this asset's emission entry; Off removes that global entry.",
        8.0,
        176.0,
        ui::MUTED,
        125,
    );
}
fn draw_illumination(d: &Draft, doc: &Document, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    let label = doc.flicker_members(d.id, d.config.circuit).map_or_else(
        |e| e,
        |members| {
            format!(
                "{} / {} fixture(s)",
                if d.config.circuit {
                    "Current shared circuit"
                } else {
                    "Individual fixture"
                },
                members.len()
            )
        },
    );
    button(
        v,
        hits,
        [8.0, 78.0, 464.0, 14.0],
        &format!("Scope > {label}"),
        Command::Scope,
    );
    field(v, hits, d, 0, [8.0, 104.0, 222.0], "Steady seconds");
    field(v, hits, d, 1, [242.0, 104.0, 230.0], "Dark seconds");
    field(v, hits, d, 2, [8.0, 124.0, 222.0], "Burst count");
    field(v, hits, d, 3, [242.0, 124.0, 230.0], "Burst gap seconds");
    field(v, hits, d, 4, [8.0, 144.0, 222.0], "Start delay seconds");
    button(
        v,
        hits,
        [242.0, 144.0, 230.0, 13.0],
        if d.config.autostart {
            "Run on load: Yes"
        } else {
            "Run on load: No / paused"
        },
        Command::Auto,
    );
    ui::text(
        v,
        "Full: existing Power when on, zero when off. No partial room-light dimmer.",
        8.0,
        166.0,
        ui::MUTED,
        125,
    );
    ui::text(
        v,
        "Static holds authored state unless its circuit has prepared Static switching.",
        8.0,
        176.0,
        ui::MUTED,
        125,
    );
}
fn draw_footer(d: &Draft, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    button(
        v,
        hits,
        [8.0, 188.0, 222.0, 13.0],
        &format!(
            "Preset > {}",
            if d.glow {
                ["Subtle fault", "Stutter glow", "Slow glow"]
                    .get(d.preset)
                    .copied()
                    .unwrap_or("Custom")
            } else {
                ["Brief dropouts", "Stutter burst", "Slow outage"]
                    .get(d.preset)
                    .copied()
                    .unwrap_or("Custom")
            }
        ),
        Command::Preset,
    );
    button(
        v,
        hits,
        [242.0, 188.0, 230.0, 13.0],
        "Reload / discard pending fields",
        Command::Reload,
    );
    button(
        v,
        hits,
        [8.0, 207.0, 222.0, 14.0],
        if d.glow {
            "Apply to ALL matching assets"
        } else {
            "Apply fixture / circuit settings"
        },
        Command::Apply,
    );
    if d.glow {
        ui::text(
            v,
            "Glow and actual presets are independent. Closing retains pending fields.",
            8.0,
            227.0,
            ui::MUTED,
            125,
        );
    } else {
        button(
            v,
            hits,
            [242.0, 207.0, 230.0, 14.0],
            "Edit generated Luau / handlers >",
            Command::Edit,
        );
        button(
            v,
            hits,
            [8.0, 225.0, 222.0, 12.0],
            "Replace link / keep source",
            Command::Replace,
        );
        ui::text(
            v,
            "Explicit takeover; never overwrites source.",
            242.0,
            227.0,
            ui::MUTED,
            62,
        );
    }
}
