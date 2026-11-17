use crate::shared::units::{PhysicalUnit, mm_to_px};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaperPreset {
    A4,
    F4,
    FourR,
    FiveR,
    Letter,
    Custom,
}

impl PaperPreset {
    pub const ALL: &'static [Self] = &[
        Self::A4,
        Self::F4,
        Self::FourR,
        Self::FiveR,
        Self::Letter,
        Self::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::A4 => "A4 (210×297 mm)",
            Self::F4 => "F4 (215×330 mm)",
            Self::FourR => "4R (102×152 mm)",
            Self::FiveR => "5R (127×178 mm)",
            Self::Letter => "Letter (216×279 mm)",
            Self::Custom => "Custom",
        }
    }

    pub fn base_dimensions_mm(self, custom_w_mm: f32, custom_h_mm: f32) -> (f32, f32) {
        match self {
            Self::A4 => (210.0, 297.0),
            Self::F4 => (215.0, 330.0),
            Self::FourR => (102.0, 152.0),
            Self::FiveR => (127.0, 178.0),
            Self::Letter => (215.9, 279.4),
            Self::Custom => (custom_w_mm.max(20.0), custom_h_mm.max(20.0)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
    Portrait,
    Landscape,
}

#[derive(Clone, Debug)]
pub struct PaperConfig {
    pub preset: PaperPreset,
    pub orientation: Orientation,
    pub custom_width_mm: f32,
    pub custom_height_mm: f32,
    pub unit: PhysicalUnit,
    /// Uniform safe print margin (top, bottom, left, right) in mm. Default: 6.0 mm.
    pub margin_mm: f32,
    pub dpi: u32,
}

impl Default for PaperConfig {
    fn default() -> Self {
        Self {
            preset: PaperPreset::A4,
            orientation: Orientation::Portrait,
            custom_width_mm: 210.0,
            custom_height_mm: 297.0,
            unit: PhysicalUnit::Mm,
            margin_mm: 6.0,
            dpi: 300,
        }
    }
}

impl PaperConfig {
    pub fn dimensions_mm(&self) -> (f32, f32) {
        let (w, h) = self
            .preset
            .base_dimensions_mm(self.custom_width_mm, self.custom_height_mm);
        match self.orientation {
            Orientation::Portrait => (w.min(h), w.max(h)),
            Orientation::Landscape => (w.max(h), w.min(h)),
        }
    }

    pub fn dimensions_px(&self) -> (f32, f32) {
        let (w_mm, h_mm) = self.dimensions_mm();
        (mm_to_px(w_mm, self.dpi), mm_to_px(h_mm, self.dpi))
    }

    pub fn margin_px(&self) -> f32 {
        mm_to_px(self.margin_mm, self.dpi)
    }

    pub fn printable_bounds_px(&self) -> (f32, f32, f32, f32) {
        let (pw, ph) = self.dimensions_px();
        let m = self.margin_px();
        let w = (pw - m * 2.0).max(32.0);
        let h = (ph - m * 2.0).max(32.0);
        (m, m, w, h)
    }
}
