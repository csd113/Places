//! Explicit developer parity/latency campaign; never automatic tests or baking.
use super::{LevelBuild, LevelDef, PropAssets, PropCatalog, TextureCache, prepare, reuse, timing};
use crate::engine::render::{build_authoring_environment_preview, build_authoring_preview};

fn identity(build: &LevelBuild) -> Result<(String, String, String), String> {
    let mut lighting = crate::engine::package::binary::Writer::new();
    build.lighting.write_compiled(&mut lighting)?;
    Ok((
        crate::engine::package::hash::sha256_hex(&crate::engine::package::mesh::write_mesh(
            &build.mesh,
        )?),
        crate::engine::package::hash::sha256_hex(&crate::engine::package::props::write_props(
            &build.batches,
        )?),
        crate::engine::package::hash::sha256_hex(lighting.as_slice()),
    ))
}

pub(in crate::editor) fn run(original: &LevelDef) -> Result<serde_json::Value, String> {
    let catalog = PropCatalog::load_default();
    let mut assets = PropAssets::load_default();
    let mut textures = TextureCache::default();
    let mut reuse = reuse::Reuse::default();
    let mut rows = Vec::new();
    // Mirror the native renderer: keep the last complete display alive during preparation.
    let mut displayed = None;
    for (case, environment) in [
        ("cold", false),
        ("noop", false),
        ("wall", false),
        ("metadata", false),
        ("undo", false),
        ("cold-environment", true),
        ("noop-environment", true),
        ("metadata-environment", true),
        ("light", true),
        ("material", true),
        ("return-inspect", false),
        ("prop", false),
        ("environment-base", true),
        ("environment-prop-move", true),
        ("environment-prop-rotate", true),
        ("environment-prop-scale", true),
        ("environment-prop-add", true),
        ("environment-prop-delete", true),
        ("environment-structural", true),
        ("environment-circuit-off", true),
        ("environment-restored", true),
    ] {
        let level = edited_level(original, case)?;
        let prepared = prepare(
            level,
            environment,
            &catalog,
            &mut assets,
            &mut textures,
            &mut reuse,
            None,
        )?;
        // The candidate now replaces the old display; the fresh legacy oracle
        // needs only the current candidate alive, avoiding a third full build.
        drop(displayed.take());
        let cpu_start = timing::cpu_ms();
        let started = std::time::Instant::now();
        let reference = if environment {
            build_authoring_environment_preview(
                &prepared.loaded.level,
                &catalog,
                &mut assets,
                &prepared.loaded.materials,
            )
        } else {
            build_authoring_preview(
                &prepared.loaded.level,
                &catalog,
                &mut assets,
                &prepared.loaded.materials,
            )
        };
        let baseline_wall_ms = started.elapsed().as_secs_f64() * 1_000.0_f64;
        let baseline_thread_cpu_ms = cpu_start
            .zip(timing::cpu_ms())
            .map(|(start, end)| end - start);
        let expected = identity(&reference)?;
        let actual = identity(&prepared.build)?;
        if actual != expected {
            return Err(format!(
                "Preview parity mismatch in {case}: reference {expected:?}, prepared {actual:?}"
            ));
        }
        rows.push(serde_json::json!({"case":case,"environment":environment,"baseline_build_wall_ms":baseline_wall_ms,"baseline_build_thread_cpu_ms":baseline_thread_cpu_ms,"candidate":prepared.metrics,"mesh_sha256":actual.0,"props_sha256":actual.1,"lighting_sha256":actual.2,"exact":true}));
        drop(displayed.replace(prepared));
    }
    Ok(serde_json::json!({"level":original.id,"rows":rows,"all_exact":true,"no_gpu_or_bake":true}))
}

fn edited_level(original: &LevelDef, case: &str) -> Result<LevelDef, String> {
    let mut level = original.clone();
    match case {
        "wall" | "metadata" => {
            if let Some(wall) = level.walls.first_mut() {
                wall.x += 0.125_f32;
            }
            if case == "metadata" {
                level.name.push_str(" edited");
            }
        }
        "metadata-environment" => level.name.push_str(" edited"),
        "light" => {
            if let Some(light) = level.ceiling_lights.first_mut() {
                light.brightness = Some(light.brightness.unwrap_or(1.0_f32) * 0.8_f32);
            }
        }
        "material" => {
            if let Some(room) = level.rooms.first_mut() {
                room.material = Some("core:carpet_beige_01".into());
            }
        }
        "environment-prop-move"
        | "environment-prop-rotate"
        | "environment-prop-scale"
        | "environment-prop-add"
        | "environment-prop-delete" => {
            if let Some(index) = level
                .props
                .iter()
                .position(|prop| !prop.occludes && prop.components.is_empty())
                .or_else(|| (!level.props.is_empty()).then_some(0))
            {
                match case {
                    "environment-prop-move" => {
                        if let Some(prop) = level.props.get_mut(index) {
                            prop.x += 0.125_f32;
                        }
                    }
                    "environment-prop-rotate" => {
                        if let Some(prop) = level.props.get_mut(index) {
                            prop.rotation_degrees += 13.0_f32;
                        }
                    }
                    "environment-prop-scale" => {
                        if let Some(prop) = level.props.get_mut(index) {
                            prop.scale *= 0.8_f32;
                        }
                    }
                    "environment-prop-add" => {
                        if let Some(prop) = level.props.get(index) {
                            let mut added = prop.clone();
                            added.id = None;
                            added.x += 1.0_f32;
                            level.props.push(added);
                        }
                    }
                    "environment-prop-delete" => {
                        drop(level.props.remove(index));
                    }
                    _ => return Err("Unknown prop edit".into()),
                }
            }
        }
        "environment-structural" => {
            if let Some(wall) = level.walls.first_mut() {
                wall.width += 0.25_f32;
            }
        }
        "environment-circuit-off" => {
            if let Some(fixture) = level.ceiling_lights.first_mut() {
                fixture.enabled = false;
            }
        }
        "prop" => {
            if let Some(prop) = level.props.first_mut() {
                prop.x += 0.125_f32;
            }
        }
        "cold"
        | "noop"
        | "undo"
        | "cold-environment"
        | "noop-environment"
        | "return-inspect"
        | "environment-base"
        | "environment-restored" => {}
        _ => return Err("Unknown preview audit case".into()),
    }
    Ok(level)
}
