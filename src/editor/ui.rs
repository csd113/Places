//! Compact retained hit regions and shared-renderer font geometry.

use crate::engine::font::get_white_uv;
use crate::engine::render::Vertex;

use super::App;
use super::document::{ObjectId, ObjectKind};

mod bake_progress;
pub(super) mod components;
pub(super) mod dock;
mod gameplay;
mod geometry;
mod inspector;
pub(super) mod library;
mod navigation;
mod runtime;
mod safety;
mod scripts;
mod transform;
mod workspace;
pub(super) use scripts::draw as scripts_popup;
pub(super) use transform::draw as transform_popup;
pub(super) use workspace::draw as workspace_popup;

pub(super) use gameplay::{action_popup, choices_popup, create_popup, gameplay_popup};
pub(super) use geometry::{geometry_popup, shape_popup};
pub(super) use library::draw_library as library_popup;
pub(super) use safety::safety_popup;

pub(super) const AXIS_COLORS: [[f32; 4]; 3] = [
    [1.0, 0.25, 0.2, 1.0],
    [0.35, 1.0, 0.35, 1.0],
    [0.3, 0.55, 1.0, 1.0],
];

pub(super) const INK: [f32; 4] = [0.94, 0.97, 1.0, 1.0];
pub(super) const MUTED: [f32; 4] = [0.68, 0.77, 0.85, 1.0];
const ACCENT: [f32; 4] = [0.48, 0.89, 0.83, 1.0];
const PANEL: [f32; 4] = [0.14, 0.17, 0.21, 1.0];
const SURFACE: [f32; 4] = [0.20, 0.24, 0.29, 1.0];
const LINE: [f32; 4] = [0.235, 0.30, 0.37, 1.0];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Workspace,
    Help,
    DockTab(dock::Tab),
    DockCatalog(dock::Catalog),
    DockChoose(usize),
    BrowserSearch,
    Advanced,
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
    Elevator(super::elevators::Command),
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
    Cinematic(super::cinematics::Command),
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
    SaveGameModePreset,
    ApplyGameModePreset(usize),
    NewGameMode,
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
    ScenePage(bool),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ControlId {
    pub action: Action,
    origin: [u32; 2],
}

pub(super) struct Hit {
    pub id: ControlId,
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

/// Rounded slate controls share a visible baseline; the primary test is emphasized.
/// The entire supplied rectangle remains a keyboard/pointer target.
pub(super) fn button(
    vertices: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    bounds: [f32; 4],
    label: &str,
    action: Action,
    active: bool,
) {
    let [x, y, width, height] = bounds;
    let primary = active && matches!(action, Action::Play | Action::Stop);
    if primary {
        rounded_rect(vertices, bounds, [0.20, 0.43, 0.43, 1.0]);
    } else {
        rounded_rect(vertices, bounds, if active { SURFACE } else { PANEL });
        rect(
            vertices,
            [x + 2.0, y + height - 0.65, (width - 4.0).max(0.0), 0.4],
            LINE,
        );
    }
    let fitted = crate::game::ui::fit_text(label, width - 6.0, 0.5);
    text(
        vertices,
        &fitted,
        x + 3.0,
        (height - 7.0).mul_add(0.5, y),
        if active { INK } else { MUTED },
        68,
    );
    if active {
        hits.push(Hit {
            id: ControlId {
                action,
                origin: [x.to_bits(), y.to_bits()],
            },
            rect: bounds,
            action,
        });
    }
}

fn rounded_rect(
    vertices: &mut Vec<Vertex>,
    [origin_x, origin_y, width, height]: [f32; 4],
    color: [f32; 4],
) {
    // A small chamfer keeps the shared quad renderer and avoids a heavy box edge.
    let edge = 1.5_f32.min(height * 0.25).min(width * 0.25);
    rect(
        vertices,
        [origin_x + edge, origin_y, edge.mul_add(-2.0, width), height],
        color,
    );
    rect(
        vertices,
        [origin_x, origin_y + edge, width, edge.mul_add(-2.0, height)],
        color,
    );
    for (left, top, offset_x, offset_y) in [
        (origin_x + edge, origin_y + edge, -edge, -edge),
        (origin_x + width - edge, origin_y + edge, edge, -edge),
        (origin_x + edge, origin_y + height - edge, -edge, edge),
        (
            origin_x + width - edge,
            origin_y + height - edge,
            edge,
            edge,
        ),
    ] {
        let [white_u, white_v, _, _] = get_white_uv();
        for pos in [
            [left, top, 0.0],
            [left + offset_x, top, 0.0],
            [left, top + offset_y, 0.0],
        ] {
            vertices.push(Vertex {
                pos,
                color,
                uv: [white_u, white_v],
                ..Vertex::UNLIT
            });
        }
    }
}

pub(super) fn build(app: &App) -> (Vec<Vertex>, Vec<Hit>) {
    let mut v = Vec::new();
    let mut hits = Vec::new();
    if app.runtime_active() || app.runtime_preparing() {
        let modal_start = runtime::draw(app, &mut v, &mut hits);
        return fit_layout(app, v, hits, modal_start, (usize::MAX, usize::MAX));
    }
    dock::shell(app, &mut v, &mut hits);
    super::construction::draw(app, &mut v, &mut hits);
    // Circuit controls remain in the workspace until a temporary preview or
    // painting operation needs an immediately accessible return/apply control.
    if app.lighting.painting || app.lighting.preview_off {
        super::lighting::toolbar(app, &mut v, &mut hits);
    }
    let modal_start = v.len();
    if let Some(popup) = &app.popup {
        popup.draw(app, &mut v, &mut hits);
    }
    let overlay = (v.len(), hits.len());
    super::lighting::feedback(app, &mut v, &mut hits);
    bake_progress::draw(app, &mut v, &mut hits);
    dock::anchor_bottom(
        app,
        v.get_mut(overlay.0..).unwrap_or_default(),
        hits.get_mut(overlay.1..).unwrap_or_default(),
    );
    fit_layout(app, v, hits, modal_start, overlay)
}

fn fit_layout(
    app: &App,
    mut v: Vec<Vertex>,
    mut hits: Vec<Hit>,
    modal_start: usize,
    overlay: (usize, usize),
) -> (Vec<Vertex>, Vec<Hit>) {
    let layout = app.viewport.layout();
    let modal = app.popup.is_some() || app.runtime_active();
    for (index, vertex) in v.iter_mut().enumerate() {
        let [x, y] = layout.point(
            [vertex.pos[0], vertex.pos[1]],
            modal && index >= modal_start && index < overlay.0,
        );
        vertex.pos[0] = x;
        vertex.pos[1] = y;
    }
    for (index, hit) in hits.iter_mut().enumerate() {
        hit.rect = layout.rect(hit.rect, app.popup.is_some() && index < overlay.1);
    }
    let [left, top, right, bottom] = layout.bounds;
    let [view_left, view_top, view_right, view_bottom] = layout.viewport();
    if app.compile.is_some() || app.viewport.local_progress().is_some() {
        let feedback_top = 30.0_f32.mul_add(-layout.scale, bottom);
        hits = hits
            .into_iter()
            .enumerate()
            .filter_map(|(index, hit)| {
                (index >= overlay.1 || hit.rect[1] + hit.rect[3] <= feedback_top).then_some(hit)
            })
            .collect();
    }
    let mut background = Vec::new();
    for bounds in [
        [left, top, right - left, view_top - top],
        [left, view_bottom, right - left, bottom - view_bottom],
    ] {
        rect(&mut background, bounds, PANEL);
    }
    if !app.runtime_active() && !app.runtime_preparing() {
        rect(
            &mut background,
            [left, view_top, view_left - left, view_bottom - view_top],
            PANEL,
        );
        rect(
            &mut background,
            [view_left - 0.4, view_top, 0.4, view_bottom - view_top],
            LINE,
        );
        rect(
            &mut background,
            [
                view_right,
                view_top,
                right - view_right,
                view_bottom - view_top,
            ],
            PANEL,
        );
        rect(
            &mut background,
            [view_right, view_top, 0.4, view_bottom - view_top],
            LINE,
        );
    }
    background.extend(v);
    if let Some(hit) = hits.iter().find(|hit| Some(hit.id) == app.ui_focus) {
        let [focus_left, focus_top, width, height] = hit.rect;
        let edge = 0.5 * layout.scale;
        for bounds in [
            [focus_left, focus_top, width, edge],
            [focus_left, focus_top + height - edge, width, edge],
            [focus_left, focus_top, edge, height],
            [focus_left + width - edge, focus_top, edge, height],
        ] {
            rect(&mut background, bounds, ACCENT);
        }
    }
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
