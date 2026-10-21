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
        assert!(
            cache
                .borrow()
                .as_ref()
                .is_some_and(|entry| !entry.visibility_affected(Aabb {
                    min: [2.0; 3],
                    max: [3.0; 3]
                })),
            "off-path motion preserves a complete receiver"
        );
        assert!(
            cache
                .borrow()
                .as_ref()
                .is_some_and(|entry| entry.visibility_affected(Aabb {
                    min: [0.4; 3],
                    max: [0.6; 3]
                })),
            "an entering caster invalidates actual sampled paths"
        );
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
        assert!(
            cache
                .borrow()
                .as_ref()
                .is_some_and(|entry| entry.visibility_affected(Aabb {
                    min: [2.0; 3],
                    max: [3.0; 3]
                })),
            "overflow cannot certify omitted paths"
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
        "in-place resource revisions invalidate answers"
    );
}

// Essential energy/visibility dependency regression: switches must retain
// immutable answers while exact HDR anchors match a complete uncached refresh.
// Eight synthetic probes and one fixture cost milliseconds; no solve or assets.
#[test]
fn source_only_switches_reuse_static_rays_with_exact_full_refresh_output() -> Result<(), String> {
    use crate::engine::{
        level::LevelDef,
        lighting::{
            LevelLighting,
            lightmap::LightmapTexel,
            probes::{ProbeDirectField, ProbeField, ProbeSample, ProbeSwitchableField},
        },
    };
    let level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"switch_dependency","name":"Switch dependency",
        "spawn":{"x":0,"z":0},"rooms":[{"x":-2,"z":-2,"width":4,"depth":4,"height":3}],
        "ceiling_lights":[{"id":"lamp","fixture":"core:pool_light_round","x":0,"z":0,"y":2.5,"switchable":true}]}"#)
        .map_err(|error| error.to_string())?;
    let mut lighting = LevelLighting::bake(&level);
    let field = ProbeField {
        min: [-1.0, 0.0, -1.0],
        cell_m: 1.0,
        dims: [2; 3],
        probes: vec![
            ProbeSample {
                irradiance: [0.3; 3],
                direction: [0.0; 3],
                axis: [0.5; 2],
                room: 0
            };
            8
        ],
        local_direct: Some(ProbeDirectField {
            light_indices: vec![0],
            probes: vec![LightmapTexel::default(); 8],
        }),
        switchable_indirect: vec![ProbeSwitchableField {
            light_index: 0,
            probes: vec![
                LightmapTexel {
                    irradiance: [0.1; 3],
                    ..LightmapTexel::default()
                };
                8
            ],
        }],
    };
    let bounds = Aabb {
        min: [-0.2, 0.6, -0.2],
        max: [0.2, 1.2, 0.2],
    };
    let mut cache = EntityStaticRayCache::default();
    let sample = |current: &LevelLighting, storage: Option<(&mut EntityStaticRayCache, u64)>| {
        super::entity_spatial_lighting_cached(
            current,
            Some(&field),
            bounds,
            glam::Mat4::IDENTITY,
            None,
            super::EntityVisibility::default(),
            storage,
        )
    };
    let first = sample(&lighting, Some((&mut cache, 1))).ok_or("missing prepared sample")?;
    let queries = cache.static_queries;
    assert!(queries > 0, "fixture must trace real immutable visibility");
    for (previous, revision, enabled) in [(1, 2, false), (2, 3, true), (3, 4, false), (4, 5, true)]
    {
        assert!(lighting.set_light_enabled(0, enabled));
        cache.advance_source_revision(previous, revision);
        let cached = sample(&lighting, Some((&mut cache, revision)));
        let full = sample(&lighting, None);
        assert_eq!(
            cached, full,
            "switch state must match every uncached HDR anchor and direct tap"
        );
        assert_eq!(
            cache.static_queries, queries,
            "energy-only switches trace no immutable rays again"
        );
        if enabled {
            assert_eq!(cached.as_deref(), Some(first.as_ref()));
        }
    }
    cache.advance_source_revision(4, 6);
    cache.prepare(None, bounds, 6);
    assert!(
        cache.rays.is_empty(),
        "a missed resource revision cannot be adopted as energy-only"
    );
    Ok(())
}
