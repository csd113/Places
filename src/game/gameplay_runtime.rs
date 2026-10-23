//! Reusable typed gameplay actions and session resource ownership.
use crate::engine::{
    physics::{BodyMotion, ItemWorld, PhysicsDef, PlayerContext},
    render::Renderer,
};
use crate::game::{
    Game,
    gameplay_actions::{GameplayAction, PhysicsAction, RemovalScope, RepresentationAction},
    pickup_catalog,
    representation::PlayerRepresentation,
};
use glam::Vec3;

const MAX_SESSION_PROPS: usize = 16;
const MAX_SESSION_KEYS: u64 = 256;

struct SessionProp {
    key: u64,
    instance: String,
    model: String,
    character: Option<String>,
    physical: bool,
}

/// Live owners supplied by a game mode or the native frame loop. Typed actions
/// never require a console to be enabled and never parse command text.
pub struct GameplayContext<'a> {
    pub game: &'a mut Game,
    pub renderer: &'a mut Renderer,
    pub physics: &'a mut ItemWorld,
    /// Stable key, authored instance ID, canonical catalog ID.
    pub authored: &'a mut Vec<(u64, String, String)>,
}

#[derive(Default)]
pub struct Runtime {
    given: Vec<SessionProp>,
    next_key: u64,
    trigger_labels: bool,
    pub(crate) crosshair_override: Option<bool>,
    placement_physics: bool,
    last_reset_count: u64,
    last_world_reset_count: u64,
    console_enabled: bool,
    pub representation: PlayerRepresentation,
}

fn player_context(game: &Game) -> PlayerContext {
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
        active: !game.noclip_enabled(),
    }
}

impl Runtime {
    pub(crate) const fn crosshair_enabled(&self, map_default: bool) -> bool {
        match self.crosshair_override {
            Some(enabled) => enabled,
            None => map_default,
        }
    }

    pub(crate) const fn reset_map_presentation(&mut self) {
        self.crosshair_override = None;
    }

    pub const fn console_disabled_transition(&mut self, enabled: bool) -> bool {
        let disabled = self.console_enabled && !enabled;
        self.console_enabled = enabled;
        disabled
    }

    /// Apply one typed request against the live world.
    /// # Errors
    /// Refuses invalid, obstructed or over-budget operations before claiming success.
    pub fn apply_action(
        &mut self,
        action: &GameplayAction,
        context: &mut GameplayContext<'_>,
    ) -> Result<String, String> {
        self.reconcile_player_reset(context.game, context.renderer, context.physics);
        context
            .physics
            .sync_context(player_context(context.game), context.game.door_colliders());
        let result = match action {
            GameplayAction::Noclip(request) => {
                let enabled = request.unwrap_or_else(|| !context.game.noclip_enabled());
                if enabled && context.physics.held().is_some() {
                    return Err("Drop the held item before enabling noclip.".into());
                }
                let current = player_context(context.game);
                context
                    .game
                    .set_noclip_with_clearance(enabled, |eye| {
                        context.physics.player_pose_clear(PlayerContext {
 half: crate::engine::collision::PLAYER_RADIUS,
                            eye,
                            feet: Vec3::new(eye.x, eye.y - crate::game::EYE_HEIGHT, eye.z),
                            height: crate::engine::collision::PLAYER_HEIGHT,
                            active: true,
                            ..current
                        })
                    })
                    .map(str::to_string)
            }
            GameplayAction::Give(id) => self.give(id, context),
            GameplayAction::Respawn => {
                let destination = context.game.developer_respawn_eye()?;
                let mut destination_context = player_context(context.game);
                destination_context.eye = destination;
                destination_context.height = crate::engine::collision::PLAYER_HEIGHT;
                destination_context.half = crate::engine::collision::PLAYER_RADIUS;
                destination_context.active = true;
                destination_context.feet = Vec3::new(
                    destination.x,
                    destination.y - crate::game::EYE_HEIGHT,
                    destination.z,
                );
                if !context.physics.player_pose_clear(destination_context) {
                    return Err(
                        "Safe spawn is blocked by a physics item. Move it clear and retry.".into(),
                    );
                }
                release_held(context.physics)?;
                context.game.developer_respawn().map(str::to_string)
            }
            GameplayAction::Position => Ok(context.game.developer_position_report()),
            GameplayAction::Remove(scope) => self.remove(*scope, context),
            GameplayAction::Physics(request) => {
                match request {
                    PhysicsAction::Pause => context.physics.pause(),
                    PhysicsAction::Resume => context.physics.resume(),
                    PhysicsAction::Step => context
                        .physics
                        .step_once(player_context(context.game), context.game.door_colliders())?,
                }
                Ok(format!(
                    "Physics {:?}; {:?}.",
                    request,
                    context.physics.simulation_state()
                ))
            }
            GameplayAction::ActivatePhysics(key) => {
                if context.physics.activate(*key) {
                    Ok(format!("Activated physics instance {key}."))
                } else {
                    Err("No movable physics instance exists for that key.".into())
                }
            }
            GameplayAction::Ragdoll { target, enabled, velocity } => context.game.entities_mut()
                .request_ragdoll(target, *enabled, *velocity)
                .map(|()| format!("Ragdoll request for '{target}' queued; any refusal appears in world status.")),
            GameplayAction::TimeScale(value) => {
                context.physics.set_time_scale(*value)?;
                context.game.set_world_time_scale(*value)?;
                Ok(format!(
                    "World time scale {value:.3}; player and held control remain normal."
                ))
            }
            GameplayAction::Trigger(id) => {
                let report = context.game.developer_trigger(id)?;
                Ok(format!("Trigger `{id}` dispatched: {report:?}."))
            }
            GameplayAction::TriggerLabels(request) => {
                self.trigger_labels = request.unwrap_or(!self.trigger_labels);
                Ok(format!(
                    "Trigger IDs {} (nearby visible trigger volumes only).",
                    if self.trigger_labels { "ON" } else { "OFF" }
                ))
            }
            GameplayAction::Become(request) => self.become_action(request, context),
        };
        self.reconcile_player_reset(context.game, context.renderer, context.physics);
        result
    }

    fn allocate_key(&mut self) -> Result<u64, String> {
        if self.given.len() >= MAX_SESSION_PROPS {
            return Err("Session prop limit reached (16). Remove props or reload the map.".into());
        }
        // Live keys never alias a deleted body, renderer object or pending input.
        // Reserve a bounded disjoint lane from authored and representation keys.
        if self.next_key >= MAX_SESSION_KEYS {
            return Err("Session key budget reached (256). Reload the map.".into());
        }
        let key = u64::MAX.saturating_sub(self.next_key);
        self.next_key = self.next_key.saturating_add(1);
        Ok(key)
    }

    fn give(&mut self, id: &str, context: &mut GameplayContext<'_>) -> Result<String, String> {
        let item = pickup_catalog::find(id).ok_or_else(|| {
            format!("`{id}` is not a curated pickup item. /give list shows eligible IDs.")
        })?;
        let profile = context
            .renderer
            .rigid_runtime_model_physics(item.id)
            .filter(|profile| profile.motion == BodyMotion::Dynamic && profile.pickup)
            .ok_or_else(|| format!("`{id}` has no valid movable pickup profile."))?;
        profile.validate()?;
        let info = context.renderer.runtime_model_info(item.id)?;
        let min = Vec3::from_array(info.bounds.0);
        let max = Vec3::from_array(info.bounds.1);
        let floor_position = context.game.developer_prop_position(min, max).ok_or_else(|| "No clear flat floor ahead. Move to open ground; /give is unavailable during noclip.".to_string())?;
        let position = Vec3::new(floor_position.x, floor_position.y + 0.01, floor_position.z);
        let key = self.allocate_key()?;
        context
            .physics
            .spawn_if_clear(key, &profile, (min, max), position, 0.0, 1.0)?;
        let character = Self::spawn_visual(
            key,
            item.id,
            info.animated,
            position.to_array(),
            0.0,
            1.0,
            context.renderer,
        )
        .inspect_err(|_| {
            let _removed = context.physics.remove(key);
        })?;
        self.given.push(SessionProp {
            key,
            instance: format!("session_{key}"),
            model: item.id.to_string(),
            character,
            physical: true,
        });
        Ok(format!(
            "Placed `{}` ({}) with movable pickup physics.",
            item.id, item.name
        ))
    }

    fn spawn_visual(
        key: u64,
        model: &str,
        animated: bool,
        position: [f32; 3],
        yaw: f32,
        scale: f32,
        renderer: &mut Renderer,
    ) -> Result<Option<String>, String> {
        if animated {
            let id = format!("developer-prop-{key}");
            renderer.spawn_runtime_character(&id, model, position, yaw, scale)?;
            let _idle_set = renderer.set_runtime_character_locomotion(
                &id,
                crate::engine::scene::LocomotionSnapshot::default(),
            );
            if !renderer.set_runtime_character_glow(&id, pickup_catalog::runtime_glow(model)) {
                let _removed = renderer.despawn_runtime_character(&id);
                return Err("Attached light budget exhausted; no item was spawned.".into());
            }
            Ok(Some(id))
        } else if renderer.spawn_runtime_model(key, model, position, yaw, scale) {
            Ok(None)
        } else {
            Err("Renderer refused the prop within its resource budget. No prop was placed.".into())
        }
    }

    fn remove(
        &mut self,
        scope: RemovalScope,
        context: &mut GameplayContext<'_>,
    ) -> Result<String, String> {
        let held = context.physics.held().ok_or_else(|| {
            "Hold a physics object before using /remove or /remove all.".to_string()
        })?;
        let model = self
            .given
            .iter()
            .find(|prop| prop.key == held)
            .map(|prop| prop.model.clone())
            .or_else(|| {
                context
                    .authored
                    .iter()
                    .find(|(key, _, _)| *key == held)
                    .map(|(_, _, model)| model.clone())
            })
            .ok_or_else(|| "Held item has no registered stable catalog identity.".to_string())?;
        let matches = |key: u64, id: &str| {
            (scope == RemovalScope::Held && key == held)
                || (scope == RemovalScope::MatchingPhysics && id == model)
        };
        let keys: Vec<_> = self
            .given
            .iter()
            .filter(|prop| prop.physical && matches(prop.key, &prop.model))
            .map(|prop| prop.key)
            .chain(
                context
                    .authored
                    .iter()
                    .filter(|(key, _, id)| matches(*key, id))
                    .map(|(key, _, _)| *key),
            )
            .filter(|key| {
                context
                    .physics
                    .body_motion(*key)
                    .is_some_and(BodyMotion::is_movable)
            })
            .collect();
        let mut removed = 0_usize;
        for key in &keys {
            if !context.physics.remove_movable(*key) {
                continue;
            }
            if let Some(prop) = self.given.iter().find(|prop| prop.key == *key) {
                despawn_visual(prop, context.renderer);
            } else {
                let _visual_removed = context.renderer.despawn_runtime_model(*key);
                if let Some((_, id, _)) = context
                    .authored
                    .iter()
                    .find(|(authored, _, _)| authored == key)
                {
                    let _entity_removed = context
                        .game
                        .entities_mut()
                        .remove_authored_physics_entity(id);
                }
            }
            removed = removed.saturating_add(1);
        }
        self.given.retain(|prop| !keys.contains(&prop.key));
        context.authored.retain(|(key, _, _)| !keys.contains(key));
        Ok(format!(
            "Removed {removed} physics instance(s) of `{model}` for this session; fixed decorations preserved."
        ))
    }

    fn check_form(
        shape: crate::game::player_shape::PlayerShape,
        context: &GameplayContext<'_>,
    ) -> Result<(), String> {
        if context.physics.held().is_some() {
            return Err("Drop the held item before changing form.".into());
        }
        let current = player_context(context.game);
        let proposed = PlayerContext {
            height: shape.height,
            half: shape.half,
            eye: Vec3::new(current.feet.x, current.feet.y + shape.eye, current.feet.z),
            ..current
        };
        if context.game.noclip_enabled()
            || !context.game.form_fits(shape)
            || !context.physics.player_pose_clear(proposed)
        {
            return Err("Not enough clear space for that body. Move clear and retry.".into());
        }
        Ok(())
    }

    fn select_form(
        &mut self,
        id: &str,
        scale: f32,
        context: &mut GameplayContext<'_>,
    ) -> Result<String, String> {
        let canonical = crate::game::representation::canonical_model_id(id);
        if !crate::game::representation::is_eligible(canonical)
            || !scale.is_finite()
            || !(0.1..=4.0).contains(&scale)
        {
            return Err("Unsupported form or scale.".into());
        }
        let info = context.renderer.runtime_model_info(canonical)?;
        let size = Vec3::new(
            info.bounds.1[0] - info.bounds.0[0],
            info.bounds.1[1] - info.bounds.0[1],
            info.bounds.1[2] - info.bounds.0[2],
        );
        let fitted_scale = (2.0 / size.max_element()).min(1.0) * scale;
        let shape = if crate::game::representation::CHARACTER_MODELS.contains(&canonical) {
            crate::game::player_shape::PlayerShape::human()
        } else {
            crate::game::player_shape::PlayerShape::from_bounds(info.bounds, fitted_scale)?
        };
        Self::check_form(shape, context)?;
        let feet = player_context(context.game).feet;
        self.representation
            .select_with_commit(
                context.renderer,
                canonical,
                feet,
                context.game.player_yaw,
                scale,
                || {
                    context.game.commit_form(Some(canonical.to_string()), shape);
                    Ok(())
                },
            )
            .map(|model| format!("Became `{model}`."))
    }

    fn become_action(
        &mut self,
        request: &RepresentationAction,
        context: &mut GameplayContext<'_>,
    ) -> Result<String, String> {
        let feet = Vec3::new(
            context.game.player_position.x,
            context.game.feet_y(),
            context.game.player_position.z,
        );
        match request {
            RepresentationAction::Select(id) => self.select_form(id, 1.0, context),
            RepresentationAction::SelectScaled { model, scale } => {
                self.select_form(model, *scale, context)
            }
            RepresentationAction::Reset => {
                let shape = crate::game::player_shape::PlayerShape::human();
                Self::check_form(shape, context)?;
                context.game.commit_form(None, shape);
                self.representation.reset(context.renderer);
                Ok("Normal player representation restored; first person.".into())
            }
            RepresentationAction::ToggleView => {
                if !self.representation.is_active() {
                    return Err("Select a form before changing its view.".into());
                }
                let first = self.representation.toggle_view();
                Ok(if first {
                    "First person form view."
                } else {
                    "Third person form view."
                }
                .into())
            }
            RepresentationAction::PlacementPhysics(enabled) => {
                self.placement_physics = *enabled;
                Ok(format!(
                    "Future /become placements: physics {}{}.",
                    if *enabled { "ON" } else { "OFF" },
                    if *enabled {
                        ", frozen until activated"
                    } else {
                        ""
                    }
                ))
            }
            RepresentationAction::Place => {
                let placement = self
                    .representation
                    .placement(feet, context.game.player_yaw)
                    .ok_or_else(|| {
                        "Select a model with /become <model> before placing it.".to_string()
                    })?;
                let key = self.allocate_key()?;
                if self.placement_physics {
                    let profile = context
                        .renderer
                        .rigid_runtime_model_physics(&placement.model)
                        .unwrap_or_else(|| PhysicsDef {
                            motion: BodyMotion::Dynamic,
                            pickup: true,
                            mass_kg: 20.0,
                            ..PhysicsDef::default()
                        });
                    let min = Vec3::from_array(placement.bounds.0);
                    let max = Vec3::from_array(placement.bounds.1);
                    // Authored spawning intentionally admits the player's own
                    // pose; dormancy prevents any impulse until interaction.
                    context.physics.spawn_dormant_if_clear(
                        key,
                        &profile,
                        (min, max),
                        Vec3::from_array(placement.position),
                        placement.yaw_degrees.to_radians(),
                        placement.scale,
                    )?;
                }
                let character = Self::spawn_visual(
                    key,
                    &placement.model,
                    placement.animated,
                    placement.position,
                    placement.yaw_degrees,
                    placement.scale,
                    context.renderer,
                )
                .inspect_err(|_| {
                    let _removed = context.physics.remove(key);
                })?;
                self.given.push(SessionProp {
                    key,
                    instance: format!("session_{key}"),
                    model: placement.model.clone(),
                    character,
                    physical: self.placement_physics,
                });
                Ok(format!(
                    "Placed `{}` duplicate at player pose; physics {} (session only).",
                    placement.model,
                    if self.placement_physics {
                        "ON, dormant"
                    } else {
                        "OFF"
                    }
                ))
            }
        }
    }

    pub fn reconcile_player_reset(
        &mut self,
        game: &mut Game,
        renderer: &mut Renderer,
        physics: &mut ItemWorld,
    ) {
        if self.last_reset_count != game.reset_count() {
            let _released = physics.release_held_for_reset();
            self.last_reset_count = game.reset_count();
            self.representation.reset(renderer);
        }
        if self.last_world_reset_count != game.world_reset_count() {
            self.clear_given_props(renderer, physics);
            let _clock_reset = game.set_world_time_scale(1.0);
            self.last_world_reset_count = game.world_reset_count();
        }
    }

    pub fn clear_given_props(&mut self, renderer: &mut Renderer, physics: &mut ItemWorld) {
        self.representation.reset(renderer);
        self.clear_session_props(renderer, physics);
    }

    pub(crate) fn clear_session_props(&mut self, renderer: &mut Renderer, physics: &mut ItemWorld) {
        for prop in self.given.drain(..) {
            let _removed = physics.remove(prop.key);
            despawn_visual(&prop, renderer);
        }
        self.trigger_labels = false;
        self.placement_physics = false;
        self.next_key = 0;
        physics.resume();
        let _scale_reset = physics.set_time_scale(1.0);
    }

    #[must_use]
    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({"props":self.given.iter().map(|prop|serde_json::json!({"key":prop.key,"model":prop.model,"physical":prop.physical,"character":prop.character})).collect::<Vec<_>>(), "form": self.representation.selected_id(), "placement_physics":self.placement_physics,"trigger_labels":self.trigger_labels})
    }

    pub fn hide_suspended_visual(&self, key: u64, renderer: &mut Renderer) {
        if let Some(id) = self
            .given
            .iter()
            .find(|prop| prop.key == key)
            .and_then(|prop| prop.character.as_deref())
        {
            let _hidden = renderer.set_runtime_character_visible(id, false);
        }
    }

    pub fn sync_visuals(&self, renderer: &mut Renderer, physics: &ItemWorld) {
        physics.for_each_pose(|key, pose| {
            if let Some(id) = self
                .given
                .iter()
                .find(|prop| prop.key == key)
                .and_then(|prop| prop.character.as_deref())
            {
                let _visible = renderer.set_runtime_character_visible(id, true);
                let _updated = renderer.set_runtime_character_pose(
                    id,
                    pose.position.to_array(),
                    pose.rotation.to_array(),
                );
            }
        });
    }

    pub fn visible_trigger_labels(
        &self,
        game: &Game,
        renderer: &mut Renderer,
    ) -> Vec<crate::engine::entities::developer::TriggerLabel> {
        if !self.trigger_labels {
            return Vec::new();
        }
        game.developer_trigger_labels()
            .into_iter()
            .filter(|label| {
                renderer.light_transmittance(
                    game.player_position.to_array(),
                    label.position.to_array(),
                    None,
                ) >= 0.99
            })
            .collect()
    }

    #[must_use]
    pub const fn trigger_labels_visible(&self) -> bool {
        self.trigger_labels
    }

    /// Exact session identity and model, allocated once when the prop is created.
    #[must_use]
    pub fn given_identity(&self, key: u64) -> Option<(&str, &str)> {
        self.given.iter().find_map(|prop| {
            (prop.key == key).then_some((prop.instance.as_str(), prop.model.as_str()))
        })
    }

    /// Model identity of one bounded session prop, shared by triggers and sound.
    #[must_use]
    pub fn given_model(&self, key: u64) -> Option<&str> {
        self.given
            .iter()
            .find_map(|prop| (prop.key == key).then_some(prop.model.as_str()))
    }

    #[must_use]
    pub fn given_key(&self, id: &str) -> Option<u64> {
        self.given
            .iter()
            .rev()
            .find_map(|prop| (prop.model == id).then_some(prop.key))
    }
}

fn release_held(physics: &mut ItemWorld) -> Result<(), String> {
    if physics.held().is_some() && !physics.release_held() {
        return Err("Held item cannot be safely released here. Move it clear and retry.".into());
    }
    Ok(())
}

fn despawn_visual(prop: &SessionProp, renderer: &mut Renderer) {
    if let Some(id) = prop.character.as_deref() {
        let _removed = renderer.despawn_runtime_character(id);
    } else {
        let _removed = renderer.despawn_runtime_model(prop.key);
    }
}
