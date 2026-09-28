//! Compiled lightmap payload: chart metadata plus the prepared KTX2 pages.
//!
//! The record is two archive entries:
//!
//! * `blobs/<sha>.lightmaps.ktx2` — the atlas pages as an RGBA16F KTX2 2D array
//!   (one mip level). Every page contributes two layers: an irradiance plane
//!   and a directional-moment plane. A switchable fixture's prepared
//!   contribution contributes two more layers per page, in its own group; the
//!   groups are the base solve first, then each recorded switchable light in
//!   order.
//! * `blobs/<sha>.lightmaps.json` — the small chart/stat record below, using
//!   the authored-input serde forms of [`LightmapPatch`] and [`Chart`] so the
//!   mapping from a world-space patch to atlas texels is explicit and
//!   inspectable.
//!
//! The player decodes the container without rebuilding a plan: the chart list
//! is the compiler's exact allocation, and the page texels are the compiler's
//! exact solve. Only one bounded conversion happens at load — half floats to
//! `f32` texels — and it is reported in the load log.

use serde::{Deserialize, Serialize};

use crate::lighting::lightmap::{
    Chart, LevelLightmaps, LightmapPage, LightmapPatch, LightmapStats, LightmapTexel,
    SwitchableLightmaps,
};
use crate::package::ktx2::{self, Ktx2Rgba16f};

use super::{MAX_LIGHTMAP_PAGE_EDGE, MAX_LIGHTMAP_PAGES};

/// Version of the lightmap metadata record.
///
/// * `2` — the HDR transport atlas with the octahedral dominant-axis encoding.
/// * `3` — the linear moment representation: `direction` is the vector sum of
///   the per-channel first moments and the planes' alpha channels are reserved.
///   Version-2 data is rejected.
pub const LIGHTMAPS_RECORD_VERSION: u16 = 3;

/// The lightmap metadata that travels beside the KTX2 pages.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LightmapsMeta {
    /// Record version.
    pub record_version: u16,
    /// Page edge in texels.
    pub page_edge: u32,
    /// Number of pages in each layer group.
    pub page_count: u32,
    /// Gutter padding used by the bake, in texels.
    pub padding: u32,
    /// Deterministic content key of the atlas, for diagnostics and cache
    /// identity.
    pub content_key: String,
    /// Bake statistics, for the developer log.
    pub stats: LightmapStats,
    /// Chart allocations, exactly as the compiler planned them.
    pub charts: Vec<ChartRecord>,
    /// Switchable light indices, one per contribution layer group that follows
    /// the base group, in group order.
    pub switchable_lights: Vec<usize>,
}

/// One chart: the world-space patch and its atlas rectangle.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChartRecord {
    /// The world-space rectangle this chart covers.
    pub patch: LightmapPatch,
    /// The atlas rectangle it occupies.
    pub chart: Chart,
}

impl LightmapsMeta {
    /// Validates the record against the decoded KTX2 payload.
    ///
    /// # Errors
    ///
    /// Returns an error for a wrong record version, a page edge or count the
    /// KTX2 payload does not match, an unsupported page size, a degenerate or
    /// non-finite patch, a chart outside its page, or an inconsistent layer
    /// group count.
    pub fn validate(&self, image: &Ktx2Rgba16f) -> Result<(), String> {
        if self.record_version != LIGHTMAPS_RECORD_VERSION {
            return Err(format!(
                "lightmap record version {} is not supported (this build reads {LIGHTMAPS_RECORD_VERSION})",
                self.record_version
            ));
        }
        if self.page_edge == 0 || self.page_edge > MAX_LIGHTMAP_PAGE_EDGE {
            return Err(format!(
                "lightmap page edge {} is out of range",
                self.page_edge
            ));
        }
        if self.page_count == 0
            || usize::try_from(self.page_count).unwrap_or(usize::MAX) > MAX_LIGHTMAP_PAGES
        {
            return Err(format!(
                "lightmap page count {} is out of range",
                self.page_count
            ));
        }
        if self.switchable_lights.len() > super::MAX_SWITCHABLE_LIGHTS {
            return Err(format!(
                "lightmap record carries {} switchable light groups (limit {})",
                self.switchable_lights.len(),
                super::MAX_SWITCHABLE_LIGHTS
            ));
        }
        let groups = usize::try_from(self.page_count)
            .map_err(|_| "lightmap page count is too large".to_string())?
            .checked_mul(self.switchable_lights.len().saturating_add(1))
            .and_then(|pages| pages.checked_mul(2))
            .ok_or_else(|| "lightmap layer count overflows".to_string())?;
        let Ok(layers) = usize::try_from(image.layers) else {
            return Err("lightmap payload layer count is too large".to_string());
        };
        if image.faces != 1 || layers != groups {
            return Err(format!(
                "lightmap payload holds {} layer(s) of {} face(s), record needs {groups}",
                image.layers, image.faces
            ));
        }
        if image.edge != self.page_edge {
            return Err(format!(
                "lightmap payload edge {} does not match the record's {}",
                image.edge, self.page_edge
            ));
        }
        if image.levels.len() != 1 {
            return Err(format!(
                "lightmap payload has {} mip levels; this record reads one",
                image.levels.len()
            ));
        }
        let page_count = usize::try_from(self.page_count)
            .map_err(|_| "lightmap page count is too large".to_string())?;
        let mut covered = 0_u64;
        for record in &self.charts {
            let patch = record.patch;
            if !patch.origin.iter().all(|value| value.is_finite())
                || !patch.u_axis.iter().all(|value| value.is_finite())
                || !patch.v_axis.iter().all(|value| value.is_finite())
            {
                return Err("lightmap chart patch has a non-finite component".to_string());
            }
            let chart = record.chart;
            if usize::from(chart.page) >= page_count {
                return Err(format!("lightmap chart references page {}", chart.page));
            }
            let x_end = chart.x.checked_add(chart.width);
            let y_end = chart.y.checked_add(chart.height);
            let (Some(x_end), Some(y_end)) = (x_end, y_end) else {
                return Err("lightmap chart rectangle overflows".to_string());
            };
            if chart.width == 0
                || chart.height == 0
                || x_end > self.page_edge
                || y_end > self.page_edge
            {
                return Err("lightmap chart rectangle is outside its page".to_string());
            }
            covered = covered
                .saturating_add(u64::from(chart.width))
                .saturating_add(u64::from(chart.height));
        }
        let page_texels = u64::from(self.page_edge)
            .saturating_mul(u64::from(self.page_edge))
            .saturating_mul(u64::from(self.page_count));
        if covered > page_texels.saturating_mul(2) {
            return Err("lightmap chart rectangles cover more than the pages hold".to_string());
        }
        Ok(())
    }

    /// A `LevelLightmaps` from validated metadata and decoded pages.
    ///
    /// # Errors
    ///
    /// Returns an error when a layer's byte length does not match the declared
    /// edge or the layer groups cannot be assembled.
    pub fn into_lightmaps(self, image: &Ktx2Rgba16f) -> Result<LevelLightmaps, String> {
        let page_edge = usize::try_from(self.page_edge)
            .map_err(|_| "lightmap page edge is too large".to_string())?;
        let page_texels = page_edge
            .checked_mul(page_edge)
            .ok_or_else(|| "lightmap page size overflows".to_string())?;
        let layer_bytes = page_texels
            .checked_mul(8)
            .ok_or_else(|| "lightmap layer size overflows".to_string())?;
        let Some(level) = image.levels.first() else {
            return Err("lightmap payload has no image data".to_string());
        };
        let page_count = usize::try_from(self.page_count)
            .map_err(|_| "lightmap page count is too large".to_string())?;
        let layer_groups = page_count
            .checked_mul(2)
            .ok_or_else(|| "lightmap layer count overflows".to_string())?;
        let expected = layer_groups
            .checked_mul(self.switchable_lights.len().saturating_add(1))
            .and_then(|count| count.checked_mul(layer_bytes))
            .ok_or_else(|| "lightmap level size overflows".to_string())?;
        if level.len() != expected {
            return Err(format!(
                "lightmap level holds {} bytes, expected {expected}",
                level.len()
            ));
        }
        let mut groups: Vec<Vec<LightmapPage>> = Vec::new();
        for group_index in 0..self.switchable_lights.len().saturating_add(1) {
            let start = group_index.saturating_mul(layer_groups);
            let mut pages = Vec::with_capacity(page_count);
            for page_index in 0..page_count {
                let irradiance = start.saturating_add(page_index.saturating_mul(2));
                let direction = irradiance.saturating_add(1);
                let irradiance_bytes = layer_at(level, irradiance, layer_bytes)?;
                let direction_bytes = layer_at(level, direction, layer_bytes)?;
                pages.push(LightmapPage {
                    width: self.page_edge,
                    height: self.page_edge,
                    texels: decode_page(irradiance_bytes, direction_bytes, page_texels)?,
                });
            }
            groups.push(pages);
        }
        let mut groups = groups.into_iter();
        let Some(base) = groups.next() else {
            return Err("lightmap payload has no base layer group".to_string());
        };
        let switchable = self
            .switchable_lights
            .iter()
            .copied()
            .zip(groups)
            .map(|(light_index, pages)| SwitchableLightmaps { light_index, pages })
            .collect();
        let charts = self
            .charts
            .into_iter()
            .map(|record| (record.patch, record.chart))
            .collect();
        Ok(LevelLightmaps {
            pages: base,
            charts,
            stats: self.stats,
            cache_key: self.content_key,
            padding: self.padding,
            switchable,
        })
    }
}

/// One layer of a decoded level, by index.
fn layer_at(level: &[u8], index: usize, layer_bytes: usize) -> Result<&[u8], String> {
    let start = index
        .checked_mul(layer_bytes)
        .ok_or_else(|| "lightmap layer offset overflows".to_string())?;
    let end = start
        .checked_add(layer_bytes)
        .ok_or_else(|| "lightmap layer range overflows".to_string())?;
    level
        .get(start..end)
        .ok_or_else(|| format!("lightmap layer {index} runs past the payload"))
}

/// Decodes one page's irradiance and directional planes.
fn decode_page(
    irradiance: &[u8],
    direction: &[u8],
    page_texels: usize,
) -> Result<Vec<LightmapTexel>, String> {
    if irradiance.len() != page_texels.saturating_mul(8)
        || direction.len() != page_texels.saturating_mul(8)
    {
        return Err("lightmap page plane size does not match its edge".to_string());
    }
    let mut texels = Vec::with_capacity(page_texels);
    for (irr, dir) in irradiance
        .as_chunks::<8>()
        .0
        .iter()
        .zip(direction.as_chunks::<8>().0.iter())
    {
        let irradiance = [f16_at(irr, 0), f16_at(irr, 2), f16_at(irr, 4)];
        let direction = [f16_at(dir, 0), f16_at(dir, 2), f16_at(dir, 4)];
        // The axis pair rides in the planes' alpha channels and is reserved:
        // writers store `[0.5, 0.5]` and the reconstruction ignores it, but the
        // record keeps its three-field shape.
        let axis = [f16_at(irr, 6), f16_at(dir, 6)];
        texels.push(
            LightmapTexel {
                irradiance,
                direction,
                axis,
            }
            .normalized(),
        );
    }
    if texels.len() != page_texels {
        return Err("lightmap page texel count does not match its edge".to_string());
    }
    Ok(texels)
}

/// One half-float channel of a texture row, by byte offset.
fn f16_at(bytes: &[u8], offset: usize) -> f32 {
    let Some(slice) = bytes.get(offset..offset.saturating_add(2)) else {
        return 0.0;
    };
    <[u8; 2]>::try_from(slice).map_or(0.0, |pair| ktx2::f16_bits_to_f32(u16::from_le_bytes(pair)))
}

/// Encodes a prepared atlas as `(meta, ktx2)` archive entries.
///
/// # Errors
///
/// Returns an error when the atlas has no pages, mixed or unsupported page
/// sizes, a page whose texel count does not match its edge, an inconsistent
/// switchable layer set, or a chart outside its page.
pub fn write_lightmaps(lightmaps: &LevelLightmaps) -> Result<(Vec<u8>, Vec<u8>), String> {
    let Some(first) = lightmaps.pages.first() else {
        return Err("lightmap atlas has no pages".to_string());
    };
    let edge = first.width;
    if edge == 0 || edge > MAX_LIGHTMAP_PAGE_EDGE || first.height != edge {
        return Err(format!("lightmap page edge {edge} is out of range"));
    }
    if lightmaps.pages.len() > MAX_LIGHTMAP_PAGES {
        return Err(format!(
            "lightmap atlas has {} pages (limit {MAX_LIGHTMAP_PAGES})",
            lightmaps.pages.len()
        ));
    }
    if lightmaps.switchable.len() > super::MAX_SWITCHABLE_LIGHTS {
        return Err(format!(
            "lightmap atlas has {} switchable groups (limit {})",
            lightmaps.switchable.len(),
            super::MAX_SWITCHABLE_LIGHTS
        ));
    }
    let mut layers: Vec<Vec<u8>> = Vec::with_capacity(lightmaps.layer_count());
    for page in &lightmaps.pages {
        if page.width != edge || page.height != edge || !page.is_consistent() {
            return Err("lightmap page does not match its declared edge".to_string());
        }
        encode_page(page, &mut layers)?;
    }
    for contribution in &lightmaps.switchable {
        if contribution.pages.len() != lightmaps.pages.len() {
            return Err("switchable lightmap group has a different page count".to_string());
        }
        for page in &contribution.pages {
            if page.width != edge || page.height != edge || !page.is_consistent() {
                return Err("switchable lightmap page does not match its edge".to_string());
            }
            encode_page(page, &mut layers)?;
        }
    }
    let ktx2 = ktx2::write_rgba16f_2d_array(edge, &layers)?;
    // The packaged record is deterministic: the bake's wall-clock time is a
    // developer measurement, reported in the build output, and never part of
    // the archive bytes.
    let mut stats = lightmaps.stats;
    stats.bake_millis = 0.0;
    let meta = LightmapsMeta {
        record_version: LIGHTMAPS_RECORD_VERSION,
        page_edge: edge,
        page_count: u32::try_from(lightmaps.pages.len())
            .map_err(|_| "lightmap page count is too large".to_string())?,
        padding: lightmaps.padding,
        content_key: lightmaps.cache_key.clone(),
        stats,
        charts: lightmaps
            .charts
            .iter()
            .map(|(patch, chart)| ChartRecord {
                patch: *patch,
                chart: *chart,
            })
            .collect(),
        switchable_lights: lightmaps
            .switchable
            .iter()
            .map(|contribution| contribution.light_index)
            .collect(),
    };
    let mut meta_bytes = serde_json::to_vec(&meta)
        .map_err(|error| format!("could not serialize lightmap record: {error}"))?;
    meta_bytes.push(b'\n');
    Ok((meta_bytes, ktx2))
}

/// Appends one page's irradiance and directional planes to a layer list.
fn encode_page(page: &LightmapPage, layers: &mut Vec<Vec<u8>>) -> Result<(), String> {
    let texels = usize::try_from(page.width)
        .ok()
        .and_then(|edge| edge.checked_mul(edge))
        .ok_or_else(|| "lightmap page size overflows".to_string())?;
    let mut irradiance = Vec::with_capacity(texels.saturating_mul(8));
    let mut direction = Vec::with_capacity(texels.saturating_mul(8));
    for texel in &page.texels {
        // The two planes' alpha channels carry the reserved axis pair; writers
        // store `[0.5, 0.5]` and the reconstruction ignores it.
        push_f16_rgba(&mut irradiance, texel.irradiance, texel.axis[0]);
        push_f16_rgba(&mut direction, texel.direction, texel.axis[1]);
    }
    if irradiance.len() != texels.saturating_mul(8) || direction.len() != texels.saturating_mul(8) {
        return Err("lightmap page texel count does not match its edge".to_string());
    }
    layers.push(irradiance);
    layers.push(direction);
    Ok(())
}

/// Appends one RGB value and an alpha channel as four half floats.
fn push_f16_rgba(out: &mut Vec<u8>, color: [f32; 3], alpha: f32) {
    for value in [color[0], color[1], color[2], alpha] {
        out.extend_from_slice(&ktx2::f32_to_f16_bits(value).to_le_bytes());
    }
}

/// Decodes and validates a scripted lightmap payload.
///
/// # Errors
///
/// Returns an error for malformed metadata JSON, a version or size outside the
/// contract, a KTX2 payload outside the supported subset, or any mismatch
/// between the record and the pages.
pub fn read_lightmaps(meta_json: &[u8], ktx2_bytes: &[u8]) -> Result<LevelLightmaps, String> {
    let meta: LightmapsMeta = serde_json::from_slice(meta_json)
        .map_err(|error| format!("lightmap record is not valid JSON: {error}"))?;
    let image = ktx2::read_rgba16f(ktx2_bytes)?;
    meta.validate(&image)?;
    meta.into_lightmaps(&image)
}

#[cfg(test)]
mod tests {
    // Test code: unwraps and indexing are idiomatic here.
    #![allow(
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used
    )]

    use super::*;

    #[test]
    fn a_round_trip_preserves_hdr_texels_and_switchable_groups() {
        let page = LightmapPage {
            width: 2,
            height: 2,
            texels: vec![
                LightmapTexel {
                    irradiance: [1.5, 0.25, 0.0],
                    direction: [0.5, 0.0, 0.75],
                    axis: [0.5, 0.5],
                };
                4
            ],
        };
        let atlas = LevelLightmaps {
            pages: vec![page.clone()],
            charts: Vec::new(),
            stats: LightmapStats::default(),
            cache_key: "test".to_string(),
            padding: 1,
            switchable: vec![SwitchableLightmaps {
                light_index: 3,
                pages: vec![page],
            }],
        };
        let (meta, ktx2) = write_lightmaps(&atlas).expect("the test atlas writes");
        let decoded = read_lightmaps(&meta, &ktx2).expect("the test atlas reads");
        assert_eq!(decoded.pages.len(), 1);
        assert_eq!(decoded.switchable.len(), 1);
        assert_eq!(decoded.switchable[0].light_index, 3);
        assert_eq!(decoded.switchable[0].pages, decoded.pages);
        let texel = decoded.pages[0].texels[0];
        assert!((texel.irradiance[0] - 1.5).abs() < 1e-3);
        assert!((texel.irradiance[1] - 0.25).abs() < 1e-3);
        assert!((texel.direction[2] - 0.75).abs() < 1e-3);
        assert!((texel.direction[0] - 0.5).abs() < 1e-3);
    }

    /// The record version bumped with the linear moment representation, so a
    /// stale version-2 record must be rejected by name before any page is
    /// decoded as if its `direction` were a dominant-lobe amplitude.
    #[test]
    fn a_version_2_lightmap_record_is_rejected() {
        let atlas = crate::lighting::lightmap::LevelLightmaps {
            pages: vec![LightmapPage {
                width: 1,
                height: 1,
                texels: vec![LightmapTexel::ZERO],
            }],
            charts: Vec::new(),
            stats: crate::lighting::lightmap::LightmapStats::default(),
            cache_key: "version-test".to_string(),
            padding: 1,
            switchable: Vec::new(),
        };
        let (meta, ktx2) = write_lightmaps(&atlas).expect("the test atlas writes");
        let text = String::from_utf8(meta).expect("the record is JSON text");
        assert!(text.contains("\"record_version\":3"), "{text}");
        let stale = text.replace("\"record_version\":3", "\"record_version\":2");
        let error = read_lightmaps(stale.as_bytes(), &ktx2).expect_err("version 2 is stale");
        assert!(error.contains("version 2"), "{error}");
    }
}
