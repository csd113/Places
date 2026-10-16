//! SDL focus/capture ownership and the frame-loop integration boundary.
use super::FrameLoop;
use crate::game::{AppState, console};
use sdl3::event::Event;
use sdl3::keyboard::Keycode;

impl FrameLoop<'_> {
    pub(crate) fn scripted_text(&mut self, text: String, submit: bool) {
        let _handled = self.handle_event(&Event::TextInput {
            timestamp: 0,
            window_id: self.window.id(),
            text,
        });
        if submit {
            self.scripted_key("Return", true);
            self.scripted_key("Return", false);
        }
    }
    pub(crate) fn scripted_key(&mut self, key: &str, pressed: bool) {
        if let Some(keycode) = Keycode::from_name(key) {
            let event = if pressed {
                Event::KeyDown {
                    timestamp: 0,
                    window_id: self.window.id(),
                    keycode: Some(keycode),
                    scancode: None,
                    keymod: sdl3::keyboard::Mod::NOMOD,
                    repeat: false,
                    which: 0,
                    raw: 0,
                }
            } else {
                Event::KeyUp {
                    timestamp: 0,
                    window_id: self.window.id(),
                    keycode: Some(keycode),
                    scancode: None,
                    keymod: sdl3::keyboard::Mod::NOMOD,
                    repeat: false,
                    which: 0,
                    raw: 0,
                }
            };
            let _handled = self.handle_event(&event);
        }
    }

    pub(crate) fn close_console(&mut self) {
        if self.console.is_open() {
            self.console.close();
            self.input_handler.clear_gameplay_inputs();
            self.game.reset_timing();
        }
    }

    pub(crate) fn reconcile_console(&mut self) {
        if self
            .console_runtime
            .console_disabled_transition(self.settings.developer_console)
        {
            let exit = self.console_runtime.apply_action(
                &crate::game::gameplay_actions::GameplayAction::Noclip(Some(false)),
                &mut crate::game::gameplay_runtime::GameplayContext {
                    game: self.game,
                    renderer: self.renderer,
                    physics: &mut self.items.world,
                    authored: &mut self.items.authored,
                },
            );
            if let Err(error) = exit {
                // Do not strand a body inside geometry when a malformed map
                // lacks a safe return destination. Let the player move clear.
                self.settings.developer_console = true;
                self.ui_state.set_status(error, true);
            } else {
                self.console_runtime
                    .clear_given_props(self.renderer, &mut self.items.world);
                self.close_console();
            }
        }
        if !self.settings.developer_console
            || self.game.app_state() != AppState::Playing
            || self.load_intent.is_some()
        {
            self.close_console();
        }
    }

    pub(crate) fn handle_console_event(&mut self, event: &Event) -> bool {
        self.reconcile_console();
        if console::toggle_key(event) {
            if self.settings.developer_console
                && self.game.app_state() == AppState::Playing
                && self.load_intent.is_none()
            {
                self.console.toggle(true, true);
                self.input_handler.clear_gameplay_inputs();
                self.game.reset_timing();
                self.sync_mouse_capture();
            }
            return true;
        }
        if !self.console.is_open() {
            return false;
        }
        if console::escape_key(event) {
            self.close_console();
            self.sync_mouse_capture();
            return true;
        }
        if let Some(command) = self.console.handle_event(event) {
            self.console_runtime.execute(
                command,
                &mut self.console,
                &mut crate::game::console::runtime::GameplayContext {
                    game: self.game,
                    renderer: self.renderer,
                    physics: &mut self.items.world,
                    authored: &mut self.items.authored,
                },
            );
            self.items.input.invalidate();
            self.items.target = None;
            let _clock_updated = self
                .game
                .set_world_time_scale(self.items.world.simulation_state().time_scale);
            self.trace_world("console_command");
        }
        true
    }
}
