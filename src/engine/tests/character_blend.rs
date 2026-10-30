//! Two primitives/three vertices protect exact material eligibility for BLEND
//! batching. Unlike ordering coverage this catches alpha/texture/mask conflicts
//! and shared vertices with incompatible emission. No GPU, assets or bake; <1 ms.
use super::vertex_emission;
use crate::engine::gltf::{PropModel, PropSubmesh, PropVertex};
use crate::engine::materials::{AlphaMode, MaterialAlpha, MaterialEmission, MaterialResponse};

#[test]
fn blend_batch_preserves_each_emission_and_refuses_incompatible_bindings() -> Result<(), String> {
    assert!(
        super::shader_source().is_some(),
        "the current production shader must enable compatible BLEND batching"
    );
    let primitive = PropSubmesh {
        double_sided: false,
        response: MaterialResponse::default(),
        material: 0,
        texture: Some(0),
        emission: MaterialEmission::NONE,
        alpha: MaterialAlpha {
            mode: AlphaMode::Blend,
            ..MaterialAlpha::default()
        },
        first_index: 0,
        index_count: 3,
    };
    let mut face = primitive;
    face.first_index = 3;
    face.emission = MaterialEmission::new([0.03, 0.1, 0.12], 1.0);
    let mut model = PropModel {
        vertices: vec![
            PropVertex {
                pos: [0.0; 3],
                normal: None,
                uv: [0.0; 2],
                color: [1.0; 4]
            };
            6
        ],
        indices: vec![0, 1, 2, 3, 4, 5],
        submeshes: vec![primitive, face],
        ..PropModel::default()
    };
    let emission = vertex_emission(&model).ok_or("compatible emission-only difference")?;
    assert_eq!(
        emission.first(),
        Some(&[0.0; 3]),
        "nonemitter remains nonemitting"
    );
    assert_eq!(
        emission.last(),
        Some(&[0.03, 0.1, 0.12]),
        "face keeps its independent emission"
    );
    model.submeshes.get_mut(1).ok_or("face")?.texture = Some(1);
    assert!(
        vertex_emission(&model).is_none(),
        "different sheets must not batch"
    );
    let alpha_face = model.submeshes.get_mut(1).ok_or("face")?;
    alpha_face.texture = Some(0);
    alpha_face.alpha.opacity = 0.5;
    assert!(
        vertex_emission(&model).is_none(),
        "different material coverage must not batch"
    );
    let masked_face = model.submeshes.get_mut(1).ok_or("face")?;
    masked_face.alpha = primitive.alpha;
    masked_face.emission.mask = Some(0);
    assert!(
        vertex_emission(&model).is_none(),
        "masked emission keeps its ordinary bindings"
    );
    model.submeshes.get_mut(1).ok_or("face")?.emission.mask = None;
    *model.indices.get_mut(3).ok_or("face index")? = 0;
    assert!(
        vertex_emission(&model).is_none(),
        "one shared vertex cannot acquire two emissions"
    );
    Ok(())
}
