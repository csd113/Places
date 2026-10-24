//! Protects editor/runtime content replacement against stale memoized hashes
//! and proves memoization does not retain decoded pixels. One RGBA pixel,
//! no device/assets/map bake, negligible runtime; existing GPU-key tests do not
//! exercise the new weak-allocation identity or Arc copy-on-write transition.
use super::{ModelImageKeys, RawImage};
use std::sync::Arc;

#[test]
fn weak_model_keys_follow_copy_on_write_content_and_release_pixels() -> Result<(), String> {
    let mut keys = ModelImageKeys::default();
    let mut image = Arc::new(RawImage {
        width: 1,
        height: 1,
        rgba: vec![17, 31, 63, 255],
    });
    let original = Arc::downgrade(&image);
    let first = keys.key(&image);
    assert_eq!(first, super::super::model_image_key(&image));
    assert_eq!(keys.get(&image), Some(first.clone()));
    *Arc::make_mut(&mut image)
        .rgba
        .first_mut()
        .ok_or("fixture pixel missing")? = 29;
    assert!(
        original.upgrade().is_none(),
        "memoization must not own the original pixels"
    );
    assert!(
        keys.get(&image).is_none(),
        "replaced content must not inherit an old key"
    );
    let replacement = keys.key(&image);
    assert_ne!(replacement, first);
    assert_eq!(replacement, super::super::model_image_key(&image));
    let latest = Arc::downgrade(&image);
    drop(image);
    assert!(
        latest.upgrade().is_none(),
        "memoization must not retain replacement pixels"
    );
    keys.retain_live();
    assert!(keys.0.is_empty());
    Ok(())
}
