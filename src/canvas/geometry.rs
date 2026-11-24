use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    sync::{Arc, Mutex, OnceLock},
};

use cosmic_text::{
    Align as CosmicAlign, Attrs, Buffer, Color as CosmicColor, Family, FontSystem, Metrics,
    Shaping, Style as CosmicStyle, SwashCache, Weight as CosmicWeight, Wrap,
};
use gpui_kit::RenderImage;
use image::{
    DynamicImage, Frame, GenericImageView, ImageBuffer, ImageDecoder as _, ImageReader, Rgba,
    RgbaImage, metadata::Orientation,
};

use crate::canvas::model::{FitMode, FreeformShapeKind, ImageTransform, TextAlignMode};

/// Converts a `DynamicImage` into GPUI's `Arc<RenderImage>` (BGRA pixel format).
pub fn dynamic_to_render_image(img: &DynamicImage) -> Arc<RenderImage> {
    let mut data = img.to_rgba8();
    for pixel in data.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Arc::new(RenderImage::new([Frame::new(data)]))
}

/// Loads an image from disk (applying EXIF orientation when present) or returns an error string.
pub fn load_dynamic_image(path: &Path) -> Result<DynamicImage, String> {
    let reader = ImageReader::open(path)
        .map_err(|e| format!("Gagal membuka gambar {}: {e}", path.display()))?
        .with_guessed_format()
        .map_err(|e| format!("Gagal membaca format gambar {}: {e}", path.display()))?;
    let mut decoder = reader
        .into_decoder()
        .map_err(|e| format!("Gagal mendekode gambar {}: {e}", path.display()))?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(decoder)
        .map_err(|e| format!("Gagal memuat piksel gambar {}: {e}", path.display()))?;
    img.apply_orientation(orientation);
    Ok(img)
}

/// Generates a monochrome/industrial studio portrait sample image in memory for quick testing.
#[allow(dead_code)]
pub fn generate_sample_photo(width: u32, height: u32) -> DynamicImage {
    let w = width.max(64);
    let h = height.max(64);
    let mut buf: RgbaImage = ImageBuffer::new(w, h);

    let cx = w as f32 * 0.5;
    let head_cy = h as f32 * 0.36;
    let head_r = w.min(h) as f32 * 0.18;
    let shoulder_cy = h as f32 * 0.92;
    let shoulder_rx = w as f32 * 0.36;
    let shoulder_ry = h as f32 * 0.38;

    for y in 0..h {
        for x in 0..w {
            let fx = x as f32;
            let fy = y as f32;

            // Subtle vignette backdrop
            let dx_norm = (fx - cx) / (w as f32 * 0.5);
            let dy_norm = (fy - h as f32 * 0.5) / (h as f32 * 0.5);
            let dist = (dx_norm * dx_norm + dy_norm * dy_norm).sqrt();
            let base = (68.0 - dist * 22.0).clamp(32.0, 74.0) as u8;
            let mut px_color = [base, base, base.saturating_add(4), 255];

            // Grid crosshair lines
            if x % 40 == 0 || y % 40 == 0 {
                px_color = [
                    base.saturating_add(10),
                    base.saturating_add(10),
                    base.saturating_add(12),
                    255,
                ];
            }

            // Silhouette head & shoulders for clear orientation/flip/rotate feedback
            let head_d = ((fx - cx).powi(2) + (fy - head_cy).powi(2)).sqrt();
            let sh_d =
                ((fx - cx) / shoulder_rx).powi(2) + ((fy - shoulder_cy) / shoulder_ry).powi(2);
            if head_d <= head_r || (fy > head_cy + head_r * 0.65 && sh_d <= 1.0) {
                px_color = [212, 212, 218, 255];
            }

            // Asymmetric badge mark on top-right of chest so horizontal/vertical flips are unmistakable
            if fx > cx + head_r * 0.25
                && fx < cx + head_r * 0.95
                && fy > head_cy + head_r * 1.45
                && fy < head_cy + head_r * 1.85
            {
                px_color = [36, 36, 40, 255];
            }

            buf.put_pixel(x, y, Rgba(px_color));
        }
    }

    DynamicImage::ImageRgba8(buf)
}

#[inline]
pub fn is_transform_needed(transform: &ImageTransform) -> bool {
    transform.quarter_turns_cw.rem_euclid(4) != 0
        || transform.flip_h
        || transform.flip_v
        || transform.fine_angle_deg.abs() > 0.05
        || transform.crop_rect[0] > 0.001
        || transform.crop_rect[1] > 0.001
        || (transform.crop_rect[2] - 1.0).abs() > 0.001
        || (transform.crop_rect[3] - 1.0).abs() > 0.001
}

/// Applies crop, 90° rotations, fine angle rotation (-45°..+45°), and horizontal/vertical flips.
pub fn apply_image_transform<'a>(
    src: &'a DynamicImage,
    transform: &ImageTransform,
) -> std::borrow::Cow<'a, DynamicImage> {
    if !is_transform_needed(transform) {
        return std::borrow::Cow::Borrowed(src);
    }
    let mut img = src.clone();

    // 1. Quarter-turn rotations (0, 90, 180, 270 CW)
    match transform.quarter_turns_cw.rem_euclid(4) {
        1 => img = img.rotate90(),
        2 => img = img.rotate180(),
        3 => img = img.rotate270(),
        _ => {}
    }

    // 2. Horizontal & Vertical flips
    if transform.flip_h {
        img = img.fliph();
    }
    if transform.flip_v {
        img = img.flipv();
    }

    // 3. Normalized crop bounding box [x, y, w, h] in 0.0..=1.0
    let [cx, cy, cw, ch] = transform.crop_rect;
    let (iw, ih) = img.dimensions();
    let x0 = ((cx.clamp(0.0, 0.95)) * iw as f32).round() as u32;
    let y0 = ((cy.clamp(0.0, 0.95)) * ih as f32).round() as u32;
    let crop_w = ((cw.clamp(0.05, 1.0 - cx)) * iw as f32).round().max(1.0) as u32;
    let crop_h = ((ch.clamp(0.05, 1.0 - cy)) * ih as f32).round().max(1.0) as u32;
    if x0 > 0 || y0 > 0 || crop_w < iw || crop_h < ih {
        img = img.crop_imm(
            x0.min(iw - 1),
            y0.min(ih - 1),
            crop_w.min(iw - x0),
            crop_h.min(ih - y0),
        );
    }

    // 4. Fine angle rotation (-45°..+45°) via bilinear/nearest sampling
    if transform.fine_angle_deg.abs() > 0.1 {
        img = rotate_fine_angle(&img, transform.fine_angle_deg);
    }

    std::borrow::Cow::Owned(img)
}

fn rotate_fine_angle(src: &DynamicImage, angle_deg: f32) -> DynamicImage {
    let rgba = src.to_rgba8();
    let (w, h) = rgba.dimensions();
    let mut out = RgbaImage::from_pixel(w, h, Rgba([20, 20, 21, 255]));
    let rad = -angle_deg.to_radians();
    let cos_a = rad.cos();
    let sin_a = rad.sin();
    let cx = w as f32 * 0.5;
    let cy = h as f32 * 0.5;

    for y in 0..h {
        for x in 0..w {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let sx = (dx * cos_a - dy * sin_a + cx).round() as i32;
            let sy = (dx * sin_a + dy * cos_a + cy).round() as i32;
            if sx >= 0 && sx < w as i32 && sy >= 0 && sy < h as i32 {
                out.put_pixel(x, y, *rgba.get_pixel(sx as u32, sy as u32));
            }
        }
    }
    DynamicImage::ImageRgba8(out)
}

/// Computes image placement geometry (draw_w, draw_h, offset_x, offset_y) for a frame.
pub fn compute_fitted_image_geometry(
    frame_w: f32,
    frame_h: f32,
    img_w: f32,
    img_h: f32,
    fit_mode: FitMode,
    zoom_scale: f32,
    pan_x_norm: f32,
    pan_y_norm: f32,
) -> (f32, f32, f32, f32) {
    let zoom = zoom_scale.clamp(0.2, 5.0);
    let (draw_w, draw_h) = match fit_mode {
        FitMode::Stretch => {
            let dw = (frame_w * zoom).max(1.0);
            let dh = (frame_h * zoom).max(1.0);
            (dw, dh)
        }
        FitMode::Cover => {
            let scale = zoom * f32::max(frame_w / img_w.max(1.0), frame_h / img_h.max(1.0));
            let dw = (img_w * scale).max(1.0);
            let dh = (img_h * scale).max(1.0);
            (dw, dh)
        }
        FitMode::Contain => {
            let scale = zoom * f32::min(frame_w / img_w.max(1.0), frame_h / img_h.max(1.0));
            let dw = (img_w * scale).max(1.0);
            let dh = (img_h * scale).max(1.0);
            (dw, dh)
        }
        FitMode::Center => {
            let base = f32::min(
                1.0,
                f32::min(frame_w / img_w.max(1.0), frame_h / img_h.max(1.0)),
            );
            let scale = zoom * base;
            let dw = (img_w * scale).max(1.0);
            let dh = (img_h * scale).max(1.0);
            (dw, dh)
        }
    };

    let center_x = frame_w * 0.5 + pan_x_norm * frame_w;
    let center_y = frame_h * 0.5 + pan_y_norm * frame_h;
    let offset_x = center_x - draw_w * 0.5;
    let offset_y = center_y - draw_h * 0.5;

    (draw_w, draw_h, offset_x, offset_y)
}

#[inline]
fn blend_pixel_over(dst: &mut [u8], src: [u8; 4], opacity: f32) {
    let eff_a = ((src[3] as f32) * opacity.clamp(0.0, 1.0)).round().clamp(0.0, 255.0) as u32;
    if eff_a == 0 {
        return;
    }
    if eff_a == 255 {
        dst[0] = src[0];
        dst[1] = src[1];
        dst[2] = src[2];
        dst[3] = 255;
        return;
    }
    let inv_a = 255 - eff_a;
    dst[0] = ((src[0] as u32 * eff_a + dst[0] as u32 * inv_a + 127) / 255) as u8;
    dst[1] = ((src[1] as u32 * eff_a + dst[1] as u32 * inv_a + 127) / 255) as u8;
    dst[2] = ((src[2] as u32 * eff_a + dst[2] as u32 * inv_a + 127) / 255) as u8;
    dst[3] = 255;
}

/// Renders an item image into a sheet buffer at the given frame rectangle, respecting pan/zoom, fit_mode, oval clipping & opacity.
pub fn composite_item_onto_sheet(
    sheet: &mut RgbaImage,
    item_img: &DynamicImage,
    frame_x: i32,
    frame_y: i32,
    frame_w: u32,
    frame_h: u32,
    pan_x_norm: f32,
    pan_y_norm: f32,
    zoom_scale: f32,
    fit_mode: FitMode,
    is_oval: bool,
    corner_radius: f32,
    bg_color: Option<[u8; 4]>,
    cut_line: bool,
    cut_line_width: f32,
    opacity: f32,
) {
    let opacity = opacity.clamp(0.0, 1.0);
    if frame_w == 0 || frame_h == 0 || opacity <= 0.001 {
        return;
    }
    let (img_w, img_h) = item_img.dimensions();
    if img_w == 0 || img_h == 0 {
        return;
    }

    let (draw_w_f, draw_h_f, offset_x_f, offset_y_f) = compute_fitted_image_geometry(
        frame_w as f32,
        frame_h as f32,
        img_w as f32,
        img_h as f32,
        fit_mode,
        zoom_scale,
        pan_x_norm,
        pan_y_norm,
    );
    let draw_w = draw_w_f as u32;
    let draw_h = draw_h_f as u32;
    let offset_x = offset_x_f as i32;
    let offset_y = offset_y_f as i32;

    let rx = frame_w as f32 * 0.5;
    let ry = frame_h as f32 * 0.5;
    let (sw, sh) = sheet.dimensions();

    let owned_holder;
    let src_slice: &[u8] = match item_img {
        DynamicImage::ImageRgba8(rgba) => rgba.as_raw(),
        other => {
            owned_holder = other.to_rgba8();
            owned_holder.as_raw()
        }
    };
    let dst_raw: &mut [u8] = sheet.as_mut();

    let denom_x = (draw_w as u64 - 1).max(1);
    let denom_y = (draw_h as u64 - 1).max(1);
    let num_x = (img_w as u64 - 1) * 256;
    let num_y = (img_h as u64 - 1) * 256;

    for py in 0..frame_h {
        let dst_y = frame_y + py as i32;
        if dst_y < 0 || dst_y >= sh as i32 {
            continue;
        }
        let dst_row = (dst_y as usize * sw as usize) * 4;

        let src_draw_y = py as i32 - offset_y;
        if src_draw_y < 0 || (src_draw_y as u32) >= draw_h {
            if let Some(bg) = bg_color {
                for px in 0..frame_w {
                    let dst_x = frame_x + px as i32;
                    if dst_x < 0 || dst_x >= sw as i32 {
                        continue;
                    }
                    if is_oval {
                        let nx = (px as f32 + 0.5 - rx) / rx.max(1.0);
                        let ny = (py as f32 + 0.5 - ry) / ry.max(1.0);
                        if nx * nx + ny * ny > 1.0 {
                            continue;
                        }
                    } else if corner_radius > 0.0 {
                        let r_c = corner_radius
                            .min(frame_w as f32 * 0.5)
                            .min(frame_h as f32 * 0.5);
                        let px_f = px as f32 + 0.5;
                        let py_f = py as f32 + 0.5;
                        let fw_f = frame_w as f32;
                        let fh_f = frame_h as f32;
                        let in_left = px_f < r_c;
                        let in_right = px_f > fw_f - r_c;
                        let in_top = py_f < r_c;
                        let in_bottom = py_f > fh_f - r_c;

                        if (in_left || in_right) && (in_top || in_bottom) {
                            let cx = if in_left { r_c } else { fw_f - r_c };
                            let cy = if in_top { r_c } else { fh_f - r_c };
                            let dx = px_f - cx;
                            let dy = py_f - cy;
                            if dx * dx + dy * dy > r_c * r_c {
                                continue;
                            }
                        }
                    }
                    let d_idx = dst_row + (dst_x as usize) * 4;
                    blend_pixel_over(&mut dst_raw[d_idx..d_idx + 4], bg, opacity);
                }
            }
            continue;
        }

        let sy_fp = (src_draw_y as u64 * num_y) / denom_y;
        let sy = (sy_fp >> 8) as usize;
        let fy = (sy_fp & 255) as u32;
        let ify = 256 - fy;
        let src_row0 = sy * (img_w as usize) * 4;
        let src_row1 = (sy + 1).min(img_h as usize - 1) * (img_w as usize) * 4;

        for px in 0..frame_w {
            let dst_x = frame_x + px as i32;
            if dst_x < 0 || dst_x >= sw as i32 {
                continue;
            }

            if is_oval {
                let nx = (px as f32 + 0.5 - rx) / rx.max(1.0);
                let ny = (py as f32 + 0.5 - ry) / ry.max(1.0);
                if nx * nx + ny * ny > 1.0 {
                    continue;
                }
            } else if corner_radius > 0.0 {
                let r_c = corner_radius
                    .min(frame_w as f32 * 0.5)
                    .min(frame_h as f32 * 0.5);
                let px_f = px as f32 + 0.5;
                let py_f = py as f32 + 0.5;
                let fw_f = frame_w as f32;
                let fh_f = frame_h as f32;
                let in_left = px_f < r_c;
                let in_right = px_f > fw_f - r_c;
                let in_top = py_f < r_c;
                let in_bottom = py_f > fh_f - r_c;

                if (in_left || in_right) && (in_top || in_bottom) {
                    let cx = if in_left { r_c } else { fw_f - r_c };
                    let cy = if in_top { r_c } else { fh_f - r_c };
                    let dx = px_f - cx;
                    let dy = py_f - cy;
                    if dx * dx + dy * dy > r_c * r_c {
                        continue;
                    }
                }
            }

            let src_draw_x = px as i32 - offset_x;
            if src_draw_x < 0 || (src_draw_x as u32) >= draw_w {
                if let Some(bg) = bg_color {
                    let d_idx = dst_row + (dst_x as usize) * 4;
                    blend_pixel_over(&mut dst_raw[d_idx..d_idx + 4], bg, opacity);
                }
                continue;
            }

            let sx_fp = (src_draw_x as u64 * num_x) / denom_x;
            let sx = (sx_fp >> 8) as usize;
            let fx = (sx_fp & 255) as u32;
            let ifx = 256 - fx;
            let sx1 = (sx + 1).min(img_w as usize - 1);

            let i00 = src_row0 + sx * 4;
            let i10 = src_row0 + sx1 * 4;
            let i01 = src_row1 + sx * 4;
            let i11 = src_row1 + sx1 * 4;

            let d_idx = dst_row + (dst_x as usize) * 4;
            let mut interp = [0u8; 4];
            for c in 0..4 {
                let top =
                    (src_slice[i00 + c] as u32 * ifx + src_slice[i10 + c] as u32 * fx + 128) >> 8;
                let bot =
                    (src_slice[i01 + c] as u32 * ifx + src_slice[i11 + c] as u32 * fx + 128) >> 8;
                interp[c] = ((top * ify + bot * fy + 128) >> 8) as u8;
            }

            if let Some(bg) = bg_color {
                let src_a = interp[3] as u32;
                let composed = if src_a == 255 {
                    interp
                } else if src_a == 0 {
                    bg
                } else {
                    let inv_a = 255 - src_a;
                    [
                        ((interp[0] as u32 * src_a + bg[0] as u32 * inv_a + 127) / 255) as u8,
                        ((interp[1] as u32 * src_a + bg[1] as u32 * inv_a + 127) / 255) as u8,
                        ((interp[2] as u32 * src_a + bg[2] as u32 * inv_a + 127) / 255) as u8,
                        (src_a + (bg[3] as u32 * inv_a + 127) / 255).min(255) as u8,
                    ]
                };
                blend_pixel_over(&mut dst_raw[d_idx..d_idx + 4], composed, opacity);
            } else {
                blend_pixel_over(&mut dst_raw[d_idx..d_idx + 4], interp, opacity);
            }
        }
    }

    if cut_line {
        let cut_color = Rgba([0, 0, 0, 255]);
        let thickness = (cut_line_width * 2.0).round().max(2.0) as i32;
        let t_f = thickness as f32;
        let fw_f = frame_w as f32;
        let fh_f = frame_h as f32;

        let y_start = (frame_y - thickness).max(0);
        let y_end = (frame_y + frame_h as i32 + thickness).min(sh as i32);
        let x_start = (frame_x - thickness).max(0);
        let x_end = (frame_x + frame_w as i32 + thickness).min(sw as i32);

        if is_oval {
            let rx_in = fw_f * 0.5;
            let ry_in = fh_f * 0.5;
            let rx_out = rx_in + t_f;
            let ry_out = ry_in + t_f;
            let cx = frame_x as f32 + rx_in;
            let cy = frame_y as f32 + ry_in;

            for y in y_start..y_end {
                let py_f = y as f32 + 0.5;
                let dy = py_f - cy;

                for x in x_start..x_end {
                    let px_f = x as f32 + 0.5;
                    let dx = px_f - cx;
                    let nx_out = dx / rx_out.max(1.0);
                    let ny_out = dy / ry_out.max(1.0);
                    if nx_out * nx_out + ny_out * ny_out > 1.0 {
                        continue;
                    }
                    let nx_in = dx / rx_in.max(1.0);
                    let ny_in = dy / ry_in.max(1.0);
                    if nx_in * nx_in + ny_in * ny_in <= 1.0 {
                        continue;
                    }
                    sheet.put_pixel(x as u32, y as u32, cut_color);
                }
            }
        } else if corner_radius > 0.0 {
            let r_in = corner_radius.min(fw_f * 0.5).min(fh_f * 0.5);
            let r_out = r_in + t_f;
            let ow_f = fw_f + 2.0 * t_f;
            let oh_f = fh_f + 2.0 * t_f;
            let ox = frame_x as f32 - t_f;
            let oy = frame_y as f32 - t_f;

            for y in y_start..y_end {
                let py_f = y as f32 + 0.5;
                let rel_y = py_f - oy;
                let in_top = rel_y < r_out;
                let in_bottom = rel_y > oh_f - r_out;

                let in_y = py_f - frame_y as f32;
                let inner_y_in = in_y >= 0.0 && in_y <= fh_f;
                let in_inner_top = in_y < r_in;
                let in_inner_bottom = in_y > fh_f - r_in;

                for x in x_start..x_end {
                    let px_f = x as f32 + 0.5;
                    let rel_x = px_f - ox;
                    let in_left = rel_x < r_out;
                    let in_right = rel_x > ow_f - r_out;

                    if (in_left || in_right) && (in_top || in_bottom) {
                        let cx = if in_left { r_out } else { ow_f - r_out };
                        let cy = if in_top { r_out } else { oh_f - r_out };
                        let dx = rel_x - cx;
                        let dy = rel_y - cy;
                        if dx * dx + dy * dy > r_out * r_out {
                            continue;
                        }
                    }

                    let in_x = px_f - frame_x as f32;
                    if inner_y_in && in_x >= 0.0 && in_x <= fw_f {
                        let in_inner_left = in_x < r_in;
                        let in_inner_right = in_x > fw_f - r_in;
                        if (in_inner_left || in_inner_right) && (in_inner_top || in_inner_bottom) {
                            let cx = if in_inner_left { r_in } else { fw_f - r_in };
                            let cy = if in_inner_top { r_in } else { fh_f - r_in };
                            let dx = in_x - cx;
                            let dy = in_y - cy;
                            if dx * dx + dy * dy <= r_in * r_in {
                                continue;
                            }
                        } else {
                            continue;
                        }
                    }

                    sheet.put_pixel(x as u32, y as u32, cut_color);
                }
            }
        } else {
            for y in y_start..y_end {
                let inside_y = y >= frame_y && y < frame_y + frame_h as i32;
                for x in x_start..x_end {
                    let inside_x = x >= frame_x && x < frame_x + frame_w as i32;
                    if inside_x && inside_y {
                        continue;
                    }
                    sheet.put_pixel(x as u32, y as u32, cut_color);
                }
            }
        }
    }
}

pub const TEXT_BOX_PAD_PX: f32 = 12.0;

fn resolve_cosmic_family(font_family: &str) -> Family<'_> {
    let trimmed = font_family.trim();
    if trimmed.is_empty() || trimmed == ".SystemUIFont" {
        Family::SansSerif
    } else {
        Family::Name(trimmed)
    }
}

fn with_text_engine<R>(f: impl FnOnce(&mut FontSystem, &mut SwashCache) -> R) -> R {
    static TEXT_ENGINE: OnceLock<Mutex<(FontSystem, SwashCache)>> = OnceLock::new();
    let engine = TEXT_ENGINE.get_or_init(|| {
        let font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        Mutex::new((font_system, swash_cache))
    });
    let mut guard = engine.lock().expect("text engine mutex poisoned");
    let (font_system, swash_cache) = &mut *guard;
    f(font_system, swash_cache)
}

/// Measures natural bounding size `(width_px, height_px)` in sheet pixels for a text box.
/// When `fixed_width_px` is `None`, width expands to fit text up to `max_paper_width_px`.
pub fn measure_text_box_size_px(
    text: &str,
    fallback_text: &str,
    font_size_pt: f32,
    font_family: &str,
    bold: bool,
    italic: bool,
    line_height: f32,
    dpi: u32,
    fixed_width_px: Option<f32>,
    max_paper_width_px: f32,
) -> (f32, f32) {
    let sample = if text.is_empty() { fallback_text } else { text };
    let font_px = (font_size_pt.clamp(6.0, 300.0) * (dpi.max(72) as f32 / 72.0)).max(4.0);
    let line_height_px = (font_px * line_height.clamp(0.9, 2.5)).max(4.0);
    let pad = TEXT_BOX_PAD_PX;
    let max_w = max_paper_width_px.max(64.0);
    let inner_max_w = match fixed_width_px {
        Some(fw) => (fw - pad * 2.0).max(20.0),
        None => (max_w - pad * 2.0).max(20.0),
    };

    let (max_run_w, run_count) = with_text_engine(|font_system, _| {
        let mut buffer = Buffer::new(font_system, Metrics::new(font_px, line_height_px));
        let mut borrowed = buffer.borrow_with(font_system);
        borrowed.set_size(Some(inner_max_w), None);
        borrowed.set_wrap(Wrap::WordOrGlyph);
        let attrs = Attrs::new()
            .family(resolve_cosmic_family(font_family))
            .weight(if bold {
                CosmicWeight::BOLD
            } else {
                CosmicWeight::NORMAL
            })
            .style(if italic {
                CosmicStyle::Italic
            } else {
                CosmicStyle::Normal
            });
        borrowed.set_text(sample, &attrs, Shaping::Advanced, None);
        borrowed.shape_until_scroll(false);

        let mut w = 0.0f32;
        let mut count = 0usize;
        for run in borrowed.layout_runs() {
            w = w.max(run.line_w);
            count += 1;
        }
        (w, count.max(1))
    });

    let total_lines = run_count.max(sample.split('\n').count()).max(1);
    let measured_h = (total_lines as f32 * line_height_px + pad * 2.0)
        .ceil()
        .max(32.0);
    match fixed_width_px {
        Some(fw) => (fw, measured_h),
        None => {
            // Extra slack avoids early soft-wrap if GPUI system fallback glyph metrics differ slightly
            let slack = (font_px * 0.25).max(12.0);
            let measured_w = (max_run_w + pad * 2.0 + slack).ceil().clamp(48.0, max_w);
            (measured_w, measured_h)
        }
    }
}

/// Shared polygon vertices in local `[0..w] x [0..h]` coordinates for Freeform shapes.
/// `Star` vertices are normalized so all 4 bounding-box edges touch `[0..w]` and `[0..h]`.
pub fn freeform_shape_polygon(kind: FreeformShapeKind, w: f32, h: f32) -> Vec<(f32, f32)> {
    match kind {
        FreeformShapeKind::Triangle => vec![(w * 0.5, 0.0), (w, h), (0.0, h)],
        FreeformShapeKind::Diamond => vec![(w * 0.5, 0.0), (w, h * 0.5), (w * 0.5, h), (0.0, h * 0.5)],
        FreeformShapeKind::Star => {
            let mut raw = Vec::with_capacity(10);
            let mut min_x = f32::INFINITY;
            let mut max_x = f32::NEG_INFINITY;
            let mut min_y = f32::INFINITY;
            let mut max_y = f32::NEG_INFINITY;
            for i in 0..10 {
                let angle = -std::f32::consts::FRAC_PI_2 + (i as f32) * std::f32::consts::PI / 5.0;
                let r = if i % 2 == 0 { 1.0 } else { 0.382 };
                let x = r * angle.cos();
                let y = r * angle.sin();
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
                raw.push((x, y));
            }
            let span_x = (max_x - min_x).max(1e-4);
            let span_y = (max_y - min_y).max(1e-4);
            raw.into_iter()
                .map(|(x, y)| ((x - min_x) / span_x * w, (y - min_y) / span_y * h))
                .collect()
        }
        FreeformShapeKind::Rectangle | FreeformShapeKind::Circle | FreeformShapeKind::Line => {
            Vec::new()
        }
    }
}

fn point_in_polygon(px: f32, py: f32, verts: &[(f32, f32)]) -> bool {
    let mut inside = false;
    let n = verts.len();
    let mut j = n.saturating_sub(1);
    for i in 0..n {
        let (xi, yi) = verts[i];
        let (xj, yj) = verts[j];
        if ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn point_to_segment_dist(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let dx = bx - ax;
    let dy = by - ay;
    let len_sq = dx * dx + dy * dy;
    if len_sq < 1e-6 {
        return (px - ax).hypot(py - ay);
    }
    let t = (((px - ax) * dx + (py - ay) * dy) / len_sq).clamp(0.0, 1.0);
    (px - (ax + t * dx)).hypot(py - (ay + t * dy))
}

fn min_dist_to_polygon_edges(px: f32, py: f32, verts: &[(f32, f32)]) -> f32 {
    let n = verts.len();
    if n < 2 {
        return f32::INFINITY;
    }
    let mut best = f32::INFINITY;
    let mut j = n - 1;
    for i in 0..n {
        let (ax, ay) = verts[j];
        let (bx, by) = verts[i];
        best = best.min(point_to_segment_dist(px, py, ax, ay, bx, by));
        j = i;
    }
    best
}

fn point_in_rounded_rect(lx: f32, ly: f32, fw: f32, fh: f32, r: f32) -> bool {
    if lx < 0.0 || lx > fw || ly < 0.0 || ly > fh {
        return false;
    }
    if r <= 0.5 {
        return true;
    }
    let in_l = lx < r;
    let in_r = lx > fw - r;
    let in_t = ly < r;
    let in_b = ly > fh - r;
    if (in_l || in_r) && (in_t || in_b) {
        let cx = if in_l { r } else { fw - r };
        let cy = if in_t { r } else { fh - r };
        (lx - cx).hypot(ly - cy) <= r
    } else {
        true
    }
}

/// Rasterizes a Freeform vector shape onto `sheet` inside `(frame_x, frame_y, frame_w, frame_h)`
/// with optional stroke, rotation, and opacity.
pub fn composite_shape_onto_sheet(
    sheet: &mut RgbaImage,
    kind: FreeformShapeKind,
    fill_rgba: [u8; 4],
    stroke_rgba: Option<[u8; 4]>,
    stroke_width: f32,
    rotation_deg: f32,
    opacity: f32,
    frame_x: i32,
    frame_y: i32,
    frame_w: u32,
    frame_h: u32,
    corner_radius: f32,
) {
    let opacity = opacity.clamp(0.0, 1.0);
    if frame_w == 0 || frame_h == 0 || opacity <= 0.001 {
        return;
    }
    let fw = frame_w as f32;
    let fh = frame_h as f32;
    let sw_eff = stroke_width.max(0.0);
    let has_stroke = stroke_rgba.is_some_and(|c| c[3] > 0) && sw_eff > 0.1;

    let rad = rotation_deg.to_radians();
    let cos_r = rad.cos();
    let sin_r = rad.sin();
    let cx_box = fw * 0.5;
    let cy_box = fh * 0.5;

    let pad = if rotation_deg.abs() > 0.01 {
        ((fw.hypot(fh) - fw.min(fh)) * 0.5).ceil() as i32 + 2
    } else {
        0
    };
    let x0 = (frame_x - pad).max(0);
    let y0 = (frame_y - pad).max(0);
    let x1 = (frame_x + frame_w as i32 + pad).min(sheet.width() as i32);
    let y1 = (frame_y + frame_h as i32 + pad).min(sheet.height() as i32);
    if x0 >= x1 || y0 >= y1 {
        return;
    }

    let poly = freeform_shape_polygon(kind, fw, fh);
    let r = corner_radius.min(fw * 0.5).min(fh * 0.5).max(0.0);

    for py in y0..y1 {
        let dy_c = (py - frame_y) as f32 + 0.5 - cy_box;
        for px in x0..x1 {
            let dx_c = (px - frame_x) as f32 + 0.5 - cx_box;
            // Inverse-rotate sample point into shape-local coordinates
            let (lx, ly) = if rotation_deg.abs() > 0.01 {
                (
                    cx_box + dx_c * cos_r + dy_c * sin_r,
                    cy_box - dx_c * sin_r + dy_c * cos_r,
                )
            } else {
                (dx_c + cx_box, dy_c + cy_box)
            };

            if kind == FreeformShapeKind::Line {
                let half_t = sw_eff.max(2.0) * 0.5;
                let d = point_to_segment_dist(lx, ly, 0.0, cy_box, fw, cy_box);
                if d <= half_t {
                    let line_col = stroke_rgba.unwrap_or(fill_rgba);
                    let dst = sheet.get_pixel_mut(px as u32, py as u32);
                    blend_pixel_over(&mut dst.0, line_col, opacity);
                }
                continue;
            }

            let (inside, on_stroke) = match kind {
                FreeformShapeKind::Rectangle => {
                    let ins = point_in_rounded_rect(lx, ly, fw, fh, r);
                    let stk = if ins && has_stroke {
                        let iw = (fw - sw_eff * 2.0).max(0.0);
                        let ih = (fh - sw_eff * 2.0).max(0.0);
                        let ir = (r - sw_eff).max(0.0);
                        iw <= 0.0
                            || ih <= 0.0
                            || !point_in_rounded_rect(lx - sw_eff, ly - sw_eff, iw, ih, ir)
                    } else {
                        false
                    };
                    (ins, stk)
                }
                FreeformShapeKind::Circle => {
                    let rx = (fw * 0.5).max(0.5);
                    let ry = (fh * 0.5).max(0.5);
                    let nx = (lx - rx) / rx;
                    let ny = (ly - ry) / ry;
                    let ins = nx * nx + ny * ny <= 1.0;
                    let stk = if ins && has_stroke {
                        let irx = (rx - sw_eff).max(0.0);
                        let iry = (ry - sw_eff).max(0.0);
                        if irx <= 0.5 || iry <= 0.5 {
                            true
                        } else {
                            let inx = (lx - rx) / irx;
                            let iny = (ly - ry) / iry;
                            inx * inx + iny * iny > 1.0
                        }
                    } else {
                        false
                    };
                    (ins, stk)
                }
                FreeformShapeKind::Triangle
                | FreeformShapeKind::Diamond
                | FreeformShapeKind::Star => {
                    let ins = point_in_polygon(lx, ly, &poly);
                    let stk = ins
                        && has_stroke
                        && min_dist_to_polygon_edges(lx, ly, &poly) <= sw_eff;
                    (ins, stk)
                }
                FreeformShapeKind::Line => (false, false),
            };

            if inside {
                let dst = sheet.get_pixel_mut(px as u32, py as u32);
                if on_stroke && let Some(sc) = stroke_rgba {
                    blend_pixel_over(&mut dst.0, sc, opacity);
                } else if fill_rgba[3] > 0 {
                    blend_pixel_over(&mut dst.0, fill_rgba, opacity);
                }
            }
        }
    }
}

/// Rasterizes a Freeform text box onto `sheet` at the sheet DPI so printed/PDF output matches canvas.
pub fn composite_text_onto_sheet(
    sheet: &mut RgbaImage,
    text: &str,
    font_size_pt: f32,
    font_family: &str,
    text_rgb: [u8; 3],
    align: TextAlignMode,
    bold: bool,
    italic: bool,
    underline: bool,
    line_height: f32,
    bg_rgba: Option<[u8; 4]>,
    opacity: f32,
    dpi: u32,
    frame_x: i32,
    frame_y: i32,
    frame_w: u32,
    frame_h: u32,
    padding_px: f32,
) {
    let opacity = opacity.clamp(0.0, 1.0);
    if frame_w == 0 || frame_h == 0 || opacity <= 0.001 {
        return;
    }

    let clip_max_x = (frame_x + frame_w as i32).min(sheet.width() as i32);
    let clip_max_y = (frame_y + frame_h as i32).min(sheet.height() as i32);
    let clip_min_x = frame_x.max(0);
    let clip_min_y = frame_y.max(0);

    if let Some(bg) = bg_rgba
        && bg[3] > 0
        && clip_min_x < clip_max_x
        && clip_min_y < clip_max_y
    {
        for py in clip_min_y..clip_max_y {
            for px in clip_min_x..clip_max_x {
                let dst = sheet.get_pixel_mut(px as u32, py as u32);
                blend_pixel_over(&mut dst.0, bg, opacity);
            }
        }
    }

    if text.trim().is_empty() {
        return;
    }

    let font_px = (font_size_pt.clamp(6.0, 300.0) * (dpi.max(72) as f32 / 72.0)).max(4.0);
    let line_height_px = (font_px * line_height.clamp(0.9, 2.5)).max(4.0);
    let pad = padding_px.max(0.0).min((frame_w.min(frame_h) as f32) * 0.25);
    let inner_w = ((frame_w as f32) - pad * 2.0).max(4.0);
    let inner_h = ((frame_h as f32) - pad * 2.0).max(4.0);

    let origin_x = frame_x + pad.round() as i32;
    let origin_y = frame_y + pad.round() as i32;

    let cosmic_align = match align {
        TextAlignMode::Left => CosmicAlign::Left,
        TextAlignMode::Center => CosmicAlign::Center,
        TextAlignMode::Right => CosmicAlign::Right,
        TextAlignMode::Justified => CosmicAlign::Justified,
    };

    with_text_engine(|font_system, swash_cache| {
        let mut buffer = Buffer::new(font_system, Metrics::new(font_px, line_height_px));
        for line in &mut buffer.lines {
            line.set_align(Some(cosmic_align));
        }
        let mut borrowed = buffer.borrow_with(font_system);
        borrowed.set_size(Some(inner_w), Some(inner_h));
        borrowed.set_wrap(Wrap::WordOrGlyph);
        let attrs = Attrs::new()
            .family(resolve_cosmic_family(font_family))
            .weight(if bold {
                CosmicWeight::BOLD
            } else {
                CosmicWeight::NORMAL
            })
            .style(if italic {
                CosmicStyle::Italic
            } else {
                CosmicStyle::Normal
            });
        borrowed.set_text(text, &attrs, Shaping::Advanced, Some(cosmic_align));
        borrowed.shape_until_scroll(false);

        borrowed.draw(
            swash_cache,
            CosmicColor::rgb(text_rgb[0], text_rgb[1], text_rgb[2]),
            |px_x, px_y, w, h, color| {
                let src_a = ((color.a() as f32) * opacity).round().clamp(0.0, 255.0) as u32;
                if src_a == 0 {
                    return;
                }
                let x0 = origin_x + px_x;
                let y0 = origin_y + px_y;
                let x1 = (x0 + w as i32).min(clip_max_x);
                let y1 = (y0 + h as i32).min(clip_max_y);
                let sx0 = x0.max(clip_min_x);
                let sy0 = y0.max(clip_min_y);
                if sx0 >= x1 || sy0 >= y1 {
                    return;
                }
                let sr = color.r() as u32;
                let sg = color.g() as u32;
                let sb = color.b() as u32;
                let inv_a = 255 - src_a;
                for py in sy0..y1 {
                    for px in sx0..x1 {
                        let dst = sheet.get_pixel_mut(px as u32, py as u32);
                        dst.0[0] = ((sr * src_a + dst.0[0] as u32 * inv_a) / 255) as u8;
                        dst.0[1] = ((sg * src_a + dst.0[1] as u32 * inv_a) / 255) as u8;
                        dst.0[2] = ((sb * src_a + dst.0[2] as u32 * inv_a) / 255) as u8;
                        dst.0[3] = 255;
                    }
                }
            },
        );

        if underline {
            let ul_thick = (font_px * 0.065).round().max(1.0) as i32;
            let ul_rgba = [text_rgb[0], text_rgb[1], text_rgb[2], 255];
            for run in borrowed.layout_runs() {
                if run.glyphs.is_empty() {
                    continue;
                }
                let first_x = run.glyphs.first().map(|g| g.x).unwrap_or(0.0);
                let last_end = run
                    .glyphs
                    .last()
                    .map(|g| g.x + g.w)
                    .unwrap_or(first_x + run.line_w);
                let ux0 = (origin_x + first_x.floor() as i32).max(clip_min_x);
                let ux1 = (origin_x + last_end.ceil() as i32).min(clip_max_x);
                let uy0 = (origin_y + (run.line_y + font_px * 0.12).round() as i32).max(clip_min_y);
                let uy1 = (uy0 + ul_thick).min(clip_max_y);
                if ux0 < ux1 && uy0 < uy1 {
                    for py in uy0..uy1 {
                        for px in ux0..ux1 {
                            let dst = sheet.get_pixel_mut(px as u32, py as u32);
                            blend_pixel_over(&mut dst.0, ul_rgba, opacity);
                        }
                    }
                }
            }
        }
    });
}

/// Exports an RgbaImage sheet to either a PDF 1.4 file (with exact physical paper dimensions in points)
/// or a PNG image file if the path ends with `.png`.
#[allow(dead_code)]
pub fn save_sheet_to_pdf_or_png(
    sheet: &RgbaImage,
    width_mm: f32,
    height_mm: f32,
    output_path: &Path,
) -> Result<(), String> {
    save_sheet_to_pdf_with_offset(sheet, width_mm, height_mm, 0.0, 0.0, output_path)
}

/// Saves the canvas sheet to a PDF file (or PNG if the extension is `.png`) with millimeter offset calibration.
pub fn save_sheet_to_pdf_with_offset(
    sheet: &RgbaImage,
    width_mm: f32,
    height_mm: f32,
    offset_x_mm: f32,
    offset_y_mm: f32,
    output_path: &Path,
) -> Result<(), String> {
    save_cropped_sheet_to_pdf(
        sheet,
        width_mm,
        height_mm,
        0.0,
        0.0,
        width_mm,
        height_mm,
        offset_x_mm,
        offset_y_mm,
        output_path,
    )
}

/// Saves the cropped canvas area directly to a PDF file using a bounding box (like Kayeview)
/// so compression and printing are super fast without processing the entire white page area.
pub fn save_cropped_sheet_to_pdf(
    sheet: &RgbaImage,
    page_w_mm: f32,
    page_h_mm: f32,
    crop_x_mm: f32,
    crop_y_mm: f32,
    crop_w_mm: f32,
    crop_h_mm: f32,
    offset_x_mm: f32,
    offset_y_mm: f32,
    output_path: &Path,
) -> Result<(), String> {
    let is_png = output_path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("png"));

    if is_png {
        return sheet
            .save(output_path)
            .map_err(|e| format!("Gagal menyimpan PNG: {e}"));
    }

    // Fast direct conversion of RGBA to RGB buffer without cloning DynamicImage
    let px_w = sheet.width();
    let px_h = sheet.height();
    let pixel_count = (px_w as usize) * (px_h as usize);
    let mut rgb = Vec::with_capacity(pixel_count * 3);
    for chunk in sheet.as_raw().chunks_exact(4) {
        rgb.push(chunk[0]);
        rgb.push(chunk[1]);
        rgb.push(chunk[2]);
    }

    let mut jpeg_bytes = Vec::with_capacity(pixel_count / 3);
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg_bytes, 92);
    image::ImageEncoder::write_image(encoder, &rgb, px_w, px_h, image::ExtendedColorType::Rgb8)
        .map_err(|e| format!("Gagal mengenkode JPEG untuk PDF: {e}"))?;
    drop(rgb);

    let mm_to_pt = 72.0 / 25.4;
    let page_w_pt = page_w_mm * mm_to_pt;
    let page_h_pt = page_h_mm * mm_to_pt;
    let crop_x_pt = crop_x_mm * mm_to_pt;
    let crop_y_pt = crop_y_mm * mm_to_pt;
    let crop_w_pt = crop_w_mm * mm_to_pt;
    let crop_h_pt = crop_h_mm * mm_to_pt;

    write_single_page_jpeg_pdf_crop(
        output_path,
        &jpeg_bytes,
        px_w,
        px_h,
        page_w_pt,
        page_h_pt,
        crop_x_pt,
        crop_y_pt,
        crop_w_pt,
        crop_h_pt,
        offset_x_mm,
        offset_y_mm,
    )
    .map_err(|e| format!("Gagal menulis PDF: {e}"))
}

pub fn write_single_page_jpeg_pdf_crop(
    path: &Path,
    jpeg_data: &[u8],
    px_w: u32,
    px_h: u32,
    page_w_pt: f32,
    page_h_pt: f32,
    crop_x_pt: f32,
    crop_y_pt: f32,
    crop_w_pt: f32,
    crop_h_pt: f32,
    offset_x_mm: f32,
    offset_y_mm: f32,
) -> std::io::Result<()> {
    let file = File::create(path)?;
    let mut out = BufWriter::new(file);
    let mut offsets = Vec::with_capacity(6);
    let mut pos: usize = 0;

    macro_rules! pdf_write {
        ($bytes:expr) => {{
            let b: &[u8] = $bytes;
            out.write_all(b)?;
            pos += b.len();
        }};
    }

    pdf_write!(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");

    // 1: Catalog
    offsets.push(pos);
    pdf_write!(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    // 2: Pages
    offsets.push(pos);
    pdf_write!(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    // 3: Page
    offsets.push(pos);
    let page_obj = format!(
        "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.2} {:.2}] /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>\nendobj\n",
        page_w_pt, page_h_pt
    );
    pdf_write!(page_obj.as_bytes());

    // 4: Image XObject
    offsets.push(pos);
    let img_header = format!(
        "4 0 obj\n<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
        px_w,
        px_h,
        jpeg_data.len()
    );
    pdf_write!(img_header.as_bytes());
    pdf_write!(jpeg_data);
    pdf_write!(b"\nendstream\nendobj\n");

    // 5: Page Content Stream with physical offset compensation and exact bounding-box matrix placement
    let off_x_pt = (offset_x_mm / 25.4) * 72.0;
    let off_y_pt = (-offset_y_mm / 25.4) * 72.0;
    let trans_x = crop_x_pt + off_x_pt;
    let trans_y = (page_h_pt - crop_y_pt - crop_h_pt) + off_y_pt;
    offsets.push(pos);
    let content_stream = format!(
        "q {:.4} 0 0 {:.4} {:.4} {:.4} cm /Im0 Do Q\n",
        crop_w_pt, crop_h_pt, trans_x, trans_y
    );
    let content_obj = format!(
        "5 0 obj\n<< /Length {} >>\nstream\n{}endstream\nendobj\n",
        content_stream.len(),
        content_stream
    );
    pdf_write!(content_obj.as_bytes());

    // XRef table
    let xref_pos = pos;
    let xref_header = format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1);
    out.write_all(xref_header.as_bytes())?;
    for off in &offsets {
        let line = format!("{:010} 00000 n \n", off);
        out.write_all(line.as_bytes())?;
    }

    let trailer = format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
        offsets.len() + 1,
        xref_pos
    );
    out.write_all(trailer.as_bytes())?;
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_composite_preserves_aspect_ratio_cover() {
        let mut sheet = RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));
        // 200x100 source image (aspect 2:1)
        let src =
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(200, 100, Rgba([10, 20, 30, 255])));

        // Composite onto 50x50 frame with Cover
        composite_item_onto_sheet(
            &mut sheet,
            &src,
            10,
            10,
            50,
            50,
            0.0,
            0.0,
            1.0,
            FitMode::Cover,
            false,
            0.0,
            None,
            false,
            1.0,
            1.0,
        );

        // Center pixel inside frame must match src color
        let p = sheet.get_pixel(35, 35);
        assert_eq!(*p, Rgba([10, 20, 30, 255]));

        // Outside frame remains white background
        assert_eq!(*sheet.get_pixel(5, 5), Rgba([255, 255, 255, 255]));
    }

    #[test]
    fn test_composite_rounded_corners_clips() {
        let mut sheet = RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));
        let src =
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(100, 100, Rgba([10, 20, 30, 255])));

        composite_item_onto_sheet(
            &mut sheet,
            &src,
            0,
            0,
            100,
            100,
            0.0,
            0.0,
            1.0,
            FitMode::Stretch,
            false,
            20.0,
            None,
            false,
            1.0,
            1.0,
        );

        // Center must be filled
        assert_eq!(*sheet.get_pixel(50, 50), Rgba([10, 20, 30, 255]));

        // Top-left corner (0, 0) is clipped and must remain white background
        assert_eq!(*sheet.get_pixel(0, 0), Rgba([255, 255, 255, 255]));
        assert_eq!(*sheet.get_pixel(1, 1), Rgba([255, 255, 255, 255]));

        // Point inside the rounded region is filled
        assert_eq!(*sheet.get_pixel(20, 20), Rgba([10, 20, 30, 255]));
    }

    #[test]
    fn test_benchmark_two_large_photos() {
        let big = DynamicImage::ImageRgba8(RgbaImage::from_pixel(
            4000,
            3000,
            Rgba([120, 150, 180, 255]),
        ));
        let mut sheet = RgbaImage::from_pixel(2600, 1800, Rgba([255, 255, 255, 255]));

        composite_item_onto_sheet(
            &mut sheet,
            &big,
            0,
            0,
            1200,
            1600,
            0.0,
            0.0,
            1.0,
            FitMode::Stretch,
            false,
            0.0,
            None,
            false,
            1.0,
            1.0,
        );
        composite_item_onto_sheet(
            &mut sheet,
            &big,
            1250,
            0,
            1300,
            1700,
            0.0,
            0.0,
            1.0,
            FitMode::Stretch,
            false,
            0.0,
            None,
            false,
            1.0,
            1.0,
        );

        assert_eq!(*sheet.get_pixel(100, 100), Rgba([120, 150, 180, 255]));
        assert_eq!(*sheet.get_pixel(1300, 100), Rgba([120, 150, 180, 255]));

        let tmp = std::env::temp_dir().join(format!("bench_out_{}.pdf", std::process::id()));
        let res =
            save_cropped_sheet_to_pdf(&sheet, 210.0, 297.0, 0.0, 0.0, 210.0, 140.0, 0.0, 0.0, &tmp);
        assert!(res.is_ok());
        assert!(tmp.exists());
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_save_sheet_to_pdf_with_offset() {
        let sheet = RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));
        let tmp = std::env::temp_dir().join(format!("test_offset_{}.pdf", std::process::id()));
        let res = save_sheet_to_pdf_with_offset(&sheet, 100.0, 100.0, 2.5, -1.5, &tmp);
        assert!(res.is_ok());
        assert!(tmp.exists());
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_composite_with_bg_color_and_alpha() {
        let mut sheet = RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));
        let src = DynamicImage::ImageRgba8(RgbaImage::from_pixel(50, 50, Rgba([0, 0, 0, 0])));
        let red_bg = [220, 38, 38, 255];

        composite_item_onto_sheet(
            &mut sheet,
            &src,
            10,
            10,
            50,
            50,
            0.0,
            0.0,
            1.0,
            FitMode::Stretch,
            false,
            0.0,
            Some(red_bg),
            false,
            1.0,
            1.0,
        );

        assert_eq!(*sheet.get_pixel(25, 25), Rgba([220, 38, 38, 255]));
    }

    #[test]
    fn test_composite_two_items_cutline() {
        let mut sheet = RgbaImage::from_pixel(300, 150, Rgba([255, 255, 255, 255]));
        let src1 =
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(100, 100, Rgba([200, 50, 50, 255])));
        let src2 =
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(100, 100, Rgba([50, 50, 200, 255])));

        // Item 1: at (10, 10), 100x100, rounded 20.0, cut_line: true
        composite_item_onto_sheet(
            &mut sheet,
            &src1,
            10,
            10,
            100,
            100,
            0.0,
            0.0,
            1.0,
            FitMode::Stretch,
            false,
            20.0,
            None,
            true,
            1.0,
            1.0,
        );

        // Item 2: at (120, 10), 100x100, rounded 0.0, cut_line: true
        composite_item_onto_sheet(
            &mut sheet,
            &src2,
            120,
            10,
            100,
            100,
            0.0,
            0.0,
            1.0,
            FitMode::Stretch,
            false,
            0.0,
            None,
            true,
            1.0,
            1.0,
        );

        let cut_color = Rgba([0, 0, 0, 255]);
        // Item 1 top cutline
        assert_eq!(*sheet.get_pixel(50, 9), cut_color);
        // Item 1 corner at (10, 10) is clipped rounded (white background, not cutline)
        assert_eq!(*sheet.get_pixel(10, 10), Rgba([255, 255, 255, 255]));
        // Item 2 top cutline
        assert_eq!(*sheet.get_pixel(170, 9), cut_color);
        // Item 2 left cutline
        assert_eq!(*sheet.get_pixel(119, 50), cut_color);
        // Item 2 outer corner is rectangular cutline
        assert_eq!(*sheet.get_pixel(119, 9), cut_color);
    }

    #[test]
    fn test_fitted_image_geometry_clipping_mask_pan() {
        // Landscape image (800x400) placed in portrait frame (300x400) with Cover fit mode
        let (dw, dh, ox, oy) = compute_fitted_image_geometry(
            300.0,
            400.0,
            800.0,
            400.0,
            FitMode::Cover,
            1.0,
            0.0,
            0.0,
        );
        // Image scale is max(300/800, 400/400) = 1.0
        assert_eq!(dw, 800.0);
        assert_eq!(dh, 400.0);
        // Centered horizontally: (300 - 800) / 2 = -250
        assert_eq!(ox, -250.0);
        assert_eq!(oy, 0.0);

        // Pan to right by 20% of frame width (+60px)
        let (_, _, ox_panned, _) = compute_fitted_image_geometry(
            300.0,
            400.0,
            800.0,
            400.0,
            FitMode::Cover,
            1.0,
            0.2,
            0.0,
        );
        assert_eq!(ox_panned, -190.0);
    }

    #[test]
    fn test_composite_text_onto_sheet_draws_within_bounds() {
        let mut sheet = RgbaImage::from_pixel(400, 200, Rgba([255, 255, 255, 255]));
        composite_text_onto_sheet(
            &mut sheet,
            "Hello KayePrint",
            24.0,
            "",
            [17, 24, 39],
            TextAlignMode::Left,
            true,
            false,
            true,
            1.25,
            Some([250, 240, 200, 255]),
            1.0,
            300,
            20,
            20,
            360,
            160,
            6.0,
        );
        let has_ink = (20..380)
            .flat_map(|x| (20..180).map(move |y| (x, y)))
            .any(|(x, y)| sheet.get_pixel(x, y).0[0] < 150);
        assert!(has_ink, "expected rasterized text pixels inside frame");
        assert_eq!(sheet.get_pixel(21, 21).0, [250, 240, 200, 255]);
        assert_eq!(sheet.get_pixel(5, 5).0, [255, 255, 255, 255]);

        let (w_short, h_short) = measure_text_box_size_px(
            "Hi",
            "Placeholder",
            24.0,
            "",
            false,
            false,
            1.25,
            300,
            None,
            2000.0,
        );
        let (w_long, _) = measure_text_box_size_px(
            "Hello KayePrint Wide Text",
            "Placeholder",
            24.0,
            "",
            false,
            false,
            1.25,
            300,
            None,
            2000.0,
        );
        let (w_bigger, h_bigger) = measure_text_box_size_px(
            "Hi",
            "Placeholder",
            48.0,
            "",
            false,
            false,
            1.25,
            300,
            None,
            2000.0,
        );
        assert!(w_long > w_short, "longer text should widen auto frame");
        assert!(w_bigger > w_short && h_bigger > h_short, "bigger font should expand auto frame");
    }

    #[test]
    fn test_star_polygon_fills_exact_bounding_box() {
        let verts = freeform_shape_polygon(FreeformShapeKind::Star, 100.0, 80.0);
        assert_eq!(verts.len(), 10);
        let min_x = verts.iter().map(|v| v.0).fold(f32::INFINITY, f32::min);
        let max_x = verts.iter().map(|v| v.0).fold(f32::NEG_INFINITY, f32::max);
        let min_y = verts.iter().map(|v| v.1).fold(f32::INFINITY, f32::min);
        let max_y = verts.iter().map(|v| v.1).fold(f32::NEG_INFINITY, f32::max);
        assert!(min_x.abs() < 1e-3);
        assert!((max_x - 100.0).abs() < 1e-3);
        assert!(min_y.abs() < 1e-3);
        assert!((max_y - 80.0).abs() < 1e-3);
    }

    #[test]
    fn test_composite_shape_stroke_line_and_opacity() {
        let mut sheet = RgbaImage::from_pixel(120, 120, Rgba([255, 255, 255, 255]));
        composite_shape_onto_sheet(
            &mut sheet,
            FreeformShapeKind::Rectangle,
            [0, 0, 255, 255],
            Some([255, 0, 0, 255]),
            6.0,
            0.0,
            1.0,
            10,
            10,
            100,
            100,
            0.0,
        );
        assert_eq!(*sheet.get_pixel(12, 50), Rgba([255, 0, 0, 255]));
        assert_eq!(*sheet.get_pixel(60, 60), Rgba([0, 0, 255, 255]));

        let mut line_sheet = RgbaImage::from_pixel(120, 60, Rgba([255, 255, 255, 255]));
        composite_shape_onto_sheet(
            &mut line_sheet,
            FreeformShapeKind::Line,
            [0, 0, 0, 255],
            Some([0, 0, 0, 255]),
            6.0,
            0.0,
            0.5,
            10,
            10,
            100,
            40,
            0.0,
        );
        // Midpoint of horizontal line at y = 10 + 20 = 30 blended 50% black over white (~127)
        let mid = line_sheet.get_pixel(60, 30).0[0];
        assert!((120..=135).contains(&mid), "expected ~50% blended gray, got {mid}");
        assert_eq!(*line_sheet.get_pixel(60, 12), Rgba([255, 255, 255, 255]));
    }
}
