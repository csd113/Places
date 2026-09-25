//! SDL/OpenGL window and context policy: the one place the game asks SDL for
//! an OpenGL context, a swap interval or a buffer swap.
//!
//! These calls are why the OpenGL backend needs the SDL window and video
//! subsystem at all. They are re-exported through `crate::render` so `main`
//! never names a `gl_*` call, and a future backend can replace them in one
//! place.

use sdl2::VideoSubsystem;

/// Requests the attributes a window must be created with for an OpenGL ES 2.0
/// context.
///
/// SDL GL attributes are sticky, so a fallback request must explicitly override
/// the profile and version again; see [`request_fallback_window_attributes`].
pub fn request_window_attributes(video: &VideoSubsystem) {
    let attr = video.gl_attr();
    attr.set_double_buffer(true);
    attr.set_depth_size(24);
    attr.set_context_profile(sdl2::video::GLProfile::GLES);
    attr.set_context_version(2, 0);
}

/// Requests the desktop compatibility attributes used when the ES 2.0 context
/// cannot be created.
pub fn request_fallback_window_attributes(video: &VideoSubsystem) {
    let attr = video.gl_attr();
    attr.set_double_buffer(true);
    attr.set_depth_size(24);
    attr.set_context_profile(sdl2::video::GLProfile::Compatibility);
    attr.set_context_version(2, 1);
}

/// Requests a swap interval and reports what the platform actually accepted.
///
/// This used to discard the result of `SDL_GL_SetSwapInterval`, which made a
/// silently ignored `VSync` request indistinguishable from a working one. The
/// requested interval, the call's return status and `SDL_GL_GetSwapInterval`
/// (a fresh query of the platform, not an echo of the request) are all logged
/// once at startup when telemetry is enabled, and the interval in force is
/// returned for the caller.
pub fn apply_swap_interval(video: &VideoSubsystem, want_vsync: bool) -> i32 {
    let requested = if want_vsync {
        sdl2::video::SwapInterval::VSync
    } else {
        sdl2::video::SwapInterval::Immediate
    };
    match video.gl_set_swap_interval(requested) {
        Ok(()) => {
            let reported = video.gl_get_swap_interval();
            crate::logging::info(format!(
                "[vsync] requested {requested:?}, SDL_GL_SetSwapInterval -> Ok, SDL_GL_GetSwapInterval -> {reported:?}",
            ));
            reported as i32
        }
        Err(error) => {
            let reported = video.gl_get_swap_interval();
            crate::logging::info(format!(
                "[vsync] requested {requested:?}, SDL_GL_SetSwapInterval -> Err({error}), SDL_GL_GetSwapInterval -> {reported:?}",
            ));
            reported as i32
        }
    }
}

/// Presents the finished frame: one double-buffered buffer swap.
pub fn present(window: &sdl2::video::Window) {
    window.gl_swap_window();
}

/// Marks the window being built as an OpenGL window.
///
/// This is the one window-builder flag the backend owns: SDL has to know the
/// window will carry an OpenGL context before it is created.
pub fn apply_window_flags(builder: &mut sdl2::video::WindowBuilder) {
    builder.opengl();
}
