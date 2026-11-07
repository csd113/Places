//! Current opaque posed geometry for a receiving character's practical-light
//! rays. World visibility and residual irradiance retain their existing paths.
use std::collections::HashMap;

use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

use crate::engine::lighting::transport::runtime_visibility::{Node, Triangle};
use crate::engine::render::common::CharacterScene;
use crate::engine::spatial::Aabb;

#[derive(PartialEq)]
struct PoseKey {
    identity: usize,
    asset: usize,
    pose: Vec<crate::engine::entity::JointPose>,
    opacity: u32,
    transform: [f32; 16],
    direct: Vec<crate::engine::render::common::light_transport::EntityDirectLight>,
}

struct GeometryCache {
    asset: usize,
    pose: Vec<crate::engine::entity::JointPose>,
    opacity: u32,
    geometry: super::posed_topology::Geometry,
}

struct PreparedGeometry {
    nodes: Vec<Node>,
    triangles: Vec<Triangle>,
    ambient: Vec<super::posed_ambient::Vertex>,
    receivers: Vec<super::posed_depth::Receiver>,
}

pub struct PosedVisibility {
    pub(super) ambient: super::posed_ambient::Ambient,
    pub(super) depth: super::posed_depth::Depth,
    pub nodes: wgpu::Buffer,
    pub triangles: wgpu::Buffer,
    pub generation: u64,
    pub error: Option<String>,
    pub node_count: usize,
    pub triangle_count: usize,
    keys: Vec<PoseKey>,
    roots: HashMap<usize, f32>,
    ambient_bases: HashMap<usize, f32>,
    direct_bases: HashMap<usize, f32>,
    geometry: HashMap<usize, GeometryCache>,
}

/// BENCH-only attribution, never selected by graphics/gameplay settings.
pub(super) fn diagnostic_mode() -> &'static str {
    #[cfg(feature = "visual-diagnostics")]
    {
        static MODE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        MODE.get_or_init(|| {
            if std::env::var("PLACES_BENCH").as_deref() != Ok("1") {
                return String::new();
            }
            std::env::var("PLACES_POSED_DIAGNOSTIC").unwrap_or_default()
        })
        .as_str()
    }
    #[cfg(not(feature = "visual-diagnostics"))]
    {
        ""
    }
}

pub(super) const fn query_source() -> &'static str {
    include_str!("posed_queries.wgsl")
}

impl PosedVisibility {
    pub fn new(device: &wgpu::Device) -> Self {
        let buffer = |label| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: &[0_u8; 48],
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            })
        };
        Self {
            ambient: super::posed_ambient::Ambient::new(device),
            depth: super::posed_depth::Depth::new(device),
            nodes: buffer("places-posed-visibility-nodes"),
            triangles: buffer("places-posed-visibility-triangles"),
            generation: 0,
            error: None,
            node_count: 0,
            triangle_count: 0,
            keys: Vec::new(),
            roots: HashMap::new(),
            ambient_bases: HashMap::new(),
            direct_bases: HashMap::new(),
            geometry: HashMap::new(),
        }
    }

    pub fn root_for(&self, character: &crate::engine::render::common::character::Character) -> f32 {
        self.roots
            .get(&std::ptr::from_ref(character).addr())
            .copied()
            .unwrap_or(0.0)
    }

    pub fn ambient_for(
        &self,
        character: &crate::engine::render::common::character::Character,
    ) -> f32 {
        self.ambient_bases
            .get(&std::ptr::from_ref(character).addr())
            .copied()
            .unwrap_or(0.0)
    }

    pub fn direct_for(
        &self,
        character: &crate::engine::render::common::character::Character,
    ) -> f32 {
        self.direct_bases
            .get(&std::ptr::from_ref(character).addr())
            .copied()
            .unwrap_or(0.0)
    }

    /// A settled pose/placement reuses its BVH and uploads nothing. Alpha-tested
    /// and blended faces preserve the existing opaque snapshot's exclusion.
    pub fn update(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, scene: &CharacterScene) {
        if diagnostic_mode() == "disabled" {
            self.roots.clear();
            self.ambient_bases.clear();
            self.ambient.clear();
            self.depth.clear();
            self.direct_bases.clear();
            return;
        }
        let characters = || {
            scene
                .characters()
                .iter()
                .chain(scene.runtime_characters())
                .filter(|character| {
                    character.is_visible()
                        && character.opacity() > 0.0
                        && character.spatial_lighting().is_some()
                        && character.asset().model.submeshes.iter().any(|part| {
                            part.alpha.mode == crate::engine::materials::AlphaMode::Opaque
                        })
                })
        };
        let keys: Vec<_> = characters()
            .map(|character| PoseKey {
                identity: std::ptr::from_ref(character).addr(),
                asset: std::ptr::from_ref(character.asset().as_ref()).addr(),
                pose: character.animator().joint_pose(),
                opacity: character.opacity().to_bits(),
                transform: character.transform().to_cols_array(),
                direct: character
                    .spatial_lighting()
                    .map(|lighting| lighting.direct.to_vec())
                    .unwrap_or_default(),
            })
            .collect();
        if self.keys == keys {
            return;
        }
        let ambient_changed = self.keys.len() != keys.len()
            || self.keys.iter().zip(&keys).any(|(old, new)| {
                old.identity != new.identity
                    || old.asset != new.asset
                    || old.pose != new.pose
                    || old.opacity != new.opacity
                    || old.transform[..12] != new.transform[..12]
            });
        self.keys = keys;
        self.geometry
            .retain(|identity, _| self.keys.iter().any(|key| key.identity == *identity));
        self.roots.clear();
        self.ambient_bases.clear();
        self.direct_bases.clear();
        self.depth.clear();
        self.error = None;
        let frame = match self.gather_geometry(characters(), diagnostic_mode() != "direct-only") {
            Ok(frame) => frame,
            Err(error) => {
                self.error = Some(error);
                self.ambient_bases.clear();
                self.ambient.clear();
                self.direct_bases.clear();
                for key in &self.keys {
                    let _old = self.roots.insert(key.identity, -1.0);
                }
                return;
            }
        };
        self.publish_frame(device, queue, frame, ambient_changed);
    }

    fn publish_frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: PreparedGeometry,
        ambient_changed: bool,
    ) {
        let PreparedGeometry {
            nodes,
            triangles,
            ambient: ambient_vertices,
            receivers,
        } = frame;
        self.node_count = nodes.len();
        self.triangle_count = triangles.len();
        let limit = device.limits().max_storage_buffer_binding_size;
        if !storage_fits(nodes.len(), triangles.len(), limit) {
            self.error = Some("Posed visibility exceeds the current storage binding budget".into());
        }
        if self.error.is_some() {
            self.ambient_bases.clear();
            self.ambient.clear();
            self.direct_bases.clear();
            for key in &self.keys {
                let _previous_root = self.roots.insert(key.identity, -1.0);
            }
            return;
        }
        if nodes.is_empty() {
            self.ambient.clear();
            self.depth.clear();
            self.direct_bases.clear();
            return;
        }
        let changed_nodes = upload(
            device,
            queue,
            &mut self.nodes,
            bytemuck::cast_slice::<Node, u8>(&nodes),
            limit,
        );
        let changed_triangles = upload(
            device,
            queue,
            &mut self.triangles,
            bytemuck::cast_slice::<Triangle, u8>(&triangles),
            limit,
        );
        let changed_ambient =
            self.prepare_ambient(device, queue, &ambient_vertices, ambient_changed);
        let changed_depth = if diagnostic_mode() == "ray-direct" {
            self.direct_bases.clear();
            false
        } else {
            match self
                .depth
                .prepare(device, queue, &self.triangles, &receivers)
            {
                Ok(changed) => changed,
                Err(error) => {
                    self.depth.clear();
                    self.direct_bases.clear();
                    crate::engine::logging::info(error);
                    false
                }
            }
        };
        if changed_nodes || changed_triangles || changed_ambient || changed_depth {
            self.generation = self.generation.wrapping_add(1);
        }
    }

    fn prepare_ambient(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vertices: &[super::posed_ambient::Vertex],
        changed: bool,
    ) -> bool {
        match self.ambient.prepare(
            device,
            queue,
            &self.nodes,
            &self.triangles,
            vertices,
            changed,
        ) {
            Ok(changed_buffer) => changed_buffer,
            Err(error) => {
                self.error = Some(error);
                self.ambient_bases.clear();
                self.ambient.clear();
                false
            }
        }
    }

    fn gather_geometry<'a>(
        &mut self,
        characters: impl Iterator<Item = &'a crate::engine::render::common::character::Character>,
        refresh_ambient: bool,
    ) -> Result<PreparedGeometry, String> {
        let mut frame = PreparedGeometry {
            nodes: Vec::new(),
            triangles: Vec::new(),
            ambient: Vec::new(),
            receivers: Vec::new(),
        };
        let mut ambient_count = 0_u32;
        for character in characters {
            let identity = std::ptr::from_ref(character).addr();
            self.cache_geometry(character)?;
            let cached = self
                .geometry
                .get(&identity)
                .ok_or("Current character visibility cache is unavailable")?;
            let triangle_first = u32::try_from(frame.triangles.len()).map_err(|e| e.to_string())?;
            let root = append_transformed(
                cached,
                character.transform(),
                &mut frame.nodes,
                &mut frame.triangles,
            )?;
            let _previous_root = self.roots.insert(identity, root);
            if root > 0.0 {
                let source_base = u32::try_from(frame.receivers.len())
                    .map_err(|e| e.to_string())?
                    .checked_mul(32)
                    .ok_or("Posed source base overflow")?;
                let _previous = self.direct_bases.insert(identity, exact_root(source_base));
                let bounds = cached
                    .geometry
                    .nodes
                    .first()
                    .ok_or("Posed bounds unavailable")?;
                frame.receivers.push(super::posed_depth::Receiver {
                    first: triangle_first,
                    count: u32::try_from(cached.geometry.triangles.len())
                        .map_err(|e| e.to_string())?,
                    bounds: crate::engine::render::common::props::transform_bounds(
                        &Aabb {
                            min: bounds.minimum,
                            max: bounds.maximum,
                        },
                        &character.transform(),
                    ),
                    sources: character
                        .spatial_lighting()
                        .ok_or("Posed source lighting unavailable")?
                        .direct,
                });
                let _previous_base = self
                    .ambient_bases
                    .insert(identity, exact_root(ambient_count));
                ambient_count = ambient_count.saturating_add(
                    u32::try_from(character.asset().model.vertices.len())
                        .map_err(|error| error.to_string())?,
                );
                if ambient_count >= 16_777_214 {
                    return Err("Posed ambient vertex range exceeds exact indexing".into());
                }
                if refresh_ambient {
                    append_ambient(character, root, &mut frame.ambient);
                }
            }
        }
        Ok(frame)
    }

    fn cache_geometry(
        &mut self,
        character: &crate::engine::render::common::character::Character,
    ) -> Result<(), String> {
        let identity = std::ptr::from_ref(character).addr();
        let asset = std::ptr::from_ref(character.asset().as_ref()).addr();
        let pose = character.animator().joint_pose();
        let opacity = character.opacity().to_bits();
        let changed = self.geometry.get(&identity).is_none_or(|cached| {
            cached.asset != asset || cached.pose != pose || cached.opacity != opacity
        });
        if changed {
            if let Some(cached) = self
                .geometry
                .get_mut(&identity)
                .filter(|cached| cached.asset == asset)
            {
                cached.geometry.refit(character)?;
                cached.pose = pose;
                cached.opacity = opacity;
            } else {
                let geometry = super::posed_topology::Geometry::new(character)?;
                drop(self.geometry.insert(
                    identity,
                    GeometryCache {
                        asset,
                        pose,
                        opacity,
                        geometry,
                    },
                ));
            }
        }
        Ok(())
    }
}

fn append_ambient(
    character: &crate::engine::render::common::character::Character,
    root: f32,
    output: &mut Vec<super::posed_ambient::Vertex>,
) {
    let model = &character.asset().model;
    let transform = character.transform();
    let normal_transform = transform.inverse().transpose();
    let (scale, rotation, _translation) = transform.to_scale_rotation_translation();
    let uniform_scale = scale.abs().max_element().max(1.0e-6);
    let first = output.len();
    let mut bounds = Aabb::EMPTY;
    let mut normals =
        crate::engine::render::common::character::SkinNormalCache::new(character.animator());
    for (index, vertex) in model.vertices.iter().enumerate() {
        let joints = model.joints.get(index).copied().unwrap_or([0; 4]);
        let weights = model.weights.get(index).copied().unwrap_or([0.0; 4]);
        let point = transform.transform_point3(Vec3::from_array(
            character
                .animator()
                .skin_position(joints, weights, vertex.pos),
        ));
        bounds.expand(point.to_array());
        let normal = normal_transform
            .transform_vector3(Vec3::from_array(vertex.normal.map_or([0.0; 3], |normal| {
                normals.skin_normal(joints, weights, normal)
            })))
            .normalize_or_zero();
        output.push(super::posed_ambient::Vertex {
            position_root: [point.x, point.y, point.z, root],
            normal_scale: [normal.x, normal.y, normal.z, uniform_scale],
            rotation: rotation.to_array(),
            reach: [0.0; 4],
        });
    }
    let reach = Vec3::from_array(bounds.max)
        .distance(Vec3::from_array(bounds.min))
        .max(0.01)
        * 1.01;
    for vertex in output.iter_mut().skip(first) {
        vertex.reach[0] = reach;
    }
}

fn append_transformed(
    cached: &GeometryCache,
    transform: Mat4,
    nodes: &mut Vec<Node>,
    triangles: &mut Vec<Triangle>,
) -> Result<f32, String> {
    if !cached
        .geometry
        .triangles
        .iter()
        .any(|triangle| triangle.a_solid[3] > 0.0)
    {
        return Ok(0.0);
    }
    let node_base = u32::try_from(nodes.len())
        .map_err(|error| format!("Posed visibility node range exceeds u32: {error}"))?;
    let triangle_base = u32::try_from(triangles.len())
        .map_err(|error| format!("Posed visibility triangle range exceeds u32: {error}"))?;
    if node_base >= 16_777_214 {
        return Err("Posed visibility root exceeds exact float indexing".into());
    }
    for source in &cached.geometry.nodes {
        let mut node = *source;
        let bounds = crate::engine::render::common::props::transform_bounds(
            &Aabb {
                min: node.minimum,
                max: node.maximum,
            },
            &transform,
        );
        node.minimum = bounds.min;
        node.maximum = bounds.max;
        node.first = node.first.saturating_add(if node.count == 0 {
            node_base
        } else {
            triangle_base
        });
        node.escape = node.escape.saturating_add(node_base);
        nodes.push(node);
    }
    for source in &cached.geometry.triangles {
        let point = transform.transform_point3(Vec3::from_slice(&source.a_solid[..3]));
        let edge0 = transform.transform_vector3(Vec3::from_slice(&source.edge0[..3]));
        let edge1 = transform.transform_vector3(Vec3::from_slice(&source.edge1[..3]));
        triangles.push(Triangle {
            a_solid: [point.x, point.y, point.z, source.a_solid[3]],
            edge0: [edge0.x, edge0.y, edge0.z, 0.0],
            edge1: [edge1.x, edge1.y, edge1.z, 0.0],
        });
    }
    Ok(exact_root(node_base))
}

#[expect(
    clippy::cast_precision_loss,
    clippy::as_conversions,
    reason = "Validated node roots are integers strictly below 2^24 and therefore exactly representable as f32"
)]
const fn exact_root(base: u32) -> f32 {
    base.saturating_add(1) as f32
}

fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    buffer: &mut wgpu::Buffer,
    bytes: &[u8],
    limit: u64,
) -> bool {
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    let changed = buffer.size() < size;
    if changed {
        *buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("places-posed-visibility-capacity"),
            size: size.next_power_of_two().min(limit),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
    }
    queue.write_buffer(buffer, 0, bytes);
    changed
}

fn storage_fits(nodes: usize, triangles: usize, limit: u64) -> bool {
    [nodes, triangles]
        .into_iter()
        .all(|count| u64::try_from(count).unwrap_or(u64::MAX).saturating_mul(48) <= limit)
}
