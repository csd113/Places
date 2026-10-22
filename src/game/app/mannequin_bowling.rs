//! Opt-in lane transformation. Inactive lanes retain ordinary item simulation.
use glam::{Quat, Vec3};

use super::FrameLoop;
use crate::engine::level::{ActionDef, PropDef};
use crate::engine::physics::{
    BodyMotion, CompoundPart, ItemPose, PhysicsDef, PrimitiveShapeDef, ShapeDef,
};

const MINI_SCALE: f32 = 0.25;

pub(super) struct Pin {
    key: u64,
    id: String,
    source: PropDef,
    bounds: (Vec3, Vec3),
    position: Vec3,
    pose: Option<ItemPose>,
    ragdolled: bool,
}

pub(super) struct Rack {
    group: String,
    target: String,
    pins: Vec<Pin>,
    balls: Vec<u64>,
}

fn proxy(group: &str) -> PhysicsDef {
    let part = |offset, half_extents| CompoundPart {
        offset,
        shape: PrimitiveShapeDef::Cuboid { half_extents },
    };
    PhysicsDef {
        motion: BodyMotion::Dynamic,
        shape: Some(ShapeDef::Compound {
            parts: vec![
                part([0.0, 0.04, 0.0], [0.18, 0.04, 0.13]),
                part([0.0, 0.82, 0.0], [0.16, 0.74, 0.10]),
                part([0.0, 1.60, 0.0], [0.085, 0.12, 0.085]),
            ],
        }),
        mass_kg: 68.0 * MINI_SCALE.powi(3),
        friction: 0.65,
        restitution: 0.0,
        pickup: false,
        reset_group: Some(group.to_string()),
        ..PhysicsDef::default()
    }
}

impl FrameLoop<'_> {
    fn publish_miniature_state(&mut self, target: &str, enabled: bool) {
        let _report = self.game.entities_mut().dispatch_on(
            target,
            &[ActionDef::SetState {
                target: None,
                name: "miniatures".into(),
                value: crate::engine::entities::components::StateValue::Bool(enabled),
            }],
        );
    }

    pub(super) fn set_mannequin_pins(
        &mut self,
        group: &str,
        requested: Option<bool>,
        target: &str,
    ) -> Result<(), String> {
        if self
            .items
            .miniatures
            .as_ref()
            .is_some_and(|rack| rack.group != group)
        {
            return Err("Only one miniature rack can be active at a time".into());
        }
        let enabled = requested.unwrap_or_else(|| self.items.miniatures.is_none());
        self.clear_mannequin_pins();
        self.synchronize_physical_player();
        let _reset = self.items.world.reset_group(group)?;
        self.items.input.invalidate();
        self.items.target = None;
        if !enabled {
            self.publish_miniature_state(target, false);
            self.trace_world("miniature_pins_off");
            return Ok(());
        }
        let loaded = self.current_level.as_ref().ok_or("No installed level")?;
        let surfaces = crate::engine::level::LevelSurfaces::new(&loaded.level);
        let ids = loaded.level.prop_instance_ids();
        let mut pins = Vec::new();
        for (index, source) in loaded.level.props.iter().enumerate().filter(|(_, prop)| {
            prop.model == "arcade:bowling_pin"
                && prop
                    .physics
                    .as_ref()
                    .and_then(|def| def.reset_group.as_deref())
                    == Some(group)
        }) {
            let id = ids.get(index).ok_or("Pin identity missing")?;
            let key = self
                .items
                .authored
                .iter()
                .find(|(_, item_id, _)| item_id == id)
                .map(|(key, _, _)| *key)
                .ok_or("Pin body missing")?;
            let (min, max) = self.renderer.rigid_runtime_model_bounds(&source.model)?;
            pins.push(Pin {
                key,
                id: format!("miniature:{id}"),
                source: source.clone(),
                bounds: (Vec3::from_array(min), Vec3::from_array(max)),
                position: Vec3::new(
                    source.x,
                    surfaces.floor_y_at(source.x, source.z).unwrap_or(0.0) + source.y,
                    source.z,
                ),
                pose: None,
                ragdolled: false,
            });
        }
        if pins.len() != 10 {
            return Err(
                "A miniature rack requires exactly ten authored bowling pins in its reset group"
                    .into(),
            );
        }
        let balls = self
            .items
            .authored
            .iter()
            .filter(|(_, _, model)| model.starts_with("arcade:bowling_ball_"))
            .map(|(key, _, _)| *key)
            .collect();
        let rack = Rack {
            group: group.to_string(),
            target: target.to_string(),
            pins,
            balls,
        };
        self.items.miniatures = Some(rack);
        let result = self.install_miniature_pins();
        if let Err(error) = result {
            self.clear_mannequin_pins();
            return Err(error);
        }
        self.publish_miniature_state(target, true);
        self.trace_world("miniature_pins_on");
        Ok(())
    }

    fn install_miniature_pins(&mut self) -> Result<(), String> {
        let rack = self
            .items
            .miniatures
            .as_ref()
            .ok_or("Miniature rack missing")?;
        let definition = proxy(&rack.group);
        for pin in &rack.pins {
            let _removed = self.items.world.remove(pin.key);
            self.items.world.spawn_authored_oriented(
                pin.key,
                &definition,
                (Vec3::new(-0.29, 0.0, -0.16), Vec3::new(0.29, 1.723, 0.16)),
                pin.position,
                Quat::IDENTITY,
                MINI_SCALE,
            )?;
            self.renderer.spawn_runtime_character(
                &pin.id,
                "mannequin",
                pin.position.to_array(),
                0.0,
                MINI_SCALE,
            )?;
            let _hidden = self.renderer.set_runtime_model_visible(pin.key, false);
        }
        Ok(())
    }

    pub(super) fn clear_mannequin_pins(&mut self) {
        let Some(rack) = self.items.miniatures.take() else {
            return;
        };
        for pin in rack.pins {
            self.renderer.cancel_ragdoll(&pin.id, &mut self.items.world);
            let _despawned = self.renderer.despawn_runtime_character(&pin.id);
            let _removed = self.items.world.remove(pin.key);
            if let Some(definition) = pin.source.physics.as_ref()
                && let Err(error) = self.items.world.spawn_authored_oriented(
                    pin.key,
                    definition,
                    pin.bounds,
                    pin.position,
                    pin.source.orientation(),
                    pin.source.scale,
                )
            {
                crate::engine::logging::warn(format!(
                    "[miniature-pins] restore {}: {error}",
                    pin.id
                ));
            }
            let _visible = self.renderer.set_runtime_model_visible(pin.key, true);
        }
        self.publish_miniature_state(&rack.target, false);
    }

    pub(super) fn update_mannequin_pins(&mut self) {
        let Some(mut rack) = self.items.miniatures.take() else {
            return;
        };
        let contacts = if rack.pins.iter().any(|pin| !pin.ragdolled) {
            self.items.world.contact_impulses()
        } else {
            Vec::new()
        };
        let active: [Option<u64>; 10] = std::array::from_fn(|index| {
            rack.pins
                .get(index)
                .filter(|pin| pin.ragdolled)
                .map(|pin| pin.key)
        });
        for pin in &mut rack.pins {
            let _hidden = self.renderer.set_runtime_model_visible(pin.key, false);
            if pin.ragdolled {
                continue;
            }
            let Some((pose, velocity)) = self.items.world.body_pose_velocity(pin.key) else {
                continue;
            };
            if pin.pose != Some(pose) {
                let _posed = self.renderer.set_runtime_character_pose(
                    &pin.id,
                    pose.position.to_array(),
                    pose.rotation.to_array(),
                );
                pin.pose = Some(pose);
            }
            let struck = contacts.iter().any(|contact| {
                contact.impulse > 0.015
                    && contact.pair.contains(&pin.key)
                    && contact.pair.iter().any(|key| {
                        *key != pin.key
                            && (rack.balls.contains(key) || active.contains(&Some(*key)))
                    })
            });
            if !struck {
                continue;
            }
            let _removed = self.items.world.remove(pin.key);
            match self
                .renderer
                .start_ragdoll_from_body(&pin.id, &mut self.items.world, velocity)
            {
                Ok(()) => {
                    pin.ragdolled = true;
                    self.items.world.alias_ragdoll_contacts(&pin.id, pin.key);
                    self.trace.record("miniature_pin_struck", self.load_generation, &serde_json::json!({"id":pin.id,"velocity":velocity.to_array(),"scale":MINI_SCALE,"solver":self.items.world.solver_resources()}).to_string());
                }
                Err(error) => {
                    crate::engine::logging::warn(format!(
                        "[miniature-pins] handoff refused: {error}"
                    ));
                    let _restored = self.items.world.spawn_authored_oriented(
                        pin.key,
                        &proxy(&rack.group),
                        (Vec3::new(-0.29, 0.0, -0.16), Vec3::new(0.29, 1.723, 0.16)),
                        pose.position,
                        pose.rotation,
                        MINI_SCALE,
                    );
                }
            }
        }
        self.items.miniatures = Some(rack);
    }
}
