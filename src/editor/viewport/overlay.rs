//! Projected editor guides submitted through the shared engine UI pass.

use glam::{Mat4, Vec2, Vec3, Vec4};

use super::camera::{dimension, pixel_float};
use crate::engine::font::get_white_uv;
use crate::engine::render::{DrawableSize, Vertex};
use crate::engine::spatial::Aabb;

/// Cached authoring-only boxes. They never enter the level mesh or package.
pub(super) struct VolumeGuide {
    pub bounds: Aabb,
    pub color: [f32; 4],
}

pub(super) fn volumes(level: &crate::engine::level::LevelDef) -> Vec<VolumeGuide> {
    let mut result: Vec<_> = level
        .volumes
        .iter()
        .map(|volume| {
            let (x0, x1, z0, z1) = volume.bounds();
            let (bottom, top) = volume.resolved_y_bounds(level);
            VolumeGuide {
                bounds: Aabb {
                    min: [x0, bottom, z0],
                    max: [x1, top, z1],
                },
                color: [0.3, 0.9, 0.72, 0.06],
            }
        })
        .collect();
    result.extend(level.fog_regions.iter().map(|fog| VolumeGuide {
        bounds: Aabb {
            min: fog.min,
            max: fog.max,
        },
        color: [0.67, 0.56, 1.0, 0.025],
    }));
    result.extend(level.ground_fog.iter().map(|fog| VolumeGuide {
        bounds: Aabb {
            min: [fog.min[0], fog.ground_y, fog.min[1]],
            max: [fog.max[0], fog.ground_y + fog.height, fog.max[1]],
        },
        color: [0.5, 0.72, 1.0, 0.025],
    }));
    result.extend(level.sound_emitters.iter().map(|sound| VolumeGuide {
        bounds: Aabb {
            min: [sound.x - 0.12, sound.y - 0.12, sound.z - 0.12],
            max: [sound.x + 0.12, sound.y + 0.12, sound.z + 0.12],
        },
        color: [0.35, 0.75, 1.0, 0.1],
    }));
    let surfaces = crate::engine::level::LevelSurfaces::new(level);
    result.extend(level.spawn_points.iter().map(|point| {
        let y = point
            .y
            .unwrap_or_else(|| surfaces.floor_y_at(point.x, point.z).unwrap_or(0.0));
        VolumeGuide {
            bounds: Aabb {
                min: [point.x - 0.2, y, point.z - 0.2],
                max: [point.x + 0.2, y + 0.4, point.z + 0.2],
            },
            color: [1.0, 0.45, 0.8, 0.06],
        }
    }));
    result
}

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
        for (axis, color) in [Vec3::X, Vec3::Y, Vec3::Z]
            .into_iter()
            .zip(crate::editor::ui::AXIS_COLORS)
        {
            self.axis(centre, axis, color);
        }
    }

    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Finite world-space navigation guides use float offsets for x-ray visibility."
    )]
    pub fn navigation(
        &mut self,
        coverage: &[crate::editor::navigation::Coverage],
        route: &[Vec3],
        start: Option<Vec3>,
        goal: Option<Vec3>,
    ) {
        for cell in coverage {
            let Vec3 { x, y, z } = cell.position;
            let h = cell.half;
            self.face(
                [
                    [x - h, y, z - h],
                    [x + h, y, z - h],
                    [x + h, y, z + h],
                    [x - h, y, z + h],
                ],
                cell.color,
            );
        }
        for pair in route.windows(2) {
            if let [first, second] = pair {
                self.line(
                    *first + Vec3::Y * 0.08,
                    *second + Vec3::Y * 0.08,
                    [1.0, 0.92, 0.3, 1.0],
                    1.3,
                );
            }
        }
        for (endpoint, color) in [(start, [0.3, 1.0, 0.5, 1.0]), (goal, [1.0, 0.35, 0.7, 1.0])] {
            if let Some(position) = endpoint {
                self.line(position, position + Vec3::Y * 0.65, color, 1.5);
                self.line(
                    position - Vec3::X * 0.18 + Vec3::Y * 0.08,
                    position + Vec3::X * 0.18 + Vec3::Y * 0.08,
                    color,
                    1.1,
                );
                self.line(
                    position - Vec3::Z * 0.18 + Vec3::Y * 0.08,
                    position + Vec3::Z * 0.18 + Vec3::Y * 0.08,
                    color,
                    1.1,
                );
            }
        }
    }

    pub fn project(&self, position: Vec3) -> Option<[f32; 2]> {
        let projected = self.matrix.mul_vec4(position.extend(1.0));
        if projected.w <= 0.000_01 || projected.z + projected.w <= 0.0 {
            return None;
        }
        let point = self.point(projected);
        (point.x >= 112.0 && point.x < 480.0 && point.y >= 38.0 && point.y < 242.0)
            .then_some(point.to_array())
    }

    /// Translucent x-ray guides keep non-rendering gameplay volumes visible
    /// while authoring. Clip every face to the viewport and near plane.
    pub fn volume(&mut self, guide: &VolumeGuide) {
        let [x0, y0, z0] = guide.bounds.min;
        let [x1, y1, z1] = guide.bounds.max;
        for face in [
            [[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0]],
            [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
            [[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]],
            [[x1, y0, z0], [x1, y0, z1], [x1, y1, z1], [x1, y1, z0]],
            [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]],
            [[x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1]],
        ] {
            self.face(face, guide.color);
        }
        let [r, g, b, _] = guide.color;
        self.box_lines(guide.bounds, [r, g, b, 0.65]);
    }

    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Finite clipped projected guide vertices use floating point interpolation."
    )]
    fn face(&mut self, face: [[f32; 3]; 4], color: [f32; 4]) {
        let projected: Vec<_> = face
            .into_iter()
            .map(|point| self.matrix * Vec3::from_array(point).extend(1.0))
            .collect();
        let mut near_clipped = Vec::new();
        for (first, second) in projected
            .iter()
            .zip(projected.iter().cycle().skip(1))
            .take(projected.len())
        {
            let first_distance = first.z + first.w;
            let second_distance = second.z + second.w;
            if first_distance > 0.0 {
                near_clipped.push(*first);
            }
            if (first_distance > 0.0) != (second_distance > 0.0) {
                near_clipped
                    .push(first.lerp(*second, first_distance / (first_distance - second_distance)));
            }
        }
        let mut polygon: Vec<_> = near_clipped
            .into_iter()
            .filter(|point| point.w > 0.000_01)
            .map(|point| self.point(point))
            .collect();
        for (axis, edge, greater) in [
            (0_usize, 112.0, true),
            (0, 480.0, false),
            (1, 38.0, true),
            (1, 242.0, false),
        ] {
            let mut clipped = Vec::new();
            let coordinate = |point: Vec2| if axis == 0 { point.x } else { point.y };
            for (first, second) in polygon
                .iter()
                .zip(polygon.iter().cycle().skip(1))
                .take(polygon.len())
            {
                let start = coordinate(*first) - edge;
                let end = coordinate(*second) - edge;
                let inside = if greater { start >= 0.0 } else { start <= 0.0 };
                let next_inside = if greater { end >= 0.0 } else { end <= 0.0 };
                if inside {
                    clipped.push(*first);
                }
                if inside != next_inside {
                    clipped.push(first.lerp(*second, start / (start - end)));
                }
            }
            polygon = clipped;
        }
        let Some(first) = polygon.first() else {
            return;
        };
        let uv = get_white_uv();
        for (second, third) in polygon.iter().skip(1).zip(polygon.iter().skip(2)) {
            for point in [first, second, third] {
                self.vertices
                    .push(Vertex::new([point.x, point.y, 0.0], color, [uv[0], uv[1]]));
            }
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
