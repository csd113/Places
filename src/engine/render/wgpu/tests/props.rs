use super::PropMaterialIdentity;
use crate::engine::materials::{MaterialAlpha, MaterialEmission, MaterialResponse};
use crate::engine::render::common::props::PropSubmeshBatch;

/// Scalar upload identities protect distinct numeric masks on untextured
/// models. No GPU, pixels, asset decode or map preparation is required.
#[test]
fn untextured_prop_materials_keep_resolved_fluorescence_mask_identity() {
    let mut source = PropSubmeshBatch {
        response: MaterialResponse::NONE,
        texture: None,
        emission: MaterialEmission::NONE,
        alpha: MaterialAlpha::OPAQUE,
        first_index: 0,
        index_count: 3,
    };
    source.response.fluorescence.strength = 1.0;
    source.response.fluorescence.mask = Some(0);
    // Different models both address their own first image with local 0.
    let first = PropMaterialIdentity::new(&source, 1, 1, None);
    let second = PropMaterialIdentity::new(&source, 2, 1, None);
    assert_eq!(first.texture, 0);
    assert_eq!(second.texture, 0);
    assert_eq!(first.fluorescence_mask, Some(1));
    assert_eq!(second.fluorescence_mask, Some(2));
    assert_ne!(first, second, "a material must retain its own mask binding");
    assert_eq!(first, PropMaterialIdentity::new(&source, 1, 1, None));

    source.response.fluorescence.mask = None;
    assert_eq!(
        PropMaterialIdentity::new(&source, 1, 1, None),
        PropMaterialIdentity::new(&source, 2, 1, None),
        "maskless untextured materials still share the white fallback"
    );
    source.response.fluorescence.mask = Some(1);
    assert_eq!(
        PropMaterialIdentity::new(&source, 1, 1, None).fluorescence_mask,
        None,
        "an invalid local mask never borrows the next model's image"
    );
    assert_eq!(
        PropMaterialIdentity::new(&source, usize::MAX, 2, None).fluorescence_mask,
        None,
        "a texture address overflow remains a missing mask"
    );
}
