//! Compare only adjacent room surfaces, never overlapping storeys or openings.
use super::{Checker, Severity};
use crate::engine::level::RoomDef;

fn adjacency(a: &RoomDef, b: &RoomDef) -> Option<([f32; 2], bool, f32)> {
    let (x0, x1, z0, z1) = a.bounds();
    let (u0, u1, v0, v1) = b.bounds();
    let x_overlap = x1.min(u1) - x0.max(u0);
    let z_overlap = z1.min(v1) - z0.max(v0);
    if z_overlap > 0.8 {
        let (left, right) = if x1 <= u0 {
            (x1, u0)
        } else if u1 <= x0 {
            (u1, x0)
        } else {
            return None;
        };
        let gap = right - left;
        if gap <= super::JOINT_NEAR_ADJACENCY_M {
            return Some((
                [
                    f32::midpoint(left, right),
                    f32::midpoint(z0.max(v0), z1.min(v1)),
                ],
                true,
                gap,
            ));
        }
    }
    if x_overlap > 0.8 {
        let (near, far) = if z1 <= v0 {
            (z1, v0)
        } else if v1 <= z0 {
            (v1, z0)
        } else {
            return None;
        };
        let gap = far - near;
        if gap <= super::JOINT_NEAR_ADJACENCY_M {
            return Some((
                [
                    f32::midpoint(x0.max(u0), x1.min(u1)),
                    f32::midpoint(near, far),
                ],
                false,
                gap,
            ));
        }
    }
    None
}
pub(super) fn check(checker: &mut Checker<'_>) {
    let level = checker.level;
    let materials = crate::engine::render::logical_materials(level);
    let boxes = level.collision_aabbs();
    for (first, a) in level.rooms.iter().enumerate() {
        for (second, b) in level.rooms.iter().enumerate().skip(first.saturating_add(1)) {
            let Some(([x, z], along_z, gap)) = adjacency(a, b) else {
                continue;
            };
            if (a.floor_y - b.floor_y).abs() > super::JOINT_ADJACENCY_M {
                continue;
            }
            let id = format!("room {first} / room {second}");
            if gap > super::JOINT_PLANE_TOL_M {
                if let Some([gap_x, gap_z]) = uncovered_gap(a, b, [x, z], along_z, gap, &boxes) {
                    checker.push("floor-footprint-crack",Severity::Warning,&id,format!("Adjacent floor footprints are separated by {gap:.4} m with an uncovered patch at this marker. Existing solid wall/threshold coverage was clipped out using the geometry emitter's rectangle subtraction. This can expose the void despite being too narrow for the flood's player body. Expansion joints or intentional slits remain valid; no room or opening is resized automatically."),[gap_x,a.floor_y,gap_z]);
                }
            } else if (a.floor_y - b.floor_y).abs() > super::JOINT_PLANE_TOL_M {
                checker.push("floor-plane-seam",Severity::Warning,&id,format!("Touching room floors differ by {:.4} m. Review whether this is an intentional step or an alignment seam. Surface regions and stair/ramp profiles retain their authored elevations.",(a.floor_y-b.floor_y).abs()),[x,a.floor_y,z]);
            }
            if gap > super::JOINT_PLANE_TOL_M || !a.ceiling.is_flat() || !b.ceiling.is_flat() {
                continue;
            }
            let roof_gap = (a.eave_y() - b.eave_y()).abs();
            if roof_gap > super::JOINT_PLANE_TOL_M && roof_gap <= super::JOINT_ADJACENCY_M {
                checker.push("ceiling-plane-seam",Severity::Warning,&id,format!("Touching flat ceilings differ by {roof_gap:.4} m. Review the intended roof height and fixture mounts; no ceiling is aligned automatically."),[x,a.eave_y(),z]);
            }
            if roof_gap > super::JOINT_PLANE_TOL_M {
                continue;
            }
            let first_mat = a
                .ceiling_material
                .as_deref()
                .unwrap_or(&level.defaults.ceiling);
            let second_mat = b
                .ceiling_material
                .as_deref()
                .unwrap_or(&level.defaults.ceiling);
            if first_mat != second_mat {
                continue;
            }
            let Some(material) = materials.entry_of(first_mat) else {
                continue;
            };
            let tile = material.tile_metres;
            if !tile.is_finite() || tile <= 0.0 {
                continue;
            }
            let mut mismatch = false;
            for delta in [-0.3_f32, 0.0, 0.3] {
                let (px, pz) = if along_z {
                    (x, z + delta)
                } else {
                    (x + delta, z)
                };
                let (first_u, first_v) = a.ceiling_tile_local(px, pz);
                let (second_u, second_v) = b.ceiling_tile_local(px, pz);
                mismatch |= [first_u - second_u, first_v - second_v].iter().any(|d| {
                    let phase = d.rem_euclid(tile);
                    phase.min(tile - phase) > 0.002
                });
            }
            if mismatch {
                checker.push("material-ceiling-continuity",Severity::Warning,&id,format!("The same ceiling material ({first_mat}, repeat {tile:.3} m) has different sampled UV phase/orientation across this seam. Explicit room tile frames may be intentional. Match tile origin/rotation manually if a continuous print is desired; fixture alignment follows those frames. Floor/wall scale comes from the shared material table. Wall corner wrapping has no per-wall UV-frame correction; automatic trim retains its own continuous run projection."),[x,a.eave_y(),z]);
            }
        }
    }
}

/// Compare measured UVs at shared cardinal wall edges. Pattern intent and
/// symmetric artwork cannot be inferred, so a phase break is review-only.
pub(super) fn wall_uv(
    checker: &mut Checker<'_>,
    mesh: &crate::engine::render::LevelMesh,
    materials: &crate::engine::materials::MaterialTable,
) {
    use crate::engine::render::SurfaceKind;
    use std::collections::{BTreeMap, BTreeSet};
    type Edge = ([f32; 3], [[f32; 2]; 2]);
    let mut edges: BTreeMap<([i32; 4], u32), Vec<Edge>> = BTreeMap::new();
    let mut reported = BTreeSet::new();
    for range in mesh
        .ranges
        .iter()
        .filter(|r| r.key.kind == SurfaceKind::Wall)
    {
        if materials
            .entry(range.key.material)
            .is_none_or(|m| m.alpha.mode != crate::engine::materials::AlphaMode::Opaque)
        {
            continue;
        }
        for &[ia, ib, ic] in range.indices.as_chunks::<3>().0 {
            for (left, right) in [(ia, ib), (ib, ic), (ic, ia)] {
                let (Some(first), Some(second)) = (
                    range.vertices.get(usize::from(left)),
                    range.vertices.get(usize::from(right)),
                ) else {
                    continue;
                };
                if (first.pos[0] - second.pos[0]).abs() > 0.0001
                    || (first.pos[2] - second.pos[2]).abs() > 0.0001
                    || (first.pos[1] - second.pos[1]).abs() < 0.25
                {
                    continue;
                }
                if first.normal[1].abs() > 0.001
                    || first.normal[0].abs().max(first.normal[2].abs()) < 0.999
                {
                    continue;
                }
                let (low, high) = if first.pos[1] < second.pos[1] {
                    (first, second)
                } else {
                    (second, first)
                };
                let key = (
                    [
                        super::quantize(low.pos[0]),
                        super::quantize(low.pos[2]),
                        super::quantize(low.pos[1]),
                        super::quantize(high.pos[1]),
                    ],
                    range.key.material,
                );
                let values = edges.entry(key).or_default();
                let seam = values.iter().any(|(normal, uv)| {
                    super::dot3(*normal, first.normal).abs() < 0.01
                        && uv.iter().zip([low.uv, high.uv]).any(|(before, after)| {
                            before.iter().zip(after).any(|(a, b)| {
                                let phase = (a - b).rem_euclid(1.0);
                                phase.min(1.0 - phase) > 0.002
                            })
                        })
                });
                if seam && reported.insert(key) {
                    let material = materials.entry(range.key.material);
                    let name = material.map_or("unknown", |m| m.id.as_str());
                    let repeat = material.map_or(1.0, |m| m.tile_metres);
                    checker.push("material-wall-corner-phase",Severity::Warning,format!("wall corner / {name}"),format!("The emitted opaque wall faces share this cardinal edge but their UV phases differ (material {name}, repeat {repeat:.3} m). This may be an intentional orientation change or invisible on symmetric artwork. Review corner wrapping visually. Current walls have no per-wall wrapping frame, so no automatic material/UV edit is offered; floor projections and generated trim keep their existing shared scale."),[low.pos[0],f32::midpoint(low.pos[1],high.pos[1]),low.pos[2]]);
                }
                values.push((first.normal, [low.uv, high.uv]));
            }
        }
    }
}

fn uncovered_gap(
    a: &RoomDef,
    b: &RoomDef,
    [x, z]: [f32; 2],
    along_z: bool,
    gap: f32,
    boxes: &[crate::engine::collision::WallAabb],
) -> Option<[f32; 2]> {
    let (x0, x1, z0, z1) = a.bounds();
    let (u0, u1, v0, v1) = b.bounds();
    let exposed = if along_z {
        (
            gap.mul_add(-0.5, x),
            gap.mul_add(0.5, x),
            z0.max(v0),
            z1.min(v1),
        )
    } else {
        (
            x0.max(u0),
            x1.min(u1),
            gap.mul_add(-0.5, z),
            gap.mul_add(0.5, z),
        )
    };
    let floor = a.floor_y.min(b.floor_y);
    let covered = boxes
        .iter()
        .filter(|solid| {
            solid.min_y <= floor + super::JOINT_PLANE_TOL_M
                && solid.max_y >= floor - super::JOINT_PLANE_TOL_M
        })
        .map(|solid| (solid.min_x, solid.max_x, solid.min_z, solid.max_z))
        .collect::<Vec<_>>();
    crate::engine::render::subtract_rectangles(exposed, &covered)
        .first()
        .map(|&(left, right, near, far)| [f32::midpoint(left, right), f32::midpoint(near, far)])
}
