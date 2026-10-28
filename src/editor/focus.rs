//! Session visibility and persistent per-object locks share selection identities.
use super::{
    App,
    actions::Popup,
    document::{ObjectId, objects},
    ui::{self, Action, Hit},
};
use crate::engine::{level::LevelDef, render::Vertex};

#[derive(Clone, Default)]
pub(super) struct Focus {
    pub hidden: [bool; 4],
    pub isolated: Option<ObjectId>,
}
pub(super) const LAYERS: [&str; 4] = [
    "Architecture",
    "Props / actors",
    "Lights",
    "Gameplay / atmosphere",
];
pub(super) const fn layer(id: ObjectId) -> usize {
    match id {
        ObjectId::Column(_)
        | ObjectId::Pillar(_)
        | ObjectId::ArcWall(_)
        | ObjectId::Room(_)
        | ObjectId::Wall(_)
        | ObjectId::Ramp(_)
        | ObjectId::FloorRegion(_)
        | ObjectId::Door(_) => 0,
        ObjectId::Prop(_) => 1,
        ObjectId::Light(_) => 2,
        ObjectId::Audio(_)
        | ObjectId::Trigger(_)
        | ObjectId::FogRegion(_)
        | ObjectId::GroundFog(_)
        | ObjectId::SpawnPoint(_) => 3,
    }
}
impl Focus {
    pub fn visible(&self, id: ObjectId) -> bool {
        !self.hidden.get(layer(id)).copied().unwrap_or(true)
    }
    pub fn included(&self, level: &LevelDef, id: ObjectId) -> bool {
        if !self.visible(id) {
            return false;
        }
        let Some(isolated) = self.isolated else {
            return true;
        };
        if id == isolated {
            return true;
        }
        // Keep floor/ceiling carriers for context and correct attachment heights.
        if matches!(id, ObjectId::Room(_)) {
            return !matches!(isolated, ObjectId::Room(_));
        }
        let Ok(pose) = objects::transform(level, id) else {
            return false;
        };
        if let ObjectId::Room(i) = isolated
            && let Some(room) = level.rooms.get(i)
        {
            let [x, y, z] = pose.position;
            return x >= room.x - 0.2
                && x <= room.x + room.width + 0.2
                && z >= room.z - 0.2
                && z <= room.z + room.depth + 0.2
                && y >= room.floor_y - 0.1
                && y < room.eave_y();
        }
        if let ObjectId::Wall(i) = isolated
            && let Some(w) = level.walls.get(i)
            && matches!(id, ObjectId::Door(_) | ObjectId::Light(_))
        {
            return pose.position[0] >= w.x - 0.2
                && pose.position[0] <= w.x + w.width + 0.2
                && pose.position[2] >= w.z - 0.2
                && pose.position[2] <= w.z + w.depth + 0.2;
        }
        false
    }
    pub fn preview(&self, level: &LevelDef) -> (LevelDef, Vec<(ObjectId, ObjectId)>) {
        let mut filtered = level.clone();
        let mut map = Vec::new();
        macro_rules! retain {
            ($field:ident,$kind:ident) => {{
                let mut original = 0_usize;
                let mut kept = 0_usize;
                filtered.$field.retain(|_| {
                    let old = ObjectId::$kind(original);
                    original = original.saturating_add(1);
                    if self.included(level, old) {
                        map.push((ObjectId::$kind(kept), old));
                        kept = kept.saturating_add(1);
                        true
                    } else {
                        false
                    }
                });
            }};
        }
        retain!(rooms, Room);
        retain!(walls, Wall);
        retain!(columns, Column);
        retain!(pillars, Pillar);
        retain!(arc_walls, ArcWall);
        retain!(ramps, Ramp);
        retain!(floor_regions, FloorRegion);
        retain!(props, Prop);
        retain!(doors, Door);
        retain!(ceiling_lights, Light);
        retain!(sound_emitters, Audio);
        retain!(volumes, Trigger);
        retain!(fog_regions, FogRegion);
        retain!(ground_fog, GroundFog);
        retain!(spawn_points, SpawnPoint);
        if self.hidden[0] || self.isolated.is_some() {
            filtered.guardrails.clear();
            filtered.half_walls.clear();
            filtered.archways.clear();
            filtered.thresholds.clear();
            filtered.baseboards.clear();
            filtered.stairs.clear();
        }
        if self.hidden[0] {
            filtered.decals.clear();
            filtered.floor_patches.clear();
        }
        if let Some(isolated) = self.isolated {
            filtered
                .decals
                .retain(|d| near_selection(level, isolated, [d.x, d.y, d.z]));
        }
        (filtered, map)
    }
}
impl App {
    pub(super) fn editable(&self, id: ObjectId) -> bool {
        self.focus.visible(id)
            && !self
                .document
                .as_ref()
                .and_then(|d| {
                    let (key, i) = id.collection();
                    d.raw().get(key)?.get(i)?.get("editor_locked")?.as_bool()
                })
                .unwrap_or(false)
    }
    pub(super) fn check_focused_edit(&self, candidate: &serde_json::Value) -> Result<(), String> {
        let Some(doc) = &self.document else {
            return Ok(());
        };
        doc.check_locked_edit(candidate)?;
        for object in doc.objects() {
            if self.editable(object.id) && self.focus.included(doc.level(), object.id) {
                continue;
            }
            let (key, i) = object.id.collection();
            if let Some(old) = doc.raw().get(key).and_then(|a| a.get(i))
                && !candidate
                    .get(key)
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|a| {
                        let before = doc
                            .raw()
                            .get(key)
                            .and_then(serde_json::Value::as_array)
                            .map_or(0, |records| records.iter().filter(|r| *r == old).count());
                        a.iter().filter(|r| *r == old).count() >= before
                    })
            {
                return Err(format!(
                    "{} is locked, hidden or outside isolation. Show and unlock it before changing its attachments.",
                    object.label
                ));
            }
        }
        Ok(())
    }
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Only layer and isolation commands belong to this handler."
    )]
    pub(super) fn focus_action(&mut self, action: Action) -> Result<bool, String> {
        match action {
            Action::Layers => self.popup = Some(Popup::Layers),
            Action::Layer(i) => {
                let hidden = self.focus.hidden.get_mut(i).ok_or("Unknown layer")?;
                *hidden = !*hidden;
                self.object_scroll = 0;
                if self.selected.is_some_and(|id| !self.focus.visible(id)) {
                    self.selected = None;
                }
                self.changed();
            }
            Action::Isolate => {
                self.focus.isolated = if self.focus.isolated.is_some() {
                    None
                } else {
                    Some(self.selected.ok_or("Select an object to isolate")?)
                };
                self.changed();
                self.status=if self.focus.isolated.is_some(){"Isolation on. Selected room includes its contents; walls include nearby doors/lights. Show all in Layers exits."}else{"All visible layers restored."}.into();
            }
            Action::ShowAll => {
                self.focus = Focus::default();
                self.changed();
                self.status = "All layers shown; isolation off. Saved object locks remain.".into();
            }
            Action::Lock => {
                let id = self.selected.ok_or("Select an object first")?;
                let doc = self.document.as_mut().ok_or("Open a project first")?;
                let mut raw = doc.raw().clone();
                let r = objects::object_mut(&mut raw, id)?;
                let locked = r
                    .get("editor_locked")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                drop(r.insert("editor_locked".into(), serde_json::Value::Bool(!locked)));
                let expected = doc.raw().clone();
                doc.apply_authoring_edit(&expected, raw)?;
                self.changed();
                self.status = if locked {
                    "Object unlocked."
                } else {
                    "Object locked. It can be selected and framed; unlock it here to edit."
                }
                .into();
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}
pub(super) fn draw(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    ui::rect(v, [130.0, 55.0, 260.0, 187.0], [0.075, 0.11, 0.14, 1.0]);
    ui::text(v, "FOCUS / LAYERS", 139.0, 64.0, ui::INK, 60);
    for (i, name) in LAYERS.into_iter().enumerate() {
        let y = f32::from(u16::try_from(i).unwrap_or(0)).mul_add(20.0, 83.0);
        let hidden = app.focus.hidden.get(i).copied().unwrap_or(false);
        ui::popup_button(
            v,
            hits,
            [139.0, y, 230.0, 16.0],
            &format!("{}  {name}", if hidden { "[hidden]" } else { "[shown]" }),
            Action::Layer(i),
        );
    }
    for (y, label, action, active) in [
        (
            166.0,
            if app.focus.isolated.is_some() {
                "Exit isolation"
            } else {
                "Isolate selection"
            },
            Action::Isolate,
            app.selected.is_some() || app.focus.isolated.is_some(),
        ),
        (
            185.0,
            if app.selected.is_some_and(|id| !app.editable(id)) {
                "Unlock selected object"
            } else {
                "Lock selected object"
            },
            Action::Lock,
            app.selected.is_some(),
        ),
        (
            204.0,
            "Show all layers / exit isolation",
            Action::ShowAll,
            true,
        ),
        (223.0, "Close", Action::ClosePopup, true),
    ] {
        ui::button(v, hits, [139.0, y, 230.0, 16.0], label, action, active);
    }
}

fn near_selection(level: &LevelDef, id: ObjectId, [x, y, z]: [f32; 3]) -> bool {
    match id {
        ObjectId::Room(i) => level.rooms.get(i).is_some_and(|r| {
            x >= r.x
                && x <= r.x + r.width
                && z >= r.z
                && z <= r.z + r.depth
                && y >= r.floor_y
                && y <= r.eave_y()
        }),
        ObjectId::Wall(i) => level.walls.get(i).is_some_and(|w| {
            x >= w.x - 0.2
                && x <= w.x + w.width + 0.2
                && z >= w.z - 0.2
                && z <= w.z + w.depth + 0.2
                && y >= w.y
                && y <= w.y + w.height.unwrap_or(3.0)
        }),
        ObjectId::Column(_)
        | ObjectId::Pillar(_)
        | ObjectId::ArcWall(_)
        | ObjectId::Prop(_)
        | ObjectId::Ramp(_)
        | ObjectId::FloorRegion(_)
        | ObjectId::Light(_)
        | ObjectId::Audio(_)
        | ObjectId::Door(_)
        | ObjectId::Trigger(_)
        | ObjectId::FogRegion(_)
        | ObjectId::GroundFog(_)
        | ObjectId::SpawnPoint(_) => false,
    }
}
