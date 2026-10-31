//! Asset-free explicit numerical/thin/overlap fixture, never a shipping map.
use places::engine::lighting::transport::diagnostics::gpu_oracle::{MAX_RAYS, Snapshot};
pub fn snapshot() -> Result<Snapshot, String> {
    let mut geometry = Vec::new();
    for z in [1.0_f32, 1.000_001, 2.0] {
        geometry.push([[-1.0, -1.0, z], [1.0, -1.0, z], [1.0, 1.0, z]]);
        geometry.push([[1.0, 1.0, z], [-1.0, -1.0, z], [-1.0, 1.0, z]]);
    }
    geometry.push([
        [-0.000_001, -1.0, 0.5],
        [0.000_001, -1.0, 0.5],
        [0.0, 1.0, 0.5],
    ]);
    geometry.push([
        [1_000_000.0, 0.0, 1.0],
        [1_000_002.0, 0.0, 1.0],
        [1_000_000.0, 2.0, 1.0],
    ]);
    let mut rays = Vec::new();
    for x in [
        -1.000_001_f32,
        -1.0,
        -0.000_001,
        0.0,
        0.000_001,
        1.0,
        1.000_001,
    ] {
        for limit in [0.5_f32, 1.0, 1.000_001, 3.0] {
            rays.push(([x, 0.0, 0.0], [0.0, 0.0, 1.0], limit));
            rays.push(([x, 0.0, 3.0], [0.0, 0.0, -1.0], limit));
        }
    }
    rays.push(([1_000_000.0, 0.0, 0.0], [0.0, 0.0, 1.0], 3.0));
    rays.push(([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], 3.0));
    rays.push(([0.0, 0.0, 0.0], [1.0e-30, 0.0, 1.0], 3.0));
    let repeated = rays.iter().copied().cycle().take(MAX_RAYS).collect();
    Snapshot::fixture(&geometry, repeated)
}
