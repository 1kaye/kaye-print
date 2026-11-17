pub mod model;
pub mod platform_unix;
pub mod platform_windows;
pub mod print_dialog;

#[allow(unused_imports)]
pub use model::{LinuxPrinterConfig, PpdOptionChoice, PrinterHardwareOptions, PrinterModalDraft};
#[allow(unused_imports)]
pub use print_dialog::{open_printer_settings_dialog, PrinterModalController};

use image::RgbaImage;
use crate::shared::paper::PaperPreset;

/// Returns the list of available printers and the default printer on the current platform.
pub fn get_printers() -> (Vec<String>, String) {
    #[cfg(windows)]
    {
        platform_windows::get_printers()
    }
    #[cfg(not(windows))]
    {
        platform_unix::get_printers()
    }
}

/// Opens the printer properties settings dialog on the current platform.
#[allow(dead_code)]
pub fn open_printer_properties(printer: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        platform_windows::open_printer_properties(printer)
    }
    #[cfg(not(windows))]
    {
        platform_unix::open_printer_properties(printer)
    }
}

/// Checks whether the selected printer is active, powered on, and connected.
pub fn check_printer_ready(printer: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        platform_windows::check_printer_ready(printer)
    }
    #[cfg(not(windows))]
    {
        platform_unix::check_printer_ready(printer)
    }
}

/// Prints the canvas sheet directly to the selected printer.
#[allow(dead_code)]
pub fn print_sheet(
    printer: &str,
    sheet: &RgbaImage,
    width_mm: f32,
    height_mm: f32,
    is_grayscale: bool,
    paper_preset: PaperPreset,
    config: &LinuxPrinterConfig,
) -> Result<(), String> {
    print_cropped_sheet(
        printer,
        sheet,
        width_mm,
        height_mm,
        0.0,
        0.0,
        width_mm,
        height_mm,
        is_grayscale,
        paper_preset,
        config,
    )
}

/// Prints the canvas sheet using bounding-box cropping (like Kayeview) for maximum speed.
pub fn print_cropped_sheet(
    printer: &str,
    sheet: &RgbaImage,
    page_w_mm: f32,
    page_h_mm: f32,
    crop_x_mm: f32,
    crop_y_mm: f32,
    crop_w_mm: f32,
    crop_h_mm: f32,
    is_grayscale: bool,
    paper_preset: PaperPreset,
    config: &LinuxPrinterConfig,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        platform_windows::print_cropped_sheet(
            printer,
            sheet,
            page_w_mm,
            page_h_mm,
            crop_x_mm,
            crop_y_mm,
            crop_w_mm,
            crop_h_mm,
            is_grayscale,
            paper_preset,
            config,
        )
    }
    #[cfg(not(windows))]
    {
        platform_unix::print_cropped_sheet(
            printer,
            sheet,
            page_w_mm,
            page_h_mm,
            crop_x_mm,
            crop_y_mm,
            crop_w_mm,
            crop_h_mm,
            is_grayscale,
            paper_preset,
            config,
        )
    }
}
