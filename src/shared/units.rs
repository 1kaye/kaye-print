use gpui_kit::{
    AnyElement, IntoElement as _, ParentElement as _, SharedString, Styled as _,
    component::select::SelectItem,
    div, rgb,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhysicalUnit {
    Mm,
    Cm,
}

impl PhysicalUnit {
    pub fn label(self) -> &'static str {
        match self {
            Self::Mm => "mm",
            Self::Cm => "cm",
        }
    }

    pub fn to_px(self, value: f32, dpi: u32) -> f32 {
        let dpi_f = dpi.max(72) as f32;
        match self {
            Self::Mm => (value / 25.4) * dpi_f,
            Self::Cm => ((value * 10.0) / 25.4) * dpi_f,
        }
    }

    pub fn from_px(self, px_val: f32, dpi: u32) -> f32 {
        let dpi_f = dpi.max(72) as f32;
        match self {
            Self::Mm => (px_val / dpi_f) * 25.4,
            Self::Cm => ((px_val / dpi_f) * 25.4) / 10.0,
        }
    }
}

#[inline]
pub fn mm_to_px(mm: f32, dpi: u32) -> f32 {
    PhysicalUnit::Mm.to_px(mm, dpi)
}

#[inline]
pub fn px_to_mm(px: f32, dpi: u32) -> f32 {
    PhysicalUnit::Mm.from_px(px, dpi)
}

#[inline]
pub fn cm_to_px(cm: f32, dpi: u32) -> f32 {
    PhysicalUnit::Cm.to_px(cm, dpi)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CropUnit {
    #[default]
    Px,
    Cm,
    Mm,
    In,
}

impl CropUnit {
    pub const ALL: [Self; 4] = [Self::Px, Self::Cm, Self::Mm, Self::In];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Px => "px",
            Self::Cm => "cm",
            Self::Mm => "mm",
            Self::In => "in",
        }
    }

    pub const fn index(self) -> usize {
        match self {
            Self::Px => 0,
            Self::Cm => 1,
            Self::Mm => 2,
            Self::In => 3,
        }
    }

    pub fn format_dimensions(self, px_w: f32, px_h: f32, dpi: u32) -> String {
        let dpi_f = dpi.max(72) as f32;
        match self {
            Self::Px => format!(
                "{} × {}",
                px_w.round().max(1.0) as u32,
                px_h.round().max(1.0) as u32
            ),
            Self::Cm => {
                let w = (px_w / dpi_f) * 2.54;
                let h = (px_h / dpi_f) * 2.54;
                format!("{w:.1} × {h:.1}")
            }
            Self::Mm => {
                let w = (px_w / dpi_f) * 25.4;
                let h = (px_h / dpi_f) * 25.4;
                format!("{w:.1} × {h:.1}")
            }
            Self::In => {
                let w = px_w / dpi_f;
                let h = px_h / dpi_f;
                format!("{w:.2} × {h:.2}")
            }
        }
    }
}

impl SelectItem for CropUnit {
    type Value = Self;

    fn title(&self) -> SharedString {
        SharedString::new_static(self.label())
    }

    fn display_title(&self) -> Option<AnyElement> {
        Some(
            div()
                .text_xs()
                .text_color(rgb(0xFFFFFF))
                .child(self.label())
                .into_any_element(),
        )
    }

    fn value(&self) -> &Self::Value {
        self
    }
}

/// Computes auto-flow grid positions `(x, y)` in pixels from left-to-right, wrapping rows
/// when the next shape exceeds `margin_left + printable_width`.
pub fn compute_auto_flow_grid(
    sizes: &[(f32, f32)],
    margin_left: f32,
    margin_top: f32,
    printable_width: f32,
    h_gap: f32,
    v_gap: f32,
) -> Vec<(f32, f32)> {
    let mut positions = Vec::with_capacity(sizes.len());
    let max_x = margin_left + printable_width.max(1.0);
    let mut cursor_x = margin_left;
    let mut cursor_y = margin_top;
    let mut row_height: f32 = 0.0;

    for &(w, h) in sizes {
        if cursor_x > margin_left && cursor_x + w > max_x {
            cursor_x = margin_left;
            cursor_y += row_height + v_gap.max(0.0);
            row_height = 0.0;
        }
        positions.push((cursor_x, cursor_y));
        cursor_x += w + h_gap.max(0.0);
        row_height = row_height.max(h);
    }

    positions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mm_and_cm_to_px_at_300_dpi() {
        let a4_w_px = mm_to_px(210.0, 300).round() as u32;
        let a4_h_px = mm_to_px(297.0, 300).round() as u32;
        assert_eq!(a4_w_px, 2480);
        assert_eq!(a4_h_px, 3508);

        let id_photo_3x4_w = cm_to_px(3.0, 300).round() as u32;
        let id_photo_3x4_h = cm_to_px(4.0, 300).round() as u32;
        assert_eq!(id_photo_3x4_w, 354);
        assert_eq!(id_photo_3x4_h, 472);
    }

    #[test]
    fn test_auto_flow_grid_wraps_rows() {
        let sizes = vec![(100.0, 120.0); 4];
        let pos = compute_auto_flow_grid(&sizes, 10.0, 20.0, 250.0, 15.0, 25.0);
        assert_eq!(pos[0], (10.0, 20.0));
        assert_eq!(pos[1], (125.0, 20.0));
        // 3rd item (125 + 100 + 15 + 100 = 340 > 260) wraps to row 2 (20 + 120 + 25 = 165)
        assert_eq!(pos[2], (10.0, 165.0));
        assert_eq!(pos[3], (125.0, 165.0));
    }

    #[test]
    fn test_crop_unit_format_dimensions() {
        assert_eq!(CropUnit::Px.format_dimensions(300.0, 600.0, 300), "300 × 600");
        assert_eq!(CropUnit::Cm.format_dimensions(300.0, 600.0, 300), "2.5 × 5.1");
        assert_eq!(CropUnit::Mm.format_dimensions(300.0, 600.0, 300), "25.4 × 50.8");
        assert_eq!(CropUnit::In.format_dimensions(300.0, 600.0, 300), "1.00 × 2.00");
    }
}
