use glow::HasContext;

use crate::font::generate_font_atlas;
use crate::level::LevelDef;

/// PocketCHIP reference resolution. The game logic and UI layout are authored
/// against this 480x272 space; it is also the default window size. It is *not*
/// an assumption about the actual drawable/framebuffer size at runtime.
pub const WINDOW_WIDTH: u32 = 480;
pub const WINDOW_HEIGHT: u32 = 272;

/// Reference space that 2D UI geometry is authored in (PocketCHIP baseline).
pub const UI_REFERENCE_WIDTH: u32 = WINDOW_WIDTH;
pub const UI_REFERENCE_HEIGHT: u32 = WINDOW_HEIGHT;

/// Physical size (in pixels) of the current drawable/framebuffer.
///
/// This is deliberately distinct from the window's logical size: on HiDPI
/// displays such as macOS Retina the drawable is larger than the window size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawableSize {
    pub width: u32,
    pub height: u32,
}

impl DrawableSize {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// True when the surface cannot be rendered to (minimized/hidden windows).
    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Aspect ratio derived from the real framebuffer, safe against zero height.
    pub fn aspect_ratio(self) -> f32 {
        if self.height == 0 {
            1.0
        } else {
            self.width as f32 / self.height as f32
        }
    }

    /// Pixel size of the integer-scaled UI region that fits this drawable while
    /// preserving the 480x272 reference aspect ratio, plus its bottom-left origin.
    pub fn ui_viewport(self) -> UiViewport {
        if self.is_empty() {
            return UiViewport {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
                scale: 1.0,
            };
        }

        let scale = (self.width as f32 / UI_REFERENCE_WIDTH as f32)
            .min(self.height as f32 / UI_REFERENCE_HEIGHT as f32)
            .max(0.0);
        let width = ((UI_REFERENCE_WIDTH as f32 * scale).round() as i32).clamp(1, self.width as i32);
        let height =
            ((UI_REFERENCE_HEIGHT as f32 * scale).round() as i32).clamp(1, self.height as i32);

        UiViewport {
            x: (self.width as i32 - width) / 2,
            y: (self.height as i32 - height) / 2,
            width,
            height,
            scale,
        }
    }
}

/// Placement of the 480x272 UI reference space inside the physical drawable.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiViewport {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale: f32,
}

/// Aspect ratio of the authored PocketCHIP reference resolution (480x272).
pub fn reference_aspect_ratio() -> f32 {
    UI_REFERENCE_WIDTH as f32 / UI_REFERENCE_HEIGHT as f32
}

/// Maps the configured (baseline) vertical field of view onto a drawable with
/// the given aspect ratio.
///
/// * Wider than the PocketCHIP baseline: the vertical FOV is unchanged, so the
///   horizontal view expands naturally ("Hor+").
/// * Narrower/taller than the baseline: the horizontal FOV is preserved instead
///   so the level is not cropped left/right; only the vertical FOV grows.
///
/// At the baseline aspect this is the identity, so PocketCHIP is unchanged.
pub fn vertical_fov_for_aspect(configured_vertical_fov_degrees: f32, aspect: f32) -> f32 {
    // Guards against a near-singular projection on very tall/portrait windows.
    const MAX_VERTICAL_FOV_DEGREES: f32 = 150.0;

    let reference = reference_aspect_ratio();
    if !aspect.is_finite() || aspect <= 0.0 || aspect >= reference {
        return configured_vertical_fov_degrees;
    }

    let half_vertical_tan = (configured_vertical_fov_degrees.to_radians() * 0.5).tan();
    let half_horizontal_tan = half_vertical_tan * reference;
    let adjusted = 2.0 * (half_horizontal_tan / aspect).atan();
    adjusted.to_degrees().clamp(
        configured_vertical_fov_degrees,
        MAX_VERTICAL_FOV_DEGREES,
    )
}

const VERTEX_SHADER_SRC: &str = r#"
#ifdef GL_ES
precision mediump float;
#endif
attribute vec3 a_pos;
attribute vec4 a_color;
attribute vec2 a_uv;
uniform mat4 u_mvp;
varying vec4 v_color;
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
varying vec4 v_color;
varying vec2 v_uv;

void main() {
    vec4 tex_color = texture2D(u_texture, v_uv);
    gl_FragColor = tex_color * v_color;
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub color: [f32; 4],
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

#[allow(clippy::too_many_arguments)]
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
    let col0 = [c0[0], c0[1], c0[2], 1.0];
    let col1 = [c1[0], c1[1], c1[2], 1.0];
    let col2 = [c2[0], c2[1], c2[2], 1.0];
    let col3 = [c3[0], c3[1], c3[2], 1.0];
    vertices.push(Vertex {
        pos: p0,
        color: col0,
        uv: uv0,
    });
    vertices.push(Vertex {
        pos: p1,
        color: col1,
        uv: uv1,
    });
    vertices.push(Vertex {
        pos: p2,
        color: col2,
        uv: uv2,
    });
    vertices.push(Vertex {
        pos: p0,
        color: col0,
        uv: uv0,
    });
    vertices.push(Vertex {
        pos: p2,
        color: col2,
        uv: uv2,
    });
    vertices.push(Vertex {
        pos: p3,
        color: col3,
        uv: uv3,
    });
}

#[allow(clippy::too_many_arguments)]
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
        let ceiling_h =
            level.ceiling_height_at(wall.x + wall.width * 0.5, wall.z + wall.depth * 0.5);
        let h = wall.resolved_height(ceiling_h);
        let y0 = wall.y.min(wall.y + h);
        let y1 = wall.y.max(wall.y + h);

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

        // Top face (y = y1, normal +Y) - visible on half-height walls or window sills
        if y1 < ceiling_h - 1e-3 {
            let top_col = scale_color(1.00, top_grad);
            add_quad(
                &mut vertices,
                [x0, y1, z1],
                top_col,
                [x0, z1],
                [x1, y1, z1],
                top_col,
                [x1, z1],
                [x1, y1, z0],
                top_col,
                [x1, z0],
                [x0, y1, z0],
                top_col,
                [x0, z0],
            );
        }

        // Bottom face (y = y0, normal -Y) - visible on raised walls or window headers
        if y0 > 1e-3 {
            let bot_col = scale_color(0.85, bot_grad);
            add_quad(
                &mut vertices,
                [x0, y0, z0],
                bot_col,
                [x0, z0],
                [x1, y0, z0],
                bot_col,
                [x1, z0],
                [x1, y0, z1],
                bot_col,
                [x1, z1],
                [x0, y0, z1],
                bot_col,
                [x0, z1],
            );
        }
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
    /// Physical framebuffer size currently being rendered to. Updated on resize
    /// and HiDPI/backing-scale changes via [`Renderer::set_drawable_size`].
    drawable_size: DrawableSize,
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

        let (initial_width, initial_height) = window.drawable_size();

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
            drawable_size: DrawableSize::new(initial_width, initial_height),
        })
    }

    /// Records the current physical framebuffer size.
    ///
    /// This renderer draws directly into the default framebuffer, so no offscreen
    /// colour/depth attachments exist to recreate; the viewport and projection are
    /// derived from this size each frame. Returns `true` when the size changed,
    /// which is where any future size-dependent GPU resource would be rebuilt.
    pub fn set_drawable_size(&mut self, size: DrawableSize) -> bool {
        if self.drawable_size == size {
            return false;
        }
        self.drawable_size = size;
        true
    }

    unsafe fn upload_texture(
        gl: &glow::Context,
        texture: glow::Texture,
        raw_image: &crate::loader::RawImage,
        repeat: bool,
    ) {
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA as i32,
                raw_image.width as i32,
                raw_image.height as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&raw_image.rgba)),
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
        }
    }

    /// Re-uploads new level geometry and textures dynamically into OpenGL without recompilation.
    pub fn set_level(&mut self, loaded: &crate::loader::LoadedLevel) {
        let mesh = build_level_geometry(&loaded.level);
        self.batches = mesh.batches;

        unsafe {
            self.gl
                .bind_buffer(glow::ARRAY_BUFFER, Some(self.level_vbo));
            let byte_slice = std::slice::from_raw_parts(
                mesh.vertices.as_ptr() as *const u8,
                mesh.vertices.len() * std::mem::size_of::<Vertex>(),
            );
            self.gl
                .buffer_data_u8_slice(glow::ARRAY_BUFFER, byte_slice, glow::STATIC_DRAW);
            self.gl.bind_buffer(glow::ARRAY_BUFFER, None);

            Self::upload_texture(&self.gl, self.wall_texture, &loaded.textures.wall, true);
            Self::upload_texture(&self.gl, self.floor_texture, &loaded.textures.floor, true);
            Self::upload_texture(
                &self.gl,
                self.ceiling_texture,
                &loaded.textures.ceiling,
                true,
            );
            Self::upload_texture(
                &self.gl,
                self.white_texture,
                &loaded.textures.fixture,
                false,
            );
        }
    }

    /// Renders the 3D level combining yaw and pitch into the view matrix.
    pub fn render_scene(
        &self,
        camera_pos: glam::Vec3,
        camera_yaw: f32,
        camera_pitch: f32,
        fov_degrees: f32,
    ) {
        let drawable = self.drawable_size;
        if drawable.is_empty() {
            return;
        }

        unsafe {
            // Render at the real drawable resolution; no fixed 480x272 target.
            self.gl
                .viewport(0, 0, drawable.width as i32, drawable.height as i32);
            self.gl
                .clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);

            self.gl.use_program(Some(self.program));

            // Derive the projection from the real framebuffer aspect ratio. The
            // configured FOV is the PocketCHIP baseline; wider displays gain
            // horizontal view, taller displays keep the horizontal view instead
            // of cropping it.
            let aspect = drawable.aspect_ratio();
            let effective_fov = vertical_fov_for_aspect(fov_degrees, aspect);
            let proj = glam::Mat4::perspective_rh(effective_fov.to_radians(), aspect, 0.1, 100.0);

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
                4,
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
                28,
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
    ///
    /// UI geometry is authored in the 480x272 reference space; the projection
    /// below stays in that space while the viewport is scaled/centred to the
    /// drawable, so the HUD keeps its proportions at any resolution.
    pub fn render_ui(&self, ui_vertices: &[Vertex]) {
        let drawable = self.drawable_size;
        if ui_vertices.is_empty() || drawable.is_empty() {
            return;
        }

        let viewport = drawable.ui_viewport();

        unsafe {
            self.gl.disable(glow::DEPTH_TEST);
            self.gl.enable(glow::BLEND);
            self.gl
                .blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);

            self.gl.use_program(Some(self.program));

            self.gl
                .viewport(viewport.x, viewport.y, viewport.width, viewport.height);

            let ortho = glam::Mat4::orthographic_rh(
                0.0,
                UI_REFERENCE_WIDTH as f32,
                UI_REFERENCE_HEIGHT as f32,
                0.0,
                -1.0,
                1.0,
            );

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
                std::mem::size_of_val(ui_vertices),
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
                4,
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
                28,
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
    fn test_drawable_aspect_ratio() {
        let cases: [(u32, u32, f32); 7] = [
            (480, 272, 480.0 / 272.0),
            (1280, 720, 16.0 / 9.0),
            (1920, 1080, 16.0 / 9.0),
            (2560, 1440, 16.0 / 9.0),
            (3840, 2160, 16.0 / 9.0),
            (1600, 1200, 4.0 / 3.0),
            (960, 544, 480.0 / 272.0), // Retina 2x of the PocketCHIP baseline
        ];
        for (w, h, expected) in cases {
            let size = DrawableSize::new(w, h);
            assert!(
                (size.aspect_ratio() - expected).abs() < 1e-5,
                "{w}x{h} aspect mismatch"
            );
            assert!(!size.is_empty());
        }
    }

    fn horizontal_fov_degrees(vertical_fov_degrees: f32, aspect: f32) -> f32 {
        let half = (vertical_fov_degrees.to_radians() * 0.5).tan() * aspect;
        (2.0 * half.atan()).to_degrees()
    }

    #[test]
    fn test_vertical_fov_baseline_is_identity() {
        let baseline = reference_aspect_ratio();
        for fov in [45.0, 60.0, 90.0, 110.0] {
            assert!((vertical_fov_for_aspect(fov, baseline) - fov).abs() < 1e-4);
        }
    }

    #[test]
    fn test_wider_displays_expand_horizontally() {
        // 16:9 and 21:9 are wider than the 480x272 baseline, so the vertical FOV
        // is unchanged and the horizontal view simply grows.
        let baseline = reference_aspect_ratio();
        for aspect in [16.0 / 9.0, 21.0 / 9.0, 32.0 / 9.0] {
            assert!(aspect > baseline);
            let vfov = vertical_fov_for_aspect(60.0, aspect);
            assert_eq!(vfov, 60.0, "wider aspect must keep vertical FOV");
            assert!(
                horizontal_fov_degrees(vfov, aspect) > horizontal_fov_degrees(60.0, baseline)
            );
        }
    }

    #[test]
    fn test_taller_displays_preserve_horizontal_view() {
        let baseline = reference_aspect_ratio();
        let baseline_hfov = horizontal_fov_degrees(60.0, baseline);
        // 16:10, 4:3, 3:2 and 1:1 are all narrower than PocketCHIP.
        for aspect in [16.0 / 10.0, 4.0 / 3.0, 3.0 / 2.0, 1.0] {
            assert!(aspect < baseline);
            let vfov = vertical_fov_for_aspect(60.0, aspect);
            assert!(vfov > 60.0, "taller aspect must widen vertical FOV");
            let hfov = horizontal_fov_degrees(vfov, aspect);
            assert!(
                (hfov - baseline_hfov).abs() < 1e-3,
                "horizontal FOV cropped: {hfov} vs {baseline_hfov}"
            );
        }
    }

    #[test]
    fn test_vertical_fov_handles_degenerate_aspects() {
        assert_eq!(vertical_fov_for_aspect(60.0, 0.0), 60.0);
        assert_eq!(vertical_fov_for_aspect(60.0, -1.0), 60.0);
        assert_eq!(vertical_fov_for_aspect(60.0, f32::NAN), 60.0);
        // Extremely tall windows are capped to keep the projection invertible.
        assert!(vertical_fov_for_aspect(60.0, 0.1) <= 150.0);
    }

    #[test]
    fn test_zero_sized_drawable_is_empty_and_safe() {
        for size in [
            DrawableSize::new(0, 0),
            DrawableSize::new(0, 272),
            DrawableSize::new(480, 0),
        ] {
            assert!(size.is_empty());
            // Must not divide by zero or panic when a window is minimized.
            assert!(size.aspect_ratio().is_finite());
            let viewport = size.ui_viewport();
            assert_eq!(viewport.width, 0);
            assert_eq!(viewport.height, 0);
        }
    }

    #[test]
    fn test_ui_viewport_is_uniform_and_centred() {
        let cases = [
            (480, 272),
            (1280, 720),
            (1920, 1080),
            (2560, 1440),
            (3840, 2160),
            (1600, 1200),
            (960, 544),
        ];
        for (w, h) in cases {
            let size = DrawableSize::new(w, h);
            let vp = size.ui_viewport();

            // Fits inside the drawable and stays centred.
            assert!(vp.width <= w as i32 && vp.height <= h as i32);
            assert!(vp.x >= 0 && vp.y >= 0);
            assert!((size.width as i32 - vp.width - 2 * vp.x).abs() <= 1);
            assert!((size.height as i32 - vp.height - 2 * vp.y).abs() <= 1);

            // Reference aspect preserved (within one pixel of rounding).
            let vp_aspect = vp.width as f32 / vp.height as f32;
            let ref_aspect = UI_REFERENCE_WIDTH as f32 / UI_REFERENCE_HEIGHT as f32;
            assert!(
                (vp_aspect - ref_aspect).abs() < 0.01,
                "{w}x{h} UI aspect distorted: {vp_aspect} vs {ref_aspect}"
            );

            // HUD never becomes microscopic at large resolutions.
            assert!(vp.scale >= 1.0, "{w}x{h} UI scale shrank: {}", vp.scale);
        }
    }

    #[test]
    fn test_ui_viewport_baseline_is_identity() {
        let vp = DrawableSize::new(480, 272).ui_viewport();
        assert_eq!(
            (vp.x, vp.y, vp.width, vp.height),
            (0, 0, 480, 272),
            "PocketCHIP UI layout must be pixel-identical to the original"
        );
        assert_eq!(vp.scale, 1.0);
    }

    #[test]
    fn test_hidpi_uses_physical_pixels_not_logical_size() {
        // A 480x272 logical window on a 2x Retina display has a 960x544 drawable.
        let logical = DrawableSize::new(480, 272);
        let physical = DrawableSize::new(960, 544);

        assert_eq!(physical.ui_viewport().scale, 2.0);
        assert_eq!(physical.ui_viewport().width, 960);
        assert_eq!(physical.ui_viewport().height, 544);
        assert!((physical.aspect_ratio() - logical.aspect_ratio()).abs() < 1e-6);
    }

    #[test]
    fn test_framebuffer_size_changes_update_scale() {
        let small = DrawableSize::new(480, 272);
        let large = DrawableSize::new(1920, 1080);
        assert_ne!(small, large);
        assert!(large.ui_viewport().scale > small.ui_viewport().scale);
        assert_eq!(large.ui_viewport().scale, 1080.0 / 272.0);
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
