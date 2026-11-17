use crate::shared::paper::{Orientation, PaperPreset};

#[derive(Clone, Debug, PartialEq)]
pub struct PpdOptionChoice {
    pub value: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PrinterHardwareOptions {
    pub media_types: Vec<PpdOptionChoice>,
    pub default_media_type: String,
    pub print_qualities: Vec<PpdOptionChoice>,
    pub default_print_quality: String,
    pub supports_borderless: bool,
}

impl Default for PrinterHardwareOptions {
    fn default() -> Self {
        Self {
            media_types: vec![
                PpdOptionChoice {
                    value: "PLAIN".into(),
                    label: "Kertas Biasa (Plain Paper)".into(),
                },
                PpdOptionChoice {
                    value: "PHOTOGLOSSY".into(),
                    label: "Kertas Foto Mengkilap (Glossy)".into(),
                },
                PpdOptionChoice {
                    value: "EMATTE".into(),
                    label: "Kertas Foto Matte".into(),
                },
            ],
            default_media_type: "PLAIN".into(),
            print_qualities: vec![
                PpdOptionChoice {
                    value: "Draft".into(),
                    label: "Draf (Cepat)".into(),
                },
                PpdOptionChoice {
                    value: "Standard".into(),
                    label: "Standar (Normal)".into(),
                },
                PpdOptionChoice {
                    value: "High".into(),
                    label: "Tinggi (Kualitas Foto)".into(),
                },
            ],
            default_print_quality: "Standard".into(),
            supports_borderless: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LinuxPrinterConfig {
    pub media_type: String,
    pub print_quality: String,
    pub copies: u32,
}

impl Default for LinuxPrinterConfig {
    fn default() -> Self {
        Self {
            media_type: "PLAIN".into(),
            print_quality: "Standard".into(),
            copies: 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PrinterModalDraft {
    pub paper_preset: PaperPreset,
    pub orientation: Orientation,
    pub media_type: String,
    pub print_quality: String,
    pub is_grayscale: bool,
    pub copies: u32,
    pub media_choices: Vec<PpdOptionChoice>,
}
