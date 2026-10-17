//! Projected editor guides submitted through the shared engine UI pass.

use glam::{Mat4, Vec2, Vec3, Vec4};

use super::camera::{dimension, pixel_float};
use crate::engine::font::get_white_uv;
use crate::engine::render::{DrawableSize, Vertex};
use crate::engine::spatial::Aabb;

pub(super) struct Guides {
    vertices: Vec<Vertex>,
    matrix: Mat4,
    size: DrawableSize,
}

impl Guides {
    pub const fn new(matrix: Mat4, size: DrawableSize) -> Self {
        Self {
            vertices: Vec::new(),
            matrix,
            size,
        }
    }

    pub fn grid(&mut self, target: Vec3) {
        let x = target.x.round();
        let z = target.z.round();
        for offset in -16_i16..=16 {
            let at = f32::from(offset);
            self.line(
                Vec3::new(x + at, 0.025, z - 16.0),
                Vec3::new(x + at, 0.025, z + 16.0),
                [0.38, 0.48, 0.56, 0.12],
                0.25,
            );
            self.line(
                Vec3::new(x - 16.0, 0.025, z + at),
                Vec3::new(x + 16.0, 0.025, z + at),
                [0.38, 0.48, 0.56, 0.12],
                0.25,
            );
        }
        self.line(
            Vec3::new(x - 16.0, 0.035, 0.0),
            Vec3::new(x + 16.0, 0.035, 0.0),
            [0.85, 0.28, 0.23, 0.32],
            0.45,
        );
        self.line(
            Vec3::new(0.0, 0.035, z - 16.0),
            Vec3::new(0.0, 0.035, z + 16.0),
            [0.25, 0.48, 0.95, 0.32],
            0.45,
        );
    }

    pub fn selection(&mut self, bounds: Aabb) {
        self.box_lines(bounds, [1.0, 0.74, 0.2, 1.0]);
        let centre = Vec3::from_array(bounds.centre());
        for (axis, color) in [
            (Vec3::X, [1.0, 0.25, 0.2, 1.0]),
            (Vec3::Y, [0.35, 1.0, 0.35, 1.0]),
            (Vec3::Z, [0.3, 0.55, 1.0, 1.0]),
        ] {
            self.axis(centre, axis, color);
        }
    }

    pub fn diagnostic(&mut self, bounds: Aabb, surfaces: &[[[f32; 3]; 3]]) {
        self.box_lines(bounds, [1.0, 0.25, 0.3, 1.0]);
        for triangle in surfaces {
            for (first, second) in triangle.iter().zip(triangle.iter().cycle().skip(1)).take(3) {
                self.line(
                    Vec3::from_array(*first),
                    Vec3::from_array(*second),
                    [1.0, 0.3, 0.45, 1.0],
                    1.2,
                );
            }
        }
    }

    fn box_lines(&mut self, bounds: Aabb, color: [f32; 4]) {
        let [x0, y0, z0] = bounds.min;
        let [x1, y1, z1] = bounds.max;
        let bottom = [
            Vec3::new(x0, y0, z0),
            Vec3::new(x1, y0, z0),
            Vec3::new(x1, y0, z1),
            Vec3::new(x0, y0, z1),
        ];
        let top = [
            Vec3::new(x0, y1, z0),
            Vec3::new(x1, y1, z0),
            Vec3::new(x1, y1, z1),
            Vec3::new(x0, y1, z1),
        ];
        for ring in [bottom, top] {
            for (start, end) in ring
                .into_iter()
                .zip(ring.into_iter().cycle().skip(1))
                .take(4)
            {
                self.line(start, end, color, 0.65);
            }
        }
        for (start, end) in bottom.into_iter().zip(top) {
            self.line(start, end, color, 0.65);
        }
    }

    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Guide axis positions use finite floating point vectors."
    )]
    fn axis(&mut self, origin: Vec3, direction: Vec3, color: [f32; 4]) {
        self.line(origin, origin + direction, color, 0.9);
    }

    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Projection, near clipping and guide width use finite floating point vectors."
    )]
    fn line(&mut self, start: Vec3, end: Vec3, color: [f32; 4], width: f32) {
        let mut first = self.matrix * start.extend(1.0);
        let mut second = self.matrix * end.extend(1.0);
        let near_first = first.z + first.w;
        let near_second = second.z + second.w;
        if near_first <= 0.0 && near_second <= 0.0 {
            return;
        }
        if near_first <= 0.0 {
            first = first.lerp(second, near_first / (near_first - near_second));
        }
        if near_second <= 0.0 {
            second = second.lerp(first, near_second / (near_second - near_first));
        }
        let Some((a, b)) = clip(self.point(first), self.point(second)) else {
            return;
        };
        let delta = b - a;
        if delta.length_squared() < 0.000_1 {
            return;
        }
        let side = Vec2::new(-delta.y, delta.x).normalize() * width * 0.5;
        let uv = get_white_uv();
        for position in [a - side, b - side, b + side, a - side, b + side, a + side] {
            self.vertices.push(Vertex::new(
                [position.x, position.y, 0.0],
                color,
                [uv[0], uv[1]],
            ));
        }
    }

    fn point(&self, clip: Vec4) -> Vec2 {
        let viewport = self.size.ui_viewport();
        Vec2::new(
            ((clip.x / clip.w + 1.0) * dimension(self.size.width))
                .mul_add(0.5, -pixel_float(viewport.x))
                * 480.0
                / pixel_float(viewport.width).max(1.0),
            ((1.0 - clip.y / clip.w) * dimension(self.size.height))
                .mul_add(0.5, -pixel_float(viewport.y))
                * 272.0
                / pixel_float(viewport.height).max(1.0),
        )
    }

    pub fn finish(self) -> Vec<Vertex> {
        self.vertices
    }
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "Screen clipping interpolates finite floating point positions."
)]
fn clip(start: Vec2, end: Vec2) -> Option<(Vec2, Vec2)> {
    let delta = end - start;
    let mut low = 0.0_f32;
    let mut high = 1.0_f32;
    for (p, q) in [
        (-delta.x, start.x - 112.0),
        (delta.x, 480.0 - start.x),
        (-delta.y, start.y - 38.0),
        (delta.y, 242.0 - start.y),
    ] {
        if p.abs() < 0.000_001 {
            if q < 0.0 {
                return None;
            }
        } else if p < 0.0 {
            low = low.max(q / p);
        } else {
            high = high.min(q / p);
        }
    }
    (low <= high).then_some((start + delta * low, start + delta * high))
}
