//! The staged install's existing bounded mip batch, polled instead of joined
//! on the UI thread. One batch process-wide may prepare at a time, including
//! cancelled owners; at most eight fitted world chains (about 43 MiB).
use super::{
    CacheOutcome, GpuTexture, PreparedTexture, RawImage, TextureCache, TextureKey, TextureOrigin,
    TextureUpload,
};
use std::sync::Arc;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc,
};

static PREPARING: AtomicBool = AtomicBool::new(false);
struct Slot;
impl Drop for Slot {
    fn drop(&mut self) {
        PREPARING.store(false, Ordering::Release);
    }
}

#[derive(Clone)]
pub struct TextureSource {
    pub key: TextureKey,
    pub image: Arc<RawImage>,
    pub origin: TextureOrigin,
}
pub struct PreparedImage {
    key: TextureKey,
    origin: TextureOrigin,
    pixels: PreparedTexture,
    diagnostic: bool,
    model_source: Option<std::sync::Weak<RawImage>>,
}
impl PreparedImage {
    pub const fn key(&self) -> &TextureKey {
        &self.key
    }
    pub fn model_source_address(&self) -> Option<usize> {
        self.model_source
            .as_ref()
            .map(|source| source.as_ptr().addr())
    }
    pub const fn is_diagnostic(&self) -> bool {
        self.diagnostic
    }
}
pub struct TextureJob {
    response: mpsc::Receiver<Vec<PreparedImage>>,
    cancelled: Arc<AtomicBool>,
}

impl Drop for TextureJob {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}
impl TextureJob {
    pub fn start(sources: Vec<TextureSource>) -> Result<Option<Self>, String> {
        Self::start_with(move |flag| {
            let mut prepared = Vec::with_capacity(sources.len());
            for mut source in sources {
                if flag.load(Ordering::Acquire) {
                    return prepared;
                }
                let model_source = if source.key.logical.is_empty() {
                    source.key.logical = super::model_image_key(&source.image);
                    Some(Arc::downgrade(&source.image))
                } else if source.key.logical.starts_with("model-image-v1:") {
                    Some(Arc::downgrade(&source.image))
                } else {
                    None
                };
                let pixels = PreparedTexture::prepare(
                    &source.image,
                    source.key.level,
                    source.key.class,
                    source.key.semantic,
                );
                prepared.push(PreparedImage {
                    key: source.key,
                    origin: source.origin,
                    pixels,
                    diagnostic: false,
                    model_source,
                });
            }
            prepared
        })
    }

    /// Decodes and prepares one PNG under the same process-wide scratch bound.
    pub fn start_decal(
        catalog: Arc<crate::engine::loader::PropCatalog>,
        root: Option<std::path::PathBuf>,
        id: Option<String>,
        level: crate::engine::quality::QualityLevel,
    ) -> Result<Option<Self>, String> {
        Self::start_with(move |flag| {
            if flag.load(Ordering::Acquire) {
                return Vec::new();
            }
            let (image, diagnostic) = id.as_ref().map_or_else(
                || {
                    (
                        Arc::new(crate::engine::materials::BuiltinImage::DecalAtlas.decode()),
                        false,
                    )
                },
                |name| match crate::engine::materials::resolve_decal_sheet(
                    catalog.assets(),
                    root.as_deref(),
                    &mut crate::engine::materials::TextureCache::new(),
                    name,
                ) {
                    Ok(sheet) => (sheet.image, false),
                    Err(error) => {
                        crate::engine::logging::warn_once(
                            format!("decal-sheet:{name}"),
                            format!("[decals] {error}; drawing the diagnostic sheet instead"),
                        );
                        (Arc::new(crate::engine::materials::missing_texture()), true)
                    }
                },
            );
            if flag.load(Ordering::Acquire) {
                return Vec::new();
            }
            let key = TextureKey {
                logical: super::model_image_key(&image),
                semantic: super::TextureSemantic::BaseColorDisplay,
                class: crate::engine::quality::TextureClass::DecalSheet,
                level,
                wrap: super::TextureWrap::Repeat,
            };
            let pixels = PreparedTexture::prepare(&image, level, key.class, key.semantic);
            vec![PreparedImage {
                key,
                origin: TextureOrigin::Catalog,
                pixels,
                diagnostic,
                model_source: None,
            }]
        })
    }

    fn start_with(
        prepare: impl FnOnce(&AtomicBool) -> Vec<PreparedImage> + Send + 'static,
    ) -> Result<Option<Self>, String> {
        if PREPARING
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Ok(None);
        }
        let slot = Slot;
        let (send, response) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancelled);
        let _thread = std::thread::Builder::new()
            .name("places-upload-mips".into())
            .spawn(move || {
                let _slot = slot;
                let prepared = prepare(&flag);
                if !flag.load(Ordering::Acquire) {
                    let _delivered = send.send(prepared);
                }
            })
            .map_err(|error| format!("Cannot prepare upload mip chains: {error}"))?;
        Ok(Some(Self {
            response,
            cancelled,
        }))
    }
    pub fn poll(&self) -> Result<Option<Vec<PreparedImage>>, String> {
        match self.response.try_recv() {
            Ok(images) => Ok(Some(images)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("Upload mip preparation stopped before completion".into())
            }
        }
    }
}

impl TextureCache {
    pub fn prepared_texture(&self, key: &TextureKey) -> Option<Arc<GpuTexture>> {
        self.get(key, TextureOrigin::Catalog).cloned()
    }
    pub fn upload_is_cached(&self, key: &TextureKey, origin: TextureOrigin) -> bool {
        self.get(key, origin).is_some()
    }
    pub fn install_prepared_image(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image: PreparedImage,
    ) -> CacheOutcome {
        if let Some(source) = image
            .model_source
            .as_ref()
            .filter(|source| source.strong_count() != 0)
        {
            self.model_keys
                .remember(source.clone(), image.key.logical.clone());
        }
        self.retire_prior_revision(&image.key, image.origin);
        if self.get(&image.key, image.origin).is_some() {
            return CacheOutcome::Reused;
        }
        let texture = Arc::new(GpuTexture::upload_prepared(
            device,
            queue,
            &self.layout,
            &self.samplers,
            &TextureUpload {
                key: &image.key,
                prepared: &image.pixels,
                origin: image.origin,
                fallback: false,
            },
        ));
        drop(self.map_mut(image.origin).insert(image.key, texture));
        CacheOutcome::Uploaded
    }
}

#[cfg(feature = "visual-diagnostics")]
pub fn is_preparing() -> bool {
    PREPARING.load(Ordering::Acquire)
}
