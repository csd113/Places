//! One immutable world visibility upload shared by all material passes.
use wgpu::util::DeviceExt;

pub struct StaticVisibility {
    pub nodes: wgpu::Buffer,
    pub triangles: wgpu::Buffer,
    pub fallback_depth: wgpu::TextureView,
}

impl StaticVisibility {
    pub fn new(
        device: &wgpu::Device,
        scene: Option<&crate::engine::lighting::transport::TransportScene>,
    ) -> Self {
        let (mut nodes, mut triangles) = scene.map_or_else(
            || (Vec::new(), Vec::new()),
            crate::engine::lighting::transport::TransportScene::runtime_visibility,
        );
        if nodes.is_empty() {
            nodes.push(bytemuck::Zeroable::zeroed());
        }
        if triangles.is_empty() {
            triangles.push(bytemuck::Zeroable::zeroed());
        }
        let limit = device.limits().max_storage_buffer_binding_size;
        let node_bytes = bytemuck::cast_slice(&nodes);
        let triangle_bytes = bytemuck::cast_slice(&triangles);
        // A missing/oversized resource fails closed in the shader. Never upload
        // a truncated blocker set that could admit light through omitted walls.
        let unavailable = u64::try_from(node_bytes.len()).unwrap_or(u64::MAX) > limit
            || u64::try_from(triangle_bytes.len()).unwrap_or(u64::MAX) > limit;
        let empty = [0_u8; 48];
        let upload = |label, bytes| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: if unavailable { &empty } else { bytes },
                usage: wgpu::BufferUsages::STORAGE,
            })
        };
        Self {
            nodes: upload("places-static-visibility-nodes", node_bytes),
            triangles: upload("places-static-visibility-triangles", triangle_bytes),
            fallback_depth: super::flashlight::depth_texture(device, 1)
                .create_view(&wgpu::TextureViewDescriptor::default()),
        }
    }
}
