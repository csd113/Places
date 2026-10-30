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
    Sign(super::signs::Command),
    Place(super::construction::Tool),
    ChainWalls,
    CancelPlacement,
    ApplyPlacement,
    Layers,
    Layer(usize),
    Isolate,
    ShowAll,
    Lock,
    ThumbnailSize,
    Flicker(super::flicker::Command),
    Lighting(super::lighting::Command),
    Duct(super::ducts::Command),
    Reuse(super::reuse::Command),
    Author(super::authoring::Command),
    Project,
    ProjectField(super::projects::Field),
    ProjectApply,
    ProjectEntry,
    ProjectMenuEntryOnly,
    MapCrosshair,
    MapSaving,
    AddCheckpoint,
    TriggerCheckpoint,
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
    PlayOptions,
    PlayCamera,
    PlayFloor,
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
    ChooseFix,
    PreviewGroup,
    CancelSafety,
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
    if app.placement.is_none()
        && !app.view.simple
        && matches!(app.selected, Some(ObjectId::Wall(_)))
    {
        button(
            &mut v,
            &mut hits,
            [120.0, 43.0, 90.0, 13.0],
            "Make Doorway",
            Action::Place(super::construction::Tool::Doorway),
            true,
        );
    }
    super::construction::draw(app, &mut v, &mut hits);
    if app.popup.is_none() && matches!(app.selected, Some(ObjectId::Prop(_))) {
        button(
            &mut v,
            &mut hits,
            [220.0, 228.0, 88.0, 10.0],
            "NPC behaviors >",
            Action::Behavior(super::behavior::Command::Open),
            matches!(app.selected, Some(ObjectId::Prop(_))),
        );
    }
    if app.popup.is_none() && matches!(app.selected, Some(ObjectId::Light(_))) {
        button(
            &mut v,
            &mut hits,
            [312.0, 228.0, 110.0, 10.0],
            "Light flicker >",
            Action::Flicker(super::flicker::Command::Open),
            matches!(app.selected, Some(ObjectId::Light(_))),
        );
    }
    if app.popup.is_none() && app.placement.is_none() {
        button(
            &mut v,
            &mut hits,
            [120.0, 59.0, 102.0, 13.0],
            "Reuse tools >",
            Action::Reuse(super::reuse::Command::Open),
            app.document.is_some(),
        );
    }
    super::lighting::toolbar(app, &mut v, &mut hits);
    let modal_start = v.len();
    if let Some(popup) = &app.popup {
        popup.draw(app, &mut v, &mut hits);
    }
    super::lighting::feedback(app, &mut v, &mut hits);
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
    fit_layout(app, v, hits, modal_start)
}
fn fit_layout(
    app: &App,
    mut v: Vec<Vertex>,
    mut hits: Vec<Hit>,
    modal_start: usize,
) -> (Vec<Vertex>, Vec<Hit>) {
    let layout = app.viewport.layout();
    for (index, vertex) in v.iter_mut().enumerate() {
        let [x, y] = layout.point(
            [vertex.pos[0], vertex.pos[1]],
            app.popup.is_some() && index >= modal_start,
        );
        vertex.pos[0] = x;
        vertex.pos[1] = y;
    }
    for hit in &mut hits {
        hit.rect = layout.rect(hit.rect, app.popup.is_some());
    }
    let [left, top, right, bottom] = layout.bounds;
    let [view_left, view_top, _, view_bottom] = layout.viewport();
    let mut background = Vec::new();
    for bounds in [
        [left, top, right - left, view_top - top],
        [left, view_top, view_left - left, view_bottom - view_top],
        [left, view_bottom, right - left, bottom - view_bottom],
    ] {
        rect(&mut background, bounds, PANEL);
    }
    background.extend(v);
    (background, hits)
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
            47.0,
            "Save+Build",
            Action::Build,
            idle && app.document.is_some(),
        ),
        (
            201.0,
            25.0,
            "Play",
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
        if !app.view.simple || !matches!(action, Action::Build) {
            button(vertices, hits, [x, 14.0, w, 11.0], label, action, enabled);
        }
    }

    button(
        vertices,
        hits,
        [229.0, 14.0, 25.0, 11.0],
        "Here",
        Action::PlayOptions,
        idle && app.document.is_some(),
    );
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
    if !app.view.simple && app.compile.is_none() && app.play.is_none() {
        button(
            vertices,
            hits,
            [257.0, 14.0, 34.0, 11.0],
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
    if !app.view.simple {
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
    }
    if !app.view.simple {
        button(
            vertices,
            hits,
            [174.0, 27.0, 49.0, 10.0],
            "Layers >",
            Action::Layers,
            app.document.is_some(),
        );
    }
    text(
        vertices,
        "RMB orbit | MMB pan | Wheel zoom | F frame",
        if app.view.simple { 114.0 } else { 227.0 },
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
    for (x, y, w, label, action) in [
        (5.0, 52.0, 49.0, "New Room", Action::Add(ObjectKind::Room)),
        (
            58.0,
            52.0,
            49.0,
            "Add Wall",
            Action::Place(super::construction::Tool::Wall),
        ),
        (
            5.0,
            66.0,
            49.0,
            "Add Door",
            Action::Place(super::construction::Tool::Door),
        ),
        (
            58.0,
            66.0,
            49.0,
            "Add Light",
            Action::Place(super::construction::Tool::Light),
        ),
    ] {
        button(
            vertices,
            hits,
            [x, y, w, 12.0],
            label,
            action,
            app.document.is_some(),
        );
    }
    let objects = app.scene_objects();
    for (index, object) in objects.iter().skip(app.object_scroll).take(3).enumerate() {
        let y = f32::from(u16::try_from(index).unwrap_or(0)).mul_add(10.0, 82.0);
        let label = format!(
            "{}{}",
            if !app.editable(object.id) {
                "[L] "
            } else if app.selected == Some(object.id) {
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
        115.0,
        MUTED,
        29,
    );
}

fn simple_selection(app: &App, vertices: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    let selected = app.selected.is_some();
    let unlocked = app.selected.is_some_and(|id| app.editable(id));
    let duct = matches!(app.selected, Some(ObjectId::Prop(index))
        if app.document.as_ref().and_then(|doc| doc.level().props.get(index))
            .is_some_and(|placed| super::ducts::supported(&placed.model)));
    let wall = matches!(app.selected, Some(ObjectId::Wall(_)));
    let sign = matches!(app.selected, Some(ObjectId::Decal(_)));
    text(
        vertices,
        if selected && !unlocked {
            "LOCKED: Layers to unlock"
        } else if wall {
            "WALL: drag body / ends"
        } else if matches!(app.selected, Some(ObjectId::Room(_))) {
            "ROOM: drag gold edges"
        } else {
            "SELECT / BUILD"
        },
        5.0,
        130.0,
        ACCENT,
        26,
    );
    for (y, label, action, enabled) in [
        (
            141.0,
            if wall {
                "Doorway"
            } else if sign {
                "Wall sign >"
            } else if duct {
                "Duct joins >"
            } else {
                "More geometry >"
            },
            if wall {
                Action::Place(super::construction::Tool::Doorway)
            } else if sign {
                Action::Sign(super::signs::Command::Open)
            } else if duct {
                Action::Duct(super::ducts::Command::Open)
            } else {
                Action::Geometry
            },
            true,
        ),
        (158.0, "Move / exact size >", Action::Transform, selected),
        (175.0, "Add props / signs >", Action::Create, true),
        (192.0, "Layers / focus >", Action::Layers, true),
    ] {
        button(
            vertices,
            hits,
            [5.0, y, if wall && y < 150.0 { 49.0 } else { 102.0 }, 14.0],
            label,
            action,
            enabled && app.document.is_some(),
        );
    }
    if wall {
        button(
            vertices,
            hits,
            [58.0, 141.0, 49.0, 14.0],
            "Build >",
            Action::Geometry,
            app.document.is_some(),
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
        match app.selected {
            Some(ObjectId::Wall(_)) => "Auto trim >",
            Some(ObjectId::Column(_) | ObjectId::Pillar(_)) => "Pillar >",
            Some(ObjectId::ArcWall(_)) => "Curve trim >",
            _ => "Configure >",
        },
        match app.selected {
            Some(ObjectId::Column(_)) => Action::Author(super::authoring::Command::Open(
                super::authoring::Tool::Column,
            )),
            Some(ObjectId::Pillar(_)) => Action::Author(super::authoring::Command::Open(
                super::authoring::Tool::Pillar,
            )),
            Some(ObjectId::ArcWall(_)) => Action::Author(super::authoring::Command::Open(
                super::authoring::Tool::ArcWall,
            )),
            Some(ObjectId::Wall(_)) => Action::Author(super::authoring::Command::Open(
                super::authoring::Tool::WallTrim,
            )),
            _ => Action::Gameplay,
        },
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
                ObjectId::Column(_)
                    | ObjectId::Pillar(_)
                    | ObjectId::ArcWall(_)
                    | ObjectId::Room(_)
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
