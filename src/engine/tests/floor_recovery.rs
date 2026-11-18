use crate::engine::level::LevelDef;
use crate::engine::lighting::LevelLighting;
use crate::engine::lighting::lightmap::{Chart, LightmapPatch, PatchKind};
use crate::engine::lighting::transport::receiver_targets;

/// Reproduces unequal authored-room recovery across adjoining charts. The
/// same join crosses an actual wall farther along, where the target must
/// remain local. Two small charts keep the regression independent of a bake.
#[test]
fn an_open_floor_join_caps_only_extra_recovery() -> Result<(), String> {
    let level = LevelDef::from_json(
        r#"{
          "schema": "places-level",
          "format_version": 1,
          "id": "floor_recovery_regression",
          "name": "Floor recovery regression",
          "spawn": {"x":1,"z":1},
          "rooms": [
            {"x":0,"z":0,"width":2,"depth":8,"floor_y":0,"height":3.8},
            {"x":2,"z":0,"width":2,"depth":8,"floor_y":0,"height":3.8}
          ],
          "ceiling_lights": [{"fixture":"core:fluorescent_panel_01","x":1,"y":3.8,"z":2}],
          "walls": [
            {"x":1.5,"y":0,"z":4.9,"width":1,"height":3.8,"depth":0.2},
            {"x":0.6,"y":0,"z":1.9,"width":0.2,"height":3.8,"depth":0.2}
          ]
        }"#,
    )
    .map_err(|error| error.to_string())?;
    let lighting = LevelLighting::bake(&level);
    let floor = |room, x: f32| {
        let end = x + 2.0;
        LightmapPatch::from_quad(
            PatchKind::Floor,
            [
                [x, 0.0, 8.0],
                [end, 0.0, 8.0],
                [end, 0.0, 0.0],
                [x, 0.0, 0.0],
            ],
            Some(room),
        )
        .ok_or_else(|| "failed to construct floor chart".to_string())
    };
    let chart = Chart {
        page: 0,
        x: 0,
        y: 0,
        width: 5,
        height: 9,
    };
    let west = floor(0, 0.0)?;
    let mut east = floor(1, 2.0)?;
    // A clipped native rectangle may carry this sub-texel fourth-corner correction.
    east.u_axis[2] = 0.000_003;
    east.diagonal_correction[2] = -0.000_003;
    let one = receiver_targets(&lighting, &[(west, chart)]);
    let joined = receiver_targets(&lighting, &[(west, chart), (east, chart)]);
    let original = |index| {
        one.get(index)
            .copied()
            .ok_or_else(|| "missing control".to_string())
    };
    let corrected = |index| {
        joined
            .get(index)
            .copied()
            .ok_or_else(|| "missing sample".to_string())
    };
    // The off-strip wall in the same broad-phase bucket cannot suppress
    // the open join at Z=2.
    assert!(original(34)?.into_iter().all(|channel| channel > 0.0));
    assert_eq!(corrected(34)?, corrected(75)?);
    assert!(
        corrected(34)?
            .into_iter()
            .all(|channel| channel.abs() <= f32::EPSILON)
    );
    // The wall crosses the same line at Z=5. Keep the original west target.
    assert_eq!(corrected(19)?, original(19)?);
    // The interior lit pool and the whole east floor retain their targets.
    assert_eq!(corrected(32)?, original(32)?);
    assert!(
        joined
            .iter()
            .skip(45)
            .flatten()
            .all(|channel| channel.abs() <= f32::EPSILON)
    );
    Ok(())
}
