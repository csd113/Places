//! A single grant owns one preview and an exact, bounded receipt ledger.

#[cfg(test)]
#[path = "../tests/agent_batch.rs"]
mod tests;

use std::collections::BTreeMap;

use crate::engine::package::hash::sha256_hex;
use glam::Vec3;
use serde::Serialize;
use serde_json::{Value, json};

use super::{
    SceneStamp,
    pilot::ReplaceFixture,
    protocol::Nonce,
    wire::{self, Action, Call, Code, Failure},
};
use crate::{
    editor::{App, Document, ObjectId, document::objects, escalators, railings},
    engine::{canonical_json::canonical_json_bytes, level::LevelDef},
};

const MAX_SCENE_BYTES: usize = 64 * 1024;
const MAX_SCENE_OBJECTS: usize = 96;
const CATALOG_PAGE: usize = 16;

#[derive(Clone)]
struct Frame {
    scene: SceneStamp,
    viewport: ([u32; 16], [u32; 4]),
    selected: Option<ObjectId>,
    hidden: [bool; 4],
    isolated: Option<ObjectId>,
    snap: Option<u32>,
}

impl Frame {
    fn live(app: &App) -> Option<Self> {
        let doc = app.document.as_ref()?;
        if !app.viewport.document_scene_ready() {
            return None;
        }
        Some(Self {
            scene: doc.scene_stamp().clone(),
            viewport: app.viewport.agent_frame(),
            selected: app.selected,
            hidden: app.focus.hidden,
            isolated: app.focus.isolated,
            snap: app.view.snap.then_some(app.step.to_bits()),
        })
    }

    fn matches(&self, other: &Self) -> bool {
        self.scene.matches(&other.scene)
            && self.viewport == other.viewport
            && self.selected == other.selected
            && self.hidden == other.hidden
            && self.isolated == other.isolated
            && self.snap == other.snap
    }
}

#[derive(Serialize)]
#[serde(tag = "change", rename_all = "snake_case")]
enum Delta {
    Railing {
        record: usize,
        owner_room: usize,
        points: Vec<[f32; 3]>,
    },
    Escalator {
        prop: usize,
        lower: [f32; 3],
        upper: [f32; 3],
        origin: [f32; 3],
        yaw: f32,
        rise: f32,
        length: f32,
    },
    Fixture {
        light: usize,
        asset_id: String,
        mount: &'static str,
    },
}

enum Resolved<'a> {
    Railing(Vec<[f32; 3]>),
    Escalator { lower: Vec3, upper: Vec3 },
    Fixture { light: usize, asset: &'a str },
}

struct Draft {
    raw: Value,
    level: LevelDef,
    byte_count: usize,
    final_selection: Option<ObjectId>,
    changes: Vec<Delta>,
}

struct Plan {
    request_id: u32,
    context: u64,
    frame: Frame,
    draft: Draft,
}

struct Receipt {
    digest: String,
    result: Value,
}

#[derive(Default)]
pub(super) struct State {
    context: Option<(u64, Frame)>,
    serial: u64,
    plan: Option<Plan>,
    receipts: BTreeMap<u32, Receipt>,
}

impl State {
    pub(super) fn observe(&mut self, app: &App) {
        if self
            .context
            .as_ref()
            .is_some_and(|(_, saved)| Frame::live(app).is_none_or(|live| !saved.matches(&live)))
        {
            self.context = None;
        }
    }

    pub(super) fn dispatch(&mut self, app: &mut App, call: Call, grant: Nonce) -> Value {
        self.observe(app);
        let result = match call {
            Call::Inspect { samples } => self.inspect(app, &samples),
            Call::Fixtures { after } => fixtures(app, after.as_deref()),
            Call::Preview {
                request_id,
                context,
                actions,
            } => self.preview(app, request_id, context, &actions, grant),
            Call::Apply { request_id, digest } => self.apply(app, request_id, &digest),
            Call::Cancel { request_id, digest } => self.cancel(request_id, &digest),
            Call::Receipt { request_id, digest } => self.receipt(request_id, &digest),
        };
        result.unwrap_or_else(|failure| json!({"outcome":"rejected","error":failure}))
    }

    fn inspect(&mut self, app: &App, samples: &[[f32; 2]]) -> Result<Value, Failure> {
        if samples.len() > wire::MAX_POINTS {
            return Err(Failure::new(
                Code::Limit,
                "Inspect at most eight floor samples at a time",
            ));
        }
        let frame = Frame::live(app).ok_or_else(not_ready)?;
        let document = app.document.as_ref().ok_or_else(not_ready)?;
        if !samples.is_empty() {
            check_scene_bound(document)?;
        }
        let serial = if let Some((previous, _)) = &self.context {
            *previous
        } else {
            self.serial = self
                .serial
                .checked_add(1)
                .ok_or_else(|| Failure::new(Code::Limit, "Resume Agent for a fresh context"))?;
            self.context = Some((self.serial, frame.clone()));
            self.serial
        };
        let floors = samples
            .iter()
            .map(|point| {
                screen(app, *point)
                    .ok()
                    .and_then(|ui| app.viewport.play_floor(ui).ok())
                    .map(|floor_hit| floor_hit.to_array())
            })
            .collect::<Vec<_>>();
        let selection = frame.selected.map(|id| {
            let (kind, index) = id.collection();
            json!({"kind":kind,"index":index,"editable":app.editable(id) && app.focus.included(document.level(),id)})
        });
        Ok(json!({
            "outcome":"inspected",
            "context":serial,
            "revision":frame.scene.revision(),
            "selection":selection,
            "hidden_layers":frame.hidden,
            "isolation":frame.isolated.map(ObjectId::collection),
            "snap_metres":frame.snap.map(f32::from_bits),
            "sample_face":"visible_walkable_floor",
            "floors":floors,
            "capabilities":["railing.place_path@1","escalator.connect_landings@1","light.replace_fixture@1"]
        }))
    }

    fn preview(
        &mut self,
        app: &App,
        id: u32,
        context: u64,
        actions: &[Action],
        grant: Nonce,
    ) -> Result<Value, Failure> {
        if id == 0
            || !(1..=wire::MAX_ACTIONS).contains(&actions.len())
            || !actions.iter().all(Action::bounded)
        {
            return Err(Failure::new(
                Code::InvalidInput,
                "Use a nonzero request ID and 1–3 well-formed pilot actions",
            ));
        }
        let bytes = canonical_json_bytes(&("places-editor-pilot-v1", grant, context, actions))
            .map_err(|_error| Failure::new(Code::InvalidInput, "Cannot canonicalize request"))?;
        let digest = sha256_hex(&bytes);
        if self.receipts.contains_key(&id) {
            return self.receipt(id, &digest);
        }
        if self.receipts.len() >= wire::MAX_REQUESTS {
            return Err(Failure::new(
                Code::Limit,
                "Take Over and explicitly Resume to start another bounded grant",
            ));
        }
        if self.plan.is_some() {
            return Err(Failure::new(
                Code::NotReady,
                "Cancel or apply the outstanding preview first",
            ));
        }
        let (_, frame) = self
            .context
            .as_ref()
            .filter(|(known, _)| *known == context)
            .ok_or_else(stale)?;
        let draft = match stage(app, actions) {
            Ok(draft) => draft,
            Err(error) => {
                let result =
                    json!({"outcome":"rejected","request_id":id,"digest":digest,"error":error});
                drop(self.receipts.insert(
                    id,
                    Receipt {
                        digest,
                        result: result.clone(),
                    },
                ));
                return Ok(result);
            }
        };
        let result = json!({
            "outcome":"previewed","request_id":id,"digest":digest,
            "base_revision":frame.scene.revision(),"changes":draft.changes
        });
        self.plan = Some(Plan {
            request_id: id,
            context,
            frame: frame.clone(),
            draft,
        });
        drop(self.receipts.insert(
            id,
            Receipt {
                digest,
                result: result.clone(),
            },
        ));
        Ok(result)
    }

    fn receipt(&self, id: u32, digest: &str) -> Result<Value, Failure> {
        let receipt = self.receipts.get(&id).ok_or_else(|| {
            Failure::new(
                Code::UnknownRequest,
                "No matching request in this grant; do not repeat an uncertain apply blindly",
            )
        })?;
        if receipt.digest != digest {
            return Err(Failure::new(
                Code::IdConflict,
                "Request ID was used with a different digest",
            ));
        }
        Ok(receipt.result.clone())
    }

    fn cancel(&mut self, id: u32, digest: &str) -> Result<Value, Failure> {
        let current = self.receipt(id, digest)?;
        if let Some(plan) = &self.plan
            && plan.request_id == id
        {
            self.plan = None;
            let result = json!({"outcome":"cancelled","request_id":id,"digest":digest});
            if let Some(receipt) = self.receipts.get_mut(&id) {
                receipt.result = result.clone();
            }
            return Ok(result);
        }
        Ok(current)
    }

    fn apply(&mut self, app: &mut App, id: u32, digest: &str) -> Result<Value, Failure> {
        let current = self.receipt(id, digest)?;
        if self.plan.as_ref().is_none_or(|plan| plan.request_id != id) {
            return Ok(current);
        }
        let plan = self.plan.take().ok_or_else(stale)?;
        let installation = if self
            .context
            .as_ref()
            .is_some_and(|(token, _)| *token == plan.context)
        {
            install(app, plan, id, digest)
        } else {
            Err(stale())
        };
        let result = match installation {
            Ok(receipt) => {
                self.context = None;
                receipt
            }
            Err(failure) => {
                json!({"outcome":"rejected","request_id":id,"digest":digest,"error":failure})
            }
        };
        if let Some(receipt) = self.receipts.get_mut(&id) {
            receipt.result = result.clone();
        }
        Ok(result)
    }
}

fn screen(app: &App, point: [f32; 2]) -> Result<[f32; 2], Failure> {
    if !point
        .iter()
        .all(|x| x.is_finite() && (0.0..1.0).contains(x))
    {
        return Err(Failure::new(
            Code::InvalidInput,
            "Use normalized coordinates inside the editor viewport",
        ));
    }
    let [left, top, right, bottom] = app.viewport.layout().viewport();
    let [x, y] = point;
    let ui = [x.mul_add(right - left, left), y.mul_add(bottom - top, top)];
    if !app.viewport.contains_ui(ui) {
        return Err(Failure::new(
            Code::InvalidInput,
            "Point is outside the editor viewport",
        ));
    }
    Ok(ui)
}

fn fixtures(app: &App, after: Option<&str>) -> Result<Value, Failure> {
    if after.is_some_and(|id| !wire::bounded_id(id)) {
        return Err(Failure::new(
            Code::InvalidInput,
            "Expected a fixture catalog cursor",
        ));
    }
    let mut lights = app
        .catalog
        .entries()
        .into_iter()
        .filter(|entry| entry.is_light() && after.is_none_or(|cursor| entry.id.as_str() > cursor))
        .collect::<Vec<_>>();
    lights.sort_unstable_by(|a, b| a.id.cmp(&b.id));
    let page = lights
        .iter()
        .take(CATALOG_PAGE)
        .map(|light| json!({"asset_id":light.id,"mount":mount(&light.id)}))
        .collect::<Vec<_>>();
    let next = lights
        .get(CATALOG_PAGE)
        .and_then(|_| lights.get(CATALOG_PAGE.saturating_sub(1)))
        .map(|entry| entry.id.as_str());
    Ok(json!({"outcome":"catalog","fixtures":page,"next":next}))
}

fn mount(asset: &str) -> &'static str {
    crate::editor::Document::fixture_mount(asset)
}

fn not_ready() -> Failure {
    Failure::new(
        Code::NotReady,
        "Wait for the accepted editor viewport to finish preparing",
    )
}

fn stale() -> Failure {
    Failure::new(
        Code::StaleContext,
        "Scene, camera or edit context changed; inspect and preview again",
    )
}

fn rejected(mut error: String) -> Failure {
    wire::truncate_message(&mut error);
    Failure {
        code: Code::Rejected,
        message: error,
        action: None,
    }
}

fn resolve<'a>(app: &App, doc: &Document, action: &'a Action) -> Result<Resolved<'a>, Failure> {
    let floor = |point| {
        app.viewport
            .play_floor(screen(app, point)?)
            .map_err(|error| Failure::new(Code::UnsupportedFloor, &error))
    };
    match action {
        Action::Railing { points } => {
            let pick = app.viewport.wall_picking().ok_or_else(not_ready)?;
            let grid = app.view.snap.then_some(app.step);
            let snapped = points
                .iter()
                .map(|&point| {
                    let ray = app
                        .viewport
                        .ray(screen(app, point)?)
                        .ok_or_else(not_ready)?;
                    pick.railing_floor(doc.level(), ray, grid)
                        .ok_or_else(|| Failure::new(Code::UnsupportedFloor, railings::FOOTING_CUE))
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Resolved::Railing(snapped))
        }
        Action::Escalator { lower, upper } => Ok(Resolved::Escalator {
            lower: floor(*lower)?,
            upper: floor(*upper)?,
        }),
        Action::Fixture { light, asset_id } => {
            let index = usize::try_from(*light)
                .map_err(|_error| Failure::new(Code::WrongSelection, "Unknown light"))?;
            Ok(Resolved::Fixture {
                light: index,
                asset: asset_id,
            })
        }
    }
}

fn check_scene_bound(doc: &Document) -> Result<(), Failure> {
    if doc.agent_scene_size() > MAX_SCENE_BYTES || doc.objects().len() > MAX_SCENE_OBJECTS {
        return Err(Failure::new(
            Code::Limit,
            "This pilot only admits small editor scenes (64 KiB, 96 editor objects)",
        ));
    }
    Ok(())
}

fn stage(app: &App, actions: &[Action]) -> Result<Draft, Failure> {
    let doc = app.document.as_ref().ok_or_else(not_ready)?;
    check_scene_bound(doc)?;
    let resolved = actions
        .iter()
        .enumerate()
        .map(|(i, action)| resolve(app, doc, action).map_err(|error| error.at(i)))
        .collect::<Result<Vec<_>, _>>()?;
    let mut raw = doc.raw().clone();
    let mut level = doc.level().clone();
    let mut selection = app.selected;
    let mut changes = Vec::with_capacity(actions.len());
    let mut byte_count = 0;
    for (i, action) in resolved.into_iter().enumerate() {
        let step = stage_action(
            &app.focus,
            &app.catalog,
            doc,
            &raw,
            &level,
            selection,
            action,
        )
        .map_err(|error| error.at(i))?;
        raw = step.raw;
        level = step.level;
        byte_count = step.byte_count;
        selection = step.selection;
        changes.push(step.change);
    }
    if raw == *doc.raw() {
        return Err(Failure::new(
            Code::InvalidInput,
            "Batch would not change the scene",
        ));
    }
    Ok(Draft {
        raw,
        level,
        byte_count,
        changes,
        final_selection: selection,
    })
}

struct Step {
    raw: Value,
    level: LevelDef,
    byte_count: usize,
    selection: Option<ObjectId>,
    change: Delta,
}

fn stage_action(
    focus: &crate::editor::focus::Focus,
    catalog: &crate::engine::assets::AssetCatalog,
    doc: &Document,
    before: &Value,
    level: &LevelDef,
    mut selection: Option<ObjectId>,
    action: Resolved<'_>,
) -> Result<Step, Failure> {
    let mut raw = before.clone();
    let objects = objects::enumerate(level);
    let change = match action {
        Resolved::Railing(points) => {
            let [start, middle @ .., end] = points.as_slice() else {
                return Err(Failure::new(
                    Code::InvalidInput,
                    "Pick at least two floor points",
                ));
            };
            let index = railings::stage_path(&mut raw, *start, middle, *end).map_err(rejected)?;
            focus
                .check_edit(before, level, &objects, &raw)
                .map_err(rejected)?;
            let (checked, byte_count) = doc
                .prepare_agent_stage(before, &objects, &raw)
                .map_err(rejected)?;
            let ObjectId::Room(owner_room) =
                railings::check_path(&checked, level, index, *start).map_err(rejected)?
            else {
                return Err(Failure::new(Code::Rejected, "Railing has no carrier room"));
            };
            return Ok(Step {
                raw,
                level: checked,
                byte_count,
                selection,
                change: Delta::Railing {
                    record: index,
                    owner_room,
                    points,
                },
            });
        }
        Resolved::Escalator { lower, upper } => {
            let fit = escalators::fit_connection(level, lower, upper).map_err(rejected)?;
            let id @ ObjectId::Prop(prop) =
                Document::stage_escalator(&mut raw, level, fit).map_err(rejected)?
            else {
                return Err(Failure::new(
                    Code::Rejected,
                    "Escalator did not produce a prop",
                ));
            };
            selection = Some(id);
            Delta::Escalator {
                prop,
                lower: lower.to_array(),
                upper: upper.to_array(),
                origin: fit.origin.to_array(),
                yaw: fit.yaw,
                rise: fit.dimensions.rise,
                length: fit.dimensions.length,
            }
        }
        Resolved::Fixture { light, asset } => {
            let id = ObjectId::Light(light);
            if selection != Some(id) || !focus.included(level, id) {
                return Err(Failure::new(
                    Code::WrongSelection,
                    "Select the visible light in the editor, then inspect again",
                ));
            }
            if !focus.editable(before, id) {
                return Err(Failure::new(
                    Code::LockedTarget,
                    "Unlock the selected light in Layers before editing",
                ));
            }
            ReplaceFixture {
                light,
                asset,
                catalog,
            }
            .check_input(level)
            .map_err(|error| Failure::new(Code::InvalidInput, &error.to_string()))?;
            Document::stage_fixture(&mut raw, level, light, asset).map_err(rejected)?;
            Delta::Fixture {
                light,
                asset_id: asset.to_owned(),
                mount: mount(asset),
            }
        }
    };
    let (checked_level, byte_count) = doc
        .prepare_agent_stage(before, &objects, &raw)
        .map_err(rejected)?;
    Ok(Step {
        raw,
        level: checked_level,
        byte_count,
        selection,
        change,
    })
}

fn install(app: &mut App, plan: Plan, request_id: u32, digest: &str) -> Result<Value, Failure> {
    let live = Frame::live(app)
        .filter(|frame| frame.matches(&plan.frame))
        .ok_or_else(stale)?;
    let doc = app.document.as_mut().ok_or_else(not_ready)?;
    let agent = &app.agent;
    let Draft {
        raw,
        level,
        byte_count,
        final_selection,
        changes,
    } = plan.draft;
    let result = doc.install_agent_batch(&live.scene, raw, level, byte_count, || {
        agent.admitted(Some(&live.scene))
    });
    if let Err(error) = result {
        return Err(if agent.admitted(Some(&live.scene)) {
            rejected(error)
        } else {
            Failure::new(
                Code::GrantLost,
                "The visible editor grant was lost before commit",
            )
        });
    }
    let revision = doc.scene_stamp().revision();
    app.agent.refresh = Some(doc.scene_stamp().clone());
    app.selected = final_selection;
    app.changed();
    Ok(
        json!({"outcome":"applied","request_id":request_id,"digest":digest,
        "base_revision":live.scene.revision(),"revision":revision,"undo_steps":1,"changes":changes}),
    )
}
