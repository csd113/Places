//! One tiny gate regression: untrusted hits/misses cannot replace CPU semantics.
use crate::engine::lighting::transport::diagnostics::gpu_oracle::{Oracle, Snapshot};

#[test]
fn gpu_candidate_certificates_preserve_cpu_any_hit_boundaries() -> Result<(), String> {
    let mut geometry = Vec::new();
    for z in [1.0_f32, 1.000_001, 2.0] {
        geometry.push([[-1.0, -1.0, z], [1.0, -1.0, z], [1.0, 1.0, z]]);
        geometry.push([[1.0, 1.0, z], [-1.0, -1.0, z], [-1.0, 1.0, z]]);
    }
    let mut rays = Vec::new();
    for x in [-1.000_001_f32, -1.0, 0.0, 1.0, 1.000_001] {
        for limit in [1.0_f32, 1.000_001, 3.0] {
            rays.push(([x, 0.0, 0.0], [0.0, 0.0, 1.0], limit));
        }
    }
    rays.push(([0.0, 0.0, 1.0], [0.0, 0.0, -1.0], 3.0));
    let snapshot = Snapshot::fixture(&geometry, rays)?;
    let mut stale = snapshot.clone();
    stale.canonical_ids.reverse();
    assert!(
        Oracle::new(stale).is_err(),
        "stale canonical identities must refuse"
    );
    let oracle = Oracle::new(snapshot)?;
    let mut has_hit = false;
    let mut has_miss = false;
    let mut certificates = 0_usize;
    for ray in &oracle.snapshot().rays {
        let reference = oracle.reference(*ray);
        has_hit |= reference;
        has_miss |= !reference;
        for candidate in [
            None,
            Some(0),
            Some(1),
            Some(2),
            Some(3),
            Some(4),
            Some(5),
            Some(u32::MAX),
        ] {
            for ambiguous in [false, true] {
                let resolved = oracle.resolve(*ray, candidate, ambiguous, false);
                assert_eq!(
                    resolved.0, reference,
                    "candidate hit/miss must preserve CPU result"
                );
                if !resolved.1 {
                    certificates = certificates.saturating_add(1);
                }
                if candidate.is_none() || ambiguous || candidate == Some(u32::MAX) {
                    assert!(
                        resolved.1,
                        "misses, ambiguity and invalid IDs always require CPU"
                    );
                }
                let control = oracle.resolve(*ray, candidate, ambiguous, true);
                assert_eq!(control, (reference, true), "verify-all always traces CPU");
            }
        }
    }
    assert!(
        has_hit && has_miss && certificates > 0,
        "exercise real hits, misses and accepted certificates"
    );
    Ok(())
}
