//! A tiny ownership regression: a cloned old preview cache must not pin unused
//! appearances in the next preview. Two one-pixel data fixtures, no files/GPU,
//! protect this distinct clone-feedback bug beyond package/image decoding tests.
use super::PropAssets;
use crate::engine::materials::RawImage;
use std::collections::BTreeSet;
use std::sync::Arc;

#[test]
fn old_preview_clone_cannot_pin_unused_appearances() {
    let mut assets = PropAssets::default();
    let old_image = Arc::new(RawImage::new(1, 1, vec![1, 2, 3, 255]));
    let new_image = Arc::new(RawImage::new(1, 1, vec![4, 5, 6, 255]));
    drop(
        assets
            .character_textures
            .insert("old".into(), Arc::clone(&old_image)),
    );
    drop(
        assets
            .character_textures
            .insert("current".into(), Arc::clone(&new_image)),
    );
    let displayed = assets.clone();
    assets.retain_preview_models(&BTreeSet::new(), &BTreeSet::from(["current"]));
    assert_eq!(assets.character_textures.len(), 1);
    assert!(!assets.character_textures.contains_key("old"));
    assert!(
        displayed
            .character_textures
            .get("old")
            .is_some_and(|image| Arc::ptr_eq(image, &old_image))
    );
    assert_eq!(
        old_image.rgba,
        [1, 2, 3, 255],
        "displayed preview remains valid until replacement"
    );
}
