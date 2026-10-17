//! Explicit headless physics campaign. No renderer, asset decode or map bake.
//!
//! Run in release mode, with and without `--features parallel-physics`:
//! `places-physics-bench --workers 4 --frames 240 --repeats 5`.
//! The JSON receipt includes exact frame-state hashes and one/four-worker parity.
//! Worker-count independence is Rapier's documented contract:
//! <https://rapier.rs/docs/user_guides/rust/determinism/>.

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "the explicit benchmark CLI reports its JSON receipt and errors"
)]

#[cfg(not(target_arch = "wasm32"))]
pub fn run() -> std::process::ExitCode {
    native::main()
}

#[cfg(target_arch = "wasm32")]
pub fn run() -> std::process::ExitCode {
    eprintln!("places-physics-bench requires a native thread-capable target");
    std::process::ExitCode::FAILURE
}

#[cfg(not(target_arch = "wasm32"))]
mod native {

    use std::collections::HashMap;
    use std::process::ExitCode;
    use std::sync::Arc;
    use std::time::Instant;

    use glam::{Quat, Vec3};
    use places::engine::assets::AssetCatalog;
    use places::engine::level::LevelDef;
    use places::engine::materials::{MaterialTable, PackMaterials};
    use places::engine::physics::{
        BodyMotion, CompoundPart, FIXED_STEP, ItemPose, ItemWorld, PhysicsDef, PlayerContext,
        PrimitiveShapeDef, ShapeDef, WaterPhysicsDef,
    };
    use serde::Serialize;
    use sha2::{Digest, Sha256};

    const WARMUP_FRAMES: u32 = 24;
    const COUNTS: [u16; 4] = [12, 32, 64, 128];

    #[derive(Clone, Copy, Serialize)]
    #[serde(rename_all = "snake_case")]
    enum Scenario {
        Contacts,
        Water,
        Dormant,
    }

    impl Scenario {
        const fn wet(self) -> bool {
            matches!(self, Self::Water)
        }

        const fn dormant(self) -> bool {
            matches!(self, Self::Dormant)
        }
    }

    struct Options {
        workers: usize,
        frames: u32,
        repeats: u16,
    }

    #[derive(Serialize)]
    struct Measurement {
        workers: usize,
        median_ms: f64,
        p95_ms: f64,
        p99_ms: f64,
        peak_ms: f64,
        peak_active: usize,
        peak_contact_pairs: usize,
        immersed_frames: usize,
        state_sha256: String,
    }

    #[derive(Serialize)]
    struct Comparison {
        scenario: Scenario,
        bodies: u16,
        repeat: u16,
        parity: bool,
        serial: Measurement,
        parallel: Measurement,
    }

    pub fn main() -> ExitCode {
        match run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("places-physics-bench: {error}");
                ExitCode::FAILURE
            }
        }
    }

    fn options() -> Result<Options, String> {
        let mut options = Options {
            workers: 4,
            frames: 240,
            repeats: 5,
        };
        let mut arguments = std::env::args().skip(1);
        while let Some(argument) = arguments.next() {
            let value = arguments
                .next()
                .ok_or_else(|| format!("{argument} needs a value"))?;
            match argument.as_str() {
                "--workers" => {
                    options.workers = value
                        .parse()
                        .map_err(|error| format!("invalid workers: {error}"))?;
                }
                "--frames" => {
                    options.frames = value
                        .parse()
                        .map_err(|error| format!("invalid frames: {error}"))?;
                }
                "--repeats" => {
                    options.repeats = value
                        .parse()
                        .map_err(|error| format!("invalid repeats: {error}"))?;
                }
                _ => {
                    return Err(format!(
                        "unknown option {argument}; use --workers N --frames N --repeats N"
                    ));
                }
            }
        }
        if !(2..=8).contains(&options.workers)
            || !(48..=2400).contains(&options.frames)
            || !(1..=20).contains(&options.repeats)
        {
            return Err("workers must be 2..=8, frames 48..=2400, repeats 1..=20".into());
        }
        Ok(options)
    }

    fn worker_pool(workers: usize) -> Result<Arc<rayon::ThreadPool>, String> {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .thread_name(move |index| format!("places-physics-bench-{workers}-{index}"))
            .build()
            .map(Arc::new)
            .map_err(|error| format!("could not create bounded benchmark pool: {error}"))
    }

    fn run() -> Result<(), String> {
        let options = options()?;
        let serial_pool = worker_pool(1)?;
        let parallel_pool = worker_pool(options.workers)?;
        let mut comparisons = Vec::new();
        for repeat in 0..options.repeats {
            for scenario in [Scenario::Contacts, Scenario::Water, Scenario::Dormant] {
                for count in COUNTS {
                    // Alternate measurement order to reduce temperature/order bias.
                    let (serial, parallel) = if repeat.is_multiple_of(2) {
                        (
                            measure(scenario, count, &serial_pool, options.frames)?,
                            measure(scenario, count, &parallel_pool, options.frames)?,
                        )
                    } else {
                        let parallel = measure(scenario, count, &parallel_pool, options.frames)?;
                        (
                            measure(scenario, count, &serial_pool, options.frames)?,
                            parallel,
                        )
                    };
                    let parity = serial.state_sha256 == parallel.state_sha256;
                    comparisons.push(Comparison {
                        scenario,
                        bodies: count,
                        repeat,
                        parity,
                        serial,
                        parallel,
                    });
                }
            }
        }
        let all_parity = comparisons.iter().all(|comparison| comparison.parity);
        let receipt = serde_json::json!({
            "fixed_step_seconds": FIXED_STEP,
            "frames_per_run": options.frames,
            "warmup_frames": WARMUP_FRAMES,
            "parallel_rapier_feature": cfg!(feature = "parallel-physics"),
            "all_parity": all_parity,
            "comparisons": comparisons,
            "method": "Only ItemWorld::step is timed. Deterministic fixture velocities are refreshed every twelve frames outside the timer. Hashes cover every frame's snapshots, contacts and solved contact impulses; timing statistics are excluded. No map or assets are baked."
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&receipt).map_err(|error| error.to_string())?
        );
        if all_parity {
            Ok(())
        } else {
            Err("worker-count frame-state parity failed".into())
        }
    }

    fn level(scenario: Scenario) -> Result<LevelDef, String> {
        let water = if scenario.wet() {
            r#"[
                {"x":-4,"z":-4,"width":10,"depth":10,"surface_y":0,"bottom_y":-2},
                {"x":-1.05,"z":-4,"width":0.11,"depth":10,"surface_y":0.04,"bottom_y":-2}
            ]"#
        } else {
            "[]"
        };
        let floor_y = if scenario.wet() { "-2" } else { "0" };
        let definition = format!(
            r#"{{
            "schema":"places-level","format_version":1,"id":"physics_workers","name":"physics workers",
            "spawn":{{"x":0,"z":0}},
            "defaults":{{"floor":"bench:wood","ceiling":"core:ceiling_tile","wall":"core:wallpaper"}},
            "rooms":[{{"x":-4,"z":-4,"width":10,"depth":10,"height":6,
                       "floor_y":{floor_y},"ceiling":{{"kind":"open"}}}}],
            "floor_patches":[{{"x":-4,"z":-4,"width":10,"depth":4,"material":"bench:carpet"}}],
            "water":{water}
        }}"#
        );
        serde_json::from_str(&definition)
            .map_err(|error| format!("invalid headless physics fixture: {error}"))
    }

    fn world(
        scenario: Scenario,
        count: u16,
        pool: &Arc<rayon::ThreadPool>,
    ) -> Result<ItemWorld, String> {
        let level = level(scenario)?;
        let pack = PackMaterials::new(
            "physics_workers",
            Some(
                r#"{
        "bench:wood":{"texture":"unused.png","ground_friction":0.6},
        "bench:carpet":{"texture":"unused.png","ground_friction":1.0}}
        "#,
            ),
            HashMap::new(),
        );
        let materials = MaterialTable::logical(&level, &AssetCatalog::builtin(), Some(&pack));
        let mut world = ItemWorld::from_level_with_materials(&level, &[], &materials);
        world.set_worker_pool(Arc::clone(pool));
        let definition = PhysicsDef {
            motion: BodyMotion::Dynamic,
            mass_kg: if scenario.wet() { 3.0 } else { 0.5 },
            linear_damping: 0.0,
            angular_damping: 0.0,
            restitution: 0.0,
            shape: Some(if scenario.wet() {
                ShapeDef::Cuboid {
                    half_extents: [0.1, 0.14, 0.1],
                }
            } else {
                ShapeDef::Compound {
                    parts: [-0.09, 0.09]
                        .into_iter()
                        .flat_map(|x| {
                            [-0.09, 0.09].into_iter().map(move |z| CompoundPart {
                                offset: [x, 0.0, z],
                                shape: PrimitiveShapeDef::Cuboid {
                                    half_extents: [0.065, 0.08, 0.065],
                                },
                            })
                        })
                        .collect(),
                }
            }),
            water: Some(WaterPhysicsDef {
                displaced_volume_m3: Some(0.006),
                ..WaterPhysicsDef::default()
            }),
            reset_group: Some("workers".into()),
            ..PhysicsDef::default()
        };
        for index in 0..count {
            let key = u64::from(index).saturating_add(1);
            let column = f32::from(index % 8);
            let row_count = if scenario.wet() { 16 } else { 8 };
            let row = f32::from(
                (index / 8)
                    .checked_rem(row_count)
                    .ok_or("invalid fixture row count")?,
            );
            let layer = f32::from(index / 64);
            let position = Vec3::new(
                column.mul_add(0.5, -2.0),
                if scenario.wet() {
                    -0.01
                } else {
                    layer.mul_add(0.162, 0.081)
                },
                row.mul_add(0.5, -2.0),
            );
            world.spawn_authored(
                key,
                &definition,
                (Vec3::splat(-0.2), Vec3::splat(0.2)),
                position,
                0.0,
                1.0,
            )?;
            if !scenario.dormant() {
                if !world.activate(key) {
                    return Err(format!("could not activate fixture body {key}"));
                }
                if scenario.wet()
                    && !world.set_fixture_pose(
                        key,
                        ItemPose {
                            position,
                            rotation: Quat::from_rotation_z(0.7),
                        },
                    )
                {
                    return Err(format!("could not tilt water fixture body {key}"));
                }
            }
        }
        Ok(world)
    }

    fn measure(
        scenario: Scenario,
        count: u16,
        pool: &Arc<rayon::ThreadPool>,
        frames: u32,
    ) -> Result<Measurement, String> {
        let mut world = world(scenario, count, pool)?;
        let player = PlayerContext {
            eye: Vec3::new(8.0, 1.6, 8.0),
            feet: Vec3::new(8.0, 0.0, 8.0),
            height: 1.8,
            view: -Vec3::Z,
            yaw: 0.0,
            active: false,
        };
        let mut samples = Vec::new();
        let mut hash = Sha256::new();
        let mut peak_active = 0;
        let mut peak_contact_pairs = 0;
        let mut immersed_frames = 0_usize;
        for frame in 0..frames {
            if !scenario.dormant() && frame.is_multiple_of(12) {
                for key in 1..=u64::from(count) {
                    let (linear, angular) = if scenario.wet() {
                        (Vec3::ZERO, Vec3::new(0.0, 0.0, 0.6))
                    } else {
                        (Vec3::new(0.25, 0.0, 0.12), Vec3::ZERO)
                    };
                    if !world.set_fixture_velocity(key, linear, angular) {
                        return Err(format!("could not refresh fixture body {key}"));
                    }
                }
            }
            let started = Instant::now();
            world.step(FIXED_STEP, player, &[]);
            let milliseconds = started.elapsed().as_secs_f64() * 1_000.0_f64;
            if frame >= WARMUP_FRAMES {
                samples.push(milliseconds);
            }
            let stats = world.stats();
            peak_active = peak_active.max(stats.active_bodies);
            let contacts = world.contact_pairs();
            peak_contact_pairs = peak_contact_pairs.max(contacts.len());
            let snapshots = world.snapshots();
            if snapshots.iter().any(|body| body.submerged_fraction > 0.0) {
                immersed_frames = immersed_frames.saturating_add(1);
            }
            if snapshots.len() != usize::from(count)
                || stats.recovered_bodies != 0
                || stats.suspended_bodies != 0
                || snapshots.iter().any(|body| !body.pose.position.is_finite())
                || (scenario.dormant() && stats.dormant_bodies != usize::from(count))
            {
                return Err(format!(
                    "physics invariant failed at frame {frame}, body count {count}"
                ));
            }
            hash.update(
                serde_json::to_vec(&(snapshots, contacts, world.contact_impulses()))
                    .map_err(|error| error.to_string())?,
            );
        }
        samples.sort_unstable_by(f64::total_cmp);
        let percentile = |numerator: usize, denominator: usize| -> Result<f64, String> {
            let index = samples
                .len()
                .saturating_mul(numerator)
                .saturating_add(denominator.saturating_sub(1))
                .checked_div(denominator)
                .ok_or("invalid percentile denominator")?
                .saturating_sub(1);
            samples
                .get(index)
                .copied()
                .ok_or_else(|| "no timed samples".into())
        };
        let middle = samples.len() / 2;
        let upper_middle = samples.get(middle).copied().ok_or("no timed samples")?;
        let median = if samples.len().is_multiple_of(2) {
            f64::midpoint(
                samples
                    .get(middle.saturating_sub(1))
                    .copied()
                    .ok_or("no timed samples")?,
                upper_middle,
            )
        } else {
            upper_middle
        };
        Ok(Measurement {
            workers: pool.current_num_threads(),
            median_ms: median,
            p95_ms: percentile(95, 100)?,
            p99_ms: percentile(99, 100)?,
            peak_ms: percentile(1, 1)?,
            peak_active,
            peak_contact_pairs,
            immersed_frames,
            state_sha256: places::engine::package::hash::digest_hex(&hash.finalize()),
        })
    }
}
