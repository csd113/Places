//! Authored RGB is sRGB; material factors and lighting are linear.
//! PNG metadata does not override a resource's semantic. Alpha is coverage.

use super::RawImage;

/// IEC 61966-2-1 decode of one normalized authored colour channel.
#[must_use]
pub fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// IEC 61966-2-1 encode, used only for storage/presentation of colour.
#[must_use]
pub fn linear_to_srgb(value: f32) -> f32 {
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        value.powf(1.0 / 2.4).mul_add(1.055, -0.055)
    }
}

/// Decode a source PNG RGB byte; numeric textures must not call this.
#[must_use]
pub fn decode_byte(value: u8) -> f32 {
    srgb_to_linear(f32::from(value) / 255.0)
}

#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "finite clamped normalized channels round into the exact u8 range"
)]
const fn byte(value: f32) -> u8 {
    value.clamp(0.0, 1.0).mul_add(255.0, 0.5) as u8
}

/// Box-filter colour in linear light, weighting RGB by straight alpha.
///
/// The output remains sRGB RGBA8; the GPU decodes it once at sampling.
/// A zero-alpha block has zero RGB, preventing hidden colours bleeding out.
#[must_use]
pub fn resize_color(source: &RawImage, requested_width: u32, requested_height: u32) -> RawImage {
    let width = requested_width.max(1);
    let height = requested_height.max(1);
    let lut: [f32; 256] =
        std::array::from_fn(|index| decode_byte(u8::try_from(index).unwrap_or(0)));
    let mut rgba = Vec::with_capacity(
        usize::try_from(width)
            .unwrap_or(0)
            .saturating_mul(usize::try_from(height).unwrap_or(0))
            .saturating_mul(4),
    );
    for y in 0..height {
        let y0 = y
            .saturating_mul(source.height)
            .checked_div(height)
            .unwrap_or(0);
        let y1 = y
            .saturating_add(1)
            .saturating_mul(source.height)
            .checked_div(height)
            .unwrap_or(0);
        for x in 0..width {
            let x0 = x
                .saturating_mul(source.width)
                .checked_div(width)
                .unwrap_or(0);
            let x1 = x
                .saturating_add(1)
                .saturating_mul(source.width)
                .checked_div(width)
                .unwrap_or(0);
            let mut sum = [0.0_f32; 3];
            let mut coverage = 0.0_f32;
            let mut count = 0.0_f32;
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let offset =
                        usize::try_from(sy.saturating_mul(source.width).saturating_add(sx))
                            .unwrap_or(usize::MAX)
                            .saturating_mul(4);
                    let Some(pixel) = source.rgba.get(offset..offset.saturating_add(4)) else {
                        continue;
                    };
                    let Some(alpha) = pixel.get(3).copied() else {
                        continue;
                    };
                    let weight = f32::from(alpha) / 255.0;
                    for (channel, value) in sum.iter_mut().zip(pixel.iter().take(3)) {
                        *channel = lut
                            .get(usize::from(*value))
                            .copied()
                            .unwrap_or(0.0)
                            .mul_add(weight, *channel);
                    }
                    coverage += weight;
                    count += 1.0;
                }
            }
            let divisor = count.max(1.0);
            rgba.extend(sum.map(|channel| byte(linear_to_srgb(channel / coverage.max(1.0e-12)))));
            rgba.push(byte(coverage / divisor));
        }
    }
    RawImage::new(width, height, rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_and_fractional_light_have_one_decode_and_one_encode() {
        for value in 0..=255_u8 {
            assert_eq!(byte(linear_to_srgb(decode_byte(value))), value);
        }
        let lit = decode_byte(128) * 0.5;
        assert!((lit - 0.107_930_25).abs() < 1.0e-6);
        assert_eq!(byte(linear_to_srgb(lit)), 92);
    }

    #[test]
    fn colour_mips_average_energy_and_preserve_coverage() {
        let image = RawImage::new(2, 1, vec![0, 0, 0, 255, 255, 255, 255, 255]);
        assert_eq!(resize_color(&image, 1, 1).rgba, vec![188, 188, 188, 255]);
        let cutout = RawImage::new(2, 1, vec![255, 0, 0, 0, 0, 255, 0, 255]);
        assert_eq!(resize_color(&cutout, 1, 1).rgba, vec![0, 255, 0, 128]);
    }
}
