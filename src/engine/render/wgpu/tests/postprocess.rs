use super::{
    POST_PRESENT_FRAGMENT_ENTRY, POST_PRESENT_RAW_FRAGMENT_ENTRY, surface_present_fragment_entry,
};

/// Four scalar format choices protect the raw window fallback without a GPU
/// or image fixture; decoding it would darken the final presented RGB.
#[test]
fn surface_copy_decodes_only_for_hardware_srgb_encoding() {
    for format in [
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    ] {
        assert_eq!(
            surface_present_fragment_entry(format),
            POST_PRESENT_FRAGMENT_ENTRY
        );
    }
    for format in [
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Bgra8Unorm,
    ] {
        assert_eq!(
            surface_present_fragment_entry(format),
            POST_PRESENT_RAW_FRAGMENT_ENTRY
        );
    }
}
