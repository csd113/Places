//! GPU-only sampling geometry for fluorescent architectural receivers.
//!
//! Prepared Full quads can span a room while the vertex-lit variant retains
//! its lighting grid. Clip both to the same world-aligned 2.5 m grid before
//! sampling UV visibility. Existing triangle boundaries and linear attributes
//! remain intact; no atlas chart, neutral mesh or light transport is rebuilt.

use crate::engine::render::common::mesh::Vertex;

#[derive(Clone, Copy)]
enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    const fn coordinate(self, [x, y, z]: [f32; 3]) -> f32 {
        match self {
            Self::X => x,
            Self::Y => y,
            Self::Z => z,
        }
    }

    const fn set(self, point: &mut [f32; 3], value: f32) {
        let [x, y, z] = point;
        match self {
            Self::X => *x = value,
            Self::Y => *y = value,
            Self::Z => *z = value,
        }
    }
}

#[derive(Clone, Copy)]
struct Corner {
    position: [f32; 3],
    /// Retain original barycentric weights until the final vertex, so atlas
    /// coordinates are rounded only once after all grid cuts.
    weights: [f32; 3],
}

/// The existing lighting-grid density and cap also bound UV sampling.
fn cuts(minimum: f32, maximum: f32) -> Vec<f32> {
    let cell = crate::engine::lighting::LIGHT_GRID_CELL_M;
    let cap = crate::engine::lighting::MAX_LIGHT_GRID_CELLS;
    let span = maximum - minimum;
    let cap_f = f32::from(u16::try_from(cap).unwrap_or(u16::MAX));
    if span > cell * cap_f {
        // Large receivers retain the existing cap rather than increasing any
        // geometry budget. Ordinary prepared charts are smaller than this.
        return crate::engine::render::common::split_span(minimum, maximum, span / cap_f)
            .into_iter()
            .skip(1)
            .map(|(start, _)| start)
            .collect();
    }
    let first = (minimum / cell).floor().mul_add(cell, cell);
    (0..cap)
        .map(|index| f32::from(u16::try_from(index).unwrap_or(u16::MAX)).mul_add(cell, first))
        .take_while(|plane| *plane < maximum)
        .filter(|plane| *plane > minimum)
        .collect()
}

fn split_polygon(polygon: Vec<Corner>, axis: Axis, plane: f32) -> Vec<Vec<Corner>> {
    let has_lower = polygon
        .iter()
        .any(|corner| axis.coordinate(corner.position) < plane);
    let has_upper = polygon
        .iter()
        .any(|corner| axis.coordinate(corner.position) > plane);
    if !has_lower || !has_upper {
        return vec![polygon];
    }
    let mut lower = Vec::new();
    let mut upper = Vec::new();
    for (start, end) in polygon
        .iter()
        .zip(polygon.iter().cycle().skip(1))
        .take(polygon.len())
    {
        let start_distance = axis.coordinate(start.position) - plane;
        let end_distance = axis.coordinate(end.position) - plane;
        if start_distance <= 0.0 {
            lower.push(*start);
        }
        if start_distance >= 0.0 {
            upper.push(*start);
        }
        if (start_distance < 0.0 && end_distance > 0.0)
            || (start_distance > 0.0 && end_distance < 0.0)
        {
            let blend = start_distance / (start_distance - end_distance);
            let mut crossing = Corner {
                position: std::array::from_fn(|component| {
                    let from = start.position.get(component).copied().unwrap_or(0.0);
                    let to = end.position.get(component).copied().unwrap_or(0.0);
                    (to - from).mul_add(blend, from)
                }),
                weights: std::array::from_fn(|component| {
                    let from = start.weights.get(component).copied().unwrap_or(0.0);
                    let to = end.weights.get(component).copied().unwrap_or(0.0);
                    (to - from).mul_add(blend, from)
                }),
            };
            axis.set(&mut crossing.position, plane);
            lower.push(crossing);
            upper.push(crossing);
        }
    }
    vec![lower, upper]
}

fn weighted_scalar(values: [f32; 3], weights: [f32; 3]) -> f32 {
    let [first, second, third] = values;
    if first.to_bits() == second.to_bits() && first.to_bits() == third.to_bits() {
        return first;
    }
    values
        .into_iter()
        .zip(weights)
        .map(|(value, weight)| value * weight)
        .sum()
}

fn interpolate(corner: Corner, source: [Vertex; 3]) -> Vertex {
    let weighted =
        |attribute: fn(Vertex) -> f32| weighted_scalar(source.map(attribute), corner.weights);
    let array = |attribute: fn(Vertex) -> [f32; 3]| {
        std::array::from_fn(|component| {
            weighted_scalar(
                source.map(|vertex| attribute(vertex).get(component).copied().unwrap_or(0.0)),
                corner.weights,
            )
        })
    };
    Vertex {
        pos: corner.position,
        color: std::array::from_fn(|component| {
            weighted_scalar(
                source.map(|vertex| vertex.color.get(component).copied().unwrap_or(0.0)),
                corner.weights,
            )
        }),
        uv: std::array::from_fn(|component| {
            weighted_scalar(
                source.map(|vertex| vertex.uv.get(component).copied().unwrap_or(0.0)),
                corner.weights,
            )
        }),
        normal: array(|vertex| vertex.normal),
        tangent: array(|vertex| vertex.tangent),
        handedness: weighted(|vertex| vertex.handedness),
        lightmap: std::array::from_fn(|component| {
            let value = weighted_scalar(
                source
                    .map(|vertex| f32::from(vertex.lightmap.get(component).copied().unwrap_or(0))),
                corner.weights,
            );
            // The original fixed-point atlas coordinate is rounded once; the
            // clamp proves this conversion cannot wrap or overflow.
            #[expect(
                clippy::as_conversions,
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "The finite interpolated atlas coordinate is rounded and clamped to the u16 range"
            )]
            let coordinate = value.round().clamp(0.0, f32::from(u16::MAX)) as u32;
            u16::try_from(coordinate).unwrap_or(u16::MAX)
        }),
        lightmap_page: source
            .first()
            .map_or(crate::engine::render::LIGHTMAP_NONE, |vertex| {
                vertex.lightmap_page
            }),
    }
}

/// Split one reactive architectural triangle without changing its surface or
/// authored attributes. The two largest projected axes cover the triangle's
/// plane; the existing lighting-grid cap bounds even exceptionally large faces.
#[must_use]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Validated finite triangle bounds and glam area checks use floating-point geometry"
)]
pub(super) fn subdivide_triangle(source: [Vertex; 3]) -> (Vec<Vertex>, Vec<u16>) {
    let unchanged = || (source.to_vec(), vec![0, 1, 2]);
    if source
        .iter()
        .any(|vertex| !vertex.pos.iter().all(|value| value.is_finite()))
        || source.iter().any(|vertex| {
            vertex.lightmap_page != source.first().map_or(0, |first| first.lightmap_page)
        })
    {
        return unchanged();
    }
    let mut axes = [Axis::X, Axis::Y, Axis::Z].map(|axis| {
        let minimum = source
            .iter()
            .map(|vertex| axis.coordinate(vertex.pos))
            .fold(f32::INFINITY, f32::min);
        let maximum = source
            .iter()
            .map(|vertex| axis.coordinate(vertex.pos))
            .fold(f32::NEG_INFINITY, f32::max);
        (maximum - minimum, axis, minimum, maximum)
    });
    axes.sort_by(|left, right| right.0.total_cmp(&left.0));
    let basis_weights = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let corners = source
        .iter()
        .zip(basis_weights)
        .map(|(vertex, weights)| Corner {
            position: vertex.pos,
            weights,
        })
        .collect();
    let mut polygons = vec![corners];
    for (_, axis, minimum, maximum) in axes.into_iter().take(2) {
        for plane in cuts(minimum, maximum) {
            polygons = polygons
                .into_iter()
                .flat_map(|polygon| split_polygon(polygon, axis, plane))
                .collect();
        }
    }
    let mut vertices = Vec::new();
    for polygon in polygons {
        let Some(origin) = polygon.first().copied() else {
            continue;
        };
        let Some(tail) = polygon.get(1..) else {
            continue;
        };
        for pair in tail.windows(2) {
            let [next, end] = pair else {
                continue;
            };
            let edge =
                glam::Vec3::from_array(next.position) - glam::Vec3::from_array(origin.position);
            let diagonal =
                glam::Vec3::from_array(end.position) - glam::Vec3::from_array(origin.position);
            if edge.cross(diagonal).length_squared() > 1.0e-12 {
                vertices.extend([origin, *next, *end].map(|corner| interpolate(corner, source)));
            }
        }
    }
    let Some(indices) = (0..vertices.len())
        .map(|slot| u16::try_from(slot).ok())
        .collect::<Option<Vec<_>>>()
    else {
        return unchanged();
    };
    if vertices.is_empty() {
        unchanged()
    } else {
        (vertices, indices)
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::arithmetic_side_effects,
        clippy::float_cmp,
        clippy::indexing_slicing,
        clippy::unwrap_used,
        reason = "Focused geometry regressions use bounded fixture arrays and exact expected attributes"
    )]

    use super::*;
    use crate::engine::lighting::transport::{TransportScene, TransportTriangle};
    use crate::engine::render::common::blacklight::{BlacklightSet, BlacklightVisibility};

    fn floor_vertex(position: [f32; 3]) -> Vertex {
        Vertex {
            pos: position,
            normal: [0.0, 1.0, 0.0],
            tangent: [1.0, 0.0, 0.0],
            ..Vertex::UNLIT
        }
    }

    #[test]
    fn sampling_preserves_surface_winding_tiling_color_frames_and_atlas_coordinates() {
        let source = [[-5.0, 0.0, -5.0], [5.0, 0.0, 5.0], [5.0, 0.0, -5.0]].map(|position| {
            let mut vertex = floor_vertex(position);
            vertex.uv = [position[0] * 0.25, position[2] * 0.25];
            vertex.color = [
                position[0].mul_add(0.02, 0.4),
                position[2].mul_add(0.01, 0.5),
                0.2,
                1.0,
            ];
            vertex.lightmap_page = 2;
            vertex
        });
        let mut stamped = source;
        stamped[0].lightmap = [6_000, 12_000];
        stamped[1].lightmap = [26_000, 32_000];
        stamped[2].lightmap = [26_000, 12_000];
        let (vertices, indices) = subdivide_triangle(stamped);
        assert!(
            vertices.len() > 3,
            "a ten-metre triangle must have interior samples"
        );
        assert_eq!(vertices.len(), indices.len());
        for vertex in &vertices {
            let [world_x, world_y, world_z] = vertex.pos;
            assert_eq!(world_y, 0.0);
            assert!(world_x.mul_add(-0.25, vertex.uv[0]).abs() < 1.0e-6);
            assert!(world_z.mul_add(-0.25, vertex.uv[1]).abs() < 1.0e-6);
            assert!((vertex.color[0] - world_x.mul_add(0.02, 0.4)).abs() < 1.0e-6);
            assert!((vertex.color[1] - world_z.mul_add(0.01, 0.5)).abs() < 1.0e-6);
            assert!(
                (f32::from(vertex.lightmap[0]) - (world_x + 5.0).mul_add(2_000.0, 6_000.0)).abs()
                    <= 1.0
            );
            assert!(
                (f32::from(vertex.lightmap[1]) - (world_z + 5.0).mul_add(2_000.0, 12_000.0)).abs()
                    <= 1.0
            );
            assert_eq!(vertex.lightmap_page, 2);
            assert_eq!(vertex.normal, [0.0, 1.0, 0.0]);
            assert_eq!(vertex.tangent, [1.0, 0.0, 0.0]);
            assert_eq!(vertex.handedness, 1.0);
        }
        let mut area = 0.0_f32;
        for &[first, next, end] in vertices.as_chunks::<3>().0 {
            let a = glam::Vec3::from_array(first.pos);
            let b = glam::Vec3::from_array(next.pos);
            let c = glam::Vec3::from_array(end.pos);
            let cross = (b - a).cross(c - a);
            assert!(cross.y > 0.0, "the floor's front remains upward");
            area = cross.y.mul_add(0.5, area);
            for axis in [Axis::X, Axis::Z] {
                let coordinates = [first, next, end].map(|vertex| axis.coordinate(vertex.pos));
                let low = coordinates.into_iter().fold(f32::INFINITY, f32::min);
                let high = coordinates.into_iter().fold(f32::NEG_INFINITY, f32::max);
                assert!(high - low <= crate::engine::lighting::LIGHT_GRID_CELL_M + 1.0e-5);
            }
        }
        assert!(
            (area - 50.0).abs() < 1.0e-4,
            "refinement must preserve the covered area"
        );
    }

    #[test]
    fn exceptionally_large_receivers_retain_the_existing_grid_cap_and_u16_indices() {
        let source = [
            [-900.0, 0.0, -900.0],
            [900.0, 0.0, 900.0],
            [900.0, 0.0, -900.0],
        ]
        .map(floor_vertex);
        let (vertices, indices) = subdivide_triangle(source);
        assert!(
            vertices.len() < 3_000,
            "the existing axis cap bounds even kilometre-scale receivers"
        );
        assert_eq!(indices.len(), vertices.len());
        assert!(
            indices
                .iter()
                .all(|index| usize::from(*index) < vertices.len())
        );
    }

    #[test]
    fn mixed_atlas_pages_are_preserved_without_fabricating_intermediate_addresses() {
        let mut source = [[-5.0, 0.0, -5.0], [5.0, 0.0, 5.0], [5.0, 0.0, -5.0]].map(floor_vertex);
        source[0].lightmap_page = 0;
        source[1].lightmap_page = 1;
        let (vertices, indices) = subdivide_triangle(source);
        assert_eq!(vertices, source);
        assert_eq!(indices, [0, 1, 2]);
    }

    #[test]
    fn reactive_grid_recovers_visible_interior_when_all_coarse_corners_are_blocked()
    -> Result<(), String> {
        let level = crate::engine::level::LevelDef::from_json(r#"{
            "format_version":3,"id":"uv_grid","name":"UV grid","spawn":{"x":0,"z":0},
            "rooms":[{"x":-6,"z":-6,"width":12,"depth":12}],
            "ceiling_lights":[{"fixture":"arcade:blacklight_tube","x":0,"z":0,"y":3,"brightness":0,"ultraviolet":1}]
        }"#).map_err(|error| error.to_string())?;
        let lighting = crate::engine::lighting::LevelLighting::bake(&level);
        let sources = BlacklightSet::from_level(&level, &lighting);
        let mut blockers = Vec::new();
        // Small overhead blockers cover only the coarse floor corners' rays.
        for center_x in [-2.5_f32, 2.5] {
            for center_z in [-2.5_f32, 2.5] {
                let corners = [
                    [center_x - 0.5, 1.5, center_z - 0.5],
                    [center_x + 0.5, 1.5, center_z - 0.5],
                    [center_x + 0.5, 1.5, center_z + 0.5],
                    [center_x - 0.5, 1.5, center_z + 0.5],
                ];
                blockers.push(
                    TransportTriangle::new(corners[0], corners[1], corners[2], [0.5; 3])
                        .ok_or("first blocker triangle")?,
                );
                blockers.push(
                    TransportTriangle::new(corners[0], corners[2], corners[3], [0.5; 3])
                        .ok_or("second blocker triangle")?,
                );
            }
        }
        let scene = TransportScene::new(blockers, Vec::new()).ok_or("scene")?;
        let context = BlacklightVisibility {
            sources: &sources,
            scene: &scene,
            dynamic: None,
        };
        let coarse = [
            [-5.0, 0.0, -5.0],
            [5.0, 0.0, 5.0],
            [5.0, 0.0, -5.0],
            [-5.0, 0.0, 5.0],
        ]
        .map(floor_vertex);
        assert!(
            coarse.iter().all(|vertex| context.vertex(vertex)[0] == 0.0),
            "four blocked coarse samples falsely darken the entire Full quad"
        );
        let (refined, _) = subdivide_triangle([coarse[0], coarse[1], coarse[2]]);
        let interior = refined
            .iter()
            .find(|vertex| vertex.pos == [0.0; 3])
            .ok_or("interior grid vertex")?;
        assert_eq!(
            context.vertex(interior)[0],
            1.0,
            "refinement restores the genuinely visible interior"
        );
        let vertex_lit = Vertex {
            color: [0.01, 0.02, 0.03, 1.0],
            ..floor_vertex([0.0; 3])
        };
        assert_eq!(
            context.vertex(interior),
            context.vertex(&vertex_lit),
            "Full and Off sample the same source state and visible position despite different ordinary light"
        );
        Ok(())
    }
}
