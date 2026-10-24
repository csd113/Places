//! Exact immutable CPU prop vertices. Repeated attributes are interned by bits;
//! runtime reads reconstruct one value on the stack, never a cached float copy.
//! Authoring keeps editable vectors, with ordinary copy-on-write on mutation.

use std::collections::HashMap;
use std::sync::Arc;

use super::Vertex;

#[derive(Clone, Debug)]
enum Attribute<const N: usize> {
    Constant([u32; N]),
    Palette {
        values: Vec<[u32; N]>,
        indices: Vec<u16>,
    },
    Direct(Vec<[u32; N]>),
}

impl<const N: usize> Attribute<N> {
    fn new(values: Vec<[u32; N]>) -> Self {
        if let Some(first) = values.first()
            && values.iter().all(|value| value == first)
        {
            return Self::Constant(*first);
        }
        let direct_bytes = values
            .capacity()
            .saturating_mul(std::mem::size_of::<[u32; N]>());
        let mut palette = Vec::new();
        let mut indices = Vec::with_capacity(values.len());
        let mut slots = HashMap::new();
        for value in &values {
            let next = palette.len();
            let Ok(index) = u16::try_from(next) else {
                return Self::Direct(values);
            };
            let slot = *slots.entry(*value).or_insert_with(|| {
                palette.push(*value);
                index
            });
            indices.push(slot);
            if palette
                .len()
                .saturating_mul(std::mem::size_of::<[u32; N]>())
                .saturating_add(indices.capacity().saturating_mul(2))
                >= direct_bytes
            {
                return Self::Direct(values);
            }
        }
        palette.shrink_to_fit();
        Self::Palette {
            values: palette,
            indices,
        }
    }

    fn get(&self, index: usize) -> Option<[u32; N]> {
        match self {
            Self::Constant(value) => Some(*value),
            Self::Palette { values, indices } => {
                values.get(usize::from(*indices.get(index)?)).copied()
            }
            Self::Direct(values) => values.get(index).copied(),
        }
    }

    const fn retained_bytes(&self) -> usize {
        match self {
            Self::Constant(_) => 0,
            Self::Palette { values, indices } => values
                .capacity()
                .saturating_mul(std::mem::size_of::<[u32; N]>())
                .saturating_add(
                    indices
                        .capacity()
                        .saturating_mul(std::mem::size_of::<u16>()),
                ),
            Self::Direct(values) => values
                .capacity()
                .saturating_mul(std::mem::size_of::<[u32; N]>()),
        }
    }
}

#[derive(Debug)]
pub struct SharedVertices {
    /// Already charged by the source-model cache; never another copy of shape.
    pub asset: Arc<crate::engine::props::LoadedPropAsset>,
    pub placements: Vec<super::canonical_props::CanonicalPlacement>,
    color: Attribute<4>,
}

impl SharedVertices {
    /// The independent exact lighting stream; upload need not reconstruct shape.
    #[must_use]
    pub fn colors(&self) -> SharedColors<'_> {
        SharedColors {
            shared: self,
            indices: 0..self.len(),
        }
    }

    fn len(&self) -> usize {
        self.asset
            .model
            .vertices
            .len()
            .saturating_mul(self.placements.len())
    }
    fn get(&self, index: usize) -> Option<Vertex> {
        let count = self.asset.model.vertices.len();
        let placement = self.placements.get(index.checked_div(count)?)?;
        let mut vertex =
            placement.vertex(self.asset.model.vertices.get(index.checked_rem(count)?)?);
        vertex.color = self.color.get(index)?.map(f32::from_bits);
        Some(vertex)
    }
    const fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            .saturating_add(self.color.retained_bytes())
            .saturating_add(
                self.placements
                    .capacity()
                    .saturating_mul(std::mem::size_of::<
                        super::canonical_props::CanonicalPlacement,
                    >()),
            )
    }
}

pub struct SharedColors<'a> {
    shared: &'a SharedVertices,
    indices: std::ops::Range<usize>,
}
impl Iterator for SharedColors<'_> {
    type Item = [f32; 4];
    fn next(&mut self) -> Option<Self::Item> {
        self.shared
            .color
            .get(self.indices.next()?)
            .map(|color| color.map(f32::from_bits))
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.indices.size_hint()
    }
}
impl ExactSizeIterator for SharedColors<'_> {}

#[derive(Debug)]
struct PackedVertices {
    len: usize,
    position: Attribute<3>,
    color: Attribute<4>,
    albedo: Attribute<3>,
    uv: Attribute<2>,
    frame: Attribute<7>,
    lightmap: Attribute<2>,
}

impl PackedVertices {
    fn new(vertices: &[Vertex]) -> Self {
        Self {
            len: vertices.len(),
            position: Attribute::new(vertices.iter().map(|v| v.pos.map(f32::to_bits)).collect()),
            color: Attribute::new(vertices.iter().map(|v| v.color.map(f32::to_bits)).collect()),
            albedo: Attribute::new(
                vertices
                    .iter()
                    .map(|v| v.caustics_albedo.map(f32::to_bits))
                    .collect(),
            ),
            uv: Attribute::new(vertices.iter().map(|v| v.uv.map(f32::to_bits)).collect()),
            frame: Attribute::new(
                vertices
                    .iter()
                    .map(|v| {
                        let [nx, ny, nz] = v.normal;
                        let [tx, ty, tz] = v.tangent;
                        [nx, ny, nz, tx, ty, tz, v.handedness].map(f32::to_bits)
                    })
                    .collect(),
            ),
            lightmap: Attribute::new(
                vertices
                    .iter()
                    .map(|v| {
                        let [u, w] = v.lightmap;
                        [
                            u32::from(u) | (u32::from(w) << 16_u32),
                            u32::from(v.lightmap_page),
                        ]
                    })
                    .collect(),
            ),
        }
    }

    fn get(&self, index: usize) -> Option<Vertex> {
        if index >= self.len {
            return None;
        }
        let [nx, ny, nz, tx, ty, tz, handedness] = self.frame.get(index)?.map(f32::from_bits);
        let [uv, page] = self.lightmap.get(index)?;
        Some(Vertex {
            pos: self.position.get(index)?.map(f32::from_bits),
            color: self.color.get(index)?.map(f32::from_bits),
            caustics_albedo: self.albedo.get(index)?.map(f32::from_bits),
            uv: self.uv.get(index)?.map(f32::from_bits),
            normal: [nx, ny, nz],
            tangent: [tx, ty, tz],
            handedness,
            lightmap: [
                u16::try_from(uv & 0xFFFF).ok()?,
                u16::try_from(uv >> 16).ok()?,
            ],
            lightmap_page: u8::try_from(page).ok()?,
        })
    }

    const fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            .saturating_add(self.position.retained_bytes())
            .saturating_add(self.color.retained_bytes())
            .saturating_add(self.albedo.retained_bytes())
            .saturating_add(self.uv.retained_bytes())
            .saturating_add(self.frame.retained_bytes())
            .saturating_add(self.lightmap.retained_bytes())
    }
}

/// Shared prop vertex storage. Package validation precedes exact interning;
/// no quantization, normal fitting or geometry change is performed here.
#[derive(Clone, Debug)]
pub struct PropVertices {
    editable: Arc<Vec<Vertex>>,
    packed: Option<Arc<PackedVertices>>,
    shared: Option<Arc<SharedVertices>>,
}

impl Default for PropVertices {
    fn default() -> Self {
        Vec::new().into()
    }
}

impl From<Vec<Vertex>> for PropVertices {
    fn from(vertices: Vec<Vertex>) -> Self {
        Self {
            editable: Arc::new(vertices),
            packed: None,
            shared: None,
        }
    }
}

impl PropVertices {
    #[must_use]
    pub fn shared(&self) -> Option<&SharedVertices> {
        self.shared.as_deref()
    }

    /// Admission has already verified exact source topology and shape. Retain
    /// only the existing independent baked-colour palette and small placements.
    /// # Errors
    /// Propagates resource pressure/cancellation before replacing ownership.
    pub fn share(
        &mut self,
        asset: Arc<crate::engine::props::LoadedPropAsset>,
        placements: Vec<super::canonical_props::CanonicalPlacement>,
    ) -> Result<(), String> {
        crate::engine::resources::checkpoint()?;
        let color = self.packed.as_ref().map_or_else(
            || Attribute::new(self.iter().map(|v| v.color.map(f32::to_bits)).collect()),
            |packed| packed.color.clone(),
        );
        let shared = SharedVertices {
            asset,
            placements,
            color,
        };
        if shared.retained_bytes() < self.retained_bytes() {
            self.shared = Some(Arc::new(shared));
            self.packed = None;
            self.editable = Arc::new(Vec::new());
        }
        Ok(())
    }

    /// Retain exact palettes only when they save at least a tenth of the input.
    /// # Errors
    /// Refuses the temporary allocation under pressure or cancellation.
    pub fn packed(vertices: Vec<Vertex>) -> Result<Self, String> {
        if vertices.len() < 64 {
            return Ok(vertices.into());
        }
        let _reservation = crate::engine::resources::allocation(
            vertices
                .capacity()
                .saturating_mul(std::mem::size_of::<Vertex>())
                .saturating_mul(2),
        )?;
        let packed = PackedVertices::new(&vertices);
        if packed.retained_bytes()
            < vertices
                .capacity()
                .saturating_mul(std::mem::size_of::<Vertex>())
                .saturating_mul(9)
                .checked_div(10)
                .unwrap_or(0)
        {
            Ok(Self {
                editable: Arc::new(Vec::new()),
                packed: Some(Arc::new(packed)),
                shared: None,
            })
        } else {
            Ok(vertices.into())
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        if let Some(shared) = &self.shared {
            return shared.len();
        }
        self.packed.as_ref().map_or(self.editable.len(), |v| v.len)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns an exact value without expanding or retaining a decoded page.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<Vertex> {
        if let Some(shared) = &self.shared {
            return shared.get(index);
        }
        self.packed
            .as_ref()
            .map_or_else(|| self.editable.get(index).copied(), |v| v.get(index))
    }

    /// Borrow editable pixels or decode this one batch for a slice-only consumer.
    #[must_use]
    pub fn as_slice(&self) -> std::borrow::Cow<'_, [Vertex]> {
        if self.packed.is_some() || self.shared.is_some() {
            std::borrow::Cow::Owned(self.iter().collect())
        } else {
            std::borrow::Cow::Borrowed(self.editable.as_slice())
        }
    }

    /// Ordered stack values, equally available in authoring and runtime forms.
    #[must_use]
    pub fn iter(&self) -> Vertices<'_> {
        Vertices {
            vertices: self,
            indices: 0..self.len(),
        }
    }

    /// Authoring's copy-on-write path; runtime immutable reads never call it.
    pub fn make_mut(&mut self) -> &mut Vec<Vertex> {
        if let Some(shared) = self.shared.take() {
            self.editable = Arc::new((0..shared.len()).filter_map(|i| shared.get(i)).collect());
        }
        if let Some(packed) = self.packed.take() {
            self.editable = Arc::new(
                (0..packed.len)
                    .filter_map(|index| packed.get(index))
                    .collect(),
            );
        }
        Arc::make_mut(&mut self.editable)
    }

    /// Heap storage for position, color, albedo, UV, frame and lightmap streams.
    /// Inline constants and the packed record's small header are separate.
    #[cfg(feature = "visual-diagnostics")]
    #[must_use]
    pub fn storage_bytes_by_attribute(&self) -> Option<[usize; 6]> {
        if let Some(shared) = &self.shared {
            return Some([0, shared.color.retained_bytes(), 0, 0, 0, 0]);
        }
        self.packed.as_ref().map(|packed| {
            [
                packed.position.retained_bytes(),
                packed.color.retained_bytes(),
                packed.albedo.retained_bytes(),
                packed.uv.retained_bytes(),
                packed.frame.retained_bytes(),
                packed.lightmap.retained_bytes(),
            ]
        })
    }

    /// One allocation charge per shared identity, not per cloned batch.
    #[must_use]
    pub fn identity(&self) -> *const () {
        if let Some(shared) = &self.shared {
            return Arc::as_ptr(shared).cast();
        }
        self.packed.as_ref().map_or_else(
            || Arc::as_ptr(&self.editable).cast(),
            |v| Arc::as_ptr(v).cast(),
        )
    }

    #[must_use]
    pub fn retained_bytes(&self) -> usize {
        if let Some(shared) = &self.shared {
            return shared
                .retained_bytes()
                .saturating_add(std::mem::size_of::<usize>().saturating_mul(2));
        }
        let heap = self.packed.as_ref().map_or_else(
            || {
                std::mem::size_of::<Vec<Vertex>>().saturating_add(
                    self.editable
                        .capacity()
                        .saturating_mul(std::mem::size_of::<Vertex>()),
                )
            },
            |v| v.retained_bytes(),
        );
        heap.saturating_add(std::mem::size_of::<usize>().saturating_mul(2))
    }
}

impl PartialEq for PropVertices {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}

pub struct Vertices<'a> {
    vertices: &'a PropVertices,
    indices: std::ops::Range<usize>,
}
impl Iterator for Vertices<'_> {
    type Item = Vertex;
    fn next(&mut self) -> Option<Vertex> {
        self.vertices.get(self.indices.next()?)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.indices.size_hint()
    }
}
impl ExactSizeIterator for Vertices<'_> {}

#[cfg(test)]
#[path = "../../tests/prop_vertex_storage.rs"]
mod tests;

impl<'a> IntoIterator for &'a PropVertices {
    type Item = Vertex;
    type IntoIter = Vertices<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
