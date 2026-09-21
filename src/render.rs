use glow::HasContext;

use crate::font::generate_font_atlas;
use crate::level::{LevelDef, PropDef, WallAxis, ceiling_height_at, wall_solid_slices};

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
        let width =
            ((UI_REFERENCE_WIDTH as f32 * scale).round() as i32).clamp(1, self.width as i32);
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
    adjusted
        .to_degrees()
        .clamp(configured_vertical_fov_degrees, MAX_VERTICAL_FOV_DEGREES)
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
    pub prop_batch: BatchRange,
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

/// Texels per metre in the derived repeating floor texture. The authored carpet
/// texture is 64x64 and is displayed once per metre, so 64 keeps the default
/// floor pixel-identical while custom textures are resampled to a sane size.
const FLOOR_TEXELS_PER_METRE: u32 = 64;
/// Number of metres covered by one repeat of the derived floor texture. Two
/// metres covers the 1 m checker tint period.
const FLOOR_TILE_METRES: f32 = 2.0;
const FLOOR_TILE_TEXELS: u32 = FLOOR_TEXELS_PER_METRE * 2;

/// Bilinearly samples an RGBA image at normalized coordinates in `[0, 1)`,
/// wrapping at the edges to mirror `GL_REPEAT`.
fn sample_bilinear(src: &crate::loader::RawImage, u: f32, v: f32) -> [u8; 4] {
    let w = src.width.max(1) as i32;
    let h = src.height.max(1) as i32;
    let x = u * w as f32 - 0.5;
    let y = v * h as f32 - 0.5;
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;

    let texel = |xx: i32, yy: i32| -> [f32; 4] {
        let cx = xx.rem_euclid(w) as u32;
        let cy = yy.rem_euclid(h) as u32;
        let idx = ((cy * src.width + cx) * 4) as usize;
        [
            src.rgba[idx] as f32,
            src.rgba[idx + 1] as f32,
            src.rgba[idx + 2] as f32,
            src.rgba[idx + 3] as f32,
        ]
    };

    let c00 = texel(x0, y0);
    let c10 = texel(x0 + 1, y0);
    let c01 = texel(x0, y0 + 1);
    let c11 = texel(x0 + 1, y0 + 1);

    let mut out = [0u8; 4];
    for k in 0..4 {
        let top = c00[k] * (1.0 - fx) + c10[k] * fx;
        let bottom = c01[k] * (1.0 - fx) + c11[k] * fx;
        out[k] = (top * (1.0 - fy) + bottom * fy).round().clamp(0.0, 255.0) as u8;
    }
    out
}

/// Builds a repeating 2x2 m floor texture that bakes the 1 m alternating
/// checker tint into the source texture, so each room floor can be a single
/// quad without per-metre geometry.
pub(crate) fn generate_floor_checker_texture(
    src: &crate::loader::RawImage,
) -> crate::loader::RawImage {
    let tile = FLOOR_TILE_TEXELS;
    let mut rgba = vec![0u8; (tile * tile * 4) as usize];

    for ty in 0..tile {
        for tx in 0..tile {
            let cell_x = tx / FLOOR_TEXELS_PER_METRE;
            let cell_y = ty / FLOOR_TEXELS_PER_METRE;
            // Matches the original per-tile vertex colours `(ix + iz) % 2 == 0`.
            let tint = if (cell_x + cell_y) % 2 == 0 {
                [0.56f32, 0.51, 0.39]
            } else {
                [0.50, 0.45, 0.34]
            };
            let u = ((tx % FLOOR_TEXELS_PER_METRE) as f32 + 0.5) / FLOOR_TEXELS_PER_METRE as f32;
            let v = ((ty % FLOOR_TEXELS_PER_METRE) as f32 + 0.5) / FLOOR_TEXELS_PER_METRE as f32;
            let s = sample_bilinear(src, u, v);

            let idx = ((ty * tile + tx) * 4) as usize;
            rgba[idx] = (s[0] as f32 * tint[0]).round().clamp(0.0, 255.0) as u8;
            rgba[idx + 1] = (s[1] as f32 * tint[1]).round().clamp(0.0, 255.0) as u8;
            rgba[idx + 2] = (s[2] as f32 * tint[2]).round().clamp(0.0, 255.0) as u8;
            rgba[idx + 3] = s[3];
        }
    }

    crate::loader::RawImage::new(tile, tile, rgba)
}

/// Merges overlapping/adjacent Y intervals into a sorted, disjoint list.
fn merge_intervals(mut intervals: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    intervals.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut merged: Vec<(f32, f32)> = Vec::with_capacity(intervals.len());
    for (bottom, top) in intervals {
        if let Some(last) = merged.last_mut()
            && bottom <= last.1 + 1e-3
        {
            last.1 = last.1.max(top);
            continue;
        }
        merged.push((bottom, top));
    }
    merged
}

/// Y ranges that are solid on exactly one of the two sides of a wall cross
/// section: the faces exposed by an opening or by the wall's end.
fn interval_symmetric_difference(left: &[(f32, f32)], right: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let left = merge_intervals(left.to_vec());
    let right = merge_intervals(right.to_vec());

    let mut cuts: Vec<f32> = Vec::with_capacity((left.len() + right.len()) * 2);
    for (bottom, top) in left.iter().chain(right.iter()) {
        cuts.push(*bottom);
        cuts.push(*top);
    }
    cuts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    cuts.dedup_by(|a, b| (*a - *b).abs() <= 1e-3);

    let covers = |intervals: &[(f32, f32)], y: f32| {
        intervals
            .iter()
            .any(|(bottom, top)| *bottom <= y && y <= *top)
    };

    let mut difference = Vec::new();
    for bounds in cuts.windows(2) {
        let (bottom, top) = (bounds[0], bounds[1]);
        if top <= bottom + 1e-3 {
            continue;
        }
        let middle = (bottom + top) * 0.5;
        if covers(&left, middle) != covers(&right, middle) {
            difference.push((bottom, top));
        }
    }
    merge_intervals(difference)
}

/// Emits a vertical quad spanning a wall's thickness at a fixed offset along
/// the wall's length axis: a wall end cap or an opening reveal.
///
/// `at` is the world coordinate along the length axis and `thickness` the
/// world span of the wall across it. UVs follow the wall face convention
/// (horizontal world coordinate, then Y).
#[allow(clippy::too_many_arguments)]
fn add_wall_cross_quad(
    vertices: &mut Vec<Vertex>,
    axis: WallAxis,
    at: f32,
    thickness: (f32, f32),
    bottom: f32,
    top: f32,
    bottom_color: [f32; 3],
    top_color: [f32; 3],
) {
    let (t0, t1) = thickness;
    match axis {
        // Length runs along X, so the cross section lies in the Z/Y plane.
        WallAxis::X => add_quad(
            vertices,
            [at, bottom, t1],
            bottom_color,
            [t1, bottom],
            [at, bottom, t0],
            bottom_color,
            [t0, bottom],
            [at, top, t0],
            top_color,
            [t0, top],
            [at, top, t1],
            top_color,
            [t1, top],
        ),
        // Length runs along Z, so the cross section lies in the X/Y plane.
        WallAxis::Z => add_quad(
            vertices,
            [t0, bottom, at],
            bottom_color,
            [t0, bottom],
            [t1, bottom, at],
            bottom_color,
            [t1, bottom],
            [t1, top, at],
            top_color,
            [t1, top],
            [t0, top, at],
            top_color,
            [t0, top],
        ),
    }
}

/// Per-face shading multipliers for a prop box, in the prop's local space.
/// The top face is brightest and the bottom darkest, so unlit props still read
/// as solid boxes.
const PROP_FACE_SHADES: [f32; 6] = [1.00, 0.62, 0.90, 0.80, 0.74, 0.86];

/// Emits one Y-rotated box for a prop: six quads tinted with the catalog
/// colour, ready to be drawn with the unshaded white texture.
fn add_prop_box(vertices: &mut Vec<Vertex>, prop: &PropDef, size: [f32; 3], color: [f32; 3]) {
    let half_w = size[0] * 0.5;
    let half_h = size[1] * 0.5;
    let half_d = size[2] * 0.5;
    let center_y = prop.y + half_h;

    let (sin_yaw, cos_yaw) = prop.rotation_degrees.to_radians().sin_cos();
    let rotate = |lx: f32, lz: f32| -> (f32, f32) {
        (
            prop.x + lx * cos_yaw + lz * sin_yaw,
            prop.z - lx * sin_yaw + lz * cos_yaw,
        )
    };
    let corner = |sx: f32, sy: f32, sz: f32| -> [f32; 3] {
        let (world_x, world_z) = rotate(sx * half_w, sz * half_d);
        [world_x, center_y + sy * half_h, world_z]
    };
    let shaded = |mult: f32| -> [f32; 3] {
        [
            (color[0] * mult).min(1.0),
            (color[1] * mult).min(1.0),
            (color[2] * mult).min(1.0),
        ]
    };

    // Corner signs per face: top, bottom, south (+Z), north (-Z), west (-X), east (+X).
    let faces: [[(f32, f32, f32); 4]; 6] = [
        [
            (-1.0, 1.0, -1.0),
            (-1.0, 1.0, 1.0),
            (1.0, 1.0, 1.0),
            (1.0, 1.0, -1.0),
        ],
        [
            (-1.0, -1.0, -1.0),
            (1.0, -1.0, -1.0),
            (1.0, -1.0, 1.0),
            (-1.0, -1.0, 1.0),
        ],
        [
            (-1.0, -1.0, 1.0),
            (1.0, -1.0, 1.0),
            (1.0, 1.0, 1.0),
            (-1.0, 1.0, 1.0),
        ],
        [
            (1.0, -1.0, -1.0),
            (-1.0, -1.0, -1.0),
            (-1.0, 1.0, -1.0),
            (1.0, 1.0, -1.0),
        ],
        [
            (-1.0, -1.0, -1.0),
            (-1.0, -1.0, 1.0),
            (-1.0, 1.0, 1.0),
            (-1.0, 1.0, -1.0),
        ],
        [
            (1.0, -1.0, 1.0),
            (1.0, -1.0, -1.0),
            (1.0, 1.0, -1.0),
            (1.0, 1.0, 1.0),
        ],
    ];
    let uvs = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];

    for (face, shade) in faces.iter().zip(PROP_FACE_SHADES) {
        let face_color = shaded(shade);
        let points = [
            corner(face[0].0, face[0].1, face[0].2),
            corner(face[1].0, face[1].1, face[1].2),
            corner(face[2].0, face[2].1, face[2].2),
            corner(face[3].0, face[3].1, face[3].2),
        ];
        add_quad_flat(
            vertices, points[0], points[1], points[2], points[3], face_color, uvs[0], uvs[1],
            uvs[2], uvs[3],
        );
    }
}

fn build_level_geometry_mesh(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
    fallback_props: &[&PropDef],
) -> LevelMesh {
    // Collect the merged room list once; geometry and ceiling lookups then
    // borrow it instead of cloning the room vector repeatedly.
    let rooms: Vec<_> = level.room_iter().collect();
    let estimate = level.estimate_geometry();
    let mut vertices = Vec::with_capacity(estimate.total_vertices as usize);
    let mut batches = LevelMeshBatches::default();

    // 1. Floor batch: one quad per rectangular room. The metre-scale checker
    //    tint is baked into the derived floor texture (see
    //    `generate_floor_checker_texture`), so no per-cell tessellation is needed.
    let floor_start = vertices.len() as i32;
    let floor_color = [1.0, 1.0, 1.0];
    for room in &rooms {
        if room.width <= 0.0 || room.depth <= 0.0 {
            continue;
        }
        let x0 = room.x;
        let x1 = room.x + room.width;
        let z0 = room.z;
        let z1 = room.z + room.depth;
        let uv = |x: f32, z: f32| [x / FLOOR_TILE_METRES, z / FLOOR_TILE_METRES];
        add_quad_flat(
            &mut vertices,
            [x0, 0.0, z0],
            [x1, 0.0, z0],
            [x1, 0.0, z1],
            [x0, 0.0, z1],
            floor_color,
            uv(x0, z0),
            uv(x1, z0),
            uv(x1, z1),
            uv(x0, z1),
        );
    }
    batches.floor_batch = BatchRange {
        start: floor_start,
        count: vertices.len() as i32 - floor_start,
    };

    // 2. Ceiling batch
    let ceiling_start = vertices.len() as i32;
    for room in &rooms {
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
            ceiling_height_at(&rooms, wall.x + wall.width * 0.5, wall.z + wall.depth * 0.5);
        let h = wall.resolved_height(ceiling_h);
        let wall_base = wall.y.min(wall.y + h);

        let north_mult = 1.00;
        let south_mult = 0.88;
        let west_mult = 0.84;
        let east_mult = 0.94;

        let top_grad = 1.05;
        let bot_grad = 0.92;
        // Reveal faces are deliberately darker than the wall faces they
        // interrupt, so doorways and windows read clearly.
        let jamb_mult = 0.78;
        let head_mult = 0.92;

        let scale_color = |mult: f32, grad: f32| -> [f32; 3] {
            [
                (base_wall[0] * mult * grad).min(1.0),
                (base_wall[1] * mult * grad).min(1.0),
                (base_wall[2] * mult * grad).min(1.0),
            ]
        };

        // The axis the wall's length runs along and the world span across its
        // thickness. Local slice offsets start at the wall's min corner.
        let axis = wall.axis();
        let (origin_x, origin_z) = wall.length_origin();
        let (t0, t1) = match axis {
            WallAxis::X => (z0, z1),
            WallAxis::Z => (x0, x1),
        };
        let slices = wall_solid_slices(wall, ceiling_h);

        // Each solid slice emits the two wall faces parallel to its length
        // axis, plus a top/bottom face where the slice does not reach the
        // ceiling or the wall base (window sills, door headers).
        for slice in &slices {
            let (l0, l1) = match axis {
                WallAxis::X => (origin_x + slice.start, origin_x + slice.end),
                WallAxis::Z => (origin_z + slice.start, origin_z + slice.end),
            };
            let (slice_bottom, slice_top) = (slice.bottom, slice.top);

            // Faces parallel to the length axis: north/south for X-axis
            // walls, west/east for Z-axis walls.
            let n_top = scale_color(north_mult, top_grad);
            let n_bot = scale_color(north_mult, bot_grad);
            let s_top = scale_color(south_mult, top_grad);
            let s_bot = scale_color(south_mult, bot_grad);
            let w_top = scale_color(west_mult, top_grad);
            let w_bot = scale_color(west_mult, bot_grad);
            let e_top = scale_color(east_mult, top_grad);
            let e_bot = scale_color(east_mult, bot_grad);

            match axis {
                WallAxis::X => {
                    // North face (z = z0, normal -Z)
                    add_quad(
                        &mut vertices,
                        [l0, slice_bottom, z0],
                        n_bot,
                        [l0, slice_bottom],
                        [l1, slice_bottom, z0],
                        n_bot,
                        [l1, slice_bottom],
                        [l1, slice_top, z0],
                        n_top,
                        [l1, slice_top],
                        [l0, slice_top, z0],
                        n_top,
                        [l0, slice_top],
                    );

                    // South face (z = z1, normal +Z)
                    add_quad(
                        &mut vertices,
                        [l1, slice_bottom, z1],
                        s_bot,
                        [l1, slice_bottom],
                        [l0, slice_bottom, z1],
                        s_bot,
                        [l0, slice_bottom],
                        [l0, slice_top, z1],
                        s_top,
                        [l0, slice_top],
                        [l1, slice_top, z1],
                        s_top,
                        [l1, slice_top],
                    );
                }
                WallAxis::Z => {
                    // West face (x = x0, normal -X)
                    add_quad(
                        &mut vertices,
                        [x0, slice_bottom, l1],
                        w_bot,
                        [l1, slice_bottom],
                        [x0, slice_bottom, l0],
                        w_bot,
                        [l0, slice_bottom],
                        [x0, slice_top, l0],
                        w_top,
                        [l0, slice_top],
                        [x0, slice_top, l1],
                        w_top,
                        [l1, slice_top],
                    );

                    // East face (x = x1, normal +X)
                    add_quad(
                        &mut vertices,
                        [x1, slice_bottom, l0],
                        e_bot,
                        [l0, slice_bottom],
                        [x1, slice_bottom, l1],
                        e_bot,
                        [l1, slice_bottom],
                        [x1, slice_top, l1],
                        e_top,
                        [l1, slice_top],
                        [x1, slice_top, l0],
                        e_top,
                        [l0, slice_top],
                    );
                }
            }

            // Top face (normal +Y): half-height walls and window sills.
            if slice_top < ceiling_h - 1e-3 {
                let top_col = scale_color(1.00, top_grad);
                match axis {
                    WallAxis::X => add_quad(
                        &mut vertices,
                        [l0, slice_top, t1],
                        top_col,
                        [l0, t1],
                        [l1, slice_top, t1],
                        top_col,
                        [l1, t1],
                        [l1, slice_top, t0],
                        top_col,
                        [l1, t0],
                        [l0, slice_top, t0],
                        top_col,
                        [l0, t0],
                    ),
                    WallAxis::Z => add_quad(
                        &mut vertices,
                        [t1, slice_top, l0],
                        top_col,
                        [l0, t1],
                        [t1, slice_top, l1],
                        top_col,
                        [l1, t1],
                        [t0, slice_top, l1],
                        top_col,
                        [l1, t0],
                        [t0, slice_top, l0],
                        top_col,
                        [l0, t0],
                    ),
                }
            }

            // Bottom face (normal -Y): visible on raised walls and on door or
            // window headers. Testing against the floor plane reproduces the
            // previous whole-wall behaviour for raised walls.
            if slice_bottom > 1e-3 {
                let bot_col = scale_color(0.85, bot_grad);
                match axis {
                    WallAxis::X => add_quad(
                        &mut vertices,
                        [l0, slice_bottom, t0],
                        bot_col,
                        [l0, t0],
                        [l1, slice_bottom, t0],
                        bot_col,
                        [l1, t0],
                        [l1, slice_bottom, t1],
                        bot_col,
                        [l1, t1],
                        [l0, slice_bottom, t1],
                        bot_col,
                        [l0, t1],
                    ),
                    WallAxis::Z => add_quad(
                        &mut vertices,
                        [t0, slice_bottom, l0],
                        bot_col,
                        [l0, t0],
                        [t0, slice_bottom, l1],
                        bot_col,
                        [l1, t0],
                        [t1, slice_bottom, l1],
                        bot_col,
                        [l1, t1],
                        [t1, slice_bottom, l0],
                        bot_col,
                        [l0, t1],
                    ),
                }
            }
        }

        // Cross-section faces: the wall's two ends (nothing is solid outside
        // the wall) and the reveals where the solid Y profile changes at a
        // slice boundary. The exposed range is the symmetric difference
        // between the solid intervals on the left and right of the boundary.
        let mut boundaries: Vec<f32> = Vec::with_capacity(slices.len() * 2 + 2);
        boundaries.push(0.0);
        boundaries.push(wall.length());
        for slice in &slices {
            boundaries.push(slice.start);
            boundaries.push(slice.end);
        }
        boundaries.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        boundaries.dedup_by(|a, b| (*a - *b).abs() <= 1e-3);

        for position in boundaries {
            let left: Vec<(f32, f32)> = slices
                .iter()
                .filter(|s| (s.end - position).abs() <= 1e-3)
                .map(|s| (s.bottom, s.top))
                .collect();
            let right: Vec<(f32, f32)> = slices
                .iter()
                .filter(|s| (s.start - position).abs() <= 1e-3)
                .map(|s| (s.bottom, s.top))
                .collect();

            let at_start = position <= 1e-3;
            let at_end = (position - wall.length()).abs() <= 1e-3;
            for (bottom, top) in interval_symmetric_difference(&left, &right) {
                // Wall ends keep the directional face shading; internal
                // reveals use the darker jamb/head colours.
                let mult = if at_start {
                    match axis {
                        WallAxis::X => west_mult,
                        WallAxis::Z => north_mult,
                    }
                } else if at_end {
                    match axis {
                        WallAxis::X => east_mult,
                        WallAxis::Z => south_mult,
                    }
                } else if bottom <= wall_base + 1e-3 {
                    jamb_mult
                } else {
                    head_mult
                };
                let at = match axis {
                    WallAxis::X => origin_x + position,
                    WallAxis::Z => origin_z + position,
                };
                add_wall_cross_quad(
                    &mut vertices,
                    axis,
                    at,
                    (t0, t1),
                    bottom,
                    top,
                    scale_color(mult, bot_grad),
                    scale_color(mult, top_grad),
                );
            }
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

    // 5. Props batch: placeholder boxes for every prop whose real model is
    //    unavailable (unknown catalogue entry, missing file, malformed GLB).
    //    Real prop geometry is added by `build_level_geometry_with_assets`,
    //    which batches instances per model and draws them with their own texture.
    let prop_start = vertices.len() as i32;
    for prop in fallback_props {
        let entry = catalog.get(&prop.model);
        let size = prop.resolved_size(entry.size);
        if !prop.x.is_finite()
            || !prop.y.is_finite()
            || !prop.z.is_finite()
            || !prop.rotation_degrees.is_finite()
            || !prop.scale.is_finite()
            || !size.iter().all(|v| v.is_finite() && *v > 0.0)
            || !entry.color.iter().all(|c| c.is_finite())
        {
            continue;
        }
        add_prop_box(&mut vertices, prop, size, entry.color);
    }
    batches.prop_batch = BatchRange {
        start: prop_start,
        count: vertices.len() as i32 - prop_start,
    };

    LevelMesh { vertices, batches }
}

/// Instanced prop geometry for one distinct prop model in a level.
///
/// Every placed instance of the same model is transformed on the CPU at level
/// load time and appended here, so the renderer binds one buffer and one
/// texture per model and issues one draw call for all of its instances. The
/// decoded model itself is parsed once and shared through
/// [`crate::props::PropAssets`].
#[derive(Clone, Debug)]
pub struct PropMeshBatch {
    /// Catalogue model path, e.g. `models/chair.glb`.
    pub model: String,
    /// Diffuse texture shared by every instance in this batch.
    pub texture: crate::loader::RawImage,
    /// Pre-transformed, non-indexed triangles (six vertices per quad).
    pub vertices: Vec<Vertex>,
}

/// Builds the level mesh with real prop geometry where possible, plus one
/// batched draw per distinct prop model.
///
/// Props whose model is missing, malformed or simply absent from the catalogue
/// still emit their catalogue-sized placeholder box into
/// `LevelMesh::batches.prop_batch`, so a broken asset degrades visibly instead
/// of vanishing, and never crashes or loops (failures are cached by
/// [`crate::props::PropAssets`]).
pub fn build_level_geometry_with_assets(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
    assets: &mut crate::props::PropAssets,
) -> (LevelMesh, Vec<PropMeshBatch>) {
    let (batches, fallbacks) = resolve_prop_instances(level, catalog, assets);
    let mesh = build_level_geometry_mesh(level, catalog, &fallbacks);
    (mesh, batches)
}

/// Builds level geometry using only built-in prop fallbacks.
///
/// Callers that can resolve the prop catalog should prefer
/// [`build_level_geometry_with_catalog`].
pub fn build_level_geometry(level: &LevelDef) -> LevelMesh {
    build_level_geometry_with_catalog(level, &crate::loader::PropCatalog::builtin())
}

/// Builds level geometry, drawing every prop as its catalogue placeholder box
/// (no GLB assets are read). Used by tests and by the asset-less fallback path.
pub fn build_level_geometry_with_catalog(
    level: &LevelDef,
    catalog: &crate::loader::PropCatalog,
) -> LevelMesh {
    let fallbacks: Vec<&PropDef> = level.props.iter().collect();
    build_level_geometry_mesh(level, catalog, &fallbacks)
}

/// Resolves every placed prop into either a batched real mesh or a fallback box,
/// sharing one decoded model (and one texture) per distinct model path.
fn resolve_prop_instances<'a>(
    level: &'a LevelDef,
    catalog: &crate::loader::PropCatalog,
    assets: &mut crate::props::PropAssets,
) -> (Vec<PropMeshBatch>, Vec<&'a PropDef>) {
    use std::collections::HashMap;

    let mut batches: Vec<PropMeshBatch> = Vec::new();
    let mut index_by_model: HashMap<String, usize> = HashMap::new();
    let mut fallbacks: Vec<&PropDef> = Vec::new();
    let mut busy_vertices = 0usize;

    for prop in &level.props {
        let entry = catalog.get(&prop.model);
        let Some(model_path) = entry.model.clone() else {
            fallbacks.push(prop);
            continue;
        };
        if busy_vertices >= crate::level::MAX_LEVEL_PROP_VERTICES {
            fallbacks.push(prop);
            continue;
        }
        let asset = match assets.resolve(&model_path) {
            Ok(asset) => asset,
            Err(error) => {
                assets.report_failure(&model_path, &error);
                fallbacks.push(prop);
                continue;
            }
        };
        if !index_by_model.contains_key(&model_path)
            && batches.len() >= crate::level::MAX_LEVEL_PROP_MODELS
        {
            fallbacks.push(prop);
            continue;
        }

        let model = prop_instance_matrix(prop);
        let batch_index = *index_by_model.entry(model_path.clone()).or_insert_with(|| {
            batches.push(PropMeshBatch {
                model: model_path.clone(),
                texture: asset.model.texture.clone(),
                vertices: Vec::with_capacity(asset.model.triangles * 6),
            });
            batches.len() - 1
        });
        let batch = &mut batches[batch_index];
        for triangle in asset.model.indices.as_chunks::<3>().0 {
            for index in triangle {
                let vertex = asset.model.vertices[*index as usize];
                let position = model.transform_point3(glam::Vec3::new(
                    vertex.pos[0],
                    vertex.pos[1],
                    vertex.pos[2],
                ));
                batch.vertices.push(Vertex {
                    pos: [position.x, position.y, position.z],
                    color: vertex.color,
                    uv: vertex.uv,
                });
            }
        }
        busy_vertices += asset.model.triangles * 3;
    }

    (batches, fallbacks)
}

/// Instance transform for a placed prop: translate, rotate about Y and scale.
///
/// This is exactly the transform the placeholder boxes use (see
/// [`add_prop_box`]), so a prop keeps its position, orientation and vertical
/// offset when its real model replaces the box. Model space is metres with the
/// origin at the floor-contact centre (see `assets/props/README.md`).
pub fn prop_instance_matrix(prop: &PropDef) -> glam::Mat4 {
    let rotation = glam::Mat4::from_rotation_y(prop.rotation_degrees.to_radians());
    let scale = glam::Mat4::from_scale(glam::Vec3::splat(prop.scale));
    glam::Mat4::from_translation(glam::Vec3::new(prop.x, prop.y, prop.z)) * rotation * scale
}

/// Applies min/mag filtering for a repeating, mipmapped texture.
///
/// Nearest filtering keeps mipmaps (`NEAREST_MIPMAP_NEAREST`) so distant
/// minification still anti-aliases instead of shimmering.
unsafe fn set_repeat_filter(gl: &glow::Context, linear: bool) {
    let (min_filter, mag_filter) = if linear {
        (glow::LINEAR_MIPMAP_LINEAR, glow::LINEAR)
    } else {
        (glow::NEAREST_MIPMAP_NEAREST, glow::NEAREST)
    };
    unsafe {
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            min_filter as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            mag_filter as i32,
        );
    }
}

unsafe fn create_texture_2d(
    gl: &glow::Context,
    width: i32,
    height: i32,
    pixels: &[u8],
    repeat: bool,
    linear: bool,
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
            set_repeat_filter(gl, linear);
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

/// One drawable batch of placed props: all instances of a single model, sharing
/// one texture, drawn as a contiguous vertex range of the prop buffer.
#[derive(Clone, Copy, Debug)]
struct PropDraw {
    texture: glow::Texture,
    start: i32,
    count: i32,
}

/// Manages OpenGL ES 2.0-compatible accelerated rendering context, textures, and scene/UI drawing.
pub struct Renderer {
    _gl_context: sdl2::video::GLContext,
    gl: glow::Context,
    program: glow::Program,
    level_vbo: glow::Buffer,
    ui_vbo: glow::Buffer,
    /// Buffer holding every batched prop instance of the current level.
    prop_vbo: glow::Buffer,
    batches: LevelMeshBatches,
    /// Catalog used to size and colour placed props. Loaded once at startup.
    prop_catalog: crate::loader::PropCatalog,
    /// Decoded prop models, shared between instances and cached across levels.
    prop_assets: crate::props::PropAssets,
    /// Per-model prop draw ranges for the current level.
    prop_draws: Vec<PropDraw>,
    /// GPU textures for prop models, keyed by catalogue model path so a level
    /// change never re-uploads a texture that is already resident.
    prop_textures: std::collections::HashMap<String, glow::Texture>,
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
    /// Whether repeating 3D textures use linear (vs nearest) filtering. Wired
    /// to the user-facing `texture_filtering` setting.
    linear_filtering: bool,
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

        let prop_catalog = crate::loader::PropCatalog::load_default();

        let (
            program,
            level_vbo,
            ui_vbo,
            prop_vbo,
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
            let wall_texture =
                create_texture_2d(&gl, 64, 64, &generate_wall_texture(), true, true)?;
            let floor_texture =
                create_texture_2d(&gl, 64, 64, &generate_carpet_texture(), true, true)?;
            let ceiling_texture =
                create_texture_2d(&gl, 64, 64, &generate_ceiling_texture(), true, true)?;
            let white_texture =
                create_texture_2d(&gl, 2, 2, &generate_white_texture(), false, false)?;
            let font_texture =
                create_texture_2d(&gl, 128, 64, &generate_font_atlas(), false, false)?;

            // Level geometry is uploaded by `rebuild_level_geometry` once the
            // renderer (and its prop asset cache) exists.
            let level_vbo = gl.create_buffer()?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(level_vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &[], glow::STATIC_DRAW);

            // Create UI VBO
            let ui_vbo = gl.create_buffer()?;
            let prop_vbo = gl.create_buffer()?;
            gl.bind_buffer(glow::ARRAY_BUFFER, None);

            (
                program,
                level_vbo,
                ui_vbo,
                prop_vbo,
                LevelMeshBatches::default(),
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

        let mut renderer = Self {
            _gl_context: gl_context,
            gl,
            program,
            level_vbo,
            ui_vbo,
            prop_vbo,
            batches,
            prop_catalog,
            prop_assets: crate::props::PropAssets::load_default(),
            prop_draws: Vec::new(),
            prop_textures: std::collections::HashMap::new(),
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
            linear_filtering: true,
            drawable_size: DrawableSize::new(initial_width, initial_height),
        };
        renderer.rebuild_level_geometry(level);
        Ok(renderer)
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

    /// Applies the user-facing texture filtering mode to the repeating 3D
    /// textures and to every cached prop texture. UI/atlas textures stay
    /// nearest-filtered to preserve crisp text.
    pub fn set_texture_filtering(&mut self, mode: &str) {
        let linear = mode != "nearest";
        if self.linear_filtering == linear {
            return;
        }
        self.linear_filtering = linear;
        unsafe {
            for texture in [self.wall_texture, self.floor_texture, self.ceiling_texture] {
                self.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                set_repeat_filter(&self.gl, linear);
            }
            for texture in self.prop_textures.values() {
                self.gl.bind_texture(glow::TEXTURE_2D, Some(*texture));
                set_repeat_filter(&self.gl, linear);
            }
            self.gl.bind_texture(glow::TEXTURE_2D, None);
        }
    }

    /// Number of draw calls the current level's props need (one per distinct
    /// model), exposed for the performance overlay and tests.
    pub fn prop_draw_count(&self) -> usize {
        self.prop_draws.len()
    }

    /// Reads back the default framebuffer as a top-down RGBA image.
    ///
    /// Used by the `LIMINAL_CAPTURE` developer/hardware path: it is the only way
    /// to inspect real prop rendering on the PocketCHIP (no screenshots over
    /// SSH) and on desktops where the window cannot be captured. Call it after
    /// drawing and before swapping buffers.
    pub fn capture_default_framebuffer(&self) -> Result<crate::loader::RawImage, String> {
        let drawable = self.drawable_size;
        if drawable.is_empty() {
            return Err("drawable has zero size; nothing to capture".into());
        }
        let width = drawable.width as usize;
        let height = drawable.height as usize;
        let mut pixels = vec![0u8; width * height * 4];
        unsafe {
            self.gl.read_pixels(
                0,
                0,
                width as i32,
                height as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut pixels)),
            );
        }
        // OpenGL returns bottom-up rows; flip into top-down image order.
        let stride = width * 4;
        let mut flipped = vec![0u8; pixels.len()];
        for row in 0..height {
            let source = (height - 1 - row) * stride;
            flipped[row * stride..(row + 1) * stride]
                .copy_from_slice(&pixels[source..source + stride]);
        }
        Ok(crate::loader::RawImage::new(
            drawable.width,
            drawable.height,
            flipped,
        ))
    }

    /// Cached prop asset statistics (models loaded/failed, triangles, texture bytes).
    pub fn prop_asset_stats(&self) -> crate::props::PropAssetStats {
        self.prop_assets.stats()
    }

    unsafe fn upload_texture(
        gl: &glow::Context,
        texture: glow::Texture,
        raw_image: &crate::loader::RawImage,
        repeat: bool,
        linear: bool,
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
                set_repeat_filter(gl, linear);
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

    /// Builds and uploads the level's static geometry plus every placed prop.
    ///
    /// Called once per level load (never per frame): each distinct prop model is
    /// parsed once, every instance transform is baked into one shared vertex
    /// buffer, and each model's texture is uploaded once and then reused for the
    /// rest of the session.
    pub fn rebuild_level_geometry(&mut self, level: &crate::level::LevelDef) {
        let (mesh, batches) =
            build_level_geometry_with_assets(level, &self.prop_catalog, &mut self.prop_assets);
        self.batches = mesh.batches;

        // Concatenate every prop batch into one buffer; each batch keeps its range.
        let total: usize = batches.iter().map(|batch| batch.vertices.len()).sum();
        let mut vertices: Vec<Vertex> = Vec::with_capacity(total);
        let mut draws: Vec<PropDraw> = Vec::with_capacity(batches.len());
        for batch in &batches {
            let texture = match self.prop_textures.get(&batch.model) {
                Some(texture) => *texture,
                None => match unsafe { self.upload_prop_texture(&batch.texture) } {
                    Ok(texture) => {
                        self.prop_textures.insert(batch.model.clone(), texture);
                        texture
                    }
                    Err(error) => {
                        eprintln!(
                            "[props] cannot upload texture for {}: {error}; skipping that batch",
                            batch.model
                        );
                        continue;
                    }
                },
            };
            let start = vertices.len() as i32;
            vertices.extend_from_slice(&batch.vertices);
            draws.push(PropDraw {
                texture,
                start,
                count: vertices.len() as i32 - start,
            });
        }

        unsafe {
            self.gl
                .bind_buffer(glow::ARRAY_BUFFER, Some(self.level_vbo));
            let level_bytes = std::slice::from_raw_parts(
                mesh.vertices.as_ptr() as *const u8,
                mesh.vertices.len() * std::mem::size_of::<Vertex>(),
            );
            self.gl
                .buffer_data_u8_slice(glow::ARRAY_BUFFER, level_bytes, glow::STATIC_DRAW);

            self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.prop_vbo));
            if vertices.is_empty() {
                self.gl
                    .buffer_data_u8_slice(glow::ARRAY_BUFFER, &[], glow::STATIC_DRAW);
            } else {
                let prop_bytes = std::slice::from_raw_parts(
                    vertices.as_ptr() as *const u8,
                    vertices.len() * std::mem::size_of::<Vertex>(),
                );
                self.gl
                    .buffer_data_u8_slice(glow::ARRAY_BUFFER, prop_bytes, glow::STATIC_DRAW);
            }
            self.gl.bind_buffer(glow::ARRAY_BUFFER, None);
        }
        self.prop_draws = draws;
    }

    /// Uploads one prop model's diffuse texture with mipmaps and CLAMP_TO_EDGE
    /// wrapping (prop UVs never tile), matching the game's filtering setting.
    unsafe fn upload_prop_texture(
        &self,
        image: &crate::loader::RawImage,
    ) -> Result<glow::Texture, String> {
        unsafe {
            let texture = self.gl.create_texture()?;
            self.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            self.gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA as i32,
                image.width as i32,
                image.height as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&image.rgba)),
            );
            self.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE as i32,
            );
            self.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE as i32,
            );
            set_repeat_filter(&self.gl, self.linear_filtering);
            self.gl.generate_mipmap(glow::TEXTURE_2D);
            self.gl.bind_texture(glow::TEXTURE_2D, None);
            Ok(texture)
        }
    }

    /// Re-uploads new level geometry and textures dynamically into OpenGL without recompilation.
    pub fn set_level(&mut self, loaded: &crate::loader::LoadedLevel) {
        self.rebuild_level_geometry(&loaded.level);
        // Bake the metre checkerboard tint into the floor texture so a room
        // floor is a single quad (see `generate_floor_checker_texture`).
        let floor_texture = generate_floor_checker_texture(&loaded.textures.floor);
        let linear = self.linear_filtering;

        unsafe {
            Self::upload_texture(
                &self.gl,
                self.wall_texture,
                &loaded.textures.wall,
                true,
                linear,
            );
            Self::upload_texture(&self.gl, self.floor_texture, &floor_texture, true, linear);
            Self::upload_texture(
                &self.gl,
                self.ceiling_texture,
                &loaded.textures.ceiling,
                true,
                linear,
            );
            Self::upload_texture(
                &self.gl,
                self.white_texture,
                &loaded.textures.fixture,
                false,
                linear,
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

            // 5. Draw placeholder boxes for props whose real model is missing
            //    (or for catalogue entries that declare none) using the unshaded
            //    white texture and their per-vertex catalog colours.
            if self.batches.prop_batch.count > 0 {
                self.gl
                    .bind_texture(glow::TEXTURE_2D, Some(self.white_texture));
                self.gl.draw_arrays(
                    glow::TRIANGLES,
                    self.batches.prop_batch.start,
                    self.batches.prop_batch.count,
                );
            }

            // 6. Draw the batched real prop geometry: one buffer, one draw call
            //    and one texture bind per distinct prop model in the level.
            if !self.prop_draws.is_empty() {
                self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.prop_vbo));
                self.gl.vertex_attrib_pointer_f32(
                    self.a_pos_loc,
                    3,
                    glow::FLOAT,
                    false,
                    std::mem::size_of::<Vertex>() as i32,
                    0,
                );
                self.gl.vertex_attrib_pointer_f32(
                    self.a_color_loc,
                    4,
                    glow::FLOAT,
                    false,
                    std::mem::size_of::<Vertex>() as i32,
                    12,
                );
                self.gl.vertex_attrib_pointer_f32(
                    self.a_uv_loc,
                    2,
                    glow::FLOAT,
                    false,
                    std::mem::size_of::<Vertex>() as i32,
                    28,
                );
                for draw in &self.prop_draws {
                    if draw.count <= 0 {
                        continue;
                    }
                    self.gl.bind_texture(glow::TEXTURE_2D, Some(draw.texture));
                    self.gl.draw_arrays(glow::TRIANGLES, draw.start, draw.count);
                }
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
    fn test_floor_geometry_does_not_scale_with_room_area() {
        let json = r#"{
            "format_version": 1,
            "id": "big",
            "name": "Big",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 100.0, "depth": 100.0, "height": 3.5 }]
        }"#;
        let level = LevelDef::from_json(json).expect("valid json");
        let mesh = build_level_geometry(&level);
        // A 100x100 m room must be a single quad (6 vertices), not 10,000 quads.
        assert_eq!(mesh.batches.floor_batch.count, 6);
        assert_eq!(mesh.batches.ceiling_batch.count, 6);
    }

    #[test]
    fn test_floor_checker_texture_bakes_tint_and_tiles() {
        let src = crate::loader::RawImage::new(64, 64, generate_carpet_texture().to_vec());
        let baked = generate_floor_checker_texture(&src);
        assert_eq!(baked.width, 128);
        assert_eq!(baked.height, 128);
        assert_eq!(baked.rgba.len(), 128 * 128 * 4);

        // Cell (0,0) uses the brighter tint, cell (1,0) the darker tint, so the
        // same source texel must differ between adjacent metre cells.
        let at = |x: usize, y: usize| -> [u8; 4] {
            let i = (y * 128 + x) * 4;
            [
                baked.rgba[i],
                baked.rgba[i + 1],
                baked.rgba[i + 2],
                baked.rgba[i + 3],
            ]
        };
        assert_ne!(at(0, 0), at(64, 0));
        assert_ne!(at(0, 0), at(0, 64));
        assert_eq!(at(0, 0), at(64, 64));
        assert_eq!(at(0, 0)[3], 255);
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
            assert!(horizontal_fov_degrees(vfov, aspect) > horizontal_fov_degrees(60.0, baseline));
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

        // Floor geometry must be one quad per room, independent of room area.
        let room_count = level.room_iter().count() as i32;
        assert_eq!(mesh.batches.floor_batch.count, room_count * 6);
        assert_eq!(mesh.batches.ceiling_batch.count, room_count * 6);
        // The whole shipped level should now be a few thousand vertices, not
        // the ~424,000 it used to be when the floor was tessellated per metre.
        assert!(
            mesh.vertices.len() < 20_000,
            "level1 unexpectedly large: {} vertices",
            mesh.vertices.len()
        );
    }

    /// Builds a compact test level: one 10x10 m room, one 10 x 0.4 m wall
    /// spanning the full ceiling height, plus the supplied openings/props.
    fn level_with_wall(openings_json: &str, props_json: &str) -> LevelDef {
        let json = format!(
            r#"{{
                "format_version": 1,
                "id": "geometry_test",
                "name": "Geometry Test",
                "spawn": {{ "x": 0.0, "z": 0.0 }},
                "room": {{ "x": -5.0, "z": -5.0, "width": 10.0, "depth": 10.0, "height": 3.5 }},
                "walls": [{{
                    "x": -5.0, "z": 0.0, "width": 10.0, "depth": 0.4, "height": 3.5,
                    "openings": {openings_json}
                }}],
                "props": {props_json}
            }}"#
        );
        LevelDef::from_json(&json).expect("valid json")
    }

    #[test]
    fn test_wall_without_openings_emits_four_faces() {
        let level = level_with_wall("[]", "[]");
        let mesh = build_level_geometry(&level);
        // Two faces parallel to the wall's length plus two end caps. The wall
        // reaches the ceiling height, so there is no top or bottom face.
        assert_eq!(mesh.batches.wall_batch.count, 4 * 6);
        assert_eq!(mesh.batches.prop_batch.count, 0);
    }

    #[test]
    fn test_wall_with_doorway_emits_more_wall_quads() {
        let plain = build_level_geometry(&level_with_wall("[]", "[]"));
        let level = level_with_wall(
            r#"[{ "kind": "door", "offset": 4.0, "width": 2.0, "height": 2.1 }]"#,
            "[]",
        );
        let door = build_level_geometry(&level);
        assert!(
            door.batches.wall_batch.count > plain.batches.wall_batch.count,
            "doorway must add jamb and header geometry"
        );
        // Three slices (2 length faces each) + door head underside + 4 cross
        // section caps (2 wall ends, 2 door jambs).
        assert_eq!(door.batches.wall_batch.count, 11 * 6);
    }

    #[test]
    fn test_wall_with_window_emits_sill_and_header_faces() {
        let level = level_with_wall(
            r#"[{ "kind": "window", "offset": 4.0, "width": 2.0, "height": 1.0, "sill": 1.0 }]"#,
            "[]",
        );
        let mesh = build_level_geometry(&level);
        // Four slices (2 length faces each): the full-height wall either side
        // of the window plus the sill and header slices, which add a sill top
        // and a head underside, plus 4 cross section caps.
        assert_eq!(mesh.batches.wall_batch.count, 14 * 6);
    }

    #[test]
    fn test_geometry_without_openings_contains_floor_ceiling_and_wall_batches() {
        let level = level_with_wall("[]", "[]");
        let mesh = build_level_geometry(&level);
        assert!(mesh.batches.floor_batch.count > 0);
        assert!(mesh.batches.ceiling_batch.count > 0);
        assert!(mesh.batches.wall_batch.count > 0);
        assert_eq!(mesh.vertices.len() % 6, 0);
    }

    #[test]
    fn test_z_axis_wall_geometry_runs_along_z() {
        let json = r#"{
            "format_version": 1,
            "id": "z_wall",
            "name": "Z Wall",
            "spawn": { "x": 5.0, "z": 5.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 },
            "walls": [{
                "x": 4.8, "z": 0.0, "width": 0.4, "depth": 10.0, "height": 3.5,
                "openings": [{ "kind": "door", "offset": 4.0, "width": 2.0, "height": 2.1 }]
            }]
        }"#;
        let level = LevelDef::from_json(json).expect("valid json");
        let mesh = build_level_geometry(&level);

        // Same decomposition as the equivalent X-axis wall: 3 slices x 2
        // length faces + door head underside + 4 cross section caps.
        assert_eq!(mesh.batches.wall_batch.count, 11 * 6);

        let start = mesh.batches.wall_batch.start as usize;
        let end = start + mesh.batches.wall_batch.count as usize;
        let wall_vertices = &mesh.vertices[start..end];
        let (min_x, max_x) = wall_vertices.iter().fold((f32::MAX, f32::MIN), |acc, v| {
            (acc.0.min(v.pos[0]), acc.1.max(v.pos[0]))
        });
        let (min_z, max_z) = wall_vertices.iter().fold((f32::MAX, f32::MIN), |acc, v| {
            (acc.0.min(v.pos[2]), acc.1.max(v.pos[2]))
        });
        // The wall spans the room in Z and only its thickness in X.
        assert!(
            min_z <= 1e-3 && max_z >= 10.0 - 1e-3,
            "z span {min_z}..{max_z}"
        );
        assert!(min_x >= 4.79 && max_x <= 5.21, "x span {min_x}..{max_x}");
    }

    #[test]
    fn test_prop_batch_is_populated_for_one_prop() {
        let level = level_with_wall(
            "[]",
            r#"[{ "model": "core:crate", "x": 1.0, "z": 1.0, "size": [1.0, 1.0, 1.0] }]"#,
        );
        let mesh = build_level_geometry(&level);
        // One Y-rotated box = 6 quads = 36 vertices, drawn after the lights.
        assert_eq!(mesh.batches.prop_batch.count, 36);
        assert_eq!(
            mesh.batches.prop_batch.start,
            mesh.batches.light_batch.start + mesh.batches.light_batch.count
        );
    }

    #[test]
    fn test_props_with_invalid_extents_are_skipped() {
        let level = level_with_wall(
            "[]",
            r#"[{ "model": "core:crate", "x": 1.0, "z": 1.0, "size": [0.0, 1.0, 1.0] }]"#,
        );
        let mesh = build_level_geometry(&level);
        assert_eq!(mesh.batches.prop_batch.count, 0);
    }

    #[test]
    fn test_prop_catalog_supplies_size_and_colour() {
        let catalog = crate::loader::PropCatalog::from_json_str(
            r##"{
                "format_version": 1,
                "props": [{
                    "id": "core:test_prop", "name": "Test Prop", "category": "Decorative",
                    "size": [1.0, 2.0, 0.5], "color": "#804020", "solid": false
                }]
            }"##,
        )
        .expect("valid catalog");
        let level = level_with_wall(
            "[]",
            r#"[{ "model": "core:test_prop", "x": 0.5, "z": 0.5, "rotation_degrees": 45.0 }]"#,
        );
        let mesh = build_level_geometry_with_catalog(&level, &catalog);
        assert_eq!(mesh.batches.prop_batch.count, 36);
    }

    /// Catalogue + assets used by the real prop-geometry tests. Reading the
    /// shipped catalogue keeps the tests honest about ids and model paths.
    fn shipped_catalog() -> crate::loader::PropCatalog {
        let catalog = crate::loader::PropCatalog::load_default();
        assert!(
            catalog.contains("core:chair"),
            "shipped catalogue must list core:chair"
        );
        catalog
    }

    fn shipped_assets() -> crate::props::PropAssets {
        let assets = crate::props::PropAssets::load_default();
        assert!(
            assets.root().is_some(),
            "assets/props must exist for these tests"
        );
        assets
    }

    fn bounds_of(vertices: &[Vertex]) -> ([f32; 3], [f32; 3]) {
        let mut min = vertices[0].pos;
        let mut max = vertices[0].pos;
        for vertex in vertices {
            for axis in 0..3 {
                min[axis] = min[axis].min(vertex.pos[axis]);
                max[axis] = max[axis].max(vertex.pos[axis]);
            }
        }
        (min, max)
    }

    #[test]
    fn real_prop_geometry_replaces_the_placeholder_box() {
        let catalog = shipped_catalog();
        let mut assets = shipped_assets();
        let level = level_with_wall("[]", r#"[{ "model": "core:chair", "x": 3.0, "z": -2.0 }]"#);
        let (mesh, batches) = build_level_geometry_with_assets(&level, &catalog, &mut assets);

        // The box placeholder is gone: the chair renders as real geometry.
        assert_eq!(mesh.batches.prop_batch.count, 0);
        assert_eq!(batches.len(), 1, "one draw batch per distinct model");
        assert_eq!(batches[0].model, "models/chair.glb");
        assert!(!batches[0].vertices.is_empty());
        assert_eq!(batches[0].texture.width, 64);

        // Placed at (3, 0, -2), resting on the floor: a 0.5 x 0.9 x 0.5 chair.
        let (low, high) = bounds_of(&batches[0].vertices);
        assert!(
            (low[1]).abs() < 0.02,
            "chair must rest on the floor, got {}",
            low[1]
        );
        assert!((high[1] - 0.9).abs() < 0.06, "seat height {}", high[1]);
        assert!(
            low[0] > 2.6 && high[0] < 3.4,
            "x bounds {:?}..{:?}",
            low[0],
            high[0]
        );
        assert!(
            low[2] > -2.4 && high[2] < -1.6,
            "z bounds {:?}..{:?}",
            low[2],
            high[2]
        );
    }

    #[test]
    fn repeated_instances_share_one_batch_and_reuse_the_model() {
        let catalog = shipped_catalog();
        let mut assets = shipped_assets();
        let mut props: Vec<String> = Vec::new();
        for index in 0..10 {
            props.push(format!(
                r#"{{ "model": "core:chair", "x": {}, "z": 0.0 }}"#,
                index as f32
            ));
        }
        let level = level_with_wall("[]", &format!("[{}]", props.join(",")));
        let (_, batches) = build_level_geometry_with_assets(&level, &catalog, &mut assets);

        assert_eq!(batches.len(), 1, "ten chairs are one draw batch");
        let single = {
            let one = level_with_wall("[]", r#"[{ "model": "core:chair", "x": 0.0, "z": 0.0 }]"#);
            let (_, batches) = build_level_geometry_with_assets(&one, &catalog, &mut assets);
            batches[0].vertices.len()
        };
        assert_eq!(
            batches[0].vertices.len(),
            single * 10,
            "each instance contributes its triangles to the shared batch"
        );
        // The decoded model is parsed once and shared by every instance.
        let stats = assets.stats();
        assert_eq!(stats.models_loaded, 1);
        assert_eq!(stats.models_failed, 0);
    }

    #[test]
    fn prop_transforms_follow_position_rotation_scale_and_vertical_offset() {
        let catalog = shipped_catalog();
        let mut assets = shipped_assets();
        // The bed is 1.4 x 0.55 x 2.0 m, so a 90 degree yaw is visible in the
        // bounds; rotation, scale and a negative vertical offset all apply.
        let level = level_with_wall(
            "[]",
            r#"[{ "model": "core:bed", "x": 1.0, "y": -0.1, "z": 4.0, "rotation_degrees": 90.0, "scale": 0.5 }]"#,
        );
        let (_, batches) = build_level_geometry_with_assets(&level, &catalog, &mut assets);
        let (low, high) = bounds_of(&batches[0].vertices);

        // Rotated: 2.0 m deep bed becomes 2.0 m of X extent, at half scale 1.0 m.
        assert!(
            (low[0] - 0.5).abs() < 0.06 && (high[0] - 1.5).abs() < 0.06,
            "rotated x bounds {:?}..{:?}",
            low[0],
            high[0]
        );
        assert!(
            (low[2] - 3.65).abs() < 0.06 && (high[2] - 4.35).abs() < 0.06,
            "rotated z bounds {:?}..{:?}",
            low[2],
            high[2]
        );
        assert!(
            (low[1] + 0.1).abs() < 0.02,
            "vertical offset must sink the prop: base at {}",
            low[1]
        );
        assert!(
            (high[1] - 0.175).abs() < 0.03,
            "half-scale bed top at {}",
            high[1]
        );
    }

    fn shipped_level(name: &str) -> crate::level::LevelDef {
        let path = format!("assets/levels/{name}.json");
        let content = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("{path} must be readable: {error}"));
        crate::level::LevelDef::from_json(&content)
            .unwrap_or_else(|error| panic!("{path} must parse: {error}"))
    }

    #[test]
    fn the_showcase_level_renders_every_core_prop_with_real_geometry() {
        let catalog = shipped_catalog();
        let mut assets = shipped_assets();
        let level = shipped_level("prop_showcase");
        let (mesh, batches) = build_level_geometry_with_assets(&level, &catalog, &mut assets);

        assert_eq!(
            mesh.batches.prop_batch.count, 0,
            "no placeholder boxes expected"
        );
        assert_eq!(
            batches.len(),
            21,
            "the showcase places every catalogue prop"
        );
        let mut models: Vec<&str> = batches.iter().map(|batch| batch.model.as_str()).collect();
        models.sort_unstable();
        models.dedup();
        assert_eq!(models.len(), 21, "each prop model appears exactly once");

        // Every catalogue prop id is exercised by this fixture.
        let used: std::collections::HashSet<&str> =
            level.props.iter().map(|prop| prop.model.as_str()).collect();
        for entry in catalog.entries() {
            assert!(
                used.contains(entry.id.as_str()),
                "the showcase must place {}",
                entry.id
            );
        }
        assert_eq!(assets.stats().models_failed, 0);

        // The intentional clipping is present in the data, not corrected.
        let sunk = level
            .props
            .iter()
            .find(|prop| prop.model == "core:crate" && prop.y < 0.0)
            .expect("the showcase keeps one crate sunk into the floor");
        assert!(sunk.solid, "the sunk crate still blocks the player");
    }

    #[test]
    fn the_stress_level_batches_repeats_into_one_draw_per_model() {
        let catalog = shipped_catalog();
        let mut assets = shipped_assets();
        let level = shipped_level("prop_stress");
        assert!(
            level.props.len() >= 100,
            "the stress level needs a real load"
        );

        let (_, batches) = build_level_geometry_with_assets(&level, &catalog, &mut assets);
        assert!(
            batches.len() <= 12,
            "{} distinct models must collapse into a handful of draw calls, got {}",
            level.props.len(),
            batches.len()
        );
        assert_eq!(assets.stats().models_failed, 0);

        let total_vertices: usize = batches.iter().map(|batch| batch.vertices.len()).sum();
        // Cross-check the expansion: one draw batch per model, but every placed
        // instance contributes its triangles. The decoded asset is shared, so
        // the cache only holds one copy per model (proving instance reuse).
        let expected: usize = level
            .props
            .iter()
            .map(|prop| {
                let entry = catalog.get(&prop.model);
                let path = entry.model.expect("stress props come from the catalogue");
                assets.resolve(&path).expect("model loads").model.triangles * 3
            })
            .sum();
        assert_eq!(total_vertices, expected);
        assert!(
            total_vertices > assets.stats().triangles * 3,
            "repeated instances must cost vertices, not extra decoded models"
        );
        assert!(
            total_vertices <= crate::level::MAX_LEVEL_PROP_VERTICES,
            "the stress level must stay inside the prop vertex budget ({} vertices)",
            total_vertices
        );

        // Ten-plus instances of one model still share a single batch entry.
        let chair_batch = batches
            .iter()
            .find(|batch| batch.model == "models/chair.glb")
            .expect("chairs are the stress level's main load");
        assert!(
            chair_batch.vertices.len() > 60 * 100,
            "sixty chairs should expand into one large shared batch, got {} vertices",
            chair_batch.vertices.len()
        );
    }

    #[test]
    fn a_broken_model_falls_back_to_the_placeholder_box_without_panicking() {
        let catalog = crate::loader::PropCatalog::from_json_str(
            r##"{
                "format_version": 1,
                "props": [{
                    "id": "core:broken", "name": "Broken", "category": "Other",
                    "size": [0.5, 1.0, 0.5], "color": "#808080",
                    "model": "models/does_not_exist.glb"
                }]
            }"##,
        )
        .expect("valid catalog");
        let mut assets = shipped_assets();
        let level = level_with_wall("[]", r#"[{ "model": "core:broken", "x": 0.0, "z": 0.0 }]"#);
        let (mesh, batches) = build_level_geometry_with_assets(&level, &catalog, &mut assets);

        assert!(batches.is_empty(), "no real geometry for a missing model");
        assert_eq!(
            mesh.batches.prop_batch.count, 36,
            "a missing model must draw its placeholder box"
        );
        assert_eq!(assets.stats().models_failed, 1);
    }
}
