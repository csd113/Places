//! Metadata planning is cheap; PNG decode and mip generation use the existing
//! bounded texture worker. A cancelled install cannot publish a partial sky.
use super::{CelestialUniform, WgpuSky};
use crate::engine::assets::AssetCatalog;
use crate::engine::level::{GlobalIlluminatorKind, LevelDef};
use crate::engine::materials::{RawImage, TextureOrigin};
use crate::engine::quality::{QualityLevel, TextureClass};
use crate::engine::render::wgpu::texture::{
    TextureCache, TextureKey, TextureSemantic, TextureWrap,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Copy)]
enum ImageKind {
    Panorama,
    Disc,
    Ordinary,
}

#[derive(Clone)]
pub(in crate::engine::render::wgpu) struct SkySource {
    pub key: TextureKey,
    root: PathBuf,
    path: String,
    kind: ImageKind,
}

impl SkySource {
    fn resolve(
        catalog: &AssetCatalog,
        root: &Path,
        id: &str,
        quality: QualityLevel,
        kind: ImageKind,
    ) -> Option<Self> {
        let Some(path) = catalog.texture_path(id) else {
            crate::engine::logging::warn_once(
                format!("sky:{id}"),
                format!("[sky] `{id}` is not a catalogued file texture; drawing no sky"),
            );
            return None;
        };
        let disc = matches!(kind, ImageKind::Disc);
        Some(Self {
            key: TextureKey {
                logical: format!("{}:{id}", if disc { "celestial" } else { "sky" }),
                semantic: TextureSemantic::BaseColorDisplay,
                class: if disc {
                    TextureClass::DecalSheet
                } else {
                    TextureClass::Sky
                },
                level: quality,
                wrap: if disc {
                    TextureWrap::Clamp
                } else {
                    TextureWrap::RepeatClampV
                },
            },
            root: root.to_path_buf(),
            path: path.to_owned(),
            kind,
        })
    }

    pub fn decode(&self) -> Option<RawImage> {
        let decoded = match self.kind {
            ImageKind::Panorama => {
                crate::engine::materials::load_sky_png_relative(&self.root, &self.path)
            }
            ImageKind::Disc => {
                crate::engine::materials::load_celestial_png_relative(&self.root, &self.path)
            }
            ImageKind::Ordinary => {
                crate::engine::materials::load_png_relative(&self.root, &self.path)
            }
        };
        match decoded {
            Ok(image) => Some(image),
            Err(error) => {
                crate::engine::logging::warn_once(
                    self.key.logical.clone(),
                    format!("[sky] {error}; drawing no sky"),
                );
                None
            }
        }
    }
}

pub(in crate::engine::render::wgpu) struct SkyPlan {
    pub sources: Vec<SkySource>,
    sun: Option<usize>,
    moon: Option<usize>,
    celestial: [CelestialUniform; 8],
    brightness: f32,
    panorama: bool,
}

impl SkyPlan {
    pub fn new(
        catalog: &AssetCatalog,
        asset_root: Option<&Path>,
        level: &LevelDef,
        quality: QualityLevel,
    ) -> Option<Self> {
        if level.sky.is_none()
            && !level
                .global_illuminators
                .iter()
                .any(|light| light.visual.is_some())
        {
            return None;
        }
        let id = level
            .sky
            .as_ref()
            .map_or("core:tex_white_fallback_01", |sky| sky.texture.trim());
        let Some(root) = asset_root else {
            crate::engine::logging::warn_once(
                format!("sky:{id}"),
                format!("[sky] the asset root is missing; drawing no sky for `{id}`"),
            );
            return None;
        };
        let kind = if level.sky.is_some() {
            ImageKind::Panorama
        } else {
            ImageKind::Ordinary
        };
        let mut sources = vec![SkySource::resolve(catalog, root, id, quality, kind)?];
        let mut celestial = [CelestialUniform {
            source: [0.0; 4],
            right: [0.0; 4],
            up: [0.0; 4],
        }; 8];
        let mut sun = None;
        let mut moon = None;
        for (slot, light) in celestial.iter_mut().zip(&level.global_illuminators) {
            let Some(visual) = &light.visual else {
                continue;
            };
            if !light.enabled || !light.bake || light.intensity == 0.0 || visual.brightness == 0.0 {
                continue;
            }
            let is_moon = light.kind == GlobalIlluminatorKind::Moon;
            let image_slot = if is_moon { &mut moon } else { &mut sun };
            if image_slot.is_none() {
                let source = SkySource::resolve(
                    catalog,
                    root,
                    light.visual_texture()?,
                    quality,
                    ImageKind::Disc,
                )?;
                *image_slot = Some(sources.len());
                sources.push(source);
            }
            *slot = CelestialUniform::new(light, visual, is_moon);
        }
        Some(Self {
            sources,
            sun,
            moon,
            celestial,
            brightness: level.sky.as_ref().map_or(1.0, |sky| sky.brightness),
            panorama: level.sky.is_some(),
        })
    }

    pub fn finish(self, cache: &TextureCache) -> Option<WgpuSky> {
        let texture = cache.prepared_texture(&self.sources.first()?.key)?;
        let disc = |slot: Option<usize>| {
            slot.map_or_else(
                || Some(Arc::clone(&texture)),
                |index| cache.prepared_texture(&self.sources.get(index)?.key),
            )
        };
        let sun = disc(self.sun)?;
        let moon = disc(self.moon)?;
        Some(WgpuSky {
            texture,
            sun,
            moon,
            celestial_count: CelestialUniform::active_count(&self.celestial),
            celestial: self.celestial,
            brightness: self.brightness,
            panorama: self.panorama,
        })
    }

    pub fn upload(
        self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cache: &mut TextureCache,
    ) -> Option<WgpuSky> {
        for source in &self.sources {
            if cache.upload_is_cached(&source.key, TextureOrigin::Catalog) {
                continue;
            }
            let image = source.decode()?;
            let _uploaded = cache.get_or_upload_with_key(
                device,
                queue,
                source.key.clone(),
                &image,
                TextureOrigin::Catalog,
            );
        }
        self.finish(cache)
    }
}
