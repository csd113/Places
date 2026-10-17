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
    trigger_bodies: Vec<crate::engine::physics::ItemTriggerBody>,
    pub input: crate::game::app::item_input::CarryInput,
    pub target: Option<InteractionTarget>,
    pub captures: std::collections::VecDeque<String>,
    pub quit_after_captures: bool,
    log: Option<std::fs::File>,
    release_blocked: bool,
    simulation_delta: Option<f32>,
    trace_bodies: bool,
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
            trigger_bodies: Vec::with_capacity(crate::engine::physics::MAX_BODIES),
            input: crate::game::app::item_input::CarryInput::default(),
            target: None,
            captures: std::collections::VecDeque::new(),
            quit_after_captures: false,
            log: open_physics_log(),
            release_blocked: false,
            simulation_delta: diagnostic_delta(),
            trace_bodies: std::env::var("PLACES_BENCH").as_deref() == Ok("1")
                && std::env::var("PLACES_PHYSICS_TRACE").as_deref() == Ok("1"),
        }
    }
}

fn player_context(game: &crate::game::Game) -> PlayerContext {
    PlayerContext {
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
    pub(super) fn activate_item_group(&mut self, group: &str) {
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
            self.items.target = Some(InteractionTarget::Drop { key });
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
            | crate::game::app::actions::Action::Focus { focused: _ }
            | crate::game::app::actions::Action::Quit {} => {}
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
        self.apply_move_script();
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
        self.update_item_triggers(false);
        self.reconcile_gameplay_reset();
        self.apply_world_commands();
    }

    fn physical_player_context(&self) -> PlayerContext {
        player_context(self.game)
    }
    pub(crate) fn install_physical_items(
        &mut self,
        level: &LevelDef,
        materials: &crate::engine::materials::MaterialTable,
    ) {
        self.items.world = crate::engine::physics::ItemWorld::from_level_with_materials(
            level,
            self.game.walls(),
            materials,
        );
        self.items.authored.clear();
        self.items.target = None;
        self.items.release_blocked = false;
        self.items.input = crate::game::app::item_input::CarryInput::default();
        let surfaces = crate::engine::level::LevelSurfaces::new(level);
        for prop in level
            .props
            .iter()
            .filter(|prop| prop.has_fixed_physics_shape())
        {
            let Some(def) = prop.physics.as_ref() else {
                continue;
            };
            let position = Vec3::new(
                prop.x,
                surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0) + prop.y,
                prop.z,
            );
            let result = self
                .renderer
                .rigid_runtime_model_bounds(&prop.model)
                .and_then(|(min, max)| {
                    self.items.world.install_fixed_shape(
                        def,
                        (Vec3::from_array(min), Vec3::from_array(max)),
                        position,
                        prop.rotation_degrees.to_radians(),
                        prop.scale,
                    )
                });
            if let Err(error) = result {
                self.fatal_error = Some(format!(
                    "Cannot install fixed physical prop `{}`: {error}",
                    prop.model
                ));
                self.game.stop();
                return;
            }
        }
        let prop_instance_ids = level.prop_instance_ids();
        for (index, prop) in level.props.iter().enumerate() {
            let Some(def) = prop.physics.as_ref().filter(|def| def.motion.is_movable()) else {
                continue;
            };
            let key =
                AUTHORED_ITEM_KEY_BASE.saturating_add(u64::try_from(index).unwrap_or(u64::MAX));
            let position = Vec3::new(
                prop.x,
                surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0) + prop.y,
                prop.z,
            );
            let result = self
                .renderer
                .rigid_runtime_model_bounds(&prop.model)
                .and_then(|(min, max)| {
                    self.items.world.spawn_authored(
                        key,
                        def,
                        (Vec3::from_array(min), Vec3::from_array(max)),
                        position,
                        prop.rotation_degrees.to_radians(),
                        prop.scale,
                    )
                });
            if let Err(error) = result {
                self.fatal_error = Some(format!(
                    "Cannot install physical prop `{}`: {error}",
                    prop.model
                ));
                self.game.stop();
                return;
            }
            if !self
                .renderer
                .spawn_runtime_prop(key, prop, position.to_array())
            {
                let _removed = self.items.world.remove(key);
                self.fatal_error = Some(format!(
                    "Renderer refused physical prop `{}` within its resource budget",
                    prop.model
                ));
                self.game.stop();
                return;
            }
            self.items.authored.push((
                key,
                prop_instance_ids.get(index).cloned().unwrap_or_default(),
                prop.model.clone(),
            ));
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
        if seed {
            self.game.seed_volume_occupants_iter(actors);
        } else {
            self.game.update_volume_occupants_iter(actors);
        }
    }

    pub(crate) fn update_physical_items(&mut self) {
        let delta = if self.gameplay_active() {
            self.items
                .simulation_delta
                .unwrap_or_else(|| self.visual_delta_seconds())
        } else {
            0.0
        };
        let started = Instant::now();
        let context = PlayerContext {
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
        })
    })
}
