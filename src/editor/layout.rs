//! Logical window size sets control density; drawable size sets pixel mapping.
//! Retina and Windows DPI therefore keep the same logical hit targets.
use crate::engine::render::DrawableSize;

#[derive(Clone, Copy)]
pub(super) struct Layout {
    pub bounds: [f32; 4],
    pub scale: f32,
}
impl Layout {
    pub fn for_window(window: &sdl3::video::Window) -> Self {
        let (width, height) = window.size_in_pixels();
        let scale = window.display_scale() / window.pixel_density();
        Self::new(
            DrawableSize::new(width, height),
            window.size(),
            if scale.is_finite() && scale > 0.0 {
                scale
            } else {
                1.0
            },
        )
    }
    pub fn new(drawable: DrawableSize, window: (u32, u32), content_scale: f32) -> Self {
        let width = f32::from(u16::try_from(window.0).unwrap_or(u16::MAX));
        let height = f32::from(u16::try_from(window.1).unwrap_or(u16::MAX));
        let density = (width / 480.0).min(height / 272.0).max(1.0);
        Self {
            bounds: drawable.ui_bounds(),
            scale: (2.5 * content_scale / density).clamp(0.65, 1.0),
        }
    }
    pub fn point(self, [x, y]: [f32; 2], modal: bool) -> [f32; 2] {
        let [left, top, right, bottom] = self.bounds;
        if modal {
            [
                x.mul_add(
                    self.scale,
                    240.0f32.mul_add(-self.scale, f32::midpoint(left, right)),
                ),
                y.mul_add(
                    self.scale,
                    136.0f32.mul_add(-self.scale, f32::midpoint(top, bottom)),
                ),
            ]
        } else {
            [
                x.mul_add(self.scale, left),
                if y >= 214.0 {
                    f32::mul_add(272.0 - y, -self.scale, bottom)
                } else {
                    y.mul_add(self.scale, top)
                },
            ]
        }
    }
    pub fn rect(self, [x, y, w, h]: [f32; 4], modal: bool) -> [f32; 4] {
        let [left, top] = self.point([x, y], modal);
        [left, top, w * self.scale, h * self.scale]
    }
    pub fn viewport(self) -> [f32; 4] {
        let [left, top, right, bottom] = self.bounds;
        [
            112.0_f32.mul_add(self.scale, left),
            38.0_f32.mul_add(self.scale, top),
            right,
            30.0f32.mul_add(-self.scale, bottom),
        ]
    }
    pub fn contains(self, point: [f32; 2]) -> bool {
        let [left, top, right, bottom] = self.viewport();
        point[0] >= left && point[0] < right && point[1] >= top && point[1] < bottom
    }
    pub fn scene_list(self, point: [f32; 2]) -> bool {
        let [left, top] = self.point([0.0, 80.0], false);
        point[0] >= left
            && point[0] < 112.0_f32.mul_add(self.scale, left)
            && point[1] >= top
            && point[1] < 121.0_f32.mul_add(self.scale, self.bounds[1])
    }
}
pub(super) fn window_to_ui(
    drawable: DrawableSize,
    window: (u32, u32),
    point: [f32; 2],
) -> Option<[f32; 2]> {
    if drawable.is_empty() || window.0 == 0 || window.1 == 0 || point.iter().any(|v| !v.is_finite())
    {
        return None;
    }
    let [left, top, right, bottom] = drawable.ui_bounds();
    let width = f32::from(u16::try_from(window.0).ok()?);
    let height = f32::from(u16::try_from(window.1).ok()?);
    Some([
        point[0].mul_add((right - left) / width, left),
        point[1].mul_add((bottom - top) / height, top),
    ])
}
