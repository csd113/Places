//! Explicit native import/pose evidence. Never a Cargo test or map bake.
mod replay;
mod state_replay;
use glam::{Mat4, Quat, Vec3};
use places::engine::props::{LoadedPropAsset, PropAssets, resolve_prop_root};
use places::engine::render::CharacterAnimator;
use serde::Serialize;

use std::io::Write;
use std::path::{Path, PathBuf};

const MODEL: &str = "entities/tall_guest/model/tall_guest.glb";
const FROZEN: &str = "74bc2d7882f1952ac2a97db4a3e5d4cab4e3f769e82c362b4fff866625611be1";
const CLIPS: [(&str, f32); 9] = [
    ("idle_listen", 3.0),
    ("walk_slow", 1.8),
    ("run_pursuit", 0.8),
    ("howl_echo", 3.2),
    ("inspect_prop", 4.8),
    ("door_enter", 1.2),
    ("door_squeeze", 1.8),
    ("door_exit", 1.2),
    ("lost_prey_agony", 4.0),
];
#[derive(Clone, Serialize)]
struct Spec {
    kind: &'static str,
    clip: &'static str,
    fraction: f32,
    from: Option<(&'static str, f32, f32)>,
    upper: f32,
    direct: Option<(f32, f32)>,
}
impl Spec {
    const fn new(kind: &'static str, clip: &'static str, fraction: f32) -> Self {
        Self {
            kind,
            clip,
            fraction,
            from: None,
            upper: 0.0,
            direct: None,
        }
    }
}
fn specifications() -> Vec<Spec> {
    let mut result = Vec::new();
    for (clip, seconds) in CLIPS {
        for frame in 0_u16..=300 {
            let time = f32::from(frame) / 60.0;
            if time > seconds + 0.0001 {
                break;
            }
            result.push(Spec::new("authored", clip, (time / seconds).min(1.0)));
        }
    }
    for (clip, _) in CLIPS {
        for (from, _) in CLIPS {
            if clip == from {
                continue;
            }
            for phase in [0.0, 0.5, 1.0] {
                for step in 0_u8..=10 {
                    let mut spec = Spec::new("blend", clip, phase);
                    spec.from = Some((from, phase, f32::from(step) / 10.0));
                    result.push(spec);
                }
            }
        }
    }
    for (clip, frames) in [("idle_listen", 60_u16), ("run_pursuit", 48)] {
        for frame in 0_u16..=frames {
            for step in 0_u8..=16 {
                let mut spec = Spec::new(
                    "distributed_candidate",
                    clip,
                    f32::from(frame) / f32::from(frames),
                );
                spec.upper = f32::from(step) / 20.0;
                result.push(spec);
            }
        }
    }
    for (clip, frame, seconds) in [
        ("idle_listen", 1_u8, 3.0),
        ("idle_listen", 46, 3.0),
        ("run_pursuit", 1, 0.8),
        ("run_pursuit", 9, 0.8),
        ("run_pursuit", 17, 0.8),
    ] {
        for yaw in [-20.0, 0.0, 20.0] {
            for pitch in [-10.0, 0.0, 10.0] {
                let mut spec = Spec::new(
                    "isolated_direct_candidate",
                    clip,
                    (f32::from(frame) - 1.0) / (30.0 * seconds),
                );
                spec.direct = Some((yaw, pitch));
                result.push(spec);
            }
        }
    }
    let mut control = Spec::new("rejected_wide_positive_control", "idle_listen", 0.0);
    control.direct = Some((-55.0, 30.0));
    result.push(control);
    result
}
fn direct_candidate(
    animator: &mut CharacterAnimator,
    yaw: f32,
    pitch: f32,
) -> Result<serde_json::Value, String> {
    let parent = animator.node_global("neck").ok_or("Missing neck")?;
    let head = animator.node_global("head").ok_or("Missing head")?;
    let local = parent.inverse().mul_mat4(&head);
    let (scale, authored, translation) = local.to_scale_rotation_translation();
    let expected = authored
        .mul_quat(Quat::from_rotation_y(yaw.to_radians()))
        .mul_quat(Quat::from_rotation_x(pitch.to_radians()));
    let posed = parent.mul_mat4(&Mat4::from_scale_rotation_translation(
        scale,
        expected,
        translation,
    ));
    if !animator.set_node_global("head", posed) {
        return Err("Native rig refused candidate".into());
    }
    let applied_global = animator.node_global("head").ok_or("Missing applied head")?;
    let (_, applied, _) = parent
        .inverse()
        .mul_mat4(&applied_global)
        .to_scale_rotation_translation();
    Ok(
        serde_json::json!({"authored_local":authored,"expected_local":expected,"applied_local":applied,"rotation_error":1.0-applied.dot(expected).abs(),"production_overlay_enabled":false}),
    )
}
fn sample(animator: &mut CharacterAnimator, spec: &Spec) -> Result<serde_json::Value, String> {
    if let Some((from, fraction, weight)) = spec.from {
        let _changed =
            animator.sample_blended_fraction(spec.clip, spec.fraction, from, fraction, weight);
    } else {
        let _changed = animator.sample_fraction(spec.clip, spec.fraction);
    }
    if spec.upper > 0.0 {
        let _upper = animator.blend_upper_inspection(spec.upper);
    }
    match spec.direct {
        Some((yaw, pitch)) => direct_candidate(animator, yaw, pitch),
        None => Ok(serde_json::Value::Null),
    }
}
fn write_vertices(
    writer: &mut impl Write,
    asset: &LoadedPropAsset,
    animator: &CharacterAnimator,
) -> Result<([f32; 3], [f32; 3]), String> {
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for ((vertex, joints), weights) in asset
        .model
        .vertices
        .iter()
        .zip(&asset.model.joints)
        .zip(&asset.model.weights)
    {
        let point = animator.skin_position(*joints, *weights, vertex.pos);
        let vector = Vec3::from_array(point);
        if !vector.is_finite() {
            return Err("Nonfinite native skin result".into());
        }
        lo = lo.min(vector);
        hi = hi.max(vector);
        for value in point {
            writer
                .write_all(&value.to_le_bytes())
                .map_err(|e| e.to_string())?;
        }
    }
    Ok((lo.to_array(), hi.to_array()))
}
fn run(out: &Path) -> Result<(), String> {
    let started = std::time::Instant::now();
    let root = resolve_prop_root().ok_or("Missing asset root")?;
    let bytes = std::fs::read(root.join(MODEL)).map_err(|e| e.to_string())?;
    let hash = places::engine::package::hash::sha256_hex(&bytes);
    if hash != FROZEN {
        return Err("Frozen asset identity mismatch".into());
    }
    let asset = PropAssets::load_default().resolve(MODEL)?;
    let mut animator = CharacterAnimator::new(&asset.model).ok_or("Missing native animator")?;
    for (name, seconds) in CLIPS {
        if animator
            .clip_duration(name)
            .is_none_or(|n| (n - seconds).abs() > 0.001)
        {
            return Err(format!("Missing/mistimed clip {name}"));
        }
    }
    std::fs::create_dir(out).map_err(|e| e.to_string())?;
    let file = std::fs::File::create(out.join("positions.f32")).map_err(|e| e.to_string())?;
    let mut writer = std::io::BufWriter::with_capacity(1_048_576, file);
    let vertex_count = u64::try_from(asset.model.vertices.len()).map_err(|e| e.to_string())?;
    let stride = vertex_count
        .checked_mul(12)
        .ok_or("Pose byte count overflow")?;
    let mut offset = 0_u64;
    let mut frames = Vec::new();
    for spec in specifications() {
        let direct = sample(&mut animator, &spec)?;
        let bounds = write_vertices(&mut writer, &asset, &animator)?;
        let globals = animator.named_globals();
        frames.push(serde_json::json!({"spec":spec,"offset_bytes":offset,"bounds":bounds,"direct":direct,"globals":globals}));
        offset = offset.checked_add(stride).ok_or("Pose bank overflow")?;
    }
    writer.flush().map_err(|e| e.to_string())?;
    let manifest = serde_json::json!({"scope":"Native Places GLB loader, interpolation, hierarchy and CPU skinning. This explicit geometry bank does not substitute for live GPU motion, collision, audio or performance acceptance.","glb_sha256":hash,"vertices":asset.model.vertices.iter().map(|v|v.pos).collect::<Vec<_>>(),"indices":asset.model.indices,"vertex_count":vertex_count,"positions_bytes":offset,"seconds":started.elapsed().as_secs_f64(),"frames":frames});
    let json = serde_json::to_vec(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(out.join("manifest.json"), json).map_err(|e| e.to_string())?;
    writeln!(
        std::io::stdout().lock(),
        "Native pose bank: {} samples, {offset} bytes, {:.2} seconds",
        frames.len(),
        started.elapsed().as_secs_f64()
    )
    .map_err(|e| e.to_string())
}
fn main() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let first = args.next().ok_or("Usage: places-tall-guest-pose-check <new-output-directory> or --impact-replay <fixture> <contacts> <new-output-file>")?;
    if first == "--state-replay" {
        let fixture = PathBuf::from(args.next().ok_or("Missing state replay fixture")?);
        let output = PathBuf::from(args.next().ok_or("Missing state replay output")?);
        if args.next().is_some() {
            return Err("Unexpected state replay argument".into());
        }
        return state_replay::run(&fixture, &output);
    }
    if first == "--impact-replay" {
        let fixture = PathBuf::from(args.next().ok_or("Missing replay fixture")?);
        let contacts = PathBuf::from(args.next().ok_or("Missing contact reports")?);
        let output = PathBuf::from(args.next().ok_or("Missing replay output")?);
        if args.next().is_some() {
            return Err("Unexpected replay argument".into());
        }
        return replay::run(&fixture, &contacts, &output);
    }
    let out = PathBuf::from(first);
    if args.next().is_some() || out.exists() {
        return Err("Use exactly one new output directory".into());
    }
    run(&out)
}
