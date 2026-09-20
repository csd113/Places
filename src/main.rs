pub mod game;
pub mod input;
pub mod level;
pub mod render;

use game::Game;
use input::InputHandler;
use render::{Renderer, WINDOW_HEIGHT, WINDOW_WIDTH};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sdl_context = sdl2::init().map_err(|e| format!("Failed to init SDL2: {e}"))?;
    let video_subsystem = sdl_context
        .video()
        .map_err(|e| format!("Failed to init video subsystem: {e}"))?;

    // Open a 480x272 game window suitable for the PocketCHIP target
    let window = video_subsystem
        .window("Liminal", WINDOW_WIDTH, WINDOW_HEIGHT)
        .position_centered()
        .opengl()
        .build()
        .map_err(|e| format!("Failed to create window: {e}"))?;

    let renderer = Renderer::new(&window, &video_subsystem)
        .map_err(|e| format!("Failed to initialize renderer: {e}"))?;

    let mut event_pump = sdl_context
        .event_pump()
        .map_err(|e| format!("Failed to init event pump: {e}"))?;

    let mut input_handler = InputHandler::new();
    let mut game = Game::new();

    // Clean main loop with event polling, scene rendering, and VSync presentation
    while game.is_running() {
        game.update_timing();

        for event in event_pump.poll_iter() {
            input_handler.handle_event(&event);
        }

        if input_handler.quit_requested() {
            game.stop();
        }

        // Update basic first-person movement from WASD input
        game.update_player_movement(input_handler.state());

        // Render hardcoded test room from the player's camera perspective
        renderer.render_test_room(
            WINDOW_WIDTH,
            WINDOW_HEIGHT,
            game.player_position,
            game.player_yaw,
        );

        // Swap window buffer (double buffered, VSync synchronized)
        window.gl_swap_window();
    }

    Ok(())
}
