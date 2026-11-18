//! Animation channel, sampler and clip metadata decoding.

use super::accessor::{COMPONENT_FLOAT, accessor_view, json_f32, json_usize, read_vec};
use super::{
    AnimationInterpolation, AnimationPath, GltfError, PropAnimation, PropAnimationChannel,
};
use crate::engine::level::MAX_ANIMATION_CHANNELS;

/// Parses every animation clip in the document.
///
/// STEP, LINEAR and CUBICSPLINE transform channels and morph-weight channels
/// are all retained; unknown interpolations, unknown paths and mismatched
/// tuple counts are rejected by name. Every channel is validated up front
/// (strictly increasing finite times, finite values, matching counts) so the
/// sampler can index it without further checks.
pub(super) fn parse_animations(
    json: &serde_json::Value,
    binary: &[u8],
) -> Result<Vec<PropAnimation>, GltfError> {
    let Some(list) = json
        .get("animations")
        .and_then(|value| value.as_array())
        .filter(|list| !list.is_empty())
    else {
        return Ok(Vec::new());
    };
    let node_count = json
        .get("nodes")
        .and_then(|value| value.as_array())
        .map_or(0, std::vec::Vec::len);
    let mut animations: Vec<PropAnimation> = Vec::with_capacity(list.len());
    let mut channel_total = 0usize;
    for (animation_index, animation) in list.iter().enumerate() {
        let name = animation
            .get("name")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        let Some(channels_json) = animation.get("channels").and_then(|value| value.as_array())
        else {
            return Err(GltfError::new(format!(
                "animation {animation_index} declares no channels"
            )));
        };
        channel_total = channel_total.saturating_add(channels_json.len());
        if channel_total > MAX_ANIMATION_CHANNELS {
            return Err(GltfError::new(format!(
                "prop model declares more than {MAX_ANIMATION_CHANNELS} animation channels; \
                 the engine ceiling is {MAX_ANIMATION_CHANNELS}"
            )));
        }
        let samplers = animation
            .get("samplers")
            .and_then(|value| value.as_array())
            .ok_or_else(|| {
                GltfError::new(format!("animation {animation_index} has no samplers"))
            })?;
        let mut channels: Vec<PropAnimationChannel> = Vec::with_capacity(channels_json.len());
        for channel in channels_json {
            channels.push(parse_animation_channel(
                channel,
                samplers,
                animation_index,
                node_count,
                json,
                binary,
            )?);
        }
        let duration = channels
            .iter()
            .map(PropAnimationChannel::duration)
            .fold(0.0f32, f32::max);
        animations.push(PropAnimation {
            name,
            duration,
            channels,
            looped: true,
            reference_speed_mps: None,
            kind: None,
        });
    }
    apply_clip_metadata(json, &mut animations);
    Ok(animations)
}

/// Applies the `asset.extras.places_entity_clips` marker to parsed clips.
///
/// The marker is optional application data, so a malformed or missing marker
/// never fails the model: every clip keeps its defaults (looping, no declared
/// reference speed). A marker entry is matched by name, case-insensitively.
fn apply_clip_metadata(json: &serde_json::Value, animations: &mut [PropAnimation]) {
    let Some(marker) = json
        .get("asset")
        .and_then(|asset| asset.get("extras"))
        .and_then(|extras| extras.get("places_entity_clips"))
    else {
        return;
    };
    // The spooner-man marker carries the walk reference speed at the
    // top level; a per-clip entry overrides it.
    let marker_walk_speed = marker
        .get("walk_reference_speed")
        .and_then(json_f32)
        .filter(|value| value.is_finite() && *value > 0.0);
    let Some(clips) = marker.get("clips").and_then(|value| value.as_array()) else {
        return;
    };
    for entry in clips {
        let Some(name) = entry.get("name").and_then(|value| value.as_str()) else {
            continue;
        };
        let Some(animation) = animations
            .iter_mut()
            .find(|animation| animation.name.eq_ignore_ascii_case(name))
        else {
            continue;
        };
        if let Some(looped) = entry.get("loop").and_then(serde_json::Value::as_bool) {
            animation.looped = looped;
        }
        let speed = entry
            .get("reference_speed_mps")
            .and_then(json_f32)
            .filter(|value| value.is_finite() && *value > 0.0);
        animation.reference_speed_mps = speed.or_else(|| {
            if animation.name.eq_ignore_ascii_case("walk") {
                marker_walk_speed
            } else {
                None
            }
        });
        animation.kind = entry
            .get("kind")
            .and_then(|value| value.as_str())
            .map(str::to_string);
    }
}

/// Parses and validates one animation channel.
fn parse_animation_channel(
    channel: &serde_json::Value,
    samplers: &[serde_json::Value],
    animation_index: usize,
    node_count: usize,
    json: &serde_json::Value,
    binary: &[u8],
) -> Result<PropAnimationChannel, GltfError> {
    let target = channel
        .get("target")
        .ok_or_else(|| GltfError::new("animation channel has no target"))?;
    let node = target
        .get("node")
        .and_then(json_usize)
        .ok_or_else(|| GltfError::new("animation channel target has no node"))?;
    if node >= node_count {
        return Err(GltfError::new(format!(
            "animation channel targets node {node}, which does not exist"
        )));
    }
    let path = parse_animation_path(target)?;
    let morph_count = if path == AnimationPath::Weights {
        morph_target_count(json, node)?
    } else {
        0
    };
    let sampler_index = channel
        .get("sampler")
        .and_then(json_usize)
        .ok_or_else(|| GltfError::new("animation channel has no sampler index"))?;
    let sampler = samplers.get(sampler_index).ok_or_else(|| {
        GltfError::new(format!(
            "animation channel references sampler {sampler_index}, which does not exist"
        ))
    })?;
    let interpolation = parse_animation_interpolation(sampler)?;
    let times = parse_animation_times(json, binary, sampler, animation_index)?;
    let values_per_key = match path {
        AnimationPath::Translation | AnimationPath::Scale => 3,
        AnimationPath::Rotation => 4,
        AnimationPath::Weights => morph_count,
    };
    // The output accessor's element width is the path's component count for a
    // transform (SCALAR components for weights).
    let element_width = match path {
        AnimationPath::Weights => 1,
        AnimationPath::Translation | AnimationPath::Rotation | AnimationPath::Scale => {
            values_per_key
        }
    };
    let output_accessor = sampler
        .get("output")
        .and_then(json_usize)
        .ok_or_else(|| GltfError::new("animation sampler has no output accessor"))?;
    let output_view = accessor_view(json, binary, output_accessor, element_width)?;
    if output_view.component_type != COMPONENT_FLOAT {
        return Err(GltfError::new("animation sampler values must be float32"));
    }
    let tuples_per_key = match interpolation {
        AnimationInterpolation::CubicSpline => 3,
        AnimationInterpolation::Linear | AnimationInterpolation::Step => 1,
    };
    let rows_per_key = times.len().saturating_mul(tuples_per_key).saturating_mul(
        if path == AnimationPath::Weights {
            morph_count
        } else {
            1
        },
    );
    if output_view.count != rows_per_key {
        return Err(GltfError::new(format!(
            "animation sampler has {} keyframes and {} value rows, expected {rows_per_key}",
            times.len(),
            output_view.count
        )));
    }
    let raw_values = read_vec(json, binary, output_accessor, element_width)?;
    let values = flatten_animation_values(&raw_values, values_per_key)?;
    Ok(PropAnimationChannel {
        node: u16::try_from(node).map_err(|error| {
            GltfError::new(format!(
                "animation node index does not fit in 16 bits: {error}"
            ))
        })?,
        path,
        interpolation,
        times,
        values,
        values_per_key,
    })
}

/// The transform path one channel targets.
fn parse_animation_path(target: &serde_json::Value) -> Result<AnimationPath, GltfError> {
    match target.get("path").and_then(|value| value.as_str()) {
        Some("translation") => Ok(AnimationPath::Translation),
        Some("rotation") => Ok(AnimationPath::Rotation),
        Some("scale") => Ok(AnimationPath::Scale),
        Some("weights") => Ok(AnimationPath::Weights),
        Some(other) => Err(GltfError::new(format!(
            "animation path '{other}' is not supported"
        ))),
        None => Err(GltfError::new("animation channel target has no path")),
    }
}

/// The interpolation one sampler declares (LINEAR when omitted).
fn parse_animation_interpolation(
    sampler: &serde_json::Value,
) -> Result<AnimationInterpolation, GltfError> {
    match sampler
        .get("interpolation")
        .and_then(|value| value.as_str())
    {
        None | Some("LINEAR") => Ok(AnimationInterpolation::Linear),
        Some("STEP") => Ok(AnimationInterpolation::Step),
        Some("CUBICSPLINE") => Ok(AnimationInterpolation::CubicSpline),
        Some(other) => Err(GltfError::new(format!(
            "animation interpolation '{other}' is not supported"
        ))),
    }
}

/// The morph-target count of the mesh a `weights` channel targets.
fn morph_target_count(json: &serde_json::Value, node: usize) -> Result<usize, GltfError> {
    let nodes = json
        .get("nodes")
        .and_then(|value| value.as_array())
        .ok_or_else(|| GltfError::new("file has no nodes"))?;
    let target_node = nodes
        .get(node)
        .ok_or_else(|| GltfError::new("animation weight channel targets a missing node"))?;
    let mesh_index = target_node
        .get("mesh")
        .and_then(json_usize)
        .ok_or_else(|| GltfError::new("animation weight channel targets a node with no mesh"))?;
    let meshes = json
        .get("meshes")
        .and_then(|value| value.as_array())
        .ok_or_else(|| GltfError::new("file has no meshes"))?;
    let mesh = meshes
        .get(mesh_index)
        .ok_or_else(|| GltfError::new("animation weight channel targets a missing mesh"))?;
    let count = mesh
        .get("primitives")
        .and_then(|value| value.as_array())
        .and_then(|primitives| primitives.first())
        .and_then(|primitive| primitive.get("targets"))
        .and_then(|value| value.as_array())
        .map_or(0, std::vec::Vec::len);
    if count == 0 {
        return Err(GltfError::new(
            "animation weight channel targets a mesh with no morph targets",
        ));
    }
    Ok(count)
}

/// Flattens raw key tuple rows into `values_per_key` floats per row.
fn flatten_animation_values(
    raw_values: &[Vec<f32>],
    values_per_key: usize,
) -> Result<Vec<f32>, GltfError> {
    let mut values: Vec<f32> = Vec::with_capacity(raw_values.len().saturating_mul(values_per_key));
    for raw in raw_values {
        if raw.len() == values_per_key {
            for value in raw {
                if !value.is_finite() {
                    return Err(GltfError::new(
                        "animation sampler contains a non-finite value",
                    ));
                }
                values.push(*value);
            }
        } else if raw.len() == 1 {
            let value = raw
                .first()
                .copied()
                .ok_or_else(|| GltfError::new("animation weight tuple declares no components"))?;
            if !value.is_finite() {
                return Err(GltfError::new(
                    "animation sampler contains a non-finite value",
                ));
            }
            values.push(value);
        } else {
            return Err(GltfError::new(format!(
                "animation sampler tuple has {} components, expected {values_per_key}",
                raw.len()
            )));
        }
    }
    Ok(values)
}

/// Reads and validates a sampler's keyframe times: float32, finite and
/// strictly increasing.
fn parse_animation_times(
    json: &serde_json::Value,
    binary: &[u8],
    sampler: &serde_json::Value,
    animation_index: usize,
) -> Result<Vec<f32>, GltfError> {
    let input_accessor = sampler
        .get("input")
        .and_then(json_usize)
        .ok_or_else(|| GltfError::new("animation sampler has no input accessor"))?;
    let input_view = accessor_view(json, binary, input_accessor, 1)?;
    if input_view.component_type != COMPONENT_FLOAT {
        return Err(GltfError::new(
            "animation sampler input times must be float32",
        ));
    }
    let raw_times = read_vec(json, binary, input_accessor, 1)?;
    if raw_times.is_empty() {
        return Err(GltfError::new(format!(
            "animation {animation_index} sampler has no keyframes"
        )));
    }
    let mut times: Vec<f32> = Vec::with_capacity(raw_times.len());
    for raw in &raw_times {
        let time = raw.first().copied().ok_or_else(|| {
            GltfError::new("animation keyframe time accessor declares no components")
        })?;
        if !time.is_finite() {
            return Err(GltfError::new(
                "animation sampler contains a non-finite keyframe time",
            ));
        }
        times.push(time);
    }
    for pair in times.windows(2) {
        let [before, after] = pair else { continue };
        if after <= before {
            return Err(GltfError::new(
                "animation keyframe times must be strictly increasing",
            ));
        }
    }
    Ok(times)
}
