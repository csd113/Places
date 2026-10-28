//! Styles edit surface choices and fixture family only, retaining geometry.
use super::{Document, ObjectId, Value, array, inside, json, objects, room_indices};

struct Style {
    wall: &'static str,
    floor: &'static str,
    ceiling: &'static str,
    trim: &'static str,
    fixture: &'static str,
}
const STYLES: [Style; 3] = [
    Style {
        wall: "core:wallpaper_yellow_01",
        floor: "core:carpet_beige_01",
        ceiling: "core:ceiling_panel_01",
        trim: "core:baseboard_office_01",
        fixture: "backrooms:light_fluorescent_recessed",
    },
    Style {
        wall: "core:wallpaper_stained_01",
        floor: "core:carpet_damp_01",
        ceiling: "core:ceiling_stained_01",
        trim: "core:baseboard_office_01",
        fixture: "backrooms:light_fluorescent_damaged",
    },
    Style {
        wall: "home:wall_paint_offwhite_01",
        floor: "home:carpet_cream_01",
        ceiling: "home:ceiling_white_01",
        trim: "home:baseboard_white_01",
        fixture: "home:ceiling_light_round",
    },
];
pub(super) fn candidate(
    doc: &Document,
    index: usize,
    style_index: usize,
    categories: [bool; 5],
    catalog: &crate::engine::assets::AssetCatalog,
) -> Result<(Value, String, Option<ObjectId>), String> {
    let style = STYLES
        .get(style_index)
        .ok_or("Style index must be 0 Office, 1 Worn or 2 Home")?;
    let room = doc
        .level()
        .rooms
        .get(index)
        .ok_or("Select one existing room")?;
    let mut raw = doc.raw().clone();
    let mut changed = 0_usize;
    let [walls, floor, ceiling, trim, fixtures] = categories;
    let room_record = objects::object_mut(&mut raw, ObjectId::Room(index))?;
    if floor {
        drop(room_record.insert("material".into(), json!(style.floor)));
    }
    if ceiling {
        drop(room_record.insert("ceiling_material".into(), json!(style.ceiling)));
    }
    // The room's own local material-only patches may intentionally be accents.
    // They are separate authored records, and remain untouched.
    let mut boundary_trim = 0_usize;
    for (i, w) in doc.level().walls.iter().enumerate() {
        let floor_y = crate::engine::level::LevelSurfaces::new(doc.level())
            .floor_y_at(w.width.mul_add(0.5, w.x), w.depth.mul_add(0.5, w.z))
            .unwrap_or(0.0);
        if (floor_y - room.floor_y).abs() > 0.01 {
            continue;
        }
        let probes = match w.axis() {
            crate::engine::level::WallAxis::X => [
                ("north", w.width.mul_add(0.5, w.x), w.z - 0.01),
                ("south", w.width.mul_add(0.5, w.x), w.z + w.depth + 0.01),
            ],
            crate::engine::level::WallAxis::Z => [
                ("west", w.x - 0.01, w.depth.mul_add(0.5, w.z)),
                ("east", w.x + w.width + 0.01, w.depth.mul_add(0.5, w.z)),
            ],
        };
        let faces: Vec<_> = probes
            .iter()
            .filter(|(_, x, z)| inside(room, *x, *z))
            .collect();
        if faces.is_empty() {
            continue;
        }
        // Partial wall overlap needs a deliberately split carrier first.
        if !inside(room, w.x, w.z) || !inside(room, w.x + w.width, w.z + w.depth) {
            return Err("A wall extends beyond the selected room. Split it before applying a room-local style.".into());
        }
        let wall = objects::object_mut(&mut raw, ObjectId::Wall(i))?;
        if walls {
            let overrides = wall
                .entry("faces")
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .ok_or("Invalid wall faces")?;
            for (face, _, _) in &faces {
                drop(overrides.insert((*face).to_string(), json!(style.wall)));
            }
            changed = changed.saturating_add(1);
        }
        if trim && let Some(t) = wall.get_mut("trim").and_then(Value::as_object_mut) {
            if faces.len() == 2 {
                drop(t.insert("material".into(), json!(style.trim)));
            } else {
                boundary_trim = boundary_trim.saturating_add(1);
            }
        }
    }
    let light_count = style_fixtures(doc, index, &mut raw, style, fixtures, ceiling, catalog)?;
    preserve_trim_geometry(doc, &raw, catalog)?;
    Ok((
        raw,
        format!(
            "Room {} style: {changed} wall faces / {light_count} ceiling fixtures; {boundary_trim} shared boundary trims retained.",
            index.saturating_add(1)
        ),
        None,
    ))
}

fn style_fixtures(
    doc: &Document,
    index: usize,
    raw: &mut Value,
    style: &Style,
    fixtures: bool,
    ceiling: bool,
    catalog: &crate::engine::assets::AssetCatalog,
) -> Result<usize, String> {
    if !fixtures && !ceiling {
        return Ok(0);
    }
    let mut prepared = doc.level().clone();
    crate::map_compiler::prepare_level(&mut prepared, catalog, None);
    let mut count = 0_usize;
    for i in room_indices(doc, index, "ceiling_lights")? {
        let original = array(doc.raw(), "ceiling_lights")
            .get(i)
            .ok_or("Missing light")?;
        if original
            .get("mount")
            .and_then(Value::as_str)
            .is_some_and(|m| m != "ceiling")
        {
            continue;
        }
        let old = prepared
            .ceiling_lights
            .get(i)
            .ok_or("Missing resolved light")?;
        let light = objects::object_mut(raw, ObjectId::Light(i))?;
        // A different ceiling grid must not silently relocate an existing fixture.
        drop(light.insert("x".into(), json!(old.x)));
        drop(light.insert("z".into(), json!(old.z)));
        drop(light.insert("align".into(), json!("none")));
        if fixtures {
            drop(light.insert("fixture".into(), json!(style.fixture)));
            count = count.saturating_add(1);
        }
    }
    Ok(count)
}
fn preserve_trim_geometry(
    doc: &Document,
    raw: &Value,
    catalog: &crate::engine::assets::AssetCatalog,
) -> Result<(), String> {
    let mut before = doc.level().clone();
    let mut after = doc.validate_candidate(raw)?;
    crate::map_compiler::prepare_level(&mut before, catalog, None);
    crate::map_compiler::prepare_level(&mut after, catalog, None);
    let geometry = |boards: Vec<crate::engine::level::BaseboardDef>| -> Result<Vec<Value>, String> {
        boards
            .into_iter()
            .map(|b| {
                let mut v = serde_json::to_value(b).map_err(|e| e.to_string())?;
                let m = v.as_object_mut().ok_or("Invalid trim")?;
                drop(m.remove("material"));
                drop(m.remove("shine"));
                Ok(v)
            })
            .collect()
    };
    if geometry(before.baseboards)? != geometry(after.baseboards)? {
        return Err("This wall finish would add/remove automatic catalog trim. Choose explicit Automatic wall trim first, or turn Walls off in this style; geometry was preserved.".into());
    }
    Ok(())
}
