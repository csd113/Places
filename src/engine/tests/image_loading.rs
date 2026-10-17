//! One numeric PNG fixture protects warm reuse and changed/error paths without
//! any shipped artwork, whole-map preparation or GPU resource allocation.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use super::{MAX_ENCODED_PNG_BYTES, RawImage, TextureCache, decode_relative_image, encode_png};

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Self, String> {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "places-image-loading-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir(&path).map_err(|error| error.to_string())?;
        Ok(Self(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _cleanup_result = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn prefetch_reuses_current_pixels_and_rejects_changed_missing_or_oversize_input()
-> Result<(), String> {
    let scratch = Scratch::new()?;
    let path = scratch.0.join("sample.png");
    let original = encode_png(&RawImage::new(1, 1, vec![1, 2, 3, 255]))?;
    let changed = encode_png(&RawImage::new(1, 1, vec![4, 5, 6, 255]))?;
    let references = [("fixture".to_string(), "sample.png".to_string())];
    for workers in [1, 2] {
        fs::write(&path, &original).map_err(|error| error.to_string())?;
        let mut cache = TextureCache::new();
        let jobs: Vec<_> = references.iter().collect();
        cache.decode_references(&scratch.0, &jobs, workers);
        let (first, _) = cache.load_relative(&scratch.0, "sample.png", "fixture")?;
        assert_eq!(cache.decoded_count(), 1, "cold prefetch decodes once");
        assert!(
            decode_relative_image(&scratch.0, "fixture", "sample.png", &cache.images)?.is_none(),
            "a warm key must skip PNG decoding rather than discard a duplicate decode"
        );
        cache.decode_references(&scratch.0, &jobs, workers);
        let (warm, _) = cache.load_relative(&scratch.0, "sample.png", "fixture")?;
        assert_eq!(
            cache.decoded_count(),
            1,
            "warm prefetch cannot decode again"
        );
        assert!(
            Arc::ptr_eq(&first, &warm),
            "warm resolution shares the existing allocation"
        );

        fs::write(&path, &changed).map_err(|error| error.to_string())?;
        cache.prefetch_catalog(&scratch.0, &references);
        let (fresh, _) = cache.load_relative(&scratch.0, "sample.png", "fixture")?;
        assert_eq!(
            cache.decoded_count(),
            2,
            "changed bytes decode a new revision"
        );
        assert_eq!(
            fresh.rgba,
            [4, 5, 6, 255],
            "current pixels replace the old revision"
        );
        assert_eq!(
            first.rgba,
            [1, 2, 3, 255],
            "a live old world keeps its original pixels"
        );
        fs::write(&path, b"broken PNG").map_err(|error| error.to_string())?;
        assert!(
            cache
                .load_relative(&scratch.0, "sample.png", "fixture")
                .is_err(),
            "a corrupt current file cannot resolve to cached stale pixels"
        );
        fs::remove_file(&path).map_err(|error| error.to_string())?;
        assert!(
            cache
                .load_relative(&scratch.0, "sample.png", "fixture")
                .is_err(),
            "a missing file cannot resolve to cached stale pixels"
        );

        let oversized = fs::File::create(&path).map_err(|error| error.to_string())?;
        oversized
            .set_len(
                u64::try_from(MAX_ENCODED_PNG_BYTES)
                    .map_err(|error| error.to_string())?
                    .saturating_add(1),
            )
            .map_err(|error| error.to_string())?;
        let error = cache
            .load_relative(&scratch.0, "sample.png", "fixture")
            .err()
            .ok_or("oversize PNG unexpectedly accepted")?;
        assert!(
            error.contains("size limit"),
            "encoded size is rejected before PNG parsing: {error}"
        );
    }
    Ok(())
}
