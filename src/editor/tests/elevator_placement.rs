//! Small source-only regressions: flush four-face fitting, atomic opening edits,
//! and real cabin clearance at sub-centimetre gap boundaries. No rendering/bake.
use super::{links, placement, wall_under_ray};
use crate::editor::document::Document;
use glam::Vec3;
use serde_json::{Value, json};

fn document(name: &str, walls: &Value, rooms: &Value) -> Result<Document, String> {
    let path = std::env::temp_dir().join(format!("places-lift-fit-{name}-{}", std::process::id()));
    let mut doc = Document::create(&path, name, name)?;
    let before = doc.raw().clone();
    doc.apply_authoring_edit(&before,json!({"schema":"places-level","format_version":1_u32,"id":name,"name":name,"spawn":{"x":0_f32,"z":-1_f32},"rooms":rooms,"walls":walls}))?;
    Ok(doc)
}
fn parse(source: &str) -> Result<Value, String> {
    serde_json::from_str(source).map_err(|e| e.to_string())
}
fn set(value: &mut Value, key: &str, replacement: Value) -> Result<(), String> {
    *value.get_mut(key).ok_or("missing fixture field")? = replacement;
    Ok(())
}
fn dispose(doc: &Document) -> Result<(), String> {
    // Each fixture has a separate temporary project, never a shipped map.
    std::fs::remove_dir_all(doc.project().root()).map_err(|e| e.to_string())
}

#[test]
fn wall_faces_fit_without_global_grid_rounding_and_undo_together() -> Result<(), String> {
    for (axis, positive) in [(false, true), (false, false), (true, true), (true, false)] {
        let mut wall = parse(
            r#"{"x":-4,"z":0.137,"width":8,"depth":0.2,"height":2.8,"openings":[{"kind":"door","offset":3.623,"width":1.1,"height":2.2,"sill":0}]}"#,
        )?;
        if axis {
            set(&mut wall, "x", json!(0.137_f32))?;
            set(&mut wall, "z", json!(-4_f32))?;
            set(&mut wall, "width", json!(0.2_f32))?;
            set(&mut wall, "depth", json!(8_f32))?;
        }
        let name = format!("fit_{}_{}", u8::from(axis), u8::from(positive));
        let mut doc = document(&name, &json!([wall]), &json!([]))?;
        let cross = if positive { -1.0_f32 } else { 1.5 };
        let eye = if axis {
            Vec3::new(cross, 1.6, 0.2)
        } else {
            Vec3::new(0.2, 1.6, cross)
        };
        let at = [0.2_f32, 0.0, 0.2];
        let fitted = placement::resolve(&doc, at, 0.0, Some(eye), Some(0), Some(0.5))?;
        let face = if positive { 0.137_f32 } else { 0.337 };
        assert!(
            (if axis { fitted.at[0] } else { fitted.at[2] } - face).abs() < 0.00001,
            "wall-normal coordinate must remain exact, including opposite face thickness"
        );
        assert!(
            (if axis { fitted.at[2] } else { fitted.at[0] } - 0.173).abs() < 0.00001,
            "existing opening centre wins over grid"
        );
        let expected = match (axis, positive) {
            (false, true) => 0_f32,
            (false, false) => 180.0,
            (true, true) => 90.0,
            (true, false) => 270.0,
        };
        assert!(
            (fitted.rotation - expected).abs() < 0.001,
            "automatic inward orientation"
        );
        let before = doc.raw().clone();
        let raw = links::place(&doc, &fitted)?;
        doc.apply_authoring_edit(&before, raw)?;
        let opening = doc
            .level()
            .walls
            .first()
            .and_then(|w| w.openings.first())
            .ok_or("missing fitted opening")?;
        assert!(
            (opening.width - 1.5).abs() < 0.00001 && (opening.height - 2.25).abs() < 0.00001,
            "opening and cabin fit in one transaction"
        );
        assert!(
            doc.undo() && doc.raw() == &before,
            "one undo restores both opening and lift"
        );
        assert!(doc.redo(), "placement is redoable");
        doc.save()?;
        dispose(&doc)?;
    }
    Ok(())
}

#[test]
fn actual_cabin_depth_width_and_height_gate_placement() -> Result<(), String> {
    let walls = parse(r#"[{"x":-3,"z":0,"width":6,"depth":0.2,"height":2.8}]"#)?;
    for (name, gap, accepted) in [("gap_exact", 2.48_f32, true), ("gap_short", 2.47, false)] {
        let mut rooms = parse(
            r#"[{"x":-3,"z":-5,"width":6,"depth":5,"height":2.8},{"x":-3,"z":3,"width":6,"depth":5,"height":2.8}]"#,
        )?;
        set(
            rooms.get_mut(1).ok_or("missing adjacent room")?,
            "z",
            json!(gap),
        )?;
        let doc = document(name, &walls, &rooms)?;
        let before = doc.raw().clone();
        let result = placement::resolve(
            &doc,
            [0.0, 0.0, -0.2],
            0.0,
            Some(Vec3::new(0.0, 1.6, -2.0)),
            None,
            Some(0.5),
        );
        assert_eq!(result.is_ok(), accepted, "actual 2.48 m depth boundary");
        if let Err(error) = result {
            assert!(
                error.contains("Not enough space") && error.contains("Room"),
                "actionable adjacent-room refusal"
            );
        }
        assert_eq!(doc.raw(), &before, "preview/refusal never mutates source");
        dispose(&doc)?;
    }
    for (name, x, accepted) in [("side_exact", 1.28_f32, true), ("side_short", 1.27, false)] {
        let mut rooms = parse(
            r#"[{"x":-3,"z":-5,"width":6,"depth":5,"height":2.8},{"x":2,"z":0.4,"width":3,"depth":3,"height":2.8}]"#,
        )?;
        set(
            rooms.get_mut(1).ok_or("missing adjacent room")?,
            "x",
            json!(x),
        )?;
        let doc = document(name, &walls, &rooms)?;
        let result = placement::resolve(&doc, [0.0, 0.0, 0.0], 0.0, None, None, None);
        assert_eq!(
            result.is_ok(),
            accepted,
            "rear body is 2.56 m wide, not the 2.90 m pocket width"
        );
        dispose(&doc)?;
    }
    for (name, height, open, accepted) in [
        ("roof_exact", 2.63_f32, false, true),
        ("roof_short", 2.62, false, false),
        ("open_low_eave", 2.0, true, true),
    ] {
        let mut room = parse(r#"{"x":-4,"z":-4,"width":8,"depth":8,"height":3}"#)?;
        set(&mut room, "height", json!(height))?;
        if open {
            drop(
                room.as_object_mut()
                    .ok_or("missing room object")?
                    .insert("ceiling".into(), json!({"kind":"open"})),
            );
        }
        let doc = document(name, &json!([]), &json!([room]))?;
        let result = placement::resolve(&doc, [0.0, 0.0, 0.0], 35.0, None, None, None);
        assert_eq!(
            result.is_ok(),
            accepted,
            "free rotated placement respects solid ceilings and permits open sky"
        );
        dispose(&doc)?;
    }
    let mut thick = walls;
    set(
        thick.get_mut(0).ok_or("missing wall")?,
        "depth",
        json!(0.25_f32),
    )?;
    let doc = document("wall_thick", &thick, &json!([]))?;
    assert!(
        placement::resolve(&doc, [0.0, 0.0, 0.0], 0.0, None, None, None).is_err(),
        "host wall cannot bury passenger-facing wood and controls"
    );
    dispose(&doc)?;
    Ok(())
}

#[test]
fn visible_storey_floor_blocks_lower_wall_but_not_horizontal_doorway_picking() -> Result<(), String>
{
    // Three source-only rays protect against editing the storey below an opaque
    // floor while retaining nearer walls and aperture-aware horizontal picking.
    let walls = parse(
        r#"[{"x":-3,"z":0,"width":6,"depth":0.2,"height":2.8,"openings":[{"kind":"door","offset":2.25,"width":1.5,"height":2.25}]}]"#,
    )?;
    let rooms = parse(
        r#"[{"x":-3,"z":-5,"width":6,"depth":5,"height":2.8},{"x":-3,"z":-5,"width":6,"depth":5,"floor_y":3.5,"height":2.8}]"#,
    )?;
    let doc = document("floor_occludes_wall", &walls, &rooms)?;
    let focus = crate::editor::focus::Focus::default();
    let eye = Vec3::new(0.0, 5.1, -4.0);
    let direction = (Vec3::new(0.0, 3.5, -2.0) - eye).normalize();
    let floor_distance = Some((3.5 - eye.y) / direction.y);
    assert!(
        wall_under_ray(doc.level(), &focus, eye, direction, floor_distance).is_none(),
        "the upper floor at z=-2 hides the lower wall at z=0"
    );
    let horizontal = wall_under_ray(
        doc.level(),
        &focus,
        Vec3::new(0.0, 1.6, -4.0),
        Vec3::Z,
        None,
    )
    .ok_or("horizontal doorway did not pick its host wall")?;
    assert_eq!(horizontal.0, 0, "empty apertures retain their host wall");
    let mut level = doc.level().clone();
    let mut nearer = level.walls.first().cloned().ok_or("missing wall")?;
    nearer.y = 3.5;
    nearer.z = -3.0;
    level.walls.push(nearer);
    assert_eq!(
        wall_under_ray(&level, &focus, eye, direction, floor_distance).map(|(index, _)| index),
        Some(1),
        "a wall in front of the upper floor remains pickable"
    );
    dispose(&doc)
}

#[test]
fn occupied_doorway_is_refused_without_touching_its_leaf() -> Result<(), String> {
    let walls = parse(
        r#"[{"x":-3,"z":0,"width":6,"depth":0.2,"height":2.8,"openings":[{"kind":"door","offset":2.45,"width":1.1,"height":2.2,"sill":0}]}]"#,
    )?;
    let rooms = parse(r#"[{"x":-3,"z":-5,"width":6,"depth":5.2,"height":2.8}]"#)?;
    let mut doc = document("occupied", &walls, &rooms)?;
    let before = doc.raw().clone();
    let mut raw = before.clone();
    let _previous = raw.as_object_mut().ok_or("missing level object")?.insert(
        "doors".into(),
        parse(r#"[{"id":"existing_leaf","x":-0.5,"z":0.1,"width":1.0,"height":2.2}]"#)?,
    );
    doc.apply_authoring_edit(&before, raw)?;
    let occupied = doc.raw().clone();
    let error = placement::resolve(&doc, [0.0, 0.0, 0.0], 0.0, None, None, None)
        .err()
        .ok_or("occupied opening accepted")?;
    assert!(
        error.contains("hinged door"),
        "specific occupied-opening explanation"
    );
    assert_eq!(
        doc.raw(),
        &occupied,
        "the existing door and opening are preserved"
    );
    dispose(&doc)
}
