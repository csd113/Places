use glow::HasContext;

pub const WINDOW_WIDTH: u32 = 480;
pub const WINDOW_HEIGHT: u32 = 272;

/// Manages OpenGL ES 2.0-compatible accelerated rendering context and scene drawing.
pub struct Renderer {
    _gl_context: sdl2::video::GLContext,
    gl: glow::Context,
}

impl Renderer {
    /// Initializes an accelerated OpenGL context with VSync and double buffering.
    pub fn new(window: &sdl2::video::Window, video: &sdl2::VideoSubsystem) -> Result<Self, String> {
        let gl_attr = video.gl_attr();
        gl_attr.set_double_buffer(true);
        gl_attr.set_depth_size(24);

        // Prefer OpenGL ES 2.0 profile for PocketCHIP (Mali-400 / Lima).
        // Fall back to desktop GL (2.1 Compatibility) if EGL/GLES context creation fails on host.
        gl_attr.set_context_profile(sdl2::video::GLProfile::GLES);
        gl_attr.set_context_version(2, 0);

        let gl_context = match window.gl_create_context() {
            Ok(ctx) => ctx,
            Err(_) => {
                gl_attr.set_context_profile(sdl2::video::GLProfile::Compatibility);
                gl_attr.set_context_version(2, 1);
                window.gl_create_context()?
            }
        };

        window.gl_make_current(&gl_context)?;

        // Request VSync / vblank-synchronized buffer swapping
        let _ = video.gl_set_swap_interval(sdl2::video::SwapInterval::VSync);

        let gl = unsafe {
            glow::Context::from_loader_function(|proc_name| {
                video.gl_get_proc_address(proc_name) as *const _
            })
        };

        unsafe {
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LEQUAL);
            // Blank scene clear color (Backrooms dark ambient tone)
            gl.clear_color(0.08, 0.08, 0.09, 1.0);
        }

        Ok(Self {
            _gl_context: gl_context,
            gl,
        })
    }

    /// Renders a simple blank background/scene by clearing color and depth buffers.
    pub fn render_blank_scene(&self, width: u32, height: u32) {
        unsafe {
            self.gl.viewport(0, 0, width as i32, height as i32);
            self.gl
                .clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
        }
    }
}
