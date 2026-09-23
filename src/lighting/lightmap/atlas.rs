//! The deterministic shelf packer, atlas pages and PNG debugging.
//!
//! Packing is deliberately simple and reproducible: charts are placed in the
//! order the mesh emitter produced them, on the current fully-open row of the
//! current page, and a new page is started only when the row and page are both
//! full. Nothing depends on hashing or on floating-point sort order, so the same
//! level always produces byte-identical pages and the same `Chart` rectangles.
//!
//! Every chart gets [`LightmapConfig::padding`] texels of gutter on all four
//! sides, outside its data rectangle. After a chart is filled, those gutter
//! texels are *dilated*: each copy the nearest texel of the chart's own border,
//! so the bilinear filter can reach half a texel past the data rectangle without
//! ever bleeding a neighbouring chart's texels into the sample.

use std::path::Path;

use super::{Chart, LightmapConfig, LightmapFailure, LightmapPatch};

/// One square atlas page of RGB8 texels, row-major from the top-left.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LightmapPage {
    pub width: u32,
    pub height: u32,
    /// `width * height * 3` bytes, red first.
    pub rgb: Vec<u8>,
}

/// An allocated page's shelf cursor.
#[derive(Clone, Copy, Debug, Default)]
struct ShelfState {
    /// Next free x on the current shelf.
    cursor_x: u32,
    /// Top of the current shelf.
    cursor_y: u32,
    /// Height of the tallest chart on the current shelf.
    shelf_height: u32,
}

/// Places chart rectangles on square pages with a deterministic shelf policy.
///
/// The allocator is designed to run *inline*, while the mesh is being emitted:
/// each chart is placed the moment its patch is built, using only the patches
/// that came before it. That is what lets the emitter write final lightmap UVs
/// into its vertices in one pass instead of patching the mesh afterwards.
#[derive(Clone, Debug)]
pub struct ShelfAllocator {
    config: LightmapConfig,
    pages: Vec<ShelfState>,
    failed: bool,
}

impl ShelfAllocator {
    /// A packer for one level build, with no pages opened yet.
    #[must_use]
    pub const fn new(config: LightmapConfig) -> Self {
        Self {
            config,
            pages: Vec::new(),
            failed: false,
        }
    }

    /// The configuration every placement is computed from.
    #[must_use]
    pub const fn config(&self) -> &LightmapConfig {
        &self.config
    }

    /// Number of pages opened so far.
    #[must_use]
    pub const fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// True when a chart could not be placed; the plan must not be baked.
    #[must_use]
    pub const fn failed(&self) -> bool {
        self.failed
    }

    /// Places one patch's chart, or returns `None` when the page budget is spent.
    ///
    /// A chart is sized by [`LightmapConfig::chart_texels`], which clamps to the
    /// page's usable edge, so every chart fits an empty page; `None` therefore
    /// always means "more pages needed than [`LightmapConfig::max_pages`]
    /// allows", never "this patch is too large". Once the allocator has failed
    /// it keeps returning `None` so the caller cannot silently continue with an
    /// incomplete atlas.
    pub fn allocate(&mut self, patch: &LightmapPatch) -> Option<Chart> {
        if self.failed {
            return None;
        }
        let (width, height) = self.config.chart_texels(patch);
        let padding = self.config.padding;
        let outer_w = width.saturating_add(padding.saturating_mul(2));
        let outer_h = height.saturating_add(padding.saturating_mul(2));
        if outer_w > self.config.page_edge || outer_h > self.config.page_edge {
            // Unreachable while `chart_texels` clamps to the usable edge; kept
            // as an explicit failure so a bad config can never place a chart
            // that spills outside its page.
            self.failed = true;
            return None;
        }
        loop {
            if self.pages.is_empty() {
                if self.config.max_pages == 0 {
                    self.failed = true;
                    return None;
                }
                self.pages.push(ShelfState::default());
            }
            let page_index = self.pages.len().saturating_sub(1);
            if let Some(chart) =
                self.try_place(page_index, outer_w, outer_h, width, height, padding)
            {
                return Some(chart);
            }
            if self.pages.len() >= self.config.max_pages {
                self.failed = true;
                return None;
            }
            self.pages.push(ShelfState::default());
        }
    }

    /// Tries to place one outer rectangle on `page_index`'s current shelf.
    fn try_place(
        &mut self,
        page_index: usize,
        outer_w: u32,
        outer_h: u32,
        width: u32,
        height: u32,
        padding: u32,
    ) -> Option<Chart> {
        let edge = self.config.page_edge;
        let page = self.pages.get_mut(page_index)?;
        if page.cursor_x.saturating_add(outer_w) > edge {
            // Close the shelf and try the next row.
            page.cursor_x = 0;
            page.cursor_y = page.cursor_y.saturating_add(page.shelf_height);
            page.shelf_height = 0;
        }
        if page.cursor_y.saturating_add(outer_h) > edge {
            return None;
        }
        let x = page.cursor_x.saturating_add(padding);
        let y = page.cursor_y.saturating_add(padding);
        page.cursor_x = page.cursor_x.saturating_add(outer_w);
        page.shelf_height = page.shelf_height.max(outer_h);
        Some(Chart {
            page: u16::try_from(page_index).unwrap_or(u16::MAX),
            x,
            y,
            width,
            height,
        })
    }
}

/// A baked set of atlas pages: one RGB8 buffer per page, gutter-dilated.
///
/// Built once per level load, after the fill pass produced every chart's
/// texels. The renderer uploads each page once and never touches it again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LightmapAtlas {
    pages: Vec<LightmapPage>,
}

impl LightmapAtlas {
    /// Fills every chart and dilates its gutter into a set of atlas pages.
    ///
    /// `page_count` is the packer's page count. `fill` receives one patch and
    /// its chart and must return `chart.width * chart.height` RGB triples,
    /// row-major. Any deviation — a wrong count, a non-finite channel, a chart
    /// that does not fit the page it was allocated on — is a named failure; the
    /// caller rebuilds the level with [`super::LightmapMode::Off`] rather than
    /// rendering a half-empty atlas.
    ///
    /// # Errors
    ///
    /// Returns the named [`LightmapFailure`] — `PageOverflow`, `Layout`,
    /// `InvalidConfig`, `FillSize` or `FillNonFinite` — when the page set or a
    /// chart's texels are not exactly what the plan described. A caller must
    /// treat every one of them as "draw the vertex-lit level".
    pub fn bake(
        config: &LightmapConfig,
        page_count: usize,
        charts: &[(LightmapPatch, Chart)],
        mut fill: impl FnMut(&LightmapPatch, &Chart) -> Vec<[f32; 3]>,
    ) -> Result<Self, LightmapFailure> {
        if page_count > config.max_pages {
            return Err(LightmapFailure::PageOverflow);
        }
        if config.page_edge == 0 || config.usable_edge() == 0 {
            return Err(LightmapFailure::InvalidConfig);
        }
        let buffer_len = page_buffer_len(config.page_edge)?;
        let mut pages: Vec<LightmapPage> = (0..page_count)
            .map(|_| LightmapPage {
                width: config.page_edge,
                height: config.page_edge,
                rgb: vec![0; buffer_len],
            })
            .collect();

        for (patch, chart) in charts {
            let Some(page) = pages.get_mut(usize::from(chart.page)) else {
                return Err(LightmapFailure::Layout);
            };
            let Some(texels) = chart_texel_count(chart) else {
                return Err(LightmapFailure::Layout);
            };
            if !chart_fits_page(chart, page) {
                return Err(LightmapFailure::Layout);
            }
            let colors = fill(patch, chart);
            if colors.len() != texels {
                return Err(LightmapFailure::FillSize);
            }
            if !colors
                .iter()
                .all(|color| color.iter().all(|value| value.is_finite()))
            {
                return Err(LightmapFailure::FillNonFinite);
            }
            write_chart(page, chart, &colors)?;
            dilate(page, chart, config.padding);
        }
        Ok(Self { pages })
    }

    /// The baked pages, in page order.
    #[must_use]
    pub fn pages(&self) -> &[LightmapPage] {
        &self.pages
    }

    /// Consumes the atlas, returning its pages.
    #[must_use]
    pub fn into_pages(self) -> Vec<LightmapPage> {
        self.pages
    }

    /// Number of resident pages.
    #[must_use]
    pub const fn page_count(&self) -> usize {
        self.pages.len()
    }
}

/// Bytes one RGB8 page of `edge` texels occupies, when addressable.
fn page_buffer_len(edge: u32) -> Result<usize, LightmapFailure> {
    let edge = usize::try_from(edge).map_err(|_| LightmapFailure::InvalidConfig)?;
    edge.checked_mul(edge)
        .and_then(|texels| texels.checked_mul(3))
        .ok_or(LightmapFailure::InvalidConfig)
}

/// Texels one chart holds, or `None` for a zero-sized chart.
fn chart_texel_count(chart: &Chart) -> Option<usize> {
    if chart.width == 0 || chart.height == 0 {
        return None;
    }
    let width = usize::try_from(chart.width).ok()?;
    let height = usize::try_from(chart.height).ok()?;
    width.checked_mul(height)
}

/// True when a chart's data rectangle lies inside its page.
fn chart_fits_page(chart: &Chart, page: &LightmapPage) -> bool {
    chart
        .x
        .checked_add(chart.width)
        .is_some_and(|right| right <= page.width)
        && chart
            .y
            .checked_add(chart.height)
            .is_some_and(|bottom| bottom <= page.height)
}

/// Copies one chart's filled texels into its page.
fn write_chart(
    page: &mut LightmapPage,
    chart: &Chart,
    colors: &[[f32; 3]],
) -> Result<(), LightmapFailure> {
    for row in 0..chart.height {
        let y = chart.y.checked_add(row).ok_or(LightmapFailure::Layout)?;
        for column in 0..chart.width {
            let x = chart.x.checked_add(column).ok_or(LightmapFailure::Layout)?;
            let index = usize::try_from(
                u64::from(row)
                    .checked_mul(u64::from(chart.width))
                    .and_then(|v| v.checked_add(u64::from(column)))
                    .ok_or(LightmapFailure::Layout)?,
            )
            .map_err(|_| LightmapFailure::Layout)?;
            let color = colors
                .get(index)
                .copied()
                .ok_or(LightmapFailure::FillSize)?;
            set_texel(page, x, y, encode_light(color));
        }
    }
    Ok(())
}

/// Encodes one linear light colour as the RGB8 triple a page stores.
fn encode_light(color: [f32; 3]) -> [u8; 3] {
    color.map(encode_channel)
}

/// Encodes one clamped linear channel as a byte, rounding to nearest.
fn encode_channel(value: f32) -> u8 {
    if value.is_nan() {
        return 0;
    }
    let clamped = value.clamp(0.0, 1.0);
    // `clamped * 255 + 0.5` is in [0.5, 255.5], so the cast cannot leave u8.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let byte = clamped.mul_add(255.0, 0.5) as u8;
    byte
}

/// Dilates a chart's border outwards into its own gutter.
///
/// Every gutter texel receives the nearest data texel of the chart (clamped to
/// the opposite edge at a corner), so a bilinear sample just outside the data
/// rectangle still resolves the chart's edge colour. Gutter rectangles never
/// overlap another chart's data or gutter, which is what the `padding`-wide
/// outer rectangle reserved at allocation time guarantees.
fn dilate(page: &mut LightmapPage, chart: &Chart, padding: u32) {
    if padding == 0 || chart.width == 0 || chart.height == 0 {
        return;
    }
    let right = chart.x.saturating_add(chart.width).saturating_sub(1);
    let bottom = chart.y.saturating_add(chart.height).saturating_sub(1);
    let x_start = chart.x.saturating_sub(padding);
    let y_start = chart.y.saturating_sub(padding);
    let x_end = chart.x.saturating_add(chart.width).saturating_add(padding);
    let y_end = chart.y.saturating_add(chart.height).saturating_add(padding);
    for y in y_start..y_end {
        for x in x_start..x_end {
            let inside = x >= chart.x && x <= right && y >= chart.y && y <= bottom;
            if inside {
                continue;
            }
            let source_x = x.clamp(chart.x, right);
            let source_y = y.clamp(chart.y, bottom);
            if let Some(color) = get_texel(page, source_x, source_y) {
                set_texel(page, x, y, color);
            }
        }
    }
}

/// Reads one RGB8 texel, or `None` when it lies outside the page.
fn get_texel(page: &LightmapPage, x: u32, y: u32) -> Option<[u8; 3]> {
    let offset = texel_offset(page, x, y)?;
    Some([
        *page.rgb.get(offset)?,
        *page.rgb.get(offset.saturating_add(1))?,
        *page.rgb.get(offset.saturating_add(2))?,
    ])
}

/// Writes one RGB8 texel if it lies inside the page.
fn set_texel(page: &mut LightmapPage, x: u32, y: u32, color: [u8; 3]) {
    let Some(offset) = texel_offset(page, x, y) else {
        return;
    };
    if let Some(slot) = page.rgb.get_mut(offset) {
        *slot = color[0];
    }
    if let Some(slot) = page.rgb.get_mut(offset.saturating_add(1)) {
        *slot = color[1];
    }
    if let Some(slot) = page.rgb.get_mut(offset.saturating_add(2)) {
        *slot = color[2];
    }
}

/// Byte offset of texel `(x, y)` in a page's row-major RGB8 buffer.
fn texel_offset(page: &LightmapPage, x: u32, y: u32) -> Option<usize> {
    if x >= page.width || y >= page.height {
        return None;
    }
    let stride = usize::try_from(page.width).ok()?.checked_mul(3)?;
    let row = usize::try_from(y).ok()?.checked_mul(stride)?;
    let column = usize::try_from(x).ok()?.checked_mul(3)?;
    row.checked_add(column)
}

/// Encodes one page as PNG bytes (RGB expanded to RGBA), for the developer
/// atlas dump and for tests.
///
/// # Errors
///
/// Returns a message when the page's buffer does not match its dimensions or
/// the PNG encoder rejects the image.
pub fn page_png_bytes(page: &LightmapPage) -> Result<Vec<u8>, String> {
    let texels = usize::try_from(page.width)
        .ok()
        .and_then(|width| {
            usize::try_from(page.height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .ok_or_else(|| "atlas page is too large to address".to_string())?;
    if texels.checked_mul(3) != Some(page.rgb.len()) {
        return Err("atlas page buffer does not match its dimensions".to_string());
    }
    let mut rgba = vec![0u8; texels.saturating_mul(4)];
    for (target, rgb) in rgba
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(page.rgb.as_chunks::<3>().0)
    {
        let [r, g, b] = *rgb;
        target.copy_from_slice(&[r, g, b, 255]);
    }
    crate::materials::encode_png(&crate::materials::RawImage::new(
        page.width,
        page.height,
        rgba,
    ))
}

/// Writes one page as a PNG under `path`, creating parent directories.
///
/// Used by the developer atlas dump (`LIMINAL_DUMP_LIGHTMAPS=1`), which puts its
/// files under `target/agent-work/atlases/`.
/// # Errors
///
/// Returns a message when the page cannot be encoded or the file cannot be
/// written.
pub fn write_page_png(page: &LightmapPage, path: &Path) -> Result<(), String> {
    let bytes = page_png_bytes(page)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, bytes).map_err(|error| error.to_string())
}
