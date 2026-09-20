use glow::HasContext;

pub const WINDOW_WIDTH: u32 = 480;
pub const WINDOW_HEIGHT: u32 = 272;

const VERTEX_SHADER_SRC: &str = r#"
#ifdef GL_ES
precision mediump float;
#endif
attribute vec3 a_pos;
attribute vec3 a_color;
uniform mat4 u_mvp;
varying vec3 v_color;

void main() {
    v_color = a_color;
    gl_Position = u_mvp * vec4(a_pos, 1.0);
}
"#;

const FRAGMENT_SHADER_SRC: &str = r#"
#ifdef GL_ES
precision mediump float;
#endif
varying vec3 v_color;

void main() {
    gl_FragColor = vec4(v_color, 1.0);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct Vertex {
    pos: [f32; 3],
    color: [f32; 3],
}

fn add_quad(
    vertices: &mut Vec<Vertex>,
    p0: [f32; 3],
    p1: [f32; 3],
    p2: [f32; 3],
    p3: [f32; 3],
    color: [f32; 3],
) {
    vertices.push(Vertex { pos: p0, color });
    vertices.push(Vertex { pos: p1, color });
    vertices.push(Vertex { pos: p2, color });
    vertices.push(Vertex { pos: p0, color });
    vertices.push(Vertex { pos: p2, color });
    vertices.push(Vertex { pos: p3, color });
}

fn build_test_room_vertices() -> Vec<Vertex> {
    let mut vertices = Vec::new();

    // Floor with alternating 1m tiles for motion verification
    for x in -6_i32..6_i32 {
        for z in -12_i32..4_i32 {
            let x0 = x as f32;
            let x1 = (x + 1) as f32;
            let z0 = z as f32;
            let z1 = (z + 1) as f32;
            let is_alt = (x + z) % 2 == 0;
            let color = if is_alt {
                [0.55, 0.50, 0.38]
            } else {
                [0.48, 0.43, 0.32]
            };
            add_quad(
                &mut vertices,
                [x0, 0.0, z0],
                [x1, 0.0, z0],
                [x1, 0.0, z1],
                [x0, 0.0, z1],
                color,
            );
        }
    }

    // Ceiling (y = 3.5)
    add_quad(
        &mut vertices,
        [-6.0, 3.5, 4.0],
        [6.0, 3.5, 4.0],
        [6.0, 3.5, -12.0],
        [-6.0, 3.5, -12.0],
        [0.72, 0.72, 0.70],
    );

    // North wall (z = -12.0)
    add_quad(
        &mut vertices,
        [-6.0, 0.0, -12.0],
        [6.0, 0.0, -12.0],
        [6.0, 3.5, -12.0],
        [-6.0, 3.5, -12.0],
        [0.82, 0.78, 0.40],
    );

    // South wall (z = 4.0)
    add_quad(
        &mut vertices,
        [6.0, 0.0, 4.0],
        [-6.0, 0.0, 4.0],
        [-6.0, 3.5, 4.0],
        [6.0, 3.5, 4.0],
        [0.60, 0.56, 0.28],
    );

    // West wall (x = -6.0)
    add_quad(
        &mut vertices,
        [-6.0, 0.0, 4.0],
        [-6.0, 0.0, -12.0],
        [-6.0, 3.5, -12.0],
        [-6.0, 3.5, 4.0],
        [0.75, 0.70, 0.35],
    );

    // East wall (x = 6.0)
    add_quad(
        &mut vertices,
        [6.0, 0.0, -12.0],
        [6.0, 0.0, 4.0],
        [6.0, 3.5, 4.0],
        [6.0, 3.5, -12.0],
        [0.78, 0.74, 0.38],
    );

    vertices
}

unsafe fn create_shader(
    gl: &glow::Context,
    shader_type: u32,
    source: &str,
) -> Result<glow::Shader, String> {
    unsafe {
        let shader = gl.create_shader(shader_type)?;
        gl.shader_source(shader, source);
        gl.compile_shader(shader);
        if !gl.get_shader_compile_status(shader) {
            let log = gl.get_shader_info_log(shader);
            gl.delete_shader(shader);
            return Err(format!("Shader compile error: {log}"));
        }
        Ok(shader)
    }
}

unsafe fn create_program(
    gl: &glow::Context,
    vert_src: &str,
    frag_src: &str,
) -> Result<glow::Program, String> {
    unsafe {
        let vs = create_shader(gl, glow::VERTEX_SHADER, vert_src)?;
        let fs = create_shader(gl, glow::FRAGMENT_SHADER, frag_src)?;

        let program = gl.create_program()?;
        gl.attach_shader(program, vs);
        gl.attach_shader(program, fs);
        gl.link_program(program);

        if !gl.get_program_link_status(program) {
            let log = gl.get_program_info_log(program);
            gl.delete_program(program);
            gl.delete_shader(vs);
            gl.delete_shader(fs);
            return Err(format!("Program link error: {log}"));
        }

        gl.delete_shader(vs);
        gl.delete_shader(fs);
        Ok(program)
    }
}

/// Manages OpenGL ES 2.0-compatible accelerated rendering context and test room scene drawing.
pub struct Renderer {
    _gl_context: sdl2::video::GLContext,
    gl: glow::Context,
    program: glow::Program,
    vbo: glow::Buffer,
    vertex_count: i32,
    u_mvp_loc: Option<glow::UniformLocation>,
    a_pos_loc: u32,
    a_color_loc: u32,
}

impl Renderer {
    /// Initializes an accelerated OpenGL context with VSync and test room shaders/geometry.
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

        let (program, vbo, vertex_count, u_mvp_loc, a_pos_loc, a_color_loc) = unsafe {
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LEQUAL);
            gl.clear_color(0.08, 0.08, 0.09, 1.0);

            let program = create_program(&gl, VERTEX_SHADER_SRC, FRAGMENT_SHADER_SRC)?;
            let a_pos_loc = gl
                .get_attrib_location(program, "a_pos")
                .ok_or_else(|| "Missing a_pos attribute".to_string())?;
            let a_color_loc = gl
                .get_attrib_location(program, "a_color")
                .ok_or_else(|| "Missing a_color attribute".to_string())?;
            let u_mvp_loc = gl.get_uniform_location(program, "u_mvp");

            let vertices = build_test_room_vertices();
            let vertex_count = vertices.len() as i32;
            let vbo = gl.create_buffer()?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));

            let byte_slice = std::slice::from_raw_parts(
                vertices.as_ptr() as *const u8,
                vertices.len() * std::mem::size_of::<Vertex>(),
            );
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, byte_slice, glow::STATIC_DRAW);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);

            (
                program,
                vbo,
                vertex_count,
                u_mvp_loc,
                a_pos_loc,
                a_color_loc,
            )
        };

        Ok(Self {
            _gl_context: gl_context,
            gl,
            program,
            vbo,
            vertex_count,
            u_mvp_loc,
            a_pos_loc,
            a_color_loc,
        })
    }

    /// Renders the hardcoded test room from the player's camera position and yaw.
    pub fn render_test_room(
        &self,
        width: u32,
        height: u32,
        camera_pos: glam::Vec3,
        camera_yaw: f32,
    ) {
        unsafe {
            self.gl.viewport(0, 0, width as i32, height as i32);
            self.gl
                .clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);

            self.gl.use_program(Some(self.program));

            let aspect = width as f32 / height.max(1) as f32;
            let proj = glam::Mat4::perspective_rh(60.0_f32.to_radians(), aspect, 0.1, 100.0);
            let forward = glam::Vec3::new(camera_yaw.sin(), 0.0, -camera_yaw.cos());
            let view = glam::Mat4::look_at_rh(camera_pos, camera_pos + forward, glam::Vec3::Y);
            let mvp = proj * view;

            if let Some(ref loc) = self.u_mvp_loc {
                self.gl
                    .uniform_matrix_4_f32_slice(Some(loc), false, &mvp.to_cols_array());
            }

            self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            self.gl.enable_vertex_attrib_array(self.a_pos_loc);
            self.gl.vertex_attrib_pointer_f32(
                self.a_pos_loc,
                3,
                glow::FLOAT,
                false,
                std::mem::size_of::<Vertex>() as i32,
                0,
            );

            self.gl.enable_vertex_attrib_array(self.a_color_loc);
            self.gl.vertex_attrib_pointer_f32(
                self.a_color_loc,
                3,
                glow::FLOAT,
                false,
                std::mem::size_of::<Vertex>() as i32,
                12,
            );

            self.gl.draw_arrays(glow::TRIANGLES, 0, self.vertex_count);

            self.gl.disable_vertex_attrib_array(self.a_pos_loc);
            self.gl.disable_vertex_attrib_array(self.a_color_loc);
            self.gl.bind_buffer(glow::ARRAY_BUFFER, None);
            self.gl.use_program(None);
        }
    }
}
