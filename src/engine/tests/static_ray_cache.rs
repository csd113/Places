//! Essential cache regression: exact directed/kinded rays, bounded overflow and
//! resource revisions must never reuse a stale visibility result. Two thousand
//! scalar entries cost under 256 KiB, without assets, a map bake or GPU work.
use super::{EntityStaticRayCache, static_ray};
use crate::engine::spatial::Aabb;
use std::cell::{Cell, RefCell};

#[test]
fn static_rays_preserve_direction_kind_capacity_and_resource_revision() {
    let mut storage = EntityStaticRayCache::default();
    let bounds = Aabb {
        min: [0.0; 3],
        max: [1.0; 3],
    };
    storage.prepare(None, bounds, 1);
    let calls = Cell::new(0_u32);
    {
        let cache = RefCell::new(Some(&mut storage));
        let query = || {
            calls.set(calls.get().saturating_add(1));
            0.25_f32
        };
        for _ in 0_u8..3 {
            assert_eq!(
                static_ray(&cache, 0, 7, [0.0; 3], [1.0; 3], query).to_bits(),
                0.25_f32.to_bits(),
                "identical rays preserve their exact result"
            );
        }
        assert_eq!(calls.get(), 1, "immutable repeated rays execute once");
        let _reverse = static_ray(&cache, 0, 7, [1.0; 3], [0.0; 3], query);
        let _kind = static_ray(&cache, 1, 7, [0.0; 3], [1.0; 3], query);
        let _label = static_ray(&cache, 0, 8, [0.0; 3], [1.0; 3], query);
        assert_eq!(
            calls.get(),
            4,
            "direction, query ownership and room labels remain distinct"
        );
        for tag in 0..2050 {
            let _value = static_ray(&cache, 2, tag, [0.0; 3], [1.0; 3], query);
        }
        assert_eq!(
            cache.borrow().as_ref().map(|value| value.rays.len()),
            Some(2048),
            "the receiver cache cannot grow past admission"
        );
        let before = calls.get();
        for _ in 0_u8..2 {
            let _overflow = static_ray(&cache, 3, 9999, [0.0; 3], [1.0; 3], query);
        }
        assert_eq!(
            calls.get(),
            before.saturating_add(2),
            "over-cap misses compute normally; no lighting is omitted"
        );
    }
    storage.prepare(None, bounds, 1);
    assert_eq!(
        storage.rays.len(),
        2048,
        "unchanged static context retains answers"
    );
    storage.prepare(None, bounds, 2);
    assert!(
        storage.rays.is_empty(),
        "in-place resource/light revisions invalidate answers"
    );
}
