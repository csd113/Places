//! Compact center-opening lift geometry, using existing file-backed materials.
use super::doors::{MeshBuilder, Paint};
use crate::engine::elevator::{
    CABIN_DOOR_Z, DEPTH, DOOR_HEIGHT, DOOR_WIDTH, Elevator, HEIGHT, LANDING_DOOR_Z, MATERIALS,
    Phase,
};
use crate::engine::gltf::PropModel;
use crate::engine::loader::RawImage;
use crate::engine::materials::{MaterialAlpha, MaterialTable};
use std::sync::Arc;

#[derive(Clone, Copy)]
pub enum Part {
    Cabin,
    Frame,
    Leaf,
}
pub struct Models {
    pub cabin: PropModel,
    pub frame: PropModel,
    pub leaf: PropModel,
    pub textures: Vec<Arc<RawImage>>,
}
impl Models {
    #[must_use]
    pub const fn model(&self, part: Part) -> &PropModel {
        match part {
            Part::Cabin => &self.cabin,
            Part::Frame => &self.frame,
            Part::Leaf => &self.leaf,
        }
    }
    #[must_use]
    pub fn alphas(model: &PropModel) -> Vec<MaterialAlpha> {
        vec![MaterialAlpha::default(); model.submeshes.len()]
    }
}
#[must_use]
pub fn models(materials: &MaterialTable, stops: usize) -> Option<Models> {
    let textures = MATERIALS
        .iter()
        .map(|id| materials.entry_of(id)?.image.clone())
        .collect::<Option<Vec<_>>>()?;
    let metal = Paint::new(0, 2.0, 0.75);
    let wall = Paint::new(1, 1.0, 0.78);
    let dark = Paint::new(2, 1.0, 0.075);
    let floor = Paint::new(3, 1.0, 0.65);
    let mut cabin = MeshBuilder::default();
    cabin.push_box([-1.2, -0.08, 0.0], [1.2, 0.0, DEPTH], floor);
    cabin.push_box([-1.2, HEIGHT, 0.24], [1.2, HEIGHT + 0.08, DEPTH], wall);
    for (min, max) in [
        ([-1.28, 0.0, 0.24], [-1.2, HEIGHT, 2.48]),
        ([1.2, 0.0, 0.24], [1.28, HEIGHT, 2.48]),
        ([-1.2, 0.0, 2.4], [1.2, HEIGHT, 2.48]),
        ([-1.2, 0.0, 0.02], [-0.675, HEIGHT, 0.14]),
        ([0.675, 0.0, 0.02], [1.2, HEIGHT, 0.14]),
        ([-0.675, DOOR_HEIGHT, 0.02], [0.675, HEIGHT, 0.14]),
    ] {
        cabin.push_box(min, max, wall);
    }
    // Recessed front header, grooved threshold, base trim and waist-high rail.
    cabin.push_box(
        [-0.675, DOOR_HEIGHT, 0.145],
        [0.675, HEIGHT - 0.05, 0.16],
        dark,
    );
    cabin.push_box([-0.68, 0.001, -0.16], [0.68, 0.006, 0.2], metal);
    for z in [-0.12, -0.06, 0.0, 0.06, 0.12] {
        cabin.push_box([-0.67, 0.006, z], [0.67, 0.008, z + 0.006], dark);
    }
    cabin.push_box([-1.19, 0.04, 2.37], [1.19, 0.12, 2.399], metal);
    cabin.push_box([-1.12, 0.94, 2.28], [1.12, 0.98, 2.32], metal);
    cabin.push_box([0.79, 0.62, 0.145], [1.09, 1.74, 0.165], metal);
    for i in 0..stops {
        let y = f32::from(u16::try_from(i).unwrap_or(0)).mul_add(-0.11, 1.58);
        cabin.push_box([0.865, y - 0.04, 0.166], [1.015, y + 0.04, 0.2], dark);
        cabin.push_box([0.92, y - 0.024, 0.2], [0.96, y + 0.024, 0.208], metal);
    }
    // White ceiling diffuser in an inset metal housing; its texture remains file-backed.
    cabin.push_box(
        [-0.7, HEIGHT - 0.035, 0.7],
        [0.7, HEIGHT - 0.02, 1.7],
        metal,
    );
    cabin.push_box(
        [-0.62, HEIGHT - 0.04, 0.78],
        [0.62, HEIGHT - 0.036, 1.62],
        Paint::new(1, 1.0, 1.4),
    );
    let mut frame = MeshBuilder::default();
    for (x0, x1) in [(-0.755, -0.675), (0.675, 0.755)] {
        frame.push_box([x0, -0.004, -0.17], [x1, DOOR_HEIGHT + 0.08, 0.01], metal);
    }
    frame.push_box(
        [-0.755, DOOR_HEIGHT + 0.025, -0.17],
        [0.755, DOOR_HEIGHT + 0.09, 0.01],
        metal,
    );
    frame.push_box(
        [-0.675, DOOR_HEIGHT, -0.14],
        [0.675, DOOR_HEIGHT + 0.025, 0.0],
        dark,
    );
    frame.push_box(
        [-0.1, DOOR_HEIGHT + 0.21, -0.16],
        [0.1, DOOR_HEIGHT + 0.36, -0.13],
        metal,
    );
    frame.push_box(
        [-0.075, DOOR_HEIGHT + 0.235, -0.165],
        [0.075, DOOR_HEIGHT + 0.335, -0.161],
        dark,
    );
    frame.push_box([-1.06, 0.98, -0.15], [-0.86, 1.18, -0.11], metal);
    frame.push_box([-0.99, 1.05, -0.16], [-0.93, 1.11, -0.151], dark);
    let mut leaf = MeshBuilder::default();
    leaf.push_box(
        [0.0, 0.006, -0.0225],
        [DOOR_WIDTH.mul_add(0.5, -0.003), DOOR_HEIGHT - 0.005, 0.0225],
        Paint::new(1, 2.0, 0.48),
    );
    Some(Models {
        cabin: cabin.into_model()?,
        frame: frame.into_model()?,
        leaf: leaf.into_model()?,
        textures,
    })
}
#[must_use]
pub fn parts(e: &Elevator) -> Vec<(Part, [f32; 3])> {
    let mut parts = vec![(Part::Cabin, e.def.point(0.0, e.y, 0.0).into())];
    let half = DOOR_WIDTH * 0.5;
    let leaves = |y, z, progress: f32| {
        let open = crate::engine::elevator::slide_fraction(progress);
        [
            (
                Part::Leaf,
                e.def.point(half.mul_add(-open, -half), y, z).into(),
            ),
            (
                Part::Leaf,
                e.def.point(half.mul_add(open, 0.003), y, z).into(),
            ),
        ]
    };
    parts.extend(leaves(e.y, CABIN_DOOR_Z, e.opening));
    for (i, stop) in e
        .def
        .stops
        .iter()
        .enumerate()
        .filter(|(_, s)| s.level.is_none())
    {
        parts.push((Part::Frame, e.def.point(0.0, stop.y, 0.0).into()));
        let open = if i == e.at && e.phase != Phase::Moving {
            e.opening
        } else {
            0.0
        };
        parts.extend(leaves(stop.y, LANDING_DOOR_Z, open));
    }
    parts
}
