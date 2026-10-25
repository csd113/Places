//! Small native-loop adapter. The physics world owns simulation and held-item
//! state; the renderer owns cached models, materials, culling and illumination.
use std::io::Write;
use std::time::Instant;

use glam::Vec3;

use super::FrameLoop;
use crate::engine::{level::LevelDef, physics::PlayerContext};
use crate::game::AppState;

const AUTHORED_ITEM_KEY_BASE: u64 = 1_u64 << 63;

#[derive(Clone, Copy)]
pub enum InteractionTarget {
    Item { key: u64, distance: f32 },
    Activate { key: u64, distance: f32 },
    World { index: usize, distance: f32 },
    Drop { key: u64 },
}

/// Presentation consumes the same resolved target as Interact. Ownership wins
/// over a stale target on the exact pickup frame, so carrying never shows a hand.
const fn interaction_hand_visible(target: Option<InteractionTarget>, held: Option<u64>) -> bool {
    held.is_none()
        && matches!(
            target,
            Some(
                InteractionTarget::Item {
                    key: _,
                    distance: _
                } | InteractionTarget::Activate {
                    key: _,
                    distance: _
                } | InteractionTarget::World {
                    index: _,
                    distance: _
                }
            )
        )
}

pub struct Items {
    pub world: crate::engine::physics::ItemWorld,
    pub authored: Vec<(u64, String, String)>,
    pub(super) miniatures: Option<super::mannequin_bowling::Rack>,
    trigger_bodies: Vec<crate::engine::physics::ItemTriggerBody>,
    pub input: crate::game::app::item_input::CarryInput,
    pub target: Option<InteractionTarget>,
    pub captures: std::collections::VecDeque<String>,
    pub quit_after_captures: bool,
    log: Option<std::fs::File>,
    release_blocked: bool,
    simulation_delta: Option<f32>,
    trace_bodies: bool,
    pub(super) carry: super::carry_runtime::CarryRuntime,
}

/// Prepared on the single loading worker. Cancellation drops this entire new
/// physics world while the displayed world's simulation remains owned by Items.
pub struct PreparedItems {
    world: crate::engine::physics::ItemWorld,
    authored: Vec<(u64, String, String)>,
    carry: super::carry_runtime::CarryRuntime,
    spawns: Vec<(u64, usize, [f32; 3])>,
}
impl PreparedItems {
    pub fn prepare(
        loaded: &crate::engine::loader::LoadedLevel,
        walls: &[crate::engine::collision::WallAabb],
        assets: &mut crate::engine::props::PropAssets,
    ) -> Result<Self, String> {
        let level = &loaded.level;
        let mut prepared = Self {
            world: crate::engine::physics::ItemWorld::from_level_with_materials(
                level,
                walls,
                &loaded.materials,
            ),
            authored: Vec::new(),
            carry: super::carry_runtime::CarryRuntime::from_level(level, &loaded.materials),
            spawns: Vec::new(),
        };
        let surfaces = crate::engine::level::LevelSurfaces::new(level);
        let ids = level.prop_instance_ids();
        let fixed = level
            .props
            .iter()
            .enumerate()
            .filter(|(_, prop)| prop.has_fixed_physics_shape());
        let movable = level.props.iter().enumerate().filter(|(_, prop)| {
            prop.physics
                .as_ref()
                .is_some_and(|definition| definition.motion.is_movable())
        });
        // Preserve collider/body insertion order: fixed shapes preceded movable
        // bodies before preparation moved off the UI thread.
        for (index, prop) in fixed.chain(movable) {
            crate::engine::resources::checkpoint()?;
            let Some(def) = &prop.physics else {
                continue;
            };
            if !prop.has_fixed_physics_shape() && !def.motion.is_movable() {
                continue;
            }
            let (min, max) = crate::engine::props::rigid_model_bounds(
                assets,
                loaded.catalog.assets(),
                &prop.model,
            )?;
            let position = Vec3::new(
                prop.x,
                surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0) + prop.y,
                prop.z,
            );
            if prop.has_fixed_physics_shape() {
                prepared.world.install_fixed_shape_oriented(
                    def,
                    (Vec3::from_array(min), Vec3::from_array(max)),
                    position,
                    prop.orientation(),
                    prop.scale,
                )?;
            } else {
                let key =
                    AUTHORED_ITEM_KEY_BASE.saturating_add(u64::try_from(index).unwrap_or(u64::MAX));
                prepared.world.spawn_authored_oriented(
                    key,
                    def,
                    (Vec3::from_array(min), Vec3::from_array(max)),
                    position,
                    prop.orientation(),
                    prop.scale,
                )?;
                prepared.spawns.push((key, index, position.to_array()));
                prepared.authored.push((
                    key,
                    ids.get(index).cloned().unwrap_or_default(),
                    prop.model.clone(),
                ));
            }
        }
        crate::engine::resources::checkpoint()?;
        Ok(prepared)
    }
}

impl Items {
    pub(crate) fn hand_visible(&self) -> bool {
        interaction_hand_visible(self.target, self.world.held())
    }
    pub(super) fn constrain_player_eye(&self, game: &crate::game::Game) -> Vec3 {
        self.world.constrain_player_eye(player_context(game))
    }
    pub(super) fn new() -> Self {
        Self {
            world: crate::engine::physics::ItemWorld::default(),
            authored: Vec::new(),
            miniatures: None,
            trigger_bodies: Vec::with_capacity(crate::engine::physics::MAX_BODIES),
            input: crate::game::app::item_input::CarryInput::default(),
            target: None,
            captures: std::collections::VecDeque::new(),
            quit_after_captures: false,
            log: open_physics_log(),
            release_blocked: false,
            simulation_delta: diagnostic_delta(),
            carry: super::carry_runtime::CarryRuntime::default(),
            trace_bodies: std::env::var("PLACES_BENCH").as_deref() == Ok("1")
                && std::env::var("PLACES_PHYSICS_TRACE").as_deref() == Ok("1"),
        }
    }
}

fn player_context(game: &crate::game::Game) -> PlayerContext {
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

fn diagnostic_delta() -> Option<f32> {
    if std::env::var("PLACES_BENCH").as_deref() != Ok("1") {
        return None;
    }
    std::env::var("PLACES_PHYSICS_FIXED_DELTA_SECONDS")
        .ok()?
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite() && *value > 0.0 && *value <= 1.0)
}

fn open_physics_log() -> Option<std::fs::File> {
    let path = std::env::var_os("PLACES_PHYSICS_LOG")?;
    let mut file = std::fs::File::create(path).ok()?;
    writeln!(file, "frame,body_count,dormant_bodies,active_bodies,sleeping_bodies,substeps,clamped_seconds,step_millis,item_update_millis,state,console_open").ok()?;
    Some(file)
}

impl FrameLoop<'_> {
    pub(super) fn unload_items(&mut self) {
        self.items.world = crate::engine::physics::ItemWorld::default();
        self.items.authored = Vec::new();
        self.items.miniatures = None;
        self.items.trigger_bodies = Vec::new();
        self.items.carry = super::carry_runtime::CarryRuntime::default();
        self.items.input.invalidate();
        self.items.target = None;
        self.items.release_blocked = false;
    }

    pub(super) fn activate_item_group(&mut self, group: &str) {
        if self.carry_group_command_blocked(group) {
            return;
        }
        match self.items.world.activate_group(group) {
            Ok(report) => self.trace.record(
                "physics_group_activated",
                self.load_generation,
                &serde_json::json!({"group":group,"activated_count":report.activated_count})
                    .to_string(),
            ),
            Err(error) => {
                crate::engine::logging::event(format!(
                    "[physics] activation `{group}` refused: {error}"
                ));
                self.trace.record(
                    "physics_group_activation_blocked",
                    self.load_generation,
                    &serde_json::json!({"group":group,"reason":error}).to_string(),
                );
            }
        }
    }

    pub(super) fn reset_item_group(&mut self, group: &str) {
        if self.carry_group_command_blocked(group) {
            return;
        }
        self.synchronize_physical_player();
        let before = self.bench.enabled().then(|| self.items.world.snapshots());
        let held_before = self.items.world.held();
        match self.items.world.reset_group(group) {
            Ok(report) => {
                if report.released_held.is_some() {
                    self.items.input.invalidate();
                }
                self.items.target = None;
                self.trace.record("physics_group_reset", self.load_generation, &serde_json::json!({"group":group,"reset_count":report.reset_count,"released_held":report.released_held}).to_string());
            }
            Err(error) => {
                crate::engine::logging::event(format!(
                    "[physics] reset `{group}` refused: {error}"
                ));
                self.trace.record(
                    "physics_group_reset_blocked",
                    self.load_generation,
                    &serde_json::json!({"group":group,"reason":error,"before":before,
                        "after":self.bench.enabled().then(||self.items.world.snapshots()),
                        "held_before":held_before,"held_after":self.items.world.held()})
                    .to_string(),
                );
            }
        }
    }
    pub(super) fn refresh_interaction_target(&mut self) {
        self.items.target = None;
        if self.game.app_state() != AppState::Playing
            || self.console.is_open()
            || !self.window_focused
            || self.load_intent.is_some()
            || self.game.noclip_enabled()
        {
            return;
        }
        if let Some(key) = self.items.world.held() {
            self.synchronize_physical_player();
            let blocker = self.items.world.first_unheld_body_hit(
                self.game.player_position,
                self.game.view_direction(),
                crate::engine::interact::MAX_INTERACTION_REACH_M,
            );
            self.items.target = Some(
                self.game
                    .carried_door_target(blocker.map(|(_, distance)| distance))
                    .map_or(InteractionTarget::Drop { key }, |(index, distance)| {
                        InteractionTarget::World { index, distance }
                    }),
            );
            return;
        }
        self.synchronize_physical_player();
        let eye = self.game.player_position;
        let view = self.game.view_direction();
        let world_target = self.game.interaction_target().and_then(|index| {
            self.game
                .interactables()
                .items()
                .get(index)
                .and_then(|item| {
                    crate::engine::interact::target_distance(
                        eye,
                        view,
                        item,
                        self.game.door_colliders(),
                    )
                })
                .map(|distance| (index, distance))
        });
        let body_target = self.items.world.first_body_hit(
            eye,
            view,
            crate::engine::interact::MAX_INTERACTION_REACH_M,
        );
        if let Some((key, distance)) = body_target
            && world_target.is_none_or(|(_, entry)| distance < entry)
        {
            if self.items.world.nearest_pickup(eye, view) == Some(key) {
                self.items.target = Some(InteractionTarget::Item { key, distance });
            } else if self.items.world.nearest_activation(eye, view) == Some(key) {
                self.items.target = Some(InteractionTarget::Activate { key, distance });
            }
            return;
        }
        self.items.target =
            world_target.map(|(index, distance)| InteractionTarget::World { index, distance });
    }

    pub(super) fn interaction_target_snapshot(&self) -> serde_json::Value {
        match self.items.target {
            Some(InteractionTarget::Item { key, distance }) => {
                serde_json::json!({"kind":"item","key":key,"distance":distance})
            }
            Some(InteractionTarget::Activate { key, distance }) => {
                serde_json::json!({"kind":"activate_physics","key":key,"distance":distance})
            }
            Some(InteractionTarget::World { index, distance }) => {
                serde_json::json!({"kind":"world","id":self.game.interactables().items().get(index).map(|item|item.id.as_str()),"distance":distance})
            }
            Some(InteractionTarget::Drop { key }) => serde_json::json!({"kind":"drop","key":key}),
            None => serde_json::Value::Null,
        }
    }
    pub(super) fn capture_item_frame(&mut self) {
        if let Some(name) = self.items.captures.pop_front() {
            self.trace_world(&format!("native_capture:{name}"));
            let Ok(paths) = crate::engine::paths::user() else {
                return;
            };
            let path = paths.data.join(format!("{name}.png"));
            super::write_capture(self.renderer, &path);
        }
        if self.items.quit_after_captures && self.items.captures.is_empty() {
            self.game.stop();
        }
    }
    pub(super) fn apply_item_script_action(&mut self, action: &crate::game::app::actions::Action) {
        match action {
            crate::game::app::actions::Action::Activate { id } => self.scripted_activate_actor(id),
            crate::game::app::actions::Action::AimItem { id } => self.scripted_aim_item(id),
            crate::game::app::actions::Action::Player {
                position,
                yaw,
                pitch,
            } => self.scripted_player_position(*position, *yaw, *pitch),
            crate::game::app::actions::Action::ItemPose {
                id,
                position,
                rotation,
            } => self.scripted_item_pose(id, *position, *rotation),
            crate::game::app::actions::Action::ItemVelocity {
                id,
                linear,
                angular,
            } => {
                let accepted = self.scripted_item_key(id).is_some_and(|key| {
                    self.items.world.set_fixture_velocity(
                        key,
                        Vec3::from_array(*linear),
                        Vec3::from_array(*angular),
                    )
                });
                if !accepted {
                    self.fatal_error = Some(format!("Native item velocity refused for `{id}`"));
                    self.game.stop();
                }
            }
            crate::game::app::actions::Action::Mouse {
                button: _,
                pressed,
                recapture,
            } => self.scripted_item_mouse(*pressed, *recapture),
            crate::game::app::actions::Action::Fixture { id, on } => {
                let _report = self.game.dispatch_actions(
                    &[crate::engine::level::ActionDef::SetLight {
                        target: Some(id.clone()),
                        on: *on,
                    }],
                    None,
                );
            }
            crate::game::app::actions::Action::Load { level: _ }
            | crate::game::app::actions::Action::NavigationGoal { id: _, position: _ }
            | crate::game::app::actions::Action::Escape {}
            | crate::game::app::actions::Action::ConsoleEnabled { enabled: _ }
            | crate::game::app::actions::Action::Key {
                key: _,
                pressed: _,
                repeat: _,
            }
            | crate::game::app::actions::Action::Text { text: _, submit: _ }
            | crate::game::app::actions::Action::AimEntity {
                id: _,
                yaw_offset_degrees: _,
            }
            | crate::game::app::actions::Action::Capture { name: _ }
            | crate::game::app::actions::Action::Checkpoint { name: _ }
            | crate::game::app::actions::Action::Resize {
                width: _,
                height: _,
            }
            | crate::game::app::actions::Action::Quality { level: _ }
            | crate::game::app::actions::Action::Lightmaps { quality: _ }
            | crate::game::app::actions::Action::LowLighting { enabled: _ }
            | crate::game::app::actions::Action::CausticsSource {
                id: _,
                on: _,
                intensity: _,
                color: _,
            }
            | crate::game::app::actions::Action::CausticsWater {
                index: _,
                tint: _,
                speed: _,
                strength: _,
            }
            | crate::game::app::actions::Action::View { yaw: _, pitch: _ }
            | crate::game::app::actions::Action::ViewAt { position: _ }
            | crate::game::app::actions::Action::Focus { focused: _ }
            | crate::game::app::actions::Action::Quit {}
            | crate::game::app::actions::Action::Ragdoll { id: _, enabled: _ } => {}
        }
    }

    fn scripted_activate_actor(&mut self, id: &str) {
        let index = self
            .game
            .interactables()
            .items()
            .iter()
            .position(|item| item.id == id && item.enabled);
        if let Some(actor_index) = index {
            if let Some(report) = self.game.dispatch_interaction_at(Some(actor_index)) {
                self.interaction_audio_feedback(actor_index, &report);
            }
        } else {
            self.fatal_error = Some(format!("Native actor activation refused for `{id}`"));
            self.game.stop();
        }
    }

    fn scripted_aim_item(&mut self, id: &str) {
        if let Some(target) = self
            .scripted_item_key(id)
            .and_then(|key| self.items.world.body_center(key))
        {
            let direction = Vec3::new(
                target.x - self.game.player_position.x,
                target.y - self.game.player_position.y,
                target.z - self.game.player_position.z,
            );
            self.game.player_yaw = direction.x.atan2(-direction.z);
            self.game.player_pitch = direction
                .y
                .atan2(direction.x.hypot(direction.z))
                .clamp(-85_f32.to_radians(), 85_f32.to_radians());
        } else {
            self.fatal_error = Some(format!("Native item aim refused for `{id}`"));
            self.game.stop();
        }
    }

    fn scripted_item_key(&self, id: &str) -> Option<u64> {
        self.items
            .authored
            .iter()
            .find_map(|(key, authored_id, _)| (authored_id == id).then_some(*key))
            .or_else(|| self.console_runtime.given_key(id))
    }

    pub(super) fn item_lighting_snapshots(&self) -> Vec<serde_json::Value> {
        self.items
            .world
            .snapshots()
            .iter()
            .filter_map(|body| self.renderer.runtime_model_lighting_snapshot(body.key))
            .collect()
    }

    pub(super) fn observe_item_mouse(&mut self, event: &sdl3::event::Event) {
        let pressed = if let sdl3::event::Event::MouseButtonDown {
            mouse_btn: sdl3::mouse::MouseButton::Left,
            timestamp: _,
            window_id: _,
            which: _,
            clicks: _,
            x: _,
            y: _,
        } = event
        {
            true
        } else if let sdl3::event::Event::MouseButtonUp {
            mouse_btn: sdl3::mouse::MouseButton::Left,
            timestamp: _,
            window_id: _,
            which: _,
            clicks: _,
            x: _,
            y: _,
        } = event
        {
            false
        } else {
            return;
        };
        self.items.input.mouse_button(
            pressed,
            crate::game::app::item_input::MouseContext {
                gameplay: self.game.app_state() == AppState::Playing
                    && self.window_focused
                    && !self.console.is_open()
                    && self.load_intent.is_none(),
                captured: self.sdl.mouse().relative_mouse_mode(self.window),
                held: self.items.world.held(),
            },
        );
    }

    pub(super) fn scripted_item_mouse(&mut self, pressed: bool, recapture: bool) {
        if recapture && pressed {
            self.sdl.mouse().set_relative_mouse_mode(self.window, false);
        }
        let event = if pressed {
            sdl3::event::Event::MouseButtonDown {
                timestamp: 0,
                window_id: self.window.id(),
                which: 0,
                mouse_btn: sdl3::mouse::MouseButton::Left,
                clicks: 1,
                x: 0.0,
                y: 0.0,
            }
        } else {
            sdl3::event::Event::MouseButtonUp {
                timestamp: 0,
                window_id: self.window.id(),
                which: 0,
                mouse_btn: sdl3::mouse::MouseButton::Left,
                clicks: 1,
                x: 0.0,
                y: 0.0,
            }
        };
        let _handled = self.handle_event(&event);
    }

    pub(super) fn scripted_item_pose(&mut self, id: &str, position: [f32; 3], rotation: [f32; 4]) {
        let key = self.scripted_item_key(id);
        let accepted = key.is_some_and(|body_key| {
            self.items.world.set_fixture_pose(
                body_key,
                crate::engine::physics::ItemPose {
                    position: Vec3::from_array(position),
                    rotation: glam::Quat::from_array(rotation),
                },
            )
        });
        if !accepted {
            self.fatal_error = Some(format!("Native item pose refused for `{id}`"));
            self.game.stop();
        }
    }

    pub(super) fn dispatch_player_interactions(&mut self) {
        if self.outcome.is_some() || self.message.is_some() {
            return;
        }
        self.refresh_interaction_target();
        if self.game.take_interact_press() && self.window_focused {
            self.dispatch_interaction();
        }
        let allowed = self.game.app_state() == AppState::Playing
            && self.window_focused
            && !self.console.is_open()
            && self.load_intent.is_none()
            && !self.game.noclip_enabled();
        if let Some(key) = self.items.input.take_throw(allowed)
            && self.items.world.held() == Some(key)
        {
            self.synchronize_physical_player();
            self.items.release_blocked = !self.items.world.throw(self.game.view_direction());
            self.trace_world(if self.items.release_blocked {
                "item_throw_blocked"
            } else {
                "item_thrown"
            });
        }
    }
    pub(super) fn update_player_input(&mut self) {
        if self.outcome.is_some() || self.message.is_some() {
            return;
        }
        self.apply_move_script();
        self.observe_held_item();
        self.game
            .entities_mut()
            .set_navigation_obstacles(self.items.world.navigation_obstacles());
        if self.window_focused
            && self.load_intent.is_none()
            && self.game.app_state() == crate::game::AppState::Playing
            && self
                .input_handler
                .state()
                .was_pressed(crate::game::input::Control::Flashlight)
        {
            self.flashlight.toggle();
            self.trace_world("flashlight_toggled");
        }
        self.game.update_player_movement_with_constraints(
            self.input_handler.state_mut(),
            self.settings,
            |game| self.items.constrain_player_eye(game),
            |game| self.items.world.player_pose_clear(player_context(game)),
        );
    }

    pub(super) fn scripted_player_position(&mut self, position: [f32; 3], yaw: f32, pitch: f32) {
        self.game.set_benchmark_player_position(
            Vec3::from_array(position),
            yaw.to_radians(),
            pitch.to_radians(),
        );
        self.synchronize_physical_player();
    }

    pub(super) fn synchronize_physical_player(&mut self) {
        self.items
            .world
            .sync_context(player_context(self.game), self.game.door_colliders());
    }
    pub(super) fn apply_live_scene_commands(&mut self) {
        self.apply_world_commands();
        self.apply_bench_lighting_actions();
        self.update_physical_items();
        self.update_mannequin_pins();
        self.update_item_triggers(false);
        self.reconcile_gameplay_reset();
        self.apply_world_commands();
    }

    fn physical_player_context(&self) -> PlayerContext {
        player_context(self.game)
    }
    pub(crate) fn install_physical_items(&mut self, level: &LevelDef, prepared: PreparedItems) {
        self.items.world = prepared.world;
        self.items.authored = prepared.authored;
        self.items.miniatures = None;
        self.items.carry = prepared.carry;
        self.items.target = None;
        self.items.release_blocked = false;
        self.items.input = crate::game::app::item_input::CarryInput::default();
        self.restore_carry_visibility(level);
        let render_spawns: Vec<_> = prepared
            .spawns
            .iter()
            .filter_map(|(key, index, position)| Some((*key, level.props.get(*index)?, *position)))
            .collect();
        if let Err(index) = self.renderer.spawn_runtime_props(&render_spawns)
            && let Some((key, prop, _)) = render_spawns.get(index)
        {
            let _removed = self.items.world.remove(*key);
            self.fatal_error = Some(format!(
                "Renderer refused physical prop `{}` within its resource budget",
                prop.model
            ));
            self.game.stop();
        }
    }

    pub(super) fn update_item_triggers(&mut self, seed: bool) {
        self.items.trigger_bodies.clear();
        self.items
            .world
            .for_each_trigger_body(|body| self.items.trigger_bodies.push(body));
        let actors = trigger_items(
            &self.items.trigger_bodies,
            &self.items.authored,
            &self.console_runtime,
        );
        self.game
            .entities_mut()
            .observe_physical_items(actors.clone());
        if seed {
            self.game.seed_volume_occupants_iter(actors);
        } else {
            self.game.update_volume_occupants_iter(actors);
        }
    }

    pub(crate) fn update_physical_items(&mut self) {
        let delta = if self.simulation_active() {
            self.items
                .simulation_delta
                .unwrap_or_else(|| self.visual_delta_seconds())
        } else {
            0.0
        };
        let started = Instant::now();
        let context = PlayerContext {
            half: self.game.body_half(),
            eye: self.game.player_position,
            feet: Vec3::new(
                self.game.player_position.x,
                self.game.feet_y(),
                self.game.player_position.z,
            ),
            height: self.game.body_height(),
            view: self.game.view_direction(),
            yaw: self.game.player_yaw,
            active: !self.game.noclip_enabled(),
        };
        self.items.world.sync_water(&self.game.water);
        self.items
            .world
            .step(delta, context, self.game.door_colliders());
        for recovery in self.items.world.take_recoveries() {
            if recovery.released_held {
                self.items.input.invalidate();
                self.items.target = None;
            }
            if recovery.outcome == crate::engine::physics::RecoveryOutcome::Suspended {
                let _hidden = self.renderer.set_runtime_model_visible(recovery.key, false);
                self.console_runtime
                    .hide_suspended_visual(recovery.key, self.renderer);
            }
            crate::engine::logging::event(format!(
                "[physics] item {} recovery: {:?} ({:?})",
                recovery.key, recovery.outcome, recovery.reason
            ));
            self.trace.record(
                "physics_recovery",
                self.load_generation,
                &serde_json::to_string(&recovery).unwrap_or_default(),
            );
        }
        let elapsed = started.elapsed().as_secs_f64() * 1000.0_f64;
        self.items.world.for_each_pose(|key, pose| {
            let _visible = self.renderer.set_runtime_model_visible(key, true);
            let _changed = self.renderer.set_runtime_model_pose(
                key,
                pose.position.to_array(),
                pose.rotation.to_array(),
            );
        });
        let stats = self.items.world.stats();
        if self.items.trace_bodies {
            self.trace.record("physics_frame", self.load_generation, &serde_json::json!({"frame": self.game.frame_count(), "held": self.items.world.held(), "stats": stats, "bodies": self.items.world.snapshots(), "contacts": self.items.world.contact_pairs(), "contact_impulses": self.items.world.contact_impulses()}).to_string());
        }
        if let Some(file) = self.items.log.as_mut()
            && writeln!(
                file,
                "{},{},{},{},{},{},{:.6},{:.6},{elapsed:.6},{:?},{}",
                self.game.frame_count(),
                stats.body_count,
                stats.dormant_bodies,
                stats.active_bodies,
                stats.sleeping_bodies,
                stats.substeps,
                stats.clamped_seconds,
                stats.step_millis,
                self.game.app_state(),
                self.console.is_open()
            )
            .is_err()
        {
            self.items.log = None;
        }
    }

    pub(crate) fn dispatch_item_interaction(&mut self) -> bool {
        if self.console.is_open()
            || self.game.app_state() != AppState::Playing
            || self.game.noclip_enabled()
        {
            return false;
        }
        self.items
            .world
            .sync_context(self.physical_player_context(), self.game.door_colliders());
        if matches!(self.items.target, Some(InteractionTarget::Drop { key: _ })) {
            self.items.release_blocked = !self.items.world.drop();
            self.trace_world(if self.items.release_blocked {
                "item_drop_blocked"
            } else {
                "item_dropped"
            });
            return true;
        }
        if let Some(InteractionTarget::Activate { key, distance: _ }) = self.items.target {
            let activated = self.items.world.activate(key);
            self.trace_world(if activated {
                "item_activated"
            } else {
                "item_activation_blocked"
            });
            return true;
        }
        let eye = self.game.player_position;
        let view = self.game.view_direction();
        if let Some(InteractionTarget::Item { key, distance: _ }) = self.items.target {
            let picked = self
                .items
                .world
                .pickup(key, eye, view, self.game.player_yaw);
            self.items.release_blocked = false;
            if picked {
                self.play_item_sound(key);
            }
            self.trace_world(if picked {
                "item_picked_up"
            } else {
                "item_pickup_blocked"
            });
            return true;
        }
        false
    }
}

fn trigger_items<'a>(
    bodies: &'a [crate::engine::physics::ItemTriggerBody],
    authored: &'a [(u64, String, String)],
    session: &'a crate::game::gameplay_runtime::Runtime,
) -> impl Iterator<Item = crate::engine::entities::TriggerItem<'a>> + Clone {
    bodies.iter().filter_map(move |body| {
        let (instance, asset) = authored
            .iter()
            .find_map(|(key, instance, asset)| {
                (*key == body.key).then_some((instance.as_str(), asset.as_str()))
            })
            .or_else(|| session.given_identity(body.key))?;
        Some(crate::engine::entities::TriggerItem {
            key: body.key,
            instance,
            asset,
            from: body.previous,
            to: body.current,
            path: Some(body.path()),
            half_extents: body.half_extents,
            held: body.held,
            sleeping: body.sleeping,
            up_y: body.up_y,
            centre_path: Some(body.centre_path()),
        })
    })
}

impl Items {
    pub(super) fn append_authoring_debug(
        &self,
        vertices: &mut Vec<crate::engine::render::Vertex>,
        game: &crate::game::Game,
        camera: &crate::engine::render::RenderCamera,
        drawable: crate::engine::render::DrawableSize,
    ) {
        let mut nearest = None;
        self.world.for_each_trigger_body(|body| {
            let centre = body.centre_path().last().copied().unwrap_or(body.current);
            let offset = Vec3::new(
                centre.x - game.player_position.x,
                centre.y - game.player_position.y,
                centre.z - game.player_position.z,
            );
            if offset.length_squared() > 144.0 || offset.dot(game.view_direction()) < 0.0 {
                return;
            }
            let rank = if body.held {
                0.0
            } else {
                offset.length_squared()
            };
            if nearest
                .as_ref()
                .is_none_or(|(previous, _, _)| rank < *previous)
            {
                nearest = Some((rank, body, centre));
            }
        });
        let Some((_, body, centre)) = nearest else {
            return;
        };
        let Some((_, instance, asset)) = self.authored.iter().find(|(key, _, _)| *key == body.key)
        else {
            return;
        };
        let state = if body.held {
            "held"
        } else if self.world.body_is_dormant(body.key) {
            "dormant"
        } else if body.sleeping {
            "sleeping"
        } else {
            "awake"
        };
        super::world_labels::append_authoring_marker(
            vertices,
            &format!(
                "{instance}: {state} centre {:.3} {:.3} {:.3}",
                centre.x, centre.y, centre.z
            ),
            centre,
            camera,
            drawable,
        );
        let item = crate::engine::entities::volumes::TriggerItem {
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
        };
        for (id, min, max, accepted) in game
            .entities()
            .developer_item_receipts(&item)
            .into_iter()
            .take(4)
        {
            super::world_labels::append_authoring_box(
                vertices, min, max, accepted, camera, drawable,
            );
            super::world_labels::append_authoring_marker(
                vertices,
                &format!(
                    "{id}: {}",
                    if accepted {
                        "eligible now"
                    } else {
                        "outside / filter / awake"
                    }
                ),
                Vec3::from_array(max),
                camera,
                drawable,
            );
        }
    }
    pub(super) fn append_release_hint(
        &mut self,
        vertices: &mut Vec<crate::engine::render::Vertex>,
    ) {
        if self.release_blocked && (self.world.held().is_none() || self.world.can_drop()) {
            self.release_blocked = false;
        }
        if (self.release_blocked || self.world.held_movement_blocked())
            && self.world.held().is_some()
        {
            crate::game::ui::draw_text(
                vertices,
                if self.release_blocked {
                    "Make room to set this down."
                } else {
                    "The item cannot follow here."
                },
                24.0,
                238.0,
                0.75,
                [0.9, 0.75, 0.5],
            );
        }
    }
}

#[cfg(test)]
#[path = "../tests/release_hint.rs"]
mod tests;
