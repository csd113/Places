//! The deterministic bottom-left skyline packer, atlas pages and PNG debugging.
//!
//! Packing is deliberately simple and reproducible: charts are placed in the
//! order the mesh emitter produced them, into the first page that has room for
//! them, at the lowest free position on that page (leftmost when two positions
//! tie). Nothing depends on hashing, sorting or floating-point order, so the
//! same level always produces byte-identical pages and the same `Chart`
//! rectangles.
//!
//! Why a skyline rather than one open shelf per page: the chart set is a
//! heterogeneous mix of small floor cells, wide room floors and long thin wall
//! strips, and a shelf's band is as tall as the tallest chart on it. Measured
//! on `places_demo`, a per-page shelf list (even first-fit over every shelf)
//! needs three 512-texel pages for the `Low` chart set, so the profile fell
//! back to vertex lighting, while the skyline packs the same set into two. The
//! skyline tracks the filled top edge per column and can therefore use the
//! vertical slack a tall chart leaves beside itself; see
//! `docs/MAP_AUTHORING_GUIDE.md` §18 for the density each profile now reaches.
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

/// One step of a page's skyline: the top edge of the filled area over
/// `[x, x + width)`, with `y` in page texels.
///
/// The segments of one page tile `0..page_edge` in ascending `x` with no gaps
/// and no overlap, and adjacent segments of equal `y` are always merged, so the
/// list is a canonical function of the placements.
#[derive(Clone, Copy, Debug)]
struct Segment {
    x: u32,
    y: u32,
    width: u32,
}

/// Places chart rectangles on square pages with a deterministic skyline policy.
///
/// The allocator is designed to run *inline*, while the mesh is being emitted:
/// each chart is placed the moment its patch is built, using only the patches
/// that came before it. That is what lets the emitter write final lightmap UVs
/// into its vertices in one pass instead of patching the mesh afterwards.
#[derive(Clone, Debug)]
pub struct SkylineAllocator {
    config: LightmapConfig,
    pages: Vec<Vec<Segment>>,
    failed: bool,
}

impl SkylineAllocator {
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
        // First fit over the open pages, in page order.
        for page_index in 0..self.pages.len() {
            if let Some(chart) =
                self.try_place(page_index, outer_w, outer_h, width, height, padding)
            {
                return Some(chart);
            }
        }
        // No open page can take it: open pages, in order, until one fits or the
        // budget is spent. An empty page always fits a chart of usable size.
        while self.pages.len() < self.config.max_pages {
            self.pages.push(vec![Segment {
                x: 0,
                y: 0,
                width: self.config.page_edge,
            }]);
            let page_index = self.pages.len().saturating_sub(1);
            if let Some(chart) =
                self.try_place(page_index, outer_w, outer_h, width, height, padding)
            {
                return Some(chart);
            }
        }
        self.failed = true;
        None
    }

    /// Tries one page: the lowest free position, leftmost on a tie, or `None`
    /// when the chart does not fit anywhere on it.
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
        let mut best: Option<(u32, u32)> = None;
        for segment in page.iter() {
            let x = segment.x;
            let Some(right) = x.checked_add(outer_w) else {
                continue;
            };
            if right > edge {
                continue;
            }
            // The skyline tiles the page, so `[x, right)` is always fully
            // covered; its top edge is the highest segment over the span.
            let mut top = 0_u32;
            for span in page.iter() {
                if span.x >= right {
                    break;
                }
                if span.x.saturating_add(span.width) > x {
                    top = top.max(span.y);
                }
            }
            if top.saturating_add(outer_h) > edge {
                continue;
            }
            let better = best
                .is_none_or(|(best_top, best_x)| top < best_top || (top == best_top && x < best_x));
            if better {
                best = Some((top, x));
            }
        }
        let (top, x) = best?;
        let right = x.checked_add(outer_w)?;
        let new_y = top.checked_add(outer_h)?;
        let mut next: Vec<Segment> = Vec::with_capacity(page.len().saturating_add(2));
        let mut inserted = false;
        for segment in page.iter() {
            let segment_right = segment.x.saturating_add(segment.width);
            if segment_right <= x {
                next.push(*segment);
                continue;
            }
            if segment.x >= right {
                if !inserted {
                    push_merged(
                        &mut next,
                        Segment {
                            x,
                            y: new_y,
                            width: outer_w,
                        },
                    );
                    inserted = true;
                }
                next.push(*segment);
                continue;
            }
            if segment.x < x {
                next.push(Segment {
                    x: segment.x,
                    y: segment.y,
                    width: x.saturating_sub(segment.x),
                });
            }
            if !inserted {
                push_merged(
                    &mut next,
                    Segment {
                        x,
                        y: new_y,
                        width: outer_w,
                    },
                );
                inserted = true;
            }
            if segment_right > right {
                next.push(Segment {
                    x: right,
                    y: segment.y,
                    width: segment_right.saturating_sub(right),
                });
            }
        }
        if !inserted {
            push_merged(
                &mut next,
                Segment {
                    x,
                    y: new_y,
                    width: outer_w,
                },
            );
        }
        *page = next;
        Some(chart_at(page_index, x, top, width, height, padding))
    }
}

/// The chart a placement at outer-rectangle `(x, y)` produces.
fn chart_at(page_index: usize, x: u32, y: u32, width: u32, height: u32, padding: u32) -> Chart {
    Chart {
        page: u16::try_from(page_index).unwrap_or(u16::MAX),
        x: x.saturating_add(padding),
        y: y.saturating_add(padding),
        width,
        height,
    }
}

/// Appends a segment, merging it with the previous one when they are adjacent
/// at the same height, so a page's skyline stays canonical.
fn push_merged(segments: &mut Vec<Segment>, segment: Segment) {
    if let Some(last) = segments.last_mut()
        && last.y == segment.y
        && last.x.saturating_add(last.width) == segment.x
    {
        last.width = last.width.saturating_add(segment.width);
        return;
    }
    segments.push(segment);
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
/// Used by the developer atlas dump (`PLACES_DUMP_LIGHTMAPS=1`), which puts its
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
