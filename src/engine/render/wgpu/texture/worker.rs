//! The staged install's existing bounded mip batch, polled instead of joined
//! on the UI thread. One batch process-wide may prepare at a time, including
//! cancelled owners; at most eight fitted world chains (about 43 MiB).
use super::{
    GpuTexture, PreparedTexture, RawImage, TextureCache, TextureKey, TextureOrigin, TextureUpload,
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
                let mut prepared = Vec::with_capacity(sources.len());
                for source in sources {
                    if flag.load(Ordering::Acquire) {
                        return;
                    }
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
                    });
                }
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
    pub fn upload_is_cached(&self, key: &TextureKey, origin: TextureOrigin) -> bool {
        self.get(key, origin).is_some()
    }
    pub fn install_prepared_image(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image: PreparedImage,
    ) {
        self.retire_prior_revision(&image.key, image.origin);
        if self.get(&image.key, image.origin).is_some() {
            return;
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
    }
}

#[cfg(feature = "visual-diagnostics")]
pub fn is_preparing() -> bool {
    PREPARING.load(Ordering::Acquire)
}
