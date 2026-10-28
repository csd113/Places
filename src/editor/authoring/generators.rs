//! Source forms for construction attachments; ordinary preview/history owns admission.
use super::{App, Field, Kind, Tool, Value, flag, number, text};
use crate::editor::document::ObjectId;

pub(super) fn fields(tool: Tool) -> Vec<Field> {
    match tool {
        Tool::Recess => vec![
            number("/offset", "Along wall / near edge m", "0.5"),
            number("/width", "Opening width m", "1.4"),
            number("/height", "Opening height m", "0.8"),
            number("/sill", "Elevation above wall base m", "1"),
            number("/recess/depth", "Depth m / keep 5 cm backing", "0.1"),
            flag(
                "/recess/positive_face",
                "Face: false north/west, true S/E",
                "false",
            ),
            text("/recess/back_material", "Back material / blank = wall", ""),
            text("/recess/side_material", "Side material / blank = wall", ""),
            number("/surround/width", "Optional frame width m / blank", ""),
            number("/surround/projection", "Frame projection m", "0.025"),
            number("/surround/trim_width", "Outer trim width m", "0"),
            text(
                "/surround/material",
                "Frame material",
                "home:baseboard_white_01",
            ),
        ],
        Tool::Surround => vec![
            number("/surround/width", "Frame width m", "0.07"),
            number("/surround/projection", "Frame projection m", "0.025"),
            number(
                "/surround/sill_projection",
                "Sill projection / 0 = none",
                "0.12",
            ),
            number("/surround/trim_width", "Outer trim / 0 = none", "0.02"),
            text(
                "/surround/material",
                "Frame material / blank = wall",
                "home:baseboard_white_01",
            ),
            text(
                "/surround/sill_material",
                "Sill material / blank = frame",
                "",
            ),
            text(
                "/surround/trim_material",
                "Trim material / blank = frame",
                "",
            ),
        ],
        Tool::Tunnel => vec![
            number("/start/0", "Start X / relative to room", "1.5"),
            number("/start/1", "Start Z / relative to room", "2"),
            number("/end/0", "End X / relative to room", "4.5"),
            number("/end/1", "End Z / relative to room", "2"),
            number("/width", "Clear width m", "1.1"),
            number("/clearance", "Headroom / crouch required m", "1.05"),
            number("/thickness", "Shell thickness m", "0.1"),
            text("/material", "Shell material / blank = wall", ""),
        ],
        Tool::RailPath => vec![
            number("/height", "Railing / fence height m", "1"),
            number("/post_spacing", "Post spacing m", "1.2"),
            text("/material", "Rail material / blank = wall", ""),
            text("/post_material", "Post material / blank = rail", ""),
            number("/x", "Start X / world metres", "1"),
            number("/z", "Start Z / world metres", "1"),
            Field {
                path: "/path".into(),
                label: "Next X,Z pairs e.g. [[4,1],[4,4]]",
                kind: Kind::Json,
                default: "[[4,1],[4,4]]",
            },
            number("/y", "World base Y / blank = follow floor", ""),
            number("/rise", "Section rise / blank = follow floor", ""),
        ],
        Tool::Column
        | Tool::Pillar
        | Tool::ArcWall
        | Tool::WallTrim
        | Tool::Room
        | Tool::Ramp
        | Tool::Stairs
        | Tool::Rail
        | Tool::Water
        | Tool::Sky
        | Tool::Weather
        | Tool::Sun
        | Tool::Ceiling
        | Tool::AcousticMaterial
        | Tool::LocalFloor
        | Tool::GlobalPatch
        | Tool::GlobalRegion
        | Tool::Route => Vec::new(),
    }
}

pub(super) fn owner(app: &App, tool: Tool) -> Result<Option<usize>, String> {
    match tool {
        Tool::Recess | Tool::Surround => match app.selected {
            Some(ObjectId::Wall(i)) => Ok(Some(i)),
            _ => Err("Select the carrier wall first".into()),
        },
        Tool::Tunnel => match app.selected {
            Some(ObjectId::Room(i)) => Ok(Some(i)),
            _ => Err("Select the room floor for both tunnel endpoints first".into()),
        },
        Tool::RailPath
        | Tool::Column
        | Tool::Pillar
        | Tool::ArcWall
        | Tool::WallTrim
        | Tool::Room
        | Tool::Ramp
        | Tool::Stairs
        | Tool::Rail
        | Tool::Water
        | Tool::Sky
        | Tool::Weather
        | Tool::Sun
        | Tool::Ceiling
        | Tool::AcousticMaterial
        | Tool::LocalFloor
        | Tool::GlobalPatch
        | Tool::GlobalRegion
        | Tool::Route => Ok(None),
    }
}

pub(super) fn collection(raw: &Value, tool: Tool, owner: Option<usize>) -> Option<&Value> {
    let parent = if let Some(i) = owner {
        raw.get(if tool == Tool::Tunnel {
            "rooms"
        } else {
            "walls"
        })?
        .get(i)?
    } else {
        raw
    };
    parent.get(tool.collection())
}

pub(super) fn collection_mut(
    raw: &mut Value,
    tool: Tool,
    owner: Option<usize>,
) -> Result<&mut Vec<Value>, String> {
    let parent = if let Some(i) = owner {
        raw.get_mut(if tool == Tool::Tunnel {
            "rooms"
        } else {
            "walls"
        })
        .and_then(|v| v.get_mut(i))
        .ok_or("Construction carrier no longer exists")?
    } else {
        raw
    };
    crate::editor::document::record_array_mut(parent, tool.collection())
}

pub(super) fn initialize(
    app: &App,
    tool: Tool,
    owner: Option<usize>,
    original: &mut Option<Value>,
) -> Result<(), String> {
    if tool == Tool::Surround && original.is_none() {
        return Err("Make a doorway/window first, then select its opening using < or >".into());
    }
    if tool == Tool::Recess && original.as_ref().is_some_and(|v| v.get("recess").is_none()) {
        return Err("This is a through-opening. Use New to add a separate recess; existing apertures are preserved".into());
    }
    if tool == Tool::RailPath && original.is_none() {
        *original = Some(serde_json::json!({"length":1_u32}));
    }
    if tool == Tool::Tunnel && original.is_none() {
        let room = app
            .document
            .as_ref()
            .and_then(|d| d.level().rooms.get(owner?))
            .ok_or("Select a room")?;
        *original = Some(
            serde_json::json!({"start":[1.1_f32,room.depth * 0.5],"end":[room.width - 1.1,room.depth * 0.5]}),
        );
    }
    if tool == Tool::Recess && original.is_none() {
        *original = Some(serde_json::json!({"kind":"recess"}));
    }
    if tool == Tool::Room && original.is_none() {
        *original = Some(super::default_room(
            app,
            app.document.as_ref().ok_or("Open a project")?,
        ));
    }
    Ok(())
}

pub(super) fn open_index(app: &App, tool: Tool) -> usize {
    let Some(doc) = &app.document else {
        return 0;
    };
    if tool == Tool::RailPath {
        return doc
            .level()
            .guardrails
            .iter()
            .position(|r| !r.path.is_empty())
            .unwrap_or_else(|| doc.level().guardrails.len());
    }
    if tool == Tool::Recess
        && let Some(ObjectId::Wall(i)) = app.selected
    {
        return doc.level().walls.get(i).map_or(0, |w| {
            w.openings
                .iter()
                .position(|o| o.recess.is_some())
                .unwrap_or(w.openings.len())
        });
    }
    match (tool, app.selected) {
        (Tool::Column, Some(ObjectId::Column(i)))
        | (Tool::Pillar, Some(ObjectId::Pillar(i)))
        | (Tool::ArcWall, Some(ObjectId::ArcWall(i)))
        | (Tool::WallTrim, Some(ObjectId::Wall(i))) => i,
        _ => 0,
    }
}
