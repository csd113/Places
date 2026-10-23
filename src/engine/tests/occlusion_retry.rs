//! A cancelled cold model lookup used to cache its placeholder as permanent
//! derived occlusion. One sub-KiB, untextured triangle protects successful retry;
//! no shipped assets, GPU, transport or map bake; under 64 KiB of working data.
use super::PropOcclusionCache;
use crate::engine::{
    loader::PropCatalog,
    props::PropAssets,
    resources::{DecodeBudget, enter},
};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Fixture(std::path::PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _removed = std::fs::remove_dir_all(&self.0);
    }
}

fn fixture() -> Result<Fixture, String> {
    let root = std::env::temp_dir().join(format!(
        "places-occlusion-retry-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).map_err(|error| error.to_string())?;
    let fixture = Fixture(root);
    let source = serde_json::json!({"asset":{"version":"2.0"},"buffers":[{"byteLength":66_u32}],"bufferViews":[{"buffer":0_u32,"byteOffset":0_u32,"byteLength":36_u32},{"buffer":0_u32,"byteOffset":36_u32,"byteLength":24_u32},{"buffer":0_u32,"byteOffset":60_u32,"byteLength":6_u32}],"accessors":[{"bufferView":0_u32,"componentType":5126_u32,"count":3_u32,"type":"VEC3"},{"bufferView":1_u32,"componentType":5126_u32,"count":3_u32,"type":"VEC2"},{"bufferView":2_u32,"componentType":5123_u32,"count":3_u32,"type":"SCALAR"}],"meshes":[{"primitives":[{"attributes":{"POSITION":0_u32,"TEXCOORD_0":1_u32},"indices":2_u32}]}],"nodes":[{"mesh":0_u32}],"scenes":[{"nodes":[0_u32]}],"scene":0_u32});
    let mut json = serde_json::to_vec(&source).map_err(|error| error.to_string())?;
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let mut binary = Vec::new();
    for value in [
        0.0_f32, 0.0_f32, 0.0_f32, 0.25_f32, 0.25_f32, 0.0_f32, 0.0_f32, 0.5_f32, 0.25_f32,
    ] {
        binary.extend(value.to_le_bytes());
    }
    for value in [0.0_f32, 0.0_f32, 1.0_f32, 0.0_f32, 0.0_f32, 1.0_f32] {
        binary.extend(value.to_le_bytes());
    }
    for value in [0_u16, 1_u16, 2_u16] {
        binary.extend(value.to_le_bytes());
    }
    while !binary.len().is_multiple_of(4) {
        binary.push(0);
    }
    let mut bytes = b"glTF".to_vec();
    bytes.extend(2_u32.to_le_bytes());
    let length = 28_usize
        .saturating_add(json.len())
        .saturating_add(binary.len());
    bytes.extend(
        u32::try_from(length)
            .map_err(|error| error.to_string())?
            .to_le_bytes(),
    );
    bytes.extend(
        u32::try_from(json.len())
            .map_err(|error| error.to_string())?
            .to_le_bytes(),
    );
    bytes.extend(0x4e4f_534a_u32.to_le_bytes());
    bytes.extend(json);
    bytes.extend(
        u32::try_from(binary.len())
            .map_err(|error| error.to_string())?
            .to_le_bytes(),
    );
    bytes.extend(0x004e_4942_u32.to_le_bytes());
    bytes.extend(binary);
    std::fs::write(fixture.0.join("tiny.glb"), bytes).map_err(|error| error.to_string())?;
    Ok(fixture)
}

#[test]
fn cancelled_cold_occlusion_can_retry_real_geometry() -> Result<(), String> {
    let fixture = fixture()?;
    let decoded = PropAssets::with_root(&fixture.0).resolve("tiny.glb")?;
    assert_eq!(
        decoded.model.vertices.len(),
        3,
        "the tiny input must be a valid source model"
    );
    let mut cache = PropOcclusionCache {
        catalog: PropCatalog::builtin(),
        assets: PropAssets::with_root(&fixture.0),
        models: HashMap::new(),
    };
    {
        let _scope = enter(Some(DecodeBudget::new(Arc::new(AtomicBool::new(true)))));
        let interrupted = cache.model_occlusion("tiny.glb", 0.15_f32);
        assert!(!interrupted.loaded, "cancelled cold input must not be read");
    }
    let retried = cache.model_occlusion("tiny.glb", 0.15_f32);
    assert!(
        retried.loaded && !retried.boxes.is_empty() && retried.vertex_count == 3,
        "retry must derive actual model occlusion rather than inherit the cancelled placeholder: {retried:?}"
    );
    Ok(())
}
