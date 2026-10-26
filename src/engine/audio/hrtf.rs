//! Measured MIT KEMAR filters, selected/interpolated off the callback.
use glam::Vec3;
use kira::Frame;

pub(super) const TAPS: usize = 140;
pub(super) type Kernel = [Frame; TAPS];
const RECORD_BYTES: usize = 283 * 4;
const DATA: &[u8] = include_bytes!("../../../assets/audio/hrtf/kemar.bin");

pub(super) fn kernel(direction: Vec3) -> Kernel {
    let reflected = direction.x < 0.0;
    let sample_direction = Vec3::new(direction.x.abs(), direction.y, direction.z);
    let mut nearest = [(f32::INFINITY, 0_usize); 3];
    for (index, record) in DATA.as_chunks::<RECORD_BYTES>().0.iter().enumerate() {
        let point = Vec3::new(
            read_float(record, 0),
            read_float(record, 4),
            read_float(record, 8),
        );
        let distance = (1.0 - sample_direction.dot(point)).max(0.000_001);
        if let Some(slot) = nearest
            .iter()
            .position(|(previous, _)| distance < *previous)
        {
            for offset in (slot.saturating_add(1)..nearest.len()).rev() {
                if let Some(previous) = nearest.get(offset.saturating_sub(1)).copied()
                    && let Some(item) = nearest.get_mut(offset)
                {
                    *item = previous;
                }
            }
            if let Some(item) = nearest.get_mut(slot) {
                *item = (distance, index);
            }
        }
    }
    let weights = nearest.map(|(distance, _)| 1.0 / (distance * distance));
    let total = weights.iter().sum::<f32>();
    let mut result = [Frame::ZERO; TAPS];
    for ((_, index), weight) in nearest.into_iter().zip(weights) {
        let record = DATA
            .get(
                index.saturating_mul(RECORD_BYTES)
                    ..index.saturating_add(1).saturating_mul(RECORD_BYTES),
            )
            .unwrap_or_default();
        for (tap, out) in result.iter_mut().enumerate() {
            let offset = 12_usize.saturating_add(tap.saturating_mul(8));
            let left = read_float(record, offset);
            let right = read_float(record, offset.saturating_add(4));
            out.left += if reflected { right } else { left } * weight / total;
            out.right += if reflected { left } else { right } * weight / total;
        }
    }
    result
}
fn read_float(data: &[u8], offset: usize) -> f32 {
    data.get(offset..offset.saturating_add(4))
        .and_then(|v| <[u8; 4]>::try_from(v).ok())
        .map_or(0.0, f32::from_le_bytes)
}

#[cfg(test)]
#[path = "../tests/hrtf.rs"]
mod tests;
