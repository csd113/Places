//! Compact retained hit regions and shared-renderer font geometry.

use crate::engine::font::get_white_uv;
use crate::engine::render::Vertex;

use super::App;
use super::document::{ObjectId, ObjectKind};

mod bake_progress;
pub(super) mod components;
mod gameplay;
mod geometry;
mod inspector;
pub(super) mod library;
mod navigation;
mod safety;
mod scripts;
mod transform;
pub(super) use scripts::draw as scripts_popup;
pub(super) use transform::draw as transform_popup;

pub(super) use gameplay::{action_popup, choices_popup, create_popup, gameplay_popup};
pub(super) use geometry::{geometry_popup, shape_popup};
use inspector::inspector;
pub(super) use library::draw_library as library_popup;
pub(super) use safety::safety_popup;

pub(super) const AXIS_COLORS: [[f32; 4]; 3] = [
    [1.0, 0.25, 0.2, 1.0],
    [0.35, 1.0, 0.35, 1.0],
    [0.3, 0.55, 1.0, 1.0],
];

pub(super) const INK: [f32; 4] = [0.88, 0.92, 0.95, 1.0];
pub(super) const MUTED: [f32; 4] = [0.68, 0.75, 0.80, 1.0];
const ACCENT: [f32; 4] = [0.37, 0.86, 0.71, 1.0];
const PANEL: [f32; 4] = [0.065, 0.09, 0.12, 1.0];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Flicker(super::flicker::Command),
    Author(super::authoring::Command),
    Project,
    ProjectField(super::projects::Field),
    ProjectApply,
    ProjectEntry,
    ProjectMenuEntryOnly,
    MapCrosshair,
    Thumbnail,
    ThumbnailChoose,
    ThumbnailRemove,
    ProjectSelect(usize),
    ProjectCheck,
    ProjectMigrate,
    Scripts,
    Behavior(super::behavior::Command),
    Template(super::templates::Command),
    ScriptNew,
    ScriptOpen(usize),
    ScriptCheck,
    ScriptDiagnostics,
    ScriptLink,
    ScriptHandler(usize),
    Capture,
    Navigation,
    NavBake,
    NavAgent(bool),
    NavPick(bool),
    NavUseAgent,
    NavCoverage,
    NavSlice(bool),
    NavAllFloors,
    NavClear,
    Create,
    Gameplay,
    GameplayActions,
    Components(usize),
    ComponentField(super::document::components::Field),
    ComponentApply,
    ComponentAdd(&'static str),
    ComponentRemove(&'static str),
    StateAdd,
    StateRemove(usize),
    BindingSelect(bool),
    BindingNew,
    BindingRemove,
    ConditionChoose(usize),
    ConditionRemove(usize),
    CatalogFixedShape,
    AttachedLightAdd,
    AttachedLightRemove(usize),
    GamePick(super::gameplay_actions::GameplayChoice),
    GameChoose(super::gameplay_actions::GameplayChoice, usize),
    GameSetting(super::document::GameplaySetting),
    RemoveTriggerStep(usize),
    EnvironmentPreview,
    MountSurface,
    ToggleLightMount,
    IgnoreFinding,
    ClearFindingIgnore,
    IgnoredFindings,
    New,
    Open,
    Save,
    Undo,
    Redo,
    BakeStatus,
    BakeLevel(usize),
    BakeCancel,
    Build,
    Play,
    Cancel,
    Stop,
    Add(ObjectKind),
    StairWizard,
    StairFloor(bool),
    StairPreview,
    StairApply,
    Geometry,
    Shape,
    CutOpening,
    UpperFloor,
    Safety,
    Finding(bool),
    PreviewFix,
    CancelFix,
    ApplyFix,
    InspectFinding,
    Select(ObjectId),
    SceneSearch,
    PreviewSound(usize),
    StopPreviewSound,
    Duplicate,
    Delete,
    Focus,
    Grid,
    Mode,
    Snap,
    Step,
    Field(usize),
    Transform,
    TransformField(usize),
    TransformPreview,
    TransformApply,
    Nudge(usize, bool),
    Assets,
    BrowserTheme,
    MaterialTarget,
    DoorAssets,
    Materials,
    ChooseAsset(usize),
    Page(bool),
    ClosePopup,
    ConfirmSave,
    ConfirmDiscard,
    ConfirmCancel,
}

pub(super) struct Hit {
    rect: [f32; 4],
    pub action: Action,
}

impl Hit {
    pub const fn center(&self) -> [f32; 2] {
        [
            self.rect[0] + self.rect[2] * 0.5,
            self.rect[1] + self.rect[3] * 0.5,
        ]
    }

    pub fn contains(&self, [x, y]: [f32; 2]) -> bool {
        let [left, top, width, height] = self.rect;
        x >= left && x < left + width && y >= top && y < top + height
    }
}

fn quad(
    vertices: &mut Vec<Vertex>,
    [left, top, width, height]: [f32; 4],
    color: [f32; 4],
    uv: [f32; 4],
) {
    let [u0, v0, u1, v1] = uv;
    for (pos, tex) in [
        ([left, top, 0.0], [u0, v0]),
        ([left + width, top, 0.0], [u1, v0]),
        ([left + width, top + height, 0.0], [u1, v1]),
        ([left, top, 0.0], [u0, v0]),
        ([left + width, top + height, 0.0], [u1, v1]),
        ([left, top + height, 0.0], [u0, v1]),
    ] {
        vertices.push(Vertex {
            pos,
            color,
            uv: tex,
            ..Vertex::UNLIT
        });
    }
}

pub(super) fn rect(vertices: &mut Vec<Vertex>, bounds: [f32; 4], color: [f32; 4]) {
    quad(vertices, bounds, color, get_white_uv());
}

pub(super) fn text(
    vertices: &mut Vec<Vertex>,
    value: &str,
    x: f32,
    y: f32,
    color: [f32; 4],
    limit: usize,
) {
    let max_width = (f32::from(u16::try_from(limit).unwrap_or(u16::MAX)) * 3.5).min(475.0 - x);
    let fitted = crate::game::ui::fit_text(value, max_width, 0.5);
    let mut cursor = x;
    for character in fitted.chars() {
        let ascii = u8::try_from(character)
            .ok()
            .filter(|c| (32..=126).contains(c))
            .unwrap_or(b'?');
        if ascii != b' ' {
            let index = ascii.saturating_sub(32);
            let col = f32::from(index % 16);
            let row = f32::from(index / 16);
            let uv = [col / 16.0, row / 8.0, (col + 1.0) / 16.0, (row + 1.0) / 8.0];
            quad(vertices, [cursor, y, 4.0, 7.0], color, uv);
        }
        cursor += 4.0;
    }
}

pub(super) fn button(
    vertices: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    bounds: [f32; 4],
    label: &str,
    action: Action,
    active: bool,
) {
    rect(
        vertices,
        bounds,
        if active {
            [0.15, 0.24, 0.28, 1.0]
        } else {
            [0.09, 0.12, 0.15, 1.0]
        },
    );
    let [x, y, width, height] = bounds;
    let fitted = crate::game::ui::fit_text(label, width - 5.0, 0.5);
    text(
        vertices,
        &fitted,
        x + 2.5,
        (height - 7.0).mul_add(0.5, y),
        if active { INK } else { MUTED },
        68,
    );
    if active {
        hits.push(Hit {
            rect: bounds,
            action,
        });
    }
}

pub(super) fn build(app: &App) -> (Vec<Vertex>, Vec<Hit>) {
    let mut v = Vec::new();
    let mut hits = Vec::new();
    rect(&mut v, [0.0, 0.0, 480.0, 38.0], PANEL);
    rect(&mut v, [0.0, 38.0, 112.0, 204.0], PANEL);
    rect(&mut v, [0.0, 242.0, 480.0, 30.0], PANEL);
    toolbar(app, &mut v, &mut hits);
    button(
        &mut v,
        &mut hits,
        [5.0, 27.0, 102.0, 10.0],
        if app.view.simple {
            "Mode: Simple >"
        } else {
            "Mode: Advanced >"
        },
        Action::Mode,
        true,
    );
    navigation(app, &mut v, &mut hits);
    if app.navigation.visible {
        navigation::draw(app, &mut v, &mut hits);
    } else {
        scene(app, &mut v, &mut hits);
        if app.view.simple {
            simple_selection(app, &mut v, &mut hits);
        } else {
            inspector(app, &mut v, &mut hits);
        }
    }
    footer(app, &mut v, &mut hits);
    if app.popup.is_none() {
        button(
            &mut v,
            &mut hits,
            [220.0, 228.0, 88.0, 10.0],
            "NPC behaviors >",
            Action::Behavior(super::behavior::Command::Open),
            matches!(app.selected, Some(ObjectId::Prop(_))),
        );
    }
    if app.popup.is_none() {
        button(
            &mut v,
            &mut hits,
            [312.0, 228.0, 110.0, 10.0],
            "Light flicker >",
            Action::Flicker(super::flicker::Command::Open),
            matches!(app.selected, Some(ObjectId::Light(_))),
        );
    }
    if let Some(popup) = &app.popup {
        popup.draw(app, &mut v, &mut hits);
    }
    bake_progress::draw(app, &mut v, &mut hits);
    if let Some(hit) = hits.iter().find(|hit| Some(hit.action) == app.ui_focus) {
        let [left, top, width, height] = hit.rect;
        for bounds in [
            [left, top, width, 0.4],
            [left, top + height - 0.4, width, 0.4],
            [left, top, 0.4, height],
            [left + width - 0.4, top, 0.4, height],
        ] {
            rect(&mut v, bounds, ACCENT);
        }
    }
    (v, hits)
}

pub(super) fn popup_button(
    v: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    bounds: [f32; 4],
    label: &str,
    action: Action,
) {
    button(v, hits, bounds, label, action, true);
}

fn toolbar(app: &App, vertices: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    text(vertices, "PLACES / EDITOR", 5.0, 4.0, ACCENT, 30);
    let title = app.document.as_ref().map_or_else(
        || "Choose New or Open to begin".into(),
        |d| {
            format!(
                "{} / {}{}{}",
                d.project().manifest().name,
                d.level_id(),
                if d.is_dirty() { " * UNSAVED" } else { "" },
                if app.viewport.is_preparing() {
                    " / preparing view"
                } else {
                    ""
                }
            )
        },
    );
    text(vertices, &title, 114.0, 4.0, INK, 101);
    let idle = app.compile.is_none() && app.play.is_none() && !app.dialog_open;
    for (x, w, label, action, enabled) in [
        (5.0, 26.0, "New", Action::New, idle),
        (33.0, 26.0, "Open", Action::Open, idle),
        (
            61.0,
            26.0,
            "Save",
            Action::Save,
            app.document.is_some() && app.compile.is_none(),
        ),
        (
            91.0,
            26.0,
            "Undo",
            Action::Undo,
            app.document
                .as_ref()
                .is_some_and(super::document::Document::can_undo),
        ),
        (
            119.0,
            26.0,
            "Redo",
            Action::Redo,
            app.document
                .as_ref()
                .is_some_and(super::document::Document::can_redo),
        ),
        (
            151.0,
            51.0,
            "Save+Build",
            Action::Build,
            idle && app.document.is_some(),
        ),
        (
            205.0,
            46.0,
            "Build+Play",
            Action::Play,
            idle && app.document.is_some(),
        ),
        (
            294.0,
            47.0,
            if app.safety_job.is_some() {
                "Checking"
            } else {
                "Safety >"
            },
            Action::Safety,
            app.document.is_some(),
        ),
    ] {
        button(vertices, hits, [x, 14.0, w, 11.0], label, action, enabled);
    }

    if app.compile.is_some() || app.play.is_some() {
        let playing = app.play.is_some();
        button(
            vertices,
            hits,
            [254.0, 14.0, 37.0, 11.0],
            if playing { "Stop Play" } else { "Cancel" },
            if playing {
                Action::Stop
            } else {
                Action::Cancel
            },
            true,
        );
    }
}

fn navigation(app: &App, vertices: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    if app.compile.is_none() && app.play.is_none() {
        button(
            vertices,
            hits,
            [254.0, 14.0, 37.0, 11.0],
            "Scripts",
            Action::Scripts,
            app.document.is_some(),
        );
    }

    button(
        vertices,
        hits,
        [347.0, 14.0, 34.0, 11.0],
        if app.view.grid { "Grid On" } else { "Grid Off" },
        Action::Grid,
        true,
    );
    button(
        vertices,
        hits,
        [384.0, 14.0, 34.0, 11.0],
        if app.view.snap { "Snap On" } else { "Snap Off" },
        Action::Snap,
        true,
    );
    button(
        vertices,
        hits,
        [421.0, 14.0, 53.0, 11.0],
        &format!("Step {:.2}m", app.step),
        Action::Step,
        true,
    );
    button(
        vertices,
        hits,
        [114.0, 27.0, 55.0, 10.0],
        if app.navigation.visible {
            "Navigation <"
        } else {
            "Navigation >"
        },
        Action::Navigation,
        app.document.is_some(),
    );
    text(
        vertices,
        "RMB orbit / WASDQE fly | MMB pan | Wheel zoom | F frame",
        174.0,
        29.0,
        MUTED,
        86,
    );
}

fn scene(app: &App, vertices: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    button(
        vertices,
        hits,
        [5.0, 39.0, 55.0, 10.0],
        "Find >",
        Action::SceneSearch,
        app.document.is_some(),
    );
    button(
        vertices,
        hits,
        [65.0, 39.0, 42.0, 10.0],
        "Project >",
        Action::Project,
        app.compile.is_none() && app.play.is_none() && !app.dialog_open,
    );
    for (x, w, label, action) in [
        (
            5.0,
            65.0,
            if app.view.simple {
                "New Room"
            } else {
                "+ Geometry"
            },
            if app.view.simple {
                Action::Add(ObjectKind::Room)
            } else {
                Action::Geometry
            },
        ),
        (73.0, 34.0, "+ Prop", Action::Add(ObjectKind::Prop)),
    ] {
        button(
            vertices,
            hits,
            [x, 52.0, w, 11.0],
            label,
            action,
            app.document.is_some(),
        );
    }
    button(
        vertices,
        hits,
        [5.0, 65.0, 102.0, 10.0],
        "+ Scene objects >",
        Action::Create,
        app.document.is_some(),
    );
    let objects = app.scene_objects();
    for (index, object) in objects.iter().skip(app.object_scroll).take(4).enumerate() {
        let y = f32::from(u16::try_from(index).unwrap_or(0)).mul_add(10.0, 79.0);
        let label = format!(
            "{}{}",
            if app.selected == Some(object.id) {
                "> "
            } else {
                "  "
            },
            object.label
        );
        button(
            vertices,
            hits,
            [5.0, y, 102.0, 9.0],
            &label,
            Action::Select(object.id),
            true,
        );
    }
    text(
        vertices,
        &format!(
            "{} {} / scroll",
            objects.len(),
            if app.object_query.is_empty() {
                "objects"
            } else {
                "matches"
            }
        ),
        5.0,
        121.0,
        MUTED,
        29,
    );
}

fn simple_selection(app: &App, vertices: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    let selected = app.selected.is_some();
    text(
        vertices,
        if selected {
            "SELECTED OBJECT"
        } else {
            "BUILD YOUR PLACE"
        },
        5.0,
        132.0,
        ACCENT,
        27,
    );
    for (y, label, action, enabled) in [
        (144.0, "Move / exact size >", Action::Transform, selected),
        (160.0, "Choose model >", Action::Assets, true),
        (176.0, "Rooms / stairs >", Action::Geometry, true),
        (192.0, "Events / scripts >", Action::Scripts, true),
    ] {
        button(
            vertices,
            hits,
            [5.0, y, 102.0, 13.0],
            label,
            action,
            enabled && app.document.is_some(),
        );
    }
}

fn footer(app: &App, vertices: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    if !app.navigation.visible {
        footer_actions(app, vertices, hits);
    }
    text(vertices, &app.status, 5.0, 247.0, INK, 133);
    text(
        vertices,
        "Cmd/Ctrl S save | Z undo | Shift Z redo | D duplicate | Delete | arrows move | R rotate | +/- scale",
        5.0,
        258.0,
        MUTED,
        135,
    );
}

fn footer_actions(app: &App, vertices: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    button(
        vertices,
        hits,
        [5.0, 214.0, 32.0, 10.0],
        "Copy",
        Action::Duplicate,
        app.selected.is_some(),
    );
    button(
        vertices,
        hits,
        [40.0, 214.0, 32.0, 10.0],
        "Delete",
        Action::Delete,
        app.selected.is_some(),
    );
    button(
        vertices,
        hits,
        [75.0, 214.0, 32.0, 10.0],
        "Frame",
        Action::Focus,
        true,
    );
    button(
        vertices,
        hits,
        [5.0, 228.0, 49.0, 10.0],
        "Configure >",
        Action::Gameplay,
        app.selected.is_some(),
    );
    button(
        vertices,
        hits,
        [58.0, 228.0, 49.0, 10.0],
        "Material",
        Action::Materials,
        app.selected.is_some_and(|id| {
            matches!(
                id,
                ObjectId::Room(_)
                    | ObjectId::Wall(_)
                    | ObjectId::Ramp(_)
                    | ObjectId::FloorRegion(_)
                    | ObjectId::Door(_)
            )
        }),
    );
    button(
        vertices,
        hits,
        [114.0, 228.0, 100.0, 10.0],
        if app.viewport.environment_preview() {
            "Preview: Lighting"
        } else {
            "Preview: Inspect"
        },
        Action::EnvironmentPreview,
        true,
    );
}
