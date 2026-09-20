use glow::HasContext;

use crate::font::generate_font_atlas;
use crate::level::LevelDef;

pub const WINDOW_WIDTH: u32 = 480;
pub const WINDOW_HEIGHT: u32 = 272;

const VERTEX_SHADER_SRC: &str = r#"
#ifdef GL_ES
precision mediump float;
#endif
attribute vec3 a_pos;
attribute vec3 a_color;
attribute vec2 a_uv;
uniform mat4 u_mvp;
varying vec3 v_color;
varying vec2 v_uv;

void main() {
    v_color = a_color;
    v_uv = a_uv;
    gl_Position = u_mvp * vec4(a_pos, 1.0);
}
"#;

const FRAGMENT_SHADER_SRC: &str = r#"
#ifdef GL_ES
precision mediump float;
#endif
uniform sampler2D u_texture;
varying vec3 v_color;
varying vec2 v_uv;

void main() {
    vec4 tex_color = texture2D(u_texture, v_uv);
    gl_FragColor = tex_color * vec4(v_color, 1.0);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub color: [f32; 3],
    pub uv: [f32; 2],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BatchRange {
    pub start: i32,
    pub count: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LevelMeshBatches {
    pub floor_batch: BatchRange,
    pub ceiling_batch: BatchRange,
    pub wall_batch: BatchRange,
    pub light_batch: BatchRange,
}

pub struct LevelMesh {
    pub vertices: Vec<Vertex>,
    pub batches: LevelMeshBatches,
}

fn add_quad(
    vertices: &mut Vec<Vertex>,
    p0: [f32; 3],
    c0: [f32; 3],
    uv0: [f32; 2],
    p1: [f32; 3],
    c1: [f32; 3],
    uv1: [f32; 2],
    p2: [f32; 3],
    c2: [f32; 3],
    uv2: [f32; 2],
    p3: [f32; 3],
    c3: [f32; 3],
    uv3: [f32; 2],
) {
    vertices.push(Vertex {
        pos: p0,
        color: c0,
        uv: uv0,
    });
    vertices.push(Vertex {
        pos: p1,
        color: c1,
        uv: uv1,
    });
    vertices.push(Vertex {
        pos: p2,
        color: c2,
        uv: uv2,
    });
    vertices.push(Vertex {
        pos: p0,
        color: c0,
        uv: uv0,
    });
    vertices.push(Vertex {
        pos: p2,
        color: c2,
        uv: uv2,
    });
    vertices.push(Vertex {
        pos: p3,
        color: c3,
        uv: uv3,
    });
}

fn add_quad_flat(
    vertices: &mut Vec<Vertex>,
    p0: [f32; 3],
    p1: [f32; 3],
    p2: [f32; 3],
    p3: [f32; 3],
    color: [f32; 3],
    uv0: [f32; 2],
    uv1: [f32; 2],
    uv2: [f32; 2],
    uv3: [f32; 2],
) {
    add_quad(
        vertices, p0, color, uv0, p1, color, uv1, p2, color, uv2, p3, color, uv3,
    );
}

pub(crate) fn generate_wall_texture() -> [u8; 64 * 64 * 4] {
    let mut data = [255u8; 64 * 64 * 4];
    for y in 0..64 {
        for x in 0..64 {
            let idx = (y * 64 + x) * 4;
            let stripe = ((x % 16) as f32 - 8.0).abs() / 8.0;
            let stripe_factor = 0.94 + 0.06 * stripe;
            let weave = if (x + y) % 2 == 0 { 1.0 } else { 0.97 };
            let groove = if x % 4 == 0 { 0.95 } else { 1.0 };
            let total = stripe_factor * weave * groove;

            data[idx] = (245.0 * total).clamp(0.0, 255.0) as u8;
            data[idx + 1] = (238.0 * total).clamp(0.0, 255.0) as u8;
            data[idx + 2] = (218.0 * total).clamp(0.0, 255.0) as u8;
            data[idx + 3] = 255;
        }
    }
    data
}

pub(crate) fn generate_carpet_texture() -> [u8; 64 * 64 * 4] {
    let mut data = [255u8; 64 * 64 * 4];
    for y in 0..64 {
        for x in 0..64 {
            let idx = (y * 64 + x) * 4;
            let lx = x % 4;
            let ly = y % 4;
            let is_center = (lx == 1 || lx == 2) && (ly == 1 || ly == 2);
            let is_crevice = lx == 0 || ly == 0;

            let loop_val = if is_center {
                1.05
            } else if is_crevice {
                0.92
            } else {
                1.00
            };

            let stipple = (((x * 37 + y * 17) % 7) as f32 / 7.0) * 0.06 - 0.03;
            let factor = (loop_val + stipple).clamp(0.85, 1.15);

            data[idx] = (232.0 * factor).clamp(0.0, 255.0) as u8;
            data[idx + 1] = (224.0 * factor).clamp(0.0, 255.0) as u8;
            data[idx + 2] = (212.0 * factor).clamp(0.0, 255.0) as u8;
            data[idx + 3] = 255;
        }
    }
    data
}

pub(crate) fn generate_ceiling_texture() -> [u8; 64 * 64 * 4] {
    let mut data = [255u8; 64 * 64 * 4];
    for y in 0..64 {
        for x in 0..64 {
            let idx = (y * 64 + x) * 4;
            let is_border = x <= 1 || x >= 62 || y <= 1 || y >= 62;

            if is_border {
                data[idx] = 160;
                data[idx + 1] = 160;
                data[idx + 2] = 155;
                data[idx + 3] = 255;
            } else {
                let is_pit = (x * 13 + y * 29) % 19 == 0;
                let factor = if is_pit { 0.88 } else { 1.0 };

                data[idx] = (245.0 * factor) as u8;
                data[idx + 1] = (245.0 * factor) as u8;
                data[idx + 2] = (240.0 * factor) as u8;
                data[idx + 3] = 255;
            }
        }
    }
    data
}

pub(crate) fn generate_white_texture() -> [u8; 2 * 2 * 4] {
    [255u8; 2 * 2 * 4]
}

pub fn build_level_geometry(level: &LevelDef) -> LevelMesh {
    let mut vertices = Vec::new();
    let mut batches = LevelMeshBatches::default();

    // 1. Floor batch
    let floor_start = vertices.len() as i32;
    for room in level.all_rooms() {
        let x_start = room.x.floor() as i32;
        let x_end = (room.x + room.width).ceil() as i32;
        let z_start = room.z.floor() as i32;
        let z_end = (room.z + room.depth).ceil() as i32;

        for ix in x_start..x_end {
            for iz in z_start..z_end {
                let x0 = (ix as f32).max(room.x);
                let x1 = ((ix + 1) as f32).min(room.x + room.width);
                let z0 = (iz as f32).max(room.z);
                let z1 = ((iz + 1) as f32).min(room.z + room.depth);
                if x1 <= x0 || z1 <= z0 {
                    continue;
                }
                let is_alt = (ix + iz) % 2 == 0;
                let color = if is_alt {
                    [0.56, 0.51, 0.39]
                } else {
                    [0.50, 0.45, 0.34]
                };

                add_quad_flat(
                    &mut vertices,
                    [x0, 0.0, z0],
                    [x1, 0.0, z0],
                    [x1, 0.0, z1],
                    [x0, 0.0, z1],
                    color,
                    [x0, z0],
                    [x1, z0],
                    [x1, z1],
                    [x0, z1],
                );
            }
        }
    }
    batches.floor_batch = BatchRange {
        start: floor_start,
        count: vertices.len() as i32 - floor_start,
    };

    // 2. Ceiling batch
    let ceiling_start = vertices.len() as i32;
    for room in level.all_rooms() {
        let h = room.height;
        let ceiling_color = [0.72, 0.72, 0.70];
        add_quad_flat(
            &mut vertices,
            [room.x, h, room.z + room.depth],
            [room.x + room.width, h, room.z + room.depth],
            [room.x + room.width, h, room.z],
            [room.x, h, room.z],
            ceiling_color,
            [room.x, room.z + room.depth],
            [room.x + room.width, room.z + room.depth],
            [room.x + room.width, room.z],
            [room.x, room.z],
        );
    }
    batches.ceiling_batch = BatchRange {
        start: ceiling_start,
        count: vertices.len() as i32 - ceiling_start,
    };

    // 3. Walls batch
    let wall_start = vertices.len() as i32;
    let base_wall = [0.85, 0.80, 0.42];

    for wall in &level.walls {
        let x0 = wall.x.min(wall.x + wall.width);
        let x1 = wall.x.max(wall.x + wall.width);
        let z0 = wall.z.min(wall.z + wall.depth);
        let z1 = wall.z.max(wall.z + wall.depth);
        let y0 = 0.0;
        let y1 = wall.height;

        let north_mult = 1.00;
        let south_mult = 0.88;
        let west_mult = 0.84;
        let east_mult = 0.94;

        let top_grad = 1.05;
        let bot_grad = 0.92;

        let scale_color = |mult: f32, grad: f32| -> [f32; 3] {
            [
                (base_wall[0] * mult * grad).min(1.0),
                (base_wall[1] * mult * grad).min(1.0),
                (base_wall[2] * mult * grad).min(1.0),
            ]
        };

        // North face (z = z0, normal -Z)
        let n_top = scale_color(north_mult, top_grad);
        let n_bot = scale_color(north_mult, bot_grad);
        add_quad(
            &mut vertices,
            [x0, y0, z0],
            n_bot,
            [x0, y0],
            [x1, y0, z0],
            n_bot,
            [x1, y0],
            [x1, y1, z0],
            n_top,
            [x1, y1],
            [x0, y1, z0],
            n_top,
            [x0, y1],
        );

        // South face (z = z1, normal +Z)
        let s_top = scale_color(south_mult, top_grad);
        let s_bot = scale_color(south_mult, bot_grad);
        add_quad(
            &mut vertices,
            [x1, y0, z1],
            s_bot,
            [x1, y0],
            [x0, y0, z1],
            s_bot,
            [x0, y0],
            [x0, y1, z1],
            s_top,
            [x0, y1],
            [x1, y1, z1],
            s_top,
            [x1, y1],
        );

        // West face (x = x0, normal -X)
        let w_top = scale_color(west_mult, top_grad);
        let w_bot = scale_color(west_mult, bot_grad);
        add_quad(
            &mut vertices,
            [x0, y0, z1],
            w_bot,
            [z1, y0],
            [x0, y0, z0],
            w_bot,
            [z0, y0],
            [x0, y1, z0],
            w_top,
            [z0, y1],
            [x0, y1, z1],
            w_top,
            [z1, y1],
        );

        // East face (x = x1, normal +X)
        let e_top = scale_color(east_mult, top_grad);
        let e_bot = scale_color(east_mult, bot_grad);
        add_quad(
            &mut vertices,
            [x1, y0, z0],
            e_bot,
            [z0, y0],
            [x1, y0, z1],
            e_bot,
            [z1, y0],
            [x1, y1, z1],
            e_top,
            [z1, y1],
            [x1, y1, z0],
            e_top,
            [z0, y1],
        );
    }
    batches.wall_batch = BatchRange {
        start: wall_start,
        count: vertices.len() as i32 - wall_start,
    };

    // 4. Ceiling lights batch
    let light_start = vertices.len() as i32;
    for light in &level.ceiling_lights {
        let (half_w, half_d) = if light.rotation_degrees as i32 % 180 != 0 {
            (0.30, 0.60)
        } else {
            (0.60, 0.30)
        };

        let y = 3.49;
        let x0 = light.x - half_w;
        let x1 = light.x + half_w;
        let z0 = light.z - half_d;
        let z1 = light.z + half_d;

        let fixture_glow = [1.00, 0.98, 0.92];
        add_quad_flat(
            &mut vertices,
            [x0, y, z1],
            [x1, y, z1],
            [x1, y, z0],
            [x0, y, z0],
            fixture_glow,
            [0.0, 1.0],
            [1.0, 1.0],
            [1.0, 0.0],
            [0.0, 0.0],
        );

        let bezel_color = [0.40, 0.40, 0.40];
        let b = 0.05;
        add_quad_flat(
            &mut vertices,
            [x0 - b, y, z0],
            [x1 + b, y, z0],
            [x1 + b, y, z0 - b],
            [x0 - b, y, z0 - b],
            bezel_color,
            [0.0, 1.0],
            [1.0, 1.0],
            [1.0, 0.0],
            [0.0, 0.0],
        );
        add_quad_flat(
            &mut vertices,
            [x0 - b, y, z1 + b],
            [x1 + b, y, z1 + b],
            [x1 + b, y, z1],
            [x0 - b, y, z1],
            bezel_color,
            [0.0, 1.0],
            [1.0, 1.0],
            [1.0, 0.0],
            [0.0, 0.0],
        );
    }
    batches.light_batch = BatchRange {
        start: light_start,
        count: vertices.len() as i32 - light_start,
    };

    LevelMesh { vertices, batches }
}

unsafe fn create_texture_2d(
    gl: &glow::Context,
    width: i32,
    height: i32,
    pixels: &[u8],
    repeat: bool,
) -> Result<glow::Texture, String> {
    unsafe {
        let texture = gl.create_texture()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));

        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA as i32,
            width,
            height,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(pixels)),
        );

        let wrap_mode = if repeat {
            glow::REPEAT as i32
        } else {
            glow::CLAMP_TO_EDGE as i32
        };
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, wrap_mode);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, wrap_mode);

        if repeat {
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR_MIPMAP_LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as i32,
            );
            gl.generate_mipmap(glow::TEXTURE_2D);
        } else {
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::NEAREST as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::NEAREST as i32,
            );
        }

        gl.bind_texture(glow::TEXTURE_2D, None);
        Ok(texture)
    }
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

/// Manages OpenGL ES 2.0-compatible accelerated rendering context, textures, and scene/UI drawing.
pub struct Renderer {
    _gl_context: sdl2::video::GLContext,
    gl: glow::Context,
    program: glow::Program,
    level_vbo: glow::Buffer,
    ui_vbo: glow::Buffer,
    batches: LevelMeshBatches,
    wall_texture: glow::Texture,
    floor_texture: glow::Texture,
    ceiling_texture: glow::Texture,
    white_texture: glow::Texture,
    font_texture: glow::Texture,
    u_mvp_loc: Option<glow::UniformLocation>,
    u_texture_loc: Option<glow::UniformLocation>,
    a_pos_loc: u32,
    a_color_loc: u32,
    a_uv_loc: u32,
}

impl Renderer {
    /// Initializes an accelerated OpenGL context with VSync, textures, and initial level geometry.
    pub fn new(
        window: &sdl2::video::Window,
        video: &sdl2::VideoSubsystem,
        level: &LevelDef,
    ) -> Result<Self, String> {
        let gl_attr = video.gl_attr();
        gl_attr.set_double_buffer(true);
        gl_attr.set_depth_size(24);

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

        let _ = video.gl_set_swap_interval(sdl2::video::SwapInterval::VSync);

        let gl = unsafe {
            glow::Context::from_loader_function(|proc_name| {
                video.gl_get_proc_address(proc_name) as *const _
            })
        };

        let (
            program,
            level_vbo,
            ui_vbo,
            batches,
            wall_texture,
            floor_texture,
            ceiling_texture,
            white_texture,
            font_texture,
            u_mvp_loc,
            u_texture_loc,
            a_pos_loc,
            a_color_loc,
            a_uv_loc,
        ) = unsafe {
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
            let a_uv_loc = gl
                .get_attrib_location(program, "a_uv")
                .ok_or_else(|| "Missing a_uv attribute".to_string())?;

            let u_mvp_loc = gl.get_uniform_location(program, "u_mvp");
            let u_texture_loc = gl.get_uniform_location(program, "u_texture");

            // Create textures
            let wall_texture = create_texture_2d(&gl, 64, 64, &generate_wall_texture(), true)?;
            let floor_texture = create_texture_2d(&gl, 64, 64, &generate_carpet_texture(), true)?;
            let ceiling_texture =
                create_texture_2d(&gl, 64, 64, &generate_ceiling_texture(), true)?;
            let white_texture = create_texture_2d(&gl, 2, 2, &generate_white_texture(), false)?;
            let font_texture = create_texture_2d(&gl, 128, 64, &generate_font_atlas(), false)?;

            // Upload initial 3D level geometry
            let mesh = build_level_geometry(level);
            let level_vbo = gl.create_buffer()?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(level_vbo));
            let byte_slice = std::slice::from_raw_parts(
                mesh.vertices.as_ptr() as *const u8,
                mesh.vertices.len() * std::mem::size_of::<Vertex>(),
            );
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, byte_slice, glow::STATIC_DRAW);

            // Create UI VBO
            let ui_vbo = gl.create_buffer()?;
            gl.bind_buffer(glow::ARRAY_BUFFER, None);

            (
                program,
                level_vbo,
                ui_vbo,
                mesh.batches,
                wall_texture,
                floor_texture,
                ceiling_texture,
                white_texture,
                font_texture,
                u_mvp_loc,
                u_texture_loc,
                a_pos_loc,
                a_color_loc,
                a_uv_loc,
            )
        };

        Ok(Self {
            _gl_context: gl_context,
            gl,
            program,
            level_vbo,
            ui_vbo,
            batches,
            wall_texture,
            floor_texture,
            ceiling_texture,
            white_texture,
            font_texture,
            u_mvp_loc,
            u_texture_loc,
            a_pos_loc,
            a_color_loc,
            a_uv_loc,
        })
    }

    /// Renders the 3D level combining yaw and pitch into the view matrix.
    pub fn render_scene(
        &self,
        width: u32,
        height: u32,
        camera_pos: glam::Vec3,
        camera_yaw: f32,
        camera_pitch: f32,
        fov_degrees: f32,
    ) {
        unsafe {
            self.gl.viewport(0, 0, width as i32, height as i32);
            self.gl
                .clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);

            self.gl.use_program(Some(self.program));

            let aspect = width as f32 / height.max(1) as f32;
            let proj = glam::Mat4::perspective_rh(fov_degrees.to_radians(), aspect, 0.1, 100.0);

            // Correctly combine yaw and pitch in the camera forward vector
            let cos_pitch = camera_pitch.cos();
            let forward = glam::Vec3::new(
                camera_yaw.sin() * cos_pitch,
                camera_pitch.sin(),
                -camera_yaw.cos() * cos_pitch,
            );
            let view = glam::Mat4::look_at_rh(camera_pos, camera_pos + forward, glam::Vec3::Y);
            let mvp = proj * view;

            if let Some(ref loc) = self.u_mvp_loc {
                self.gl
                    .uniform_matrix_4_f32_slice(Some(loc), false, &mvp.to_cols_array());
            }

            if let Some(ref loc) = self.u_texture_loc {
                self.gl.uniform_1_i32(Some(loc), 0);
            }
            self.gl.active_texture(glow::TEXTURE0);

            self.gl
                .bind_buffer(glow::ARRAY_BUFFER, Some(self.level_vbo));
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

            self.gl.enable_vertex_attrib_array(self.a_uv_loc);
            self.gl.vertex_attrib_pointer_f32(
                self.a_uv_loc,
                2,
                glow::FLOAT,
                false,
                std::mem::size_of::<Vertex>() as i32,
                24,
            );

            // 1. Draw floor with repeating carpet texture
            if self.batches.floor_batch.count > 0 {
                self.gl
                    .bind_texture(glow::TEXTURE_2D, Some(self.floor_texture));
                self.gl.draw_arrays(
                    glow::TRIANGLES,
                    self.batches.floor_batch.start,
                    self.batches.floor_batch.count,
                );
            }

            // 2. Draw ceiling with repeating acoustic panel texture
            if self.batches.ceiling_batch.count > 0 {
                self.gl
                    .bind_texture(glow::TEXTURE_2D, Some(self.ceiling_texture));
                self.gl.draw_arrays(
                    glow::TRIANGLES,
                    self.batches.ceiling_batch.start,
                    self.batches.ceiling_batch.count,
                );
            }

            // 3. Draw walls with repeating yellow wallpaper texture
            if self.batches.wall_batch.count > 0 {
                self.gl
                    .bind_texture(glow::TEXTURE_2D, Some(self.wall_texture));
                self.gl.draw_arrays(
                    glow::TRIANGLES,
                    self.batches.wall_batch.start,
                    self.batches.wall_batch.count,
                );
            }

            // 4. Draw fixtures using unshaded white texture
            if self.batches.light_batch.count > 0 {
                self.gl
                    .bind_texture(glow::TEXTURE_2D, Some(self.white_texture));
                self.gl.draw_arrays(
                    glow::TRIANGLES,
                    self.batches.light_batch.start,
                    self.batches.light_batch.count,
                );
            }

            self.gl.disable_vertex_attrib_array(self.a_pos_loc);
            self.gl.disable_vertex_attrib_array(self.a_color_loc);
            self.gl.disable_vertex_attrib_array(self.a_uv_loc);
            self.gl.bind_texture(glow::TEXTURE_2D, None);
            self.gl.bind_buffer(glow::ARRAY_BUFFER, None);
            self.gl.use_program(None);
        }
    }

    /// Renders a 2D UI overlay on top of the scene using an orthographic projection and the font atlas.
    pub fn render_ui(&self, width: u32, height: u32, ui_vertices: &[Vertex]) {
        if ui_vertices.is_empty() {
            return;
        }

        unsafe {
            self.gl.disable(glow::DEPTH_TEST);
            self.gl.enable(glow::BLEND);
            self.gl
                .blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);

            self.gl.use_program(Some(self.program));

            let ortho =
                glam::Mat4::orthographic_rh(0.0, width as f32, height as f32, 0.0, -1.0, 1.0);

            if let Some(ref loc) = self.u_mvp_loc {
                self.gl
                    .uniform_matrix_4_f32_slice(Some(loc), false, &ortho.to_cols_array());
            }

            if let Some(ref loc) = self.u_texture_loc {
                self.gl.uniform_1_i32(Some(loc), 0);
            }
            self.gl.active_texture(glow::TEXTURE0);
            self.gl
                .bind_texture(glow::TEXTURE_2D, Some(self.font_texture));

            self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.ui_vbo));
            let byte_slice = std::slice::from_raw_parts(
                ui_vertices.as_ptr() as *const u8,
                ui_vertices.len() * std::mem::size_of::<Vertex>(),
            );
            self.gl
                .buffer_data_u8_slice(glow::ARRAY_BUFFER, byte_slice, glow::DYNAMIC_DRAW);

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

            self.gl.enable_vertex_attrib_array(self.a_uv_loc);
            self.gl.vertex_attrib_pointer_f32(
                self.a_uv_loc,
                2,
                glow::FLOAT,
                false,
                std::mem::size_of::<Vertex>() as i32,
                24,
            );

            self.gl
                .draw_arrays(glow::TRIANGLES, 0, ui_vertices.len() as i32);

            self.gl.disable_vertex_attrib_array(self.a_pos_loc);
            self.gl.disable_vertex_attrib_array(self.a_color_loc);
            self.gl.disable_vertex_attrib_array(self.a_uv_loc);
            self.gl.bind_texture(glow::TEXTURE_2D, None);
            self.gl.bind_buffer(glow::ARRAY_BUFFER, None);
            self.gl.use_program(None);

            self.gl.disable(glow::BLEND);
            self.gl.enable(glow::DEPTH_TEST);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_texture_dimensions() {
        let wall = generate_wall_texture();
        let carpet = generate_carpet_texture();
        let ceiling = generate_ceiling_texture();
        let white = generate_white_texture();

        assert_eq!(wall.len(), 64 * 64 * 4);
        assert_eq!(carpet.len(), 64 * 64 * 4);
        assert_eq!(ceiling.len(), 64 * 64 * 4);
        assert_eq!(white.len(), 2 * 2 * 4);

        for i in 0..(64 * 64) {
            assert_eq!(wall[i * 4 + 3], 255);
            assert_eq!(carpet[i * 4 + 3], 255);
            assert_eq!(ceiling[i * 4 + 3], 255);
        }
    }

    #[test]
    fn test_build_geometry_from_test_room() {
        let json = include_str!("../assets/levels/test_room.json");
        let level = LevelDef::from_json(json).expect("valid test_room json");
        let mesh = build_level_geometry(&level);
        assert!(!mesh.vertices.is_empty());
        assert_eq!(mesh.vertices.len() % 6, 0);
        assert!(mesh.batches.floor_batch.count > 0);
        assert!(mesh.batches.ceiling_batch.count > 0);
        assert!(mesh.batches.wall_batch.count > 0);
    }

    #[test]
    fn test_build_geometry_from_level1() {
        let json = include_str!("../assets/levels/level1.json");
        let level = LevelDef::from_json(json).expect("valid level1 json");
        let mesh = build_level_geometry(&level);
        assert!(!mesh.vertices.is_empty());
        assert_eq!(mesh.vertices.len() % 6, 0);
        assert!(mesh.batches.floor_batch.count > 0);
        assert!(mesh.batches.ceiling_batch.count > 0);
        assert!(mesh.batches.wall_batch.count > 0);
        assert!(mesh.batches.light_batch.count > 0);
    }
}
