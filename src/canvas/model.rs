use std::{path::PathBuf, sync::Arc};

use gpui_kit::{Hsla, ObjectFit, RenderImage, SharedString};
use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};

use crate::{
    canvas::geometry::{
        TEXT_BOX_PAD_PX, apply_image_transform, composite_item_onto_sheet,
        composite_shape_onto_sheet, composite_text_onto_sheet, dynamic_to_render_image,
    },
    shared::{
        i18n::{DEFAULT_LANGUAGE, Language},
        paper::PaperConfig,
        units::px_to_mm,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutMode {
    FullImage,
    IdPhoto,
    Freeform,
}

impl LayoutMode {
    #[allow(dead_code)]
    pub fn index(self) -> usize {
        match self {
            Self::FullImage => 0,
            Self::IdPhoto => 1,
            Self::Freeform => 2,
        }
    }

    #[allow(dead_code)]
    pub fn from_index(idx: usize) -> Self {
        match idx {
            0 => Self::FullImage,
            1 => Self::IdPhoto,
            _ => Self::Freeform,
        }
    }

    pub fn label_for(self, lang: Language) -> SharedString {
        match self {
            Self::FullImage => lang.tr("app.mode_full_image"),
            Self::IdPhoto => lang.tr("app.mode_id_photo"),
            Self::Freeform => lang.tr("app.mode_freeform"),
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub struct ContextMenuState {
    pub item_id: u64,
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug)]
pub enum CanvasDrag {
    None,
    PanCanvas {
        start_mx: f32,
        start_my: f32,
        orig_pan_x: f32,
        orig_pan_y: f32,
    },
    MoveItem {
        id: u64,
        start_mx: f32,
        start_my: f32,
        orig_x: f32,
        orig_y: f32,
    },
    ResizeItem {
        id: u64,
        start_mx: f32,
        start_my: f32,
        orig_w: f32,
        orig_h: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FitMode {
    Cover,
    Contain,
    Stretch,
    Center,
}

impl FitMode {
    pub const ALL: &'static [Self] = &[Self::Cover, Self::Contain, Self::Stretch, Self::Center];

    pub fn label_for(self, lang: Language) -> SharedString {
        match self {
            Self::Cover => lang.tr("app.fit_cover"),
            Self::Contain => lang.tr("app.fit_contain"),
            Self::Stretch => lang.tr("app.fit_stretch"),
            Self::Center => lang.tr("app.fit_center"),
        }
    }

    #[allow(dead_code)]
    pub fn to_gpui_object_fit(self) -> ObjectFit {
        match self {
            Self::Cover => ObjectFit::Cover,
            Self::Contain => ObjectFit::Contain,
            Self::Stretch => ObjectFit::Fill,
            Self::Center => ObjectFit::ScaleDown,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeKind {
    Rectangle,
    CircleOval,
}

impl ShapeKind {
    pub const ALL: &'static [Self] = &[Self::Rectangle, Self::CircleOval];

    pub fn label_for(self, lang: Language) -> SharedString {
        match self {
            Self::Rectangle => lang.tr("app.shape_rect"),
            Self::CircleOval => lang.tr("app.shape_oval"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdPhotoPreset {
    TwoByThree,
    ThreeByFour,
    FourBySix,
    Custom,
}

impl IdPhotoPreset {
    pub const ALL: &'static [Self] = &[
        Self::TwoByThree,
        Self::ThreeByFour,
        Self::FourBySix,
        Self::Custom,
    ];

    pub fn label_for(self, lang: Language) -> SharedString {
        match self {
            Self::TwoByThree => SharedString::new_static("2x3"),
            Self::ThreeByFour => SharedString::new_static("3x4"),
            Self::FourBySix => SharedString::new_static("4x6"),
            Self::Custom => lang.tr("app.custom"),
        }
    }

    pub fn dimensions_cm(self, custom_w_cm: f32, custom_h_cm: f32) -> (f32, f32) {
        match self {
            Self::TwoByThree => (2.0, 3.0),
            Self::ThreeByFour => (3.0, 4.0),
            Self::FourBySix => (4.0, 6.0),
            Self::Custom => (custom_w_cm.max(1.0), custom_h_cm.max(1.0)),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImageTransform {
    pub quarter_turns_cw: i32,
    pub fine_angle_deg: f32,
    pub flip_h: bool,
    pub flip_v: bool,
    pub pan_x: f32,
    pub pan_y: f32,
    pub zoom_scale: f32,
    pub crop_rect: [f32; 4],
}

impl Default for ImageTransform {
    fn default() -> Self {
        Self {
            quarter_turns_cw: 0,
            fine_angle_deg: 0.0,
            flip_h: false,
            flip_v: false,
            pan_x: 0.0,
            pan_y: 0.0,
            zoom_scale: 1.0,
            crop_rect: [0.0, 0.0, 1.0, 1.0],
        }
    }
}

impl ImageTransform {
    pub fn is_modified(&self) -> bool {
        self.quarter_turns_cw != 0
            || self.fine_angle_deg.abs() > 0.01
            || self.flip_h
            || self.flip_v
            || self.pan_x.abs() > 0.001
            || self.pan_y.abs() > 0.001
            || (self.zoom_scale - 1.0).abs() > 0.001
            || self.crop_rect[0].abs() > 0.001
            || self.crop_rect[1].abs() > 0.001
            || (self.crop_rect[2] - 1.0).abs() > 0.001
            || (self.crop_rect[3] - 1.0).abs() > 0.001
    }

    /// Rotates 90° clockwise and transforms normalized crop rectangle
    /// so that the cropped subject remains consistent after rotation.
    pub fn rotate_cw_90(&mut self) {
        self.quarter_turns_cw = (self.quarter_turns_cw + 1).rem_euclid(4);
        // 90° rotation swaps horizontal and vertical flip axes relative to image orientation
        std::mem::swap(&mut self.flip_h, &mut self.flip_v);
        let [x, y, w, h] = self.crop_rect;
        let new_x = (1.0 - y - h).clamp(0.0, 1.0);
        let new_y = x.clamp(0.0, 1.0);
        let new_w = h.clamp(0.0, 1.0 - new_x);
        let new_h = w.clamp(0.0, 1.0 - new_y);
        self.crop_rect = [new_x, new_y, new_w, new_h];
    }

    /// Toggles horizontal flip and mirrors the normalized crop rectangle.
    pub fn toggle_flip_h(&mut self) {
        self.flip_h = !self.flip_h;
        let [x, y, w, h] = self.crop_rect;
        let new_x = (1.0 - x - w).clamp(0.0, 1.0);
        let new_w = w.clamp(0.0, 1.0 - new_x);
        self.crop_rect = [new_x, y, new_w, h];
    }
}

#[derive(Clone)]
pub struct ImageAsset {
    pub label: String,
    #[allow(dead_code)]
    pub path: Option<PathBuf>,
    pub raw_image: Arc<DynamicImage>,
    /// Downscaled preview source (max 960px) for fast real-time UI rendering & editing.
    pub preview_raw: Arc<DynamicImage>,
    pub rendered: Arc<RenderImage>,
    pub orig_width: u32,
    pub orig_height: u32,
}

impl ImageAsset {
    pub fn from_dynamic(
        label: impl Into<String>,
        path: Option<PathBuf>,
        raw: DynamicImage,
        transform: &ImageTransform,
    ) -> Self {
        let (orig_width, orig_height) = raw.dimensions();
        let preview = if orig_width > 1200 || orig_height > 1200 {
            raw.resize(1200, 1200, image::imageops::FilterType::Triangle)
        } else {
            raw.clone()
        };
        let processed = apply_image_transform(&preview, transform);
        let rendered = dynamic_to_render_image(&processed);
        Self {
            label: label.into(),
            path,
            raw_image: Arc::new(raw),
            preview_raw: Arc::new(preview),
            rendered,
            orig_width,
            orig_height,
        }
    }

    pub fn rebuild_rendered(&mut self, transform: &ImageTransform) {
        let processed = apply_image_transform(&self.preview_raw, transform);
        self.rendered = dynamic_to_render_image(&processed);
    }

    pub fn display_dimensions(&self, transform: &ImageTransform) -> (f32, f32) {
        let is_rotated_90 = transform.quarter_turns_cw.rem_euclid(2) != 0;
        let (raw_w, raw_h) = if is_rotated_90 {
            (
                self.orig_height.max(1) as f32,
                self.orig_width.max(1) as f32,
            )
        } else {
            (
                self.orig_width.max(1) as f32,
                self.orig_height.max(1) as f32,
            )
        };
        let [_, _, cw, ch] = transform.crop_rect;
        let w = (raw_w * cw.clamp(0.05, 1.0)).max(1.0);
        let h = (raw_h * ch.clamp(0.05, 1.0)).max(1.0);
        (w, h)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAlignMode {
    Left,
    Center,
    Right,
    Justified,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextBoxData {
    pub content: String,
    pub font_size_pt: f32,
    pub font_family: String,
    pub color: Hsla,
    pub align: TextAlignMode,
    /// Becomes `true` once the user manually resizes the frame width on canvas.
    pub manual_size: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FreeformShapeKind {
    Rectangle,
    Circle,
    Triangle,
    Diamond,
    Star,
}

impl FreeformShapeKind {
    pub const ALL: &'static [Self] = &[
        Self::Rectangle,
        Self::Circle,
        Self::Triangle,
        Self::Diamond,
        Self::Star,
    ];

    pub fn label_for(self, lang: Language) -> SharedString {
        match self {
            Self::Rectangle => lang.tr("app.shape_rect"),
            Self::Circle => lang.tr("app.shape_circle"),
            Self::Triangle => lang.tr("app.shape_triangle"),
            Self::Diamond => lang.tr("app.shape_diamond"),
            Self::Star => lang.tr("app.shape_star"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShapeBoxData {
    pub kind: FreeformShapeKind,
    pub fill_color: Hsla,
}

#[derive(Clone)]
pub struct CanvasItem {
    pub id: u64,
    pub x_px: f32,
    pub y_px: f32,
    pub width_px: f32,
    pub height_px: f32,
    #[allow(dead_code)]
    pub rotation_deg: f32,
    pub lock_aspect: bool,
    pub shape_kind: ShapeKind,
    pub fit_mode: FitMode,
    pub cut_line: bool,
    pub cut_line_width: f32,
    #[allow(dead_code)]
    pub preset: Option<IdPhotoPreset>,
    pub image: Option<ImageAsset>,
    pub text: Option<TextBoxData>,
    pub shape: Option<ShapeBoxData>,
    pub transform: ImageTransform,
    pub corner_radius: f32,
    pub bg_color: Option<Hsla>,
    pub custom_name: Option<String>,
}

impl CanvasItem {
    pub fn is_text(&self) -> bool {
        self.text.is_some()
    }

    pub fn is_shape(&self) -> bool {
        self.shape.is_some()
    }

    pub fn is_printable(&self) -> bool {
        self.image.is_some()
            || self.text.as_ref().is_some_and(|t| !t.content.trim().is_empty())
            || self.shape.is_some()
    }

    #[allow(dead_code)]
    pub fn layer_name(&self, index: usize) -> SharedString {
        self.layer_name_for(index, DEFAULT_LANGUAGE)
    }

    pub fn layer_name_for(&self, index: usize, lang: Language) -> SharedString {
        if let Some(custom) = &self.custom_name {
            let trimmed = custom.trim();
            if !trimmed.is_empty() {
                return SharedString::from(trimmed.to_string());
            }
        }
        if let Some(asset) = &self.image {
            let trimmed = asset.label.trim();
            if !trimmed.is_empty() {
                return SharedString::from(trimmed.to_string());
            }
        }
        if let Some(tb) = &self.text {
            if let Some(first_line) = tb.content.lines().map(str::trim).find(|l| !l.is_empty()) {
                return SharedString::from(first_line.to_string());
            }
            return lang.fallback_text(index + 1);
        }
        if let Some(sb) = &self.shape {
            return sb.kind.label_for(lang);
        }
        lang.fallback_photo(index + 1)
    }

    pub fn assign_image(&mut self, mut asset: ImageAsset) {
        asset.rebuild_rendered(&self.transform);
        self.image = Some(asset);
    }

    pub fn update_transform(&mut self, new_transform: ImageTransform) {
        let needs_pixel_rebuild = self.transform.quarter_turns_cw != new_transform.quarter_turns_cw
            || (self.transform.fine_angle_deg - new_transform.fine_angle_deg).abs() > 0.05
            || self.transform.flip_h != new_transform.flip_h
            || self.transform.flip_v != new_transform.flip_v
            || self.transform.crop_rect != new_transform.crop_rect;
        self.transform = new_transform;
        if needs_pixel_rebuild && let Some(asset) = &mut self.image {
            asset.rebuild_rendered(&self.transform);
        }
    }
}

#[derive(Clone)]
pub struct CroppedPrintSheet {
    pub sheet: RgbaImage,
    pub crop_x_mm: f32,
    pub crop_y_mm: f32,
    pub crop_w_mm: f32,
    pub crop_h_mm: f32,
}

#[derive(Clone)]
pub struct PrintRenderSnapshot {
    pub paper: PaperConfig,
    pub mode: LayoutMode,
    pub items: Vec<CanvasItem>,
}

impl PrintRenderSnapshot {
    fn composite_items_onto_sheet(
        &self,
        sheet: &mut RgbaImage,
        items: &[&CanvasItem],
        offset_x_px: f32,
        offset_y_px: f32,
    ) {
        let mut transform_cache: Vec<(usize, ImageTransform, image::DynamicImage)> = Vec::new();
        let is_freeform = self.mode == LayoutMode::Freeform;
        for item in items {
            if let Some(asset) = &item.image {
                let ptr = std::sync::Arc::as_ptr(&asset.raw_image) as usize;
                let transformed = if !crate::canvas::geometry::is_transform_needed(&item.transform)
                {
                    asset.raw_image.as_ref()
                } else if let Some(idx) = transform_cache
                    .iter()
                    .position(|(p, t, _)| *p == ptr && t == &item.transform)
                {
                    &transform_cache[idx].2
                } else {
                    let img = apply_image_transform(&asset.raw_image, &item.transform).into_owned();
                    transform_cache.push((ptr, item.transform.clone(), img));
                    &transform_cache.last().unwrap().2
                };

                let (pan_x, pan_y, zoom, fit_mode, is_oval, cut_line) = if is_freeform {
                    (0.0, 0.0, 1.0, FitMode::Stretch, false, false)
                } else {
                    (
                        item.transform.pan_x,
                        item.transform.pan_y,
                        item.transform.zoom_scale,
                        item.fit_mode,
                        item.shape_kind == ShapeKind::CircleOval,
                        item.cut_line,
                    )
                };

                let bg_rgba = item.bg_color.map(|c| {
                    let rgb = c.to_rgb();
                    [
                        (rgb.r * 255.0).round().clamp(0.0, 255.0) as u8,
                        (rgb.g * 255.0).round().clamp(0.0, 255.0) as u8,
                        (rgb.b * 255.0).round().clamp(0.0, 255.0) as u8,
                        (rgb.a * 255.0).round().clamp(0.0, 255.0) as u8,
                    ]
                });

                composite_item_onto_sheet(
                    sheet,
                    transformed,
                    (item.x_px - offset_x_px).round() as i32,
                    (item.y_px - offset_y_px).round() as i32,
                    item.width_px.round().max(1.0) as u32,
                    item.height_px.round().max(1.0) as u32,
                    pan_x,
                    pan_y,
                    zoom,
                    fit_mode,
                    is_oval,
                    item.corner_radius,
                    bg_rgba,
                    cut_line,
                    item.cut_line_width,
                );
            } else if is_freeform && let Some(tb) = &item.text {
                let rgb = tb.color.to_rgb();
                let text_rgb = [
                    (rgb.r * 255.0).round().clamp(0.0, 255.0) as u8,
                    (rgb.g * 255.0).round().clamp(0.0, 255.0) as u8,
                    (rgb.b * 255.0).round().clamp(0.0, 255.0) as u8,
                ];
                composite_text_onto_sheet(
                    sheet,
                    &tb.content,
                    tb.font_size_pt,
                    &tb.font_family,
                    text_rgb,
                    tb.align,
                    self.paper.dpi,
                    (item.x_px - offset_x_px).round() as i32,
                    (item.y_px - offset_y_px).round() as i32,
                    item.width_px.round().max(1.0) as u32,
                    item.height_px.round().max(1.0) as u32,
                    TEXT_BOX_PAD_PX,
                );
            } else if is_freeform && let Some(sb) = &item.shape {
                let rgb = sb.fill_color.to_rgb();
                let fill_rgba = [
                    (rgb.r * 255.0).round().clamp(0.0, 255.0) as u8,
                    (rgb.g * 255.0).round().clamp(0.0, 255.0) as u8,
                    (rgb.b * 255.0).round().clamp(0.0, 255.0) as u8,
                    (rgb.a * 255.0).round().clamp(0.0, 255.0) as u8,
                ];
                composite_shape_onto_sheet(
                    sheet,
                    sb.kind,
                    fill_rgba,
                    (item.x_px - offset_x_px).round() as i32,
                    (item.y_px - offset_y_px).round() as i32,
                    item.width_px.round().max(1.0) as u32,
                    item.height_px.round().max(1.0) as u32,
                    item.corner_radius,
                );
            }
        }
    }

    /// Renders the workspace area containing image/text content with a bounding box (Kayeview method)
    /// to trim millions of empty pixels and speed up printing by orders of magnitude.
    pub fn render_cropped_sheet(&self) -> CroppedPrintSheet {
        let (pw_mm, ph_mm) = self.paper.dimensions_mm();
        let (pw_f, ph_f) = self.paper.dimensions_px();
        let active_items: Vec<&CanvasItem> =
            self.items.iter().filter(|i| i.is_printable()).collect();

        if active_items.is_empty() {
            return CroppedPrintSheet {
                sheet: RgbaImage::from_pixel(1, 1, Rgba([255, 255, 255, 255])),
                crop_x_mm: 0.0,
                crop_y_mm: 0.0,
                crop_w_mm: pw_mm,
                crop_h_mm: ph_mm,
            };
        }

        let mut min_x = pw_f;
        let mut min_y = ph_f;
        let mut max_x = 0.0f32;
        let mut max_y = 0.0f32;

        for item in &active_items {
            let pad = if item.cut_line {
                (item.cut_line_width * 2.0).round().max(2.0) + 2.0
            } else {
                0.0
            };
            min_x = min_x.min((item.x_px - pad).max(0.0));
            min_y = min_y.min((item.y_px - pad).max(0.0));
            max_x = max_x.max((item.x_px + item.width_px + pad).min(pw_f));
            max_y = max_y.max((item.y_px + item.height_px + pad).min(ph_f));
        }

        let crop_x_px = min_x.floor();
        let crop_y_px = min_y.floor();
        let crop_w_px = (max_x - crop_x_px).ceil().max(1.0) as u32;
        let crop_h_px = (max_y - crop_y_px).ceil().max(1.0) as u32;

        let mut sheet = RgbaImage::from_pixel(crop_w_px, crop_h_px, Rgba([255, 255, 255, 255]));
        self.composite_items_onto_sheet(&mut sheet, &active_items, crop_x_px, crop_y_px);

        let dpi = self.paper.dpi;
        CroppedPrintSheet {
            sheet,
            crop_x_mm: px_to_mm(crop_x_px, dpi),
            crop_y_mm: px_to_mm(crop_y_px, dpi),
            crop_w_mm: px_to_mm(crop_w_px as f32, dpi),
            crop_h_mm: px_to_mm(crop_h_px as f32, dpi),
        }
    }

    #[allow(dead_code)]
    pub fn render_sheet(&self) -> RgbaImage {
        let (pw_f, ph_f) = self.paper.dimensions_px();
        let pw = pw_f.round().max(100.0) as u32;
        let ph = ph_f.round().max(100.0) as u32;
        let mut sheet = RgbaImage::from_pixel(pw, ph, Rgba([255, 255, 255, 255]));
        let refs: Vec<&CanvasItem> = self.items.iter().filter(|i| i.is_printable()).collect();
        self.composite_items_onto_sheet(&mut sheet, &refs, 0.0, 0.0);
        sheet
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crop_rotation_and_flip_consistency() {
        let mut tf = ImageTransform::default();
        tf.crop_rect = [0.2, 0.3, 0.4, 0.5];

        // 1. Rotate 90 CW
        tf.rotate_cw_90();
        assert_eq!(tf.quarter_turns_cw, 1);
        assert!((tf.crop_rect[0] - 0.2).abs() < 1e-4);
        assert!((tf.crop_rect[1] - 0.2).abs() < 1e-4);
        assert!((tf.crop_rect[2] - 0.5).abs() < 1e-4);
        assert!((tf.crop_rect[3] - 0.4).abs() < 1e-4);

        // 2. Rotate 90 CW 3 more times (360° total) back to original orientation
        tf.rotate_cw_90();
        tf.rotate_cw_90();
        tf.rotate_cw_90();
        assert_eq!(tf.quarter_turns_cw, 0);
        assert!((tf.crop_rect[0] - 0.2).abs() < 1e-4);
        assert!((tf.crop_rect[1] - 0.3).abs() < 1e-4);
        assert!((tf.crop_rect[2] - 0.4).abs() < 1e-4);
        assert!((tf.crop_rect[3] - 0.5).abs() < 1e-4);

        // 3. Flip Horizontal
        tf.toggle_flip_h();
        assert!(tf.flip_h);
        assert!((tf.crop_rect[0] - 0.4).abs() < 1e-4);
        assert!((tf.crop_rect[1] - 0.3).abs() < 1e-4);

        // 4. Flip Horizontal back
        tf.toggle_flip_h();
        assert!(!tf.flip_h);
        assert!((tf.crop_rect[0] - 0.2).abs() < 1e-4);
        assert!((tf.crop_rect[1] - 0.3).abs() < 1e-4);
    }
}
