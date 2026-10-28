//! Contextual circuit membership, switch assignment and private lighting views.
use super::{
    App,
    actions::Popup,
    document::{ObjectId, ObjectKind},
    ui::{self, Action, Hit},
};
use crate::engine::render::Vertex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Command {
    Open,
    Fixture(usize),
    Switch(usize),
    Page(bool),
    Apply,
    Unlink,
    Paint,
    Preview,
    NewSwitch,
    Local,
    CancelLocal,
}

#[derive(Default)]
pub(super) struct Draft {
    pub fixtures: Vec<usize>,
    pub switch: Option<usize>,
    pub painting: bool,
    pub preview_off: bool,
    page: usize,
    expected: Option<serde_json::Value>,
}

impl App {
    pub(super) fn lighting_action(&mut self, action: Action) -> Result<bool, String> {
        let Action::Lighting(command) = action else {
            return Ok(false);
        };
        if command == Command::Open {
            self.open_lighting()?;
            return Ok(true);
        }
        match command {
            Command::Open => {}
            Command::Fixture(index) => self.toggle_circuit_fixture(index),
            Command::Switch(index) => self.lighting.switch = Some(index),
            Command::Page(next) => {
                self.lighting.page = if next {
                    self.lighting.page.saturating_add(1)
                } else {
                    self.lighting.page.saturating_sub(1)
                }
            }
            Command::Paint => {
                self.lighting.painting = true;
                self.popup = None;
                self.status =
                    "Click fixtures to add/remove selection; Circuits returns to assignment."
                        .into();
            }
            Command::Apply | Command::Unlink => {
                let doc = self.document.as_mut().ok_or("Open a project first")?;
                if self.lighting.expected.as_ref() != Some(doc.raw()) {
                    return Err("Scene changed; reopen Circuits to refresh selection".into());
                }
                if command == Command::Apply {
                    doc.assign_circuit(
                        &self.lighting.fixtures,
                        self.lighting
                            .switch
                            .ok_or("Choose a switch prop or add a wall switch")?,
                    )?;
                } else {
                    doc.unlink_circuit(&self.lighting.fixtures)?;
                }
                self.lighting.expected = Some(doc.raw().clone());
                self.changed();
                self.status = "Circuit updated in one undo step. Full lighting builds real switched illumination.".into();
            }
            Command::Preview => {
                self.lighting.preview_off = !self.lighting.preview_off;
                let mut level = self
                    .document
                    .as_ref()
                    .ok_or("Open a project first")?
                    .level()
                    .clone();
                for index in &self.lighting.fixtures {
                    if let Some(light) = level.ceiling_lights.get_mut(*index) {
                        light.enabled &= !self.lighting.preview_off;
                    }
                }
                self.viewport.set_environment_preview(true);
                self.viewport.rebuild(&level);
                self.popup = None;
                self.status = "Editor circuit state only. Play runs native power and flicker with prepared illumination.".into();
            }
            Command::Local => {
                let doc = self.document.as_ref().ok_or("Open a project first")?;
                let scope = local_scope(doc.level(), self.selected)?;
                self.focus = super::focus::Focus::default();
                self.viewport.set_focus(self.focus.clone());
                self.viewport.start_local_preview(doc.level(), scope);
                self.popup = None;
                self.status = "Local preview: non-final; full scene lights/occluders, bounded receiver halo. No export/cache writes.".into();
            }
            Command::CancelLocal => {
                self.changed();
                self.status = "Local preview cancelled / invalidated. Final packages and bake caches unchanged.".into();
            }
            Command::NewSwitch => {
                let doc = self.document.as_mut().ok_or("Open a project first")?;
                let [x, _, z] = self.viewport.target();
                let id = doc.add_with_catalog(
                    ObjectKind::Prop,
                    [x, 1.2, z],
                    Some("home:wall_switch"),
                    &self.catalog,
                )?;
                let ObjectId::Prop(index) = id else {
                    return Err("Missing wall switch".into());
                };
                self.lighting.switch = Some(index);
                self.lighting.expected = Some(doc.raw().clone());
                self.selected = Some(id);
                self.changed();
                self.status =
                    "Wall switch added at view centre; assign lights, then Move to mount it."
                        .into();
            }
        }
        Ok(true)
    }

    fn open_lighting(&mut self) -> Result<(), String> {
        let doc = self.document.as_ref().ok_or("Open a project first")?;
        if self.lighting.preview_off {
            self.viewport.rebuild(doc.level());
        }
        if !self.lighting.painting {
            self.lighting = Draft::default();
            let selected_group = match self.selected {
                Some(ObjectId::Light(i)) => {
                    self.lighting.fixtures.push(i);
                    doc.level()
                        .ceiling_lights
                        .get(i)
                        .and_then(|l| l.switch_group.as_deref())
                }
                Some(ObjectId::Prop(i)) => {
                    self.lighting.switch = Some(i);
                    doc.level()
                        .props
                        .get(i)
                        .and_then(|p| p.light_switch.as_deref())
                }
                _ => None,
            };
            if let Some(group) = selected_group {
                self.lighting.fixtures = doc
                    .level()
                    .ceiling_lights
                    .iter()
                    .enumerate()
                    .filter_map(|(i, l)| (l.switch_group.as_deref() == Some(group)).then_some(i))
                    .collect();
                self.lighting.switch = doc
                    .level()
                    .props
                    .iter()
                    .position(|p| p.light_switch.as_deref() == Some(group));
            }
            self.lighting.expected = Some(doc.raw().clone());
        }
        self.lighting.painting = false;
        self.popup = Some(Popup::Lighting);
        Ok(())
    }

    fn toggle_circuit_fixture(&mut self, index: usize) {
        if let Some(position) = self.lighting.fixtures.iter().position(|i| *i == index) {
            let _removed = self.lighting.fixtures.remove(position);
        } else {
            self.lighting.fixtures.push(index);
            self.lighting.fixtures.sort_unstable();
        }
    }

    pub(super) fn paint_circuit_pick(&mut self, picked: Option<ObjectId>) -> bool {
        if !self.lighting.painting {
            return false;
        }
        if let Some(ObjectId::Light(index)) = picked {
            self.toggle_circuit_fixture(index);
            self.status = format!(
                "{} fixtures selected; Circuits to choose switch / Apply",
                self.lighting.fixtures.len()
            );
        }
        true
    }
}

pub(super) fn toolbar(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    if app.popup.is_none() && app.document.is_some() {
        if matches!(
            app.selected,
            Some(ObjectId::Room(_) | ObjectId::FloorRegion(_))
        ) {
            ui::button(
                v,
                hits,
                [312.0, 214.0, 154.0, 10.0],
                "Preview room / region",
                Action::Lighting(Command::Local),
                !app.viewport.is_preparing(),
            );
        }
        ui::button(
            v,
            hits,
            [220.0, 214.0, 88.0, 10.0],
            "Light circuits >",
            Action::Lighting(Command::Open),
            true,
        );
        if app.lighting.preview_off {
            ui::button(
                v,
                hits,
                [312.0, 214.0, 100.0, 10.0],
                "Restore circuit on",
                Action::Lighting(Command::Preview),
                true,
            );
            ui::text(
                v,
                "EDITOR CIRCUIT OFF / NOT A BAKE",
                220.0,
                200.0,
                ui::INK,
                60,
            );
        }
        if app.lighting.painting {
            ui::text(
                v,
                "PAINT FIXTURES / Circuits to apply",
                220.0,
                188.0,
                ui::INK,
                60,
            );
        }
    }
}

pub(super) fn draw(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    ui::rect(v, [112.0, 40.0, 364.0, 178.0], [0.07, 0.10, 0.13, 1.0]);
    ui::text(v, "LIGHT CIRCUITS", 120.0, 47.0, ui::INK, 60);
    ui::text(
        v,
        "Select fixtures, choose a fixed prop switch, Apply.",
        120.0,
        58.0,
        ui::MUTED,
        80,
    );
    let Some(doc) = &app.document else {
        return;
    };
    draw_members(app, doc, v, hits);
    let page_count = doc
        .level()
        .props
        .len()
        .max(doc.level().ceiling_lights.len())
        .div_ceil(6)
        .max(1);
    for (x, label, cmd, enabled) in [
        (
            120.0,
            "Previous",
            Command::Page(false),
            app.lighting.page > 0,
        ),
        (
            185.0,
            "Next",
            Command::Page(true),
            app.lighting.page.saturating_add(1) < page_count,
        ),
        (250.0, "Paint", Command::Paint, true),
        (315.0, "New switch", Command::NewSwitch, true),
    ] {
        ui::button(
            v,
            hits,
            [x, 156.0, 62.0, 12.0],
            label,
            Action::Lighting(cmd),
            enabled,
        );
    }
    ui::text(
        v,
        &format!(
            "{} fixtures selected / Full lighting required",
            app.lighting.fixtures.len()
        ),
        120.0,
        174.0,
        ui::MUTED,
        78,
    );
    for (x, label, cmd) in [
        (120.0, "Assign switch", Command::Apply),
        (211.0, "Unlink", Command::Unlink),
        (
            285.0,
            if app.lighting.preview_off {
                "Preview on"
            } else {
                "Preview off"
            },
            Command::Preview,
        ),
    ] {
        ui::button(
            v,
            hits,
            [x, 188.0, if x < 200.0 { 86.0 } else { 70.0 }, 13.0],
            label,
            Action::Lighting(cmd),
            !app.lighting.fixtures.is_empty(),
        );
    }
    ui::button(
        v,
        hits,
        [391.0, 188.0, 73.0, 13.0],
        "Close",
        Action::ClosePopup,
        true,
    );
}

fn local_scope(
    level: &crate::engine::level::LevelDef,
    selected: Option<ObjectId>,
) -> Result<[f32; 4], String> {
    match selected {
        Some(ObjectId::Room(index)) => level
            .rooms
            .get(index)
            .map(|r| [r.x, r.z, r.x + r.width, r.z + r.depth])
            .ok_or_else(|| "Missing room".into()),
        Some(ObjectId::FloorRegion(index)) => level
            .floor_regions
            .get(index)
            .map(|r| [r.x, r.z, r.x + r.width, r.z + r.depth])
            .ok_or_else(|| "Missing region".into()),
        _ => Err("Select a room or floor region for local lighting preview".into()),
    }
}

fn draw_members(app: &App, doc: &super::Document, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    let ids = doc.level().light_instance_ids();
    for (row, (index, light)) in doc
        .level()
        .ceiling_lights
        .iter()
        .enumerate()
        .skip(app.lighting.page.saturating_mul(6))
        .take(6)
        .enumerate()
    {
        let name = ids.get(index).map_or("fixture", String::as_str);
        let label = format!(
            "{} {}{}",
            if app.lighting.fixtures.contains(&index) {
                "[x]"
            } else {
                "[ ]"
            },
            name,
            if light.switch_group.is_some() {
                " / linked"
            } else {
                ""
            }
        );
        let y = f32::from(u16::try_from(row).unwrap_or(0)).mul_add(13.0, 74.0);
        ui::button(
            v,
            hits,
            [120.0, y, 176.0, 11.0],
            &label,
            Action::Lighting(Command::Fixture(index)),
            true,
        );
    }
    for (row, (index, prop)) in doc
        .level()
        .props
        .iter()
        .enumerate()
        .skip(app.lighting.page.saturating_mul(6))
        .take(6)
        .enumerate()
    {
        let label = format!(
            "{} {}",
            if app.lighting.switch == Some(index) {
                ">"
            } else {
                " "
            },
            prop.id.as_deref().unwrap_or(&prop.model)
        );
        let y = f32::from(u16::try_from(row).unwrap_or(0)).mul_add(13.0, 74.0);
        ui::button(
            v,
            hits,
            [302.0, y, 164.0, 11.0],
            &label,
            Action::Lighting(Command::Switch(index)),
            prop.physics.is_none() && prop.float.is_none() && prop.animated_door.is_none(),
        );
    }
}

pub(super) fn feedback(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    if app.compile.is_none()
        && let Some(progress) = app.viewport.local_progress()
    {
        ui::rect(v, [0.0, 242.0, 480.0, 30.0], [0.07, 0.10, 0.13, 1.0]);
        ui::text(
            v,
            "LOCAL PREVIEW / NON-FINAL / outside bounce approximate",
            5.0,
            244.0,
            ui::INK,
            100,
        );
        ui::text(v, &progress, 5.0, 257.0, ui::MUTED, 86);
        ui::button(
            v,
            hits,
            [409.0, 257.0, 65.0, 9.0],
            "Cancel / clear",
            Action::Lighting(Command::CancelLocal),
            true,
        );
    }
}
