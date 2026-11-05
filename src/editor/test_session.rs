//! Disposable first-person mode: production simulation owners, no document or profile writes.
mod commands;
mod physics;

pub(super) use physics::PreparedPhysics;

use std::collections::VecDeque;

use glam::Vec3;
use sdl3::event::Event;
use sdl3::keyboard::Keycode;
use sdl3::mouse::MouseButton;

use crate::engine::audio::{AudioSettings, AudioSystem, Listener};
use crate::engine::render::{RenderCamera, Renderer};
use crate::game::input::{Control, InputHandler};
use crate::game::play_spawn::Pose;
use crate::game::settings::Settings;
use crate::game::{AppState, Game};

const LIMITATIONS: &str = "Quick test: approximate lighting. Cross-level travel, saved checkpoints, player forms, ragdolls, mannequin pin games, actor carry/use assemblies require a final package test.";

// The authoring schema bounds each message's text. This additional session cap
// bounds the backlog when one handler emits several messages in a single tick.
const MAX_MESSAGES: usize = 16;

struct Message {
    title: String,
    rows: Vec<String>,
    page: usize,
    completed: bool,
}

pub(super) struct MessageView<'a> {
    pub title: &'a str,
    pub rows: &'a [String],
    pub page: usize,
    pub completed: bool,
}
pub(super) struct Session {
    game: Game,
    physics: PreparedPhysics,
    input: InputHandler,
    settings: Settings,
    warning: String,
    status: String,
    messages: VecDeque<Message>,
    throw_requested: bool,
    flashlight: bool,
    interactions: u64,
    world_reset: u64,
    seen_diagnostics: usize,
    audio_stats: crate::engine::audio::AudioStats,
    audio: crate::game::audio::WorldAudio,
}

impl Session {
    pub fn new(
        prepared: super::test_prepare::PreparedTest,
        pose: Option<Pose>,
        renderer: &mut Renderer,
        audio: &mut AudioSystem,
    ) -> Result<Self, String> {
        let spawn = crate::game::spawn_position(&prepared.level);
        let yaw = prepared.level.spawn.yaw_degrees.to_radians();
        let mut game = Game::new(spawn, yaw, prepared.world);
        // Game recovery may choose a safe fallback for the player application.
        // Authoring must instead report the exact authored or temporary pose.
        game.apply_play_pose(pose.unwrap_or(Pose::new(spawn, yaw, 0.0)?))?;
        if !prepared
            .physics
            .world
            .player_pose_clear(physics::player_context(&game))
        {
            return Err("Test spawn overlaps a movable physics item. Choose a clear floor.".into());
        }
        if game.entities().script_failed() {
            return Err(format!(
                "Test script initialization: {}",
                game.entities().script_diagnostics().join(" | "),
            ));
        }
        prepared.physics.install(&prepared.level, renderer)?;
        audio.clear_world();
        audio.set_acoustic_scene(crate::engine::audio::acoustics::AcousticScene::build(
            &prepared.level,
        ));
        game.set_app_state(AppState::Playing);
        game.reset_timing();
        let mut session = Self {
            game,
            physics: prepared.physics,
            input: InputHandler::new(),
            settings: Settings::default(),
            warning: prepared.warning,
            status: "Testing the current unsaved scene.".into(),
            messages: VecDeque::new(),
            throw_requested: false,
            flashlight: false,
            interactions: 0,
            world_reset: 0,
            seen_diagnostics: 0,
            audio_stats: audio.stats(),
            audio: prepared.audio,
        };
        session.audio.light_toggles(&session.game.light_states());
        session.physics.update_triggers(&mut session.game, true);
        session.apply_commands(renderer, audio)?;
        session.update_script_status()?;
        Ok(session)
    }

    pub fn event(&mut self, event: &Event) {
        if self.message_event(event) {
            return;
        }

        if crate::game::input::pressed_key(event) == Some(Keycode::F1) {
            self.game.request_hint();
            return;
        }
        self.input
            .handle_gameplay_event(event, &self.settings.bindings);
        if matches!(
            event,
            Event::MouseButtonDown {
                mouse_btn: MouseButton::Left,
                timestamp: _,
                window_id: _,
                which: _,
                clicks: _,
                x: _,
                y: _,
            }
        ) {
            self.throw_requested = true;
        }
    }

    pub fn update(
        &mut self,
        seconds: f32,
        renderer: &mut Renderer,
        audio: &mut AudioSystem,
    ) -> Result<(), String> {
        if self.pause_for_message(audio) {
            return Ok(());
        }

        self.game.update_timing();
        self.game.set_benchmark_simulation_delta(seconds);
        if self.input.state().was_pressed(Control::Flashlight) {
            self.flashlight = !self.flashlight;
        }
        self.game
            .entities_mut()
            .set_navigation_obstacles(self.physics.world.navigation_obstacles());
        let physical = &self.physics.world;
        self.game.update_player_movement_with_constraints(
            self.input.state_mut(),
            &self.settings,
            |game| physical.constrain_player_eye(physics::player_context(game)),
            |game| physical.player_pose_clear(physics::player_context(game)),
        );
        if self.game.world_reset_count() != self.world_reset {
            return Err(
                "The world requested a complete reset. Return to editing and restart this test."
                    .into(),
            );
        }
        self.physics.world.sync_context(
            physics::player_context(&self.game),
            self.game.door_colliders(),
        );
        if self.game.take_interact_press() {
            self.interact(audio)?;
        }
        self.apply_commands(renderer, audio)?;
        self.update_script_status()?;
        if self.pause_for_message(audio) {
            return Ok(());
        }
        if std::mem::take(&mut self.throw_requested) && self.physics.world.held().is_some() {
            self.status = if self.physics.world.throw(self.game.view_direction()) {
                "Threw held item".into()
            } else {
                "Throw blocked: move the item clear".into()
            };
        }
        self.audio.update_elevator_ride(&self.game, true, audio);
        for (position, cue) in self.game.entities_mut().take_elevator_sounds() {
            let _played = audio.play(
                cue,
                crate::engine::audio::PlayRequest {
                    position: Some(position),
                    spatial: Some(true),
                    attenuation: Some([1.5, 12.0]),
                    gain: 0.7,
                    ..crate::engine::audio::PlayRequest::default()
                },
            );
        }
        renderer.sync_doors(self.game.doors());
        renderer.sync_elevators(self.game.entities().elevators());
        let toggles = self.game.take_light_toggles();
        renderer.apply_light_toggles(&toggles);
        self.audio.light_toggles(&toggles);
        self.physics.step(&mut self.game, seconds, renderer);
        self.apply_commands(renderer, audio)?;
        self.update_script_status()?;
        if self.pause_for_message(audio) {
            return Ok(());
        }
        self.advance_presentation(seconds, renderer);
        if let Some(step) = self.audio.footstep(&mut self.game, &self.physics.world) {
            let _played = audio.play(step.cue, step.request());
        }
        self.audio.update_doors(
            self.game.doors(),
            self.game.delta_seconds(),
            audio,
            |_, _, _| {},
        );
        self.audio
            .update_environment(self.game.player_position, seconds, audio);
        self.update_audio(audio, false);
        self.update_script_status()?;
        if !self.game.is_running() {
            return Err("The test runtime stopped; all temporary state was discarded.".into());
        }
        Ok(())
    }

    fn enqueue_message(
        &mut self,
        title: String,
        lines: &[String],
        completed: bool,
    ) -> Result<(), String> {
        if self.messages.len() >= MAX_MESSAGES {
            return Err("Quick test message queue exceeded 16 messages; return to editing and reduce the script's message burst.".into());
        }
        self.messages.push_back(Message {
            title,
            rows: crate::game::ui::wrap_guidance(lines),
            page: 0,
            completed,
        });
        self.input.clear_gameplay_inputs();
        self.throw_requested = false;
        self.game.reset_timing();
        Ok(())
    }

    fn message_event(&mut self, event: &Event) -> bool {
        let Some(message) = self.messages.front_mut() else {
            return false;
        };
        let mut dismiss = false;
        if let Some(key) = crate::game::input::pressed_key(event) {
            match crate::game::ui::guidance_navigation(key, message.page, message.rows.len()) {
                crate::game::ui::GuidanceNavigation::Page(page) => message.page = page,
                crate::game::ui::GuidanceNavigation::Dismiss => {
                    // Escape belongs to the editor's Stop contract, including
                    // when this adapter is called by verification tooling.
                    dismiss = !message.completed && key == Keycode::Return;
                }
                crate::game::ui::GuidanceNavigation::Ignored => {}
            }
        }
        if dismiss {
            drop(self.messages.pop_front());
        }
        self.input.clear_gameplay_inputs();
        self.throw_requested = false;
        self.game.reset_timing();
        true
    }

    fn pause_for_message(&mut self, audio: &mut AudioSystem) -> bool {
        if self.messages.is_empty() {
            return false;
        }
        self.input.clear_gameplay_inputs();
        self.throw_requested = false;
        self.game.reset_timing();
        self.audio.reset_movement();
        self.update_audio(audio, true);
        true
    }

    pub fn message_view(&self) -> Option<MessageView<'_>> {
        self.messages.front().map(|message| MessageView {
            title: &message.title,
            rows: &message.rows,
            page: message.page,
            completed: message.completed,
        })
    }
    fn update_script_status(&mut self) -> Result<(), String> {
        let entities = self.game.entities();
        if entities.script_failed() {
            return Err(format!(
                "Test script: {}",
                entities.script_diagnostics().join(" | ")
            ));
        }
        let diagnostics = entities.script_diagnostics();
        if diagnostics.len() != self.seen_diagnostics {
            if let Some(notice) = diagnostics.last() {
                self.status.clone_from(notice);
            }
            self.seen_diagnostics = diagnostics.len();
        }
        Ok(())
    }

    fn advance_presentation(&mut self, seconds: f32, renderer: &mut Renderer) {
        let delta = self.game.scale_world_delta(seconds);
        let characters = renderer.advance_characters(delta, self.game.entity_frames());
        renderer.update_water_player(self.game.player_position, self.game.feet_y());
        let _updated = renderer.update_dynamic(delta);
        renderer.sync_characters();
        for id in &characters.finished {
            self.game.entities_mut().notify_animation_complete(id, "");
        }
        // The native player shares this same attachment/visibility policy.
        crate::game::install_flashlight(
            renderer,
            self.flashlight,
            self.game.player_position,
            self.game.player_yaw,
            self.game.player_pitch,
        );
        let world_delta = self.game.world_delta_seconds();
        let eye = self.game.player_position;
        self.game
            .entities_mut()
            .update_light_observations(world_delta, eye, |id, point| {
                renderer.flashlight_illumination(point.to_array(), Some(id))
            });
    }

    fn update_audio(&mut self, audio: &mut AudioSystem, paused: bool) {
        audio.update(
            Listener {
                position: self.game.player_position,
                forward: self.game.view_direction(),
                up: Vec3::Y,
            },
            paused,
            self.physics.world.simulation_state().time_scale,
            AudioSettings::default(),
        );
        audio.update_source_positions(|source| {
            self.audio
                .source_position(source)
                .or_else(|| self.game.entities().audio_source_position(source.0))
        });
        audio.update_acoustic_doors(self.game.doors());
        audio.drain_events(|event| self.game.entities_mut().observe_audio_start(event));
        self.audio_stats = audio.stats();
    }

    pub const fn camera(&self) -> RenderCamera {
        RenderCamera::new(
            self.game.player_position,
            self.game.player_yaw,
            self.game.player_pitch,
            self.settings.fov_degrees,
        )
    }

    pub fn warning(&self) -> &str {
        &self.warning
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    /// Same live AI report consumed by the player diagnostics, kept in memory.
    pub fn trace(&self) -> String {
        let aimed = self
            .game
            .interaction_target()
            .and_then(|index| self.game.interactables().get(index))
            .map_or("-", |item| item.id.as_str());
        format!(
            "frame={} sim_time={:.2} stimuli={} sequences={} aimed={}\n{}",
            self.game.frame_count(),
            self.game.entities().sim_time(),
            self.game.entities().stimuli().len(),
            self.game.entities().active_sequences(),
            aimed,
            self.game.entities().ai().debug_report(),
        )
    }
    pub fn snapshot(&self) -> serde_json::Value {
        let stimuli: Vec<_> = self.game.entities().stimuli().iter().map(|noise| serde_json::json!({
            "position":noise.position.to_array(),"radius":noise.audible_radius(0.0),
            "category":noise.category,"source":noise.source.map(|handle|[handle.index(),handle.generation()]),
            "age":noise.age,"stance":noise.player_movement,"serial":noise.movement_serial,
        })).collect();
        let agents: Vec<_> = self.game.entities().ai().agents().iter().map(|agent| serde_json::json!({
            "id":agent.instance_id,"position":agent.position.to_array(),"clock":agent.clock,
            "perception_phase_time":agent.last_perception_at,
            "typed_sense_sample_time":agent.next_task_sense - crate::engine::ai::PERCEPTION_INTERVAL_S,
            "contact":agent.contact.as_ref().map(|contact|serde_json::json!({"position":contact.position.to_array(),"heard_only":contact.heard_only,"age":contact.age,"role":contact.role})),
            "tasks":agent.controller.runs.iter().map(|run|serde_json::json!({"key":run.request.key,"serial":run.token.serial})).collect::<Vec<_>>(),
        })).collect();
        let hearing = serde_json::json!({"time":self.game.entities().sim_time(),"stimuli":stimuli,"agents":agents});
        serde_json::json!({
            "hearing": hearing,
            "eye": self.game.player_position.to_array(),
            "yaw": self.game.player_yaw,
            "pitch": self.game.player_pitch,
            "feet_y": self.game.feet_y(),
            "doors": self.game.doors().checkpoint(),
            "physics": self.physics.world.stats(),
            "bodies": self.physics.world.snapshots(),
            "held_item": self.physics.world.held(),
            "held_controls": crate::game::input::ALL_CONTROLS.iter().filter(|control| self.input.state().is_held(**control)).count(),
            "interaction_count": self.interactions,
            "interaction_target": self.target_snapshot(),
            "scripts": self.game.entities().script_resources(),
            "script_diagnostics": self.game.entities().script_diagnostics(),
            "script_failed": self.game.entities().script_failed(),
            "navigation_ready": self.game.navigation().is_some(),
            "audio": self.audio_stats,
            "warning": self.warning,
            "limitations": LIMITATIONS,
            "status": self.status(),
            "message": self.messages.front().map(|message| serde_json::json!({
                "title": message.title, "rows": message.rows, "page": message.page,
                "page_count": message.rows.len().div_ceil(6).max(1),
                "completed": message.completed,
            })),
            "queued_messages": self.messages.len().saturating_sub(1),
            "temporary": true,
        })
    }

    pub fn stop(&mut self, audio: &mut AudioSystem) {
        self.input.clear_gameplay_inputs();
        self.throw_requested = false;
        self.messages.clear();
        audio.clear_world();
        self.audio = crate::game::audio::WorldAudio::default();
        self.audio_stats = audio.stats();
        self.game.stop();
    }
}
