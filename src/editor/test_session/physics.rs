//! Production physics owner prepared off the UI thread for one private test.
use crate::engine::level::{LevelDef, LevelSurfaces};
use crate::engine::loader::LoadedLevel;
use crate::engine::physics::{ItemWorld, PlayerContext};
use crate::engine::props::PropAssets;
use crate::engine::render::Renderer;
use crate::game::{CollisionWorld, Game};
use glam::Vec3;

pub(in crate::editor) struct PreparedPhysics {
    pub(super) world: ItemWorld,
    authored: Vec<(u64, String, String)>,
    spawns: Vec<(u64, usize, [f32; 3])>,
    trigger_bodies: Vec<crate::engine::physics::ItemTriggerBody>,
}

pub(super) fn player_context(game: &Game) -> PlayerContext {
    PlayerContext {
        half: game.body_half(),
        eye: game.player_position,
        feet: Vec3::new(
            game.player_position.x,
            game.feet_y(),
            game.player_position.z,
        ),
        height: game.body_height(),
        view: game.view_direction(),
        yaw: game.player_yaw,
        active: !game.noclip_enabled() && !game.player_is_dead(),
    }
}

impl PreparedPhysics {
    pub fn prepare(
        loaded: &LoadedLevel,
        collision: &CollisionWorld,
        assets: &mut PropAssets,
    ) -> Result<Self, String> {
        let level = &loaded.level;
        let mut prepared = Self {
            world: ItemWorld::from_level_with_materials(level, &collision.walls, &loaded.materials),
            authored: Vec::new(),
            spawns: Vec::new(),
            trigger_bodies: Vec::with_capacity(crate::engine::physics::MAX_BODIES),
        };
        let surfaces = LevelSurfaces::new(level);
        let ids = level.prop_instance_ids();
        let fixed = level
            .props
            .iter()
            .enumerate()
            .filter(|(_, prop)| prop.has_fixed_physics_shape());
        let movable = level
            .props
            .iter()
            .enumerate()
            .filter(|(_, prop)| !prop.has_fixed_physics_shape() && prop.is_physics_movable());
        for (index, prop) in fixed.chain(movable) {
            crate::engine::resources::checkpoint()?;
            let Some(definition) = &prop.physics else {
                continue;
            };
            let (min, max) = crate::engine::props::rigid_model_bounds(
                assets,
                loaded.catalog.assets(),
                &prop.model,
            )?;
            let bounds = (Vec3::from_array(min), Vec3::from_array(max));
            let position = Vec3::new(
                prop.x,
                surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0) + prop.y,
                prop.z,
            );
            if prop.has_fixed_physics_shape() {
                prepared.world.install_fixed_shape_oriented(
                    definition,
                    bounds,
                    position,
                    prop.orientation(),
                    prop.scale,
                )?;
            } else {
                let key = (1_u64 << 63_i32)
                    .saturating_add(u64::try_from(index).map_err(|error| error.to_string())?);
                prepared.world.spawn_authored_oriented(
                    key,
                    definition,
                    bounds,
                    position,
                    prop.orientation(),
                    prop.scale,
                )?;
                prepared.spawns.push((key, index, position.to_array()));
                prepared.authored.push((
                    key,
                    ids.get(index)
                        .cloned()
                        .ok_or("Physical prop identity is missing")?,
                    prop.model.clone(),
                ));
            }
        }
        crate::engine::resources::checkpoint()?;
        Ok(prepared)
    }

    pub(super) fn install(&self, level: &LevelDef, renderer: &mut Renderer) -> Result<(), String> {
        let spawns = self
            .spawns
            .iter()
            .map(|(key, index, position)| {
                Ok((
                    *key,
                    level
                        .props
                        .get(*index)
                        .ok_or("Physical prop disappeared from test snapshot")?,
                    *position,
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        if let Err(index) = renderer.spawn_runtime_props(&spawns) {
            for (key, _, _) in &spawns {
                let _removed = renderer.despawn_runtime_model(*key);
            }
            return Err(format!(
                "Renderer refused physical prop {index} in the private test"
            ));
        }
        Ok(())
    }

    pub(super) fn step(&mut self, game: &mut Game, seconds: f32, renderer: &mut Renderer) {
        self.world.sync_water(&game.water);
        self.world
            .step(seconds, player_context(game), game.door_colliders());
        for recovery in self.world.take_recoveries() {
            if recovery.outcome == crate::engine::physics::RecoveryOutcome::Suspended {
                let _hidden = renderer.set_runtime_model_visible(recovery.key, false);
            }
        }
        self.world.for_each_pose(|key, pose| {
            let _visible = renderer.set_runtime_model_visible(key, true);
            let _changed = renderer.set_runtime_model_pose(
                key,
                pose.position.to_array(),
                pose.rotation.to_array(),
            );
        });
        self.update_triggers(game, false);
    }

    pub(super) fn update_triggers(&mut self, game: &mut Game, seed: bool) {
        self.trigger_bodies.clear();
        self.world
            .for_each_trigger_body(|body| self.trigger_bodies.push(body));
        let occupants = self.trigger_bodies.iter().filter_map(|body| {
            let (_, instance, asset) = self.authored.iter().find(|(key, _, _)| *key == body.key)?;
            Some(crate::engine::entities::TriggerItem {
                key: body.key,
                instance,
                asset,
                from: body.previous,
                to: body.current,
                half_extents: body.half_extents,
                held: body.held,
                sleeping: body.sleeping,
                up_y: body.up_y,
                path: Some(body.path()),
                centre_path: Some(body.centre_path()),
            })
        });
        game.entities_mut()
            .observe_physical_items(occupants.clone());
        game.entities_mut()
            .observe_item_releases(&self.trigger_bodies);
        if seed {
            game.seed_volume_occupants_iter(occupants);
        } else {
            game.update_volume_occupants_iter(occupants);
        }
        let held = self.world.held().and_then(|key| {
            self.authored
                .iter()
                .find(|(body, _, _)| *body == key)
                .map(|(_, instance, _)| instance.clone())
        });
        game.entities_mut().observe_held_item(held);
    }
}
