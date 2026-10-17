//! These CPU-only regressions protect explicit source eligibility and compiled
//! metadata preservation, plus bounded per-water controls. Tiny source-only
//! fixtures load no assets, solve no transport and allocate no GPU resources.

use crate::engine::level::{CausticsDef, GlobalIlluminatorDef, LevelDef, LightDef};
use crate::engine::lighting::LevelLighting;
use crate::engine::package::lighting::{read_lighting, write_lighting};

#[test]
fn caustic_source_eligibility_survives_compiled_lighting() -> Result<(), String> {
    let level = LevelDef::from_json(
        r#"{"format_version":3,"id":"caustics_metadata","name":"Caustics metadata",
        "spawn":{"x":0,"z":0},"ceiling_lights":[
            {"fixture":"core:pool_light_round","x":0,"z":0},
            {"fixture":"core:pool_light_wall","x":1,"z":0,"mount":"wall","y":1},
            {"fixture":"core:pool_light_round","x":2,"z":0,"caustics":false},
            {"fixture":"core:fluorescent_panel_01","x":3,"z":0},
            {"fixture":"home:ceiling_light_round","x":4,"z":0},
            {"fixture":"core:pool_light_round","x":5,"z":0,"enabled":false,"brightness":0,"color":[0,0,0]}
        ],"global_illuminators":[
            {"id":"sun","kind":"sun","direction":[0,-1,0]},
            {"id":"moon","kind":"moon","direction":[0,-1,0]},
            {"id":"generic","direction":[0,-1,0]},
            {"id":"focused","direction":[0,-1,0],"caustics":true}
        ]}"#,
    )
    .map_err(|error| error.to_string())?;
    let expected = [true, true, false, false, false, true];
    assert!(
        level
            .ceiling_lights
            .iter()
            .zip(expected)
            .all(|(light, eligible)| light.caustics() == eligible),
        "only pool fixture families default to eligible, and explicit opt-out persists"
    );
    let mut fluorescent = level
        .ceiling_lights
        .get(3)
        .ok_or("missing fluorescent fixture")?
        .clone();
    fluorescent.caustics = Some(true);
    assert!(
        !fluorescent.caustics(),
        "a fluorescent override must never become eligible"
    );
    let generic: LightDef = serde_json::from_str("{}").map_err(|error| error.to_string())?;
    assert!(
        !generic.to_source([0.0; 3], 0.0, 1.0).caustics,
        "generic sources default to ineligible"
    );
    let focused: LightDef =
        serde_json::from_str(r#"{"caustics":true}"#).map_err(|error| error.to_string())?;
    assert!(
        focused.to_source([0.0; 3], 0.0, 1.0).caustics,
        "generic sources may explicitly opt in"
    );
    let forbidden: GlobalIlluminatorDef =
        serde_json::from_str(r#"{"id":"moon","kind":"moon","direction":[0,-1,0],"caustics":true}"#)
            .map_err(|error| error.to_string())?;
    assert!(!forbidden.caustics(), "moon never resolves to eligible");
    assert!(
        forbidden.validate().is_err(),
        "moon opt-in must produce a validation error"
    );

    let original = LevelLighting::bake(&level);
    let dark_source = original
        .fixture_light_index(5)
        .ok_or("missing initially dark source")?;
    assert!(
        original.runtime_caustic_local_factor(dark_source, [5.0, 0.0, 0.0]) > 0.0,
        "the prepared basis must survive initially disabled, zero and black source states"
    );
    let decoded = read_lighting(&write_lighting(&original)?)?;
    assert_eq!(
        original.lights().len(),
        decoded.lights().len(),
        "compilation preserves every local source"
    );
    assert!(
        original
            .lights()
            .iter()
            .zip(decoded.lights())
            .all(|(before, after)| before.source == after.source),
        "compiled local metadata includes source eligibility"
    );
    assert_eq!(
        original.global_lights(),
        decoded.global_lights(),
        "compiled global metadata includes source eligibility"
    );
    assert_eq!(
        decoded
            .global_lights()
            .iter()
            .map(|light| light.caustics)
            .collect::<Vec<_>>(),
        [true, false, false, true],
        "sun defaults on, moon and directional default off, directional opt-in persists"
    );
    Ok(())
}

#[test]
fn legacy_source_flags_hydrate_from_stable_fixture_and_prop_identities() -> Result<(), String> {
    // Reproduce the all-false source flags produced by legacy PLLT readers in
    // a tiny field, without coupling the regression to a shipped package.
    let level = LevelDef::from_json(
        r#"{"format_version":3,"id":"caustics_legacy","name":"Legacy metadata",
        "spawn":{"x":0,"z":0},"ceiling_lights":[
            {"fixture":"core:pool_light_wall","x":0,"z":0,"mount":"wall","y":1},
            {"fixture":"core:pool_light_round","x":1,"z":0,"caustics":false},
            {"fixture":"core:fluorescent_panel_01","x":2,"z":0}
        ],"props":[{"model":"metadata:source_owner","occludes":false,"lights":[{}, {"caustics":true,"enabled":false}]}]}"#,
    )
    .map_err(|error| error.to_string())?;
    let mut legacy_definition = level.clone();
    for fixture in &mut legacy_definition.ceiling_lights {
        fixture.caustics = Some(false);
    }
    for prop in &mut legacy_definition.props {
        for light in &mut prop.lights {
            light.caustics = Some(false);
        }
    }
    let mut legacy = LevelLighting::bake(&legacy_definition);
    assert!(
        legacy.lights().iter().all(|light| !light.source.caustics),
        "legacy readers initialize every local source as ineligible"
    );
    let previous_enabled = legacy
        .lights()
        .iter()
        .map(|light| light.source.enabled)
        .collect::<Vec<_>>();
    legacy.resolve_caustics_eligibility(&level);
    assert_eq!(
        legacy
            .lights()
            .iter()
            .map(|light| light.source.caustics)
            .collect::<Vec<_>>(),
        [true, true, false, false, false, true],
        "both wall apertures hydrate, pool opt-out and fluorescent stay off, prop source order preserves explicit opt-in"
    );
    assert_eq!(
        legacy
            .lights()
            .iter()
            .map(|light| light.source.enabled)
            .collect::<Vec<_>>(),
        previous_enabled,
        "hydration changes no current source enabled state"
    );
    Ok(())
}

#[test]
fn water_caustics_defaults_roundtrip_and_reject_nonfinite_or_unbounded_controls()
-> Result<(), String> {
    let defaults: CausticsDef = serde_json::from_str("{}").map_err(|error| error.to_string())?;
    assert_eq!(
        defaults,
        CausticsDef::default(),
        "omitted controls use the renderer defaults"
    );
    assert!(
        defaults.is_valid(),
        "default controls satisfy the bounded authoring contract"
    );
    let authored: CausticsDef =
        serde_json::from_str(r#"{"tint":[0.2,0.6,1],"speed":2,"strength":0.4}"#)
            .map_err(|error| error.to_string())?;
    let encoded = serde_json::to_string(&authored).map_err(|error| error.to_string())?;
    let decoded: CausticsDef = serde_json::from_str(&encoded).map_err(|error| error.to_string())?;
    assert_eq!(
        authored, decoded,
        "authored water controls survive semantic serialization"
    );
    for invalid in [
        CausticsDef {
            tint: [f32::NAN, 1.0, 1.0],
            ..defaults
        },
        CausticsDef {
            tint: [1.01, 1.0, 1.0],
            ..defaults
        },
        CausticsDef {
            speed: f32::INFINITY,
            ..defaults
        },
        CausticsDef {
            speed: -0.01,
            ..defaults
        },
        CausticsDef {
            speed: 8.01,
            ..defaults
        },
        CausticsDef {
            strength: -0.01,
            ..defaults
        },
        CausticsDef {
            strength: 1.01,
            ..defaults
        },
    ] {
        assert!(
            !invalid.is_valid(),
            "nonfinite or unbounded water controls must be rejected"
        );
    }
    Ok(())
}
