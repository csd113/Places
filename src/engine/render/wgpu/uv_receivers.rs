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
