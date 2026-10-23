//! Explicit capture-only GPU/CPU oracle, never ordinary gameplay or timed runs.
use super::{WgpuCharacters, WorldVertex};
use crate::engine::render::common::character::CharacterScene;

fn read_vertices(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &wgpu::Buffer,
) -> Result<Vec<WorldVertex>, String> {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("places-skin-audit-readback"),
        size: source.size(),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("places-skin-audit-copy"),
    });
    encoder.copy_buffer_to_buffer(source, 0, &buffer, 0, source.size());
    let _submission = queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _delivery = sender.send(result);
        });
    let _poll = device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| e.to_string())?;
    receiver
        .recv()
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let vertices = {
        let mapped = buffer
            .slice(..)
            .get_mapped_range()
            .map_err(|e| e.to_string())?;
        bytemuck::try_cast_slice::<u8, WorldVertex>(&mapped)
            .map_err(|e| e.to_string())?
            .to_vec()
    };
    buffer.unmap();
    Ok(vertices)
}

impl WgpuCharacters {
    pub(in crate::engine::render::wgpu) fn audit_deformation(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &CharacterScene,
    ) -> Result<(), String> {
        let mut compared = 0_usize;
        let mut actors = 0_usize;
        let mut max_position = 0.0_f32;
        let mut max_normal = 0.0_f32;
        for (slot, entry) in self.characters.iter().enumerate() {
            if !(entry.visible || entry.casts_flashlight_shadow)
                || !entry.pose.gpu.as_ref().is_some_and(|g| g.active)
            {
                continue;
            }
            let Some(character) = entry.key.resolve(scene) else {
                continue;
            };
            let owner = self
                .characters
                .get(entry.shared_pose.unwrap_or(slot))
                .ok_or("missing shared pose owner")?;
            let actual = read_vertices(device, queue, &owner.vertex_buffer)?;
            let mut expected = Vec::new();
            let _bounds = Self::fill_posed_vertices(
                &character.asset().model,
                character.animator(),
                character.albedo(),
                &mut expected,
            );
            if actual.len() != expected.len() {
                return Err("GPU skin vertex count differs".into());
            }
            for (index, (gpu, cpu)) in actual.iter().zip(&expected).enumerate() {
                for (value, reference) in gpu.position.into_iter().zip(cpu.position) {
                    let error = (value - reference).abs();
                    max_position = max_position.max(error);
                    if !value.is_finite() || error > reference.abs().mul_add(2.0e-5, 2.0e-5) {
                        return Err(format!(
                            "skin position differs: actor={slot} vertex={index} GPU={value} CPU={reference}"
                        ));
                    }
                }
                for (value, reference) in gpu.normal.into_iter().zip(cpu.normal) {
                    let error = (value - reference).abs();
                    max_normal = max_normal.max(error);
                    if !value.is_finite() || error > 0.002 {
                        return Err(format!(
                            "skin normal differs: actor={slot} vertex={index} GPU={value} CPU={reference}"
                        ));
                    }
                }
                for ((value, min), max) in gpu
                    .position
                    .into_iter()
                    .zip(entry.pose.bounds.min)
                    .zip(entry.pose.bounds.max)
                {
                    if value < min - 1.0e-4 || value > max + 1.0e-4 {
                        return Err(format!(
                            "GPU pose escaped bounds: actor={slot} vertex={index}"
                        ));
                    }
                }
                let actual_bytes: &[u8] = bytemuck::bytes_of(gpu);
                let expected_bytes: &[u8] = bytemuck::bytes_of(cpu);
                if actual_bytes.get(24..) != expected_bytes.get(24..) {
                    return Err(format!(
                        "GPU skin changed invariant wire fields: actor={slot} vertex={index}"
                    ));
                }
            }
            compared = compared.saturating_add(actual.len());
            actors = actors.saturating_add(1);
        }
        crate::engine::logging::info(format!(
            "[skin-audit] actors={actors} vertices={compared} max_position_error={max_position:.9} max_normal_error={max_normal:.9} reused={}",
            self.characters
                .iter()
                .filter(|c| c.shared_pose.is_some())
                .count()
        ));
        Ok(())
    }
}
