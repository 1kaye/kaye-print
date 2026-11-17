use std::process::Command;
use image::RgbaImage;

use crate::{
    printer::model::{LinuxPrinterConfig, PpdOptionChoice, PrinterHardwareOptions},
    shared::paper::PaperPreset,
};

fn is_usb_printer_connected() -> bool {
    if std::path::Path::new("/dev/usb/lp0").exists() {
        return true;
    }
    if let Ok(entries) = std::fs::read_dir("/dev/usb") {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with("lp") {
                return true;
            }
        }
    }
    if let Ok(entries) = std::fs::read_dir("/sys/bus/usb/devices") {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Ok(content) = std::fs::read_to_string(path.join("bInterfaceClass")) {
                if content.trim() == "07" {
                    return true;
                }
            }
            if let Ok(subentries) = std::fs::read_dir(&path) {
                for sub in subentries.flatten() {
                    let class_file = sub.path().join("bInterfaceClass");
                    if let Ok(content) = std::fs::read_to_string(class_file) {
                        if content.trim() == "07" {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

/// Checks whether the specified printer is currently active, powered on, and connected.
pub fn check_printer_ready(printer: &str) -> Result<(), String> {
    if printer.is_empty() || printer == "(Tidak ada printer ditemukan)" {
        return Err("Tidak ada printer yang dipilih".into());
    }

    // 1. Periksa status antrean CUPS via lpstat -p <printer>
    if let Ok(out) = Command::new("lpstat")
        .args(["-p", printer])
        .env("LC_ALL", "C")
        .output()
    {
        if !out.status.success() {
            return Err(format!(
                "Printer '{printer}' tidak ditemukan di sistem CUPS"
            ));
        }

        let text = format!(
            "{} {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
        .to_lowercase();

        if text.contains("unplugged")
            || text.contains("turned off")
            || text.contains("offline")
            || text.contains("not responding")
            || (text.contains("disabled") && !text.contains("idle"))
        {
            return Err(format!(
                "Printer '{printer}' sedang mati atau tidak terhubung (kabel terputus). Nyalakan printer terlebih dahulu."
            ));
        }
    }

    // 2. Jika printer menggunakan antarmuka USB, verifikasi koneksi fisik USB
    if let Ok(out) = Command::new("lpstat")
        .args(["-v", printer])
        .env("LC_ALL", "C")
        .output()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        if text.contains("usb://") && !is_usb_printer_connected() {
            let p_status = Command::new("lpstat")
                .args(["-p", printer])
                .env("LC_ALL", "C")
                .output();
            let is_cups_ok = p_status.map_or(false, |p_out| {
                let p_text = String::from_utf8_lossy(&p_out.stdout).to_lowercase();
                p_text.contains("idle") || p_text.contains("printing")
            });
            if !is_cups_ok {
                return Err(format!(
                    "Printer USB '{printer}' tidak terdeteksi (perangkat mati atau kabel USB terputus). Nyalakan printer terlebih dahulu."
                ));
            }
        }
    }

    Ok(())
}

/// Returns the list of installed CUPS printers that are active/powered on, along with the default printer.
pub fn get_printers() -> (Vec<String>, String) {
    let mut configured_printers = Vec::new();
    let mut default_name = String::new();

    if let Ok(out) = Command::new("lpstat")
        .args(["-p", "-d"])
        .env("LC_ALL", "C")
        .output()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("printer ") {
                if let Some(name) = rest.split_whitespace().next() {
                    if !name.is_empty() && !configured_printers.contains(&name.to_string()) {
                        configured_printers.push(name.to_string());
                    }
                }
            } else if let Some(def) = line.strip_prefix("system default destination:") {
                default_name = def.trim().to_string();
            }
        }
    }

    if configured_printers.is_empty() {
        if let Ok(out) = Command::new("lpstat").arg("-a").env("LC_ALL", "C").output() {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                if let Some(name) = line.split_whitespace().next() {
                    if !name.is_empty() && !configured_printers.contains(&name.to_string()) {
                        configured_printers.push(name.to_string());
                    }
                }
            }
        }
    }

    // Filter active/online printers
    let online_printers: Vec<String> = configured_printers
        .iter()
        .filter(|p| check_printer_ready(p).is_ok())
        .cloned()
        .collect();

    let final_printers = if online_printers.is_empty() {
        configured_printers
    } else {
        online_printers
    };

    let final_default = if final_printers.contains(&default_name) {
        default_name
    } else {
        final_printers.first().cloned().unwrap_or_default()
    };

    (final_printers, final_default)
}

/// Fetches printer hardware options (MediaType, PrintQuality, Borderless) via `lpoptions -p <printer> -l`.
pub fn get_printer_hardware_options(printer: &str) -> PrinterHardwareOptions {
    if printer.is_empty() || printer == "(Tidak ada printer ditemukan)" {
        return PrinterHardwareOptions::default();
    }

    let mut opts = PrinterHardwareOptions::default();

    if let Ok(out) = Command::new("lpoptions")
        .args(["-p", printer, "-l"])
        .env("LC_ALL", "C")
        .output()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        let mut parsed_media: Vec<PpdOptionChoice> = Vec::new();
        let mut parsed_quality: Vec<PpdOptionChoice> = Vec::new();
        let mut has_borderless = false;

        for line in text.lines() {
            let line = line.trim();
            if let Some((left, right)) = line.split_once(':') {
                let key = left.split('/').next().unwrap_or("").trim();
                let tokens: Vec<&str> = right.split_whitespace().collect();

                if key.eq_ignore_ascii_case("MediaType") || key.eq_ignore_ascii_case("Media") {
                    for token in tokens {
                        let (is_default, val) = if let Some(s) = token.strip_prefix('*') {
                            (true, s)
                        } else {
                            (false, token)
                        };
                        let label = match val.to_ascii_uppercase().as_str() {
                            "PLAIN" => "Kertas Biasa (Plain Paper)".into(),
                            "PHOTOGLOSSY" => "Kertas Foto Glossy".into(),
                            "EPREMGLOSS" => "Premium Glossy".into(),
                            "EPSGLOS" => "Photo Paper Glossy".into(),
                            "EMATTE" => "Kertas Foto Matte".into(),
                            "EULTRAGLOSSY" => "Ultra Glossy".into(),
                            "EPQIJ" => "Premium Inkjet".into(),
                            "ENVELOPE" => "Amplop (Envelope)".into(),
                            other => other.to_string(),
                        };
                        if is_default {
                            opts.default_media_type = val.to_string();
                        }
                        parsed_media.push(PpdOptionChoice {
                            value: val.to_string(),
                            label,
                        });
                    }
                } else if key.eq_ignore_ascii_case("PrintQuality")
                    || key.eq_ignore_ascii_case("Quality")
                {
                    for token in tokens {
                        let (is_default, val) = if let Some(s) = token.strip_prefix('*') {
                            (true, s)
                        } else {
                            (false, token)
                        };
                        let label = match val.to_ascii_lowercase().as_str() {
                            "draft" | "fast" => "Draf (Cepat)".into(),
                            "standard" | "normal" => "Standar (Normal)".into(),
                            "high" | "best" | "photo" => "Tinggi (Kualitas Foto)".into(),
                            other => other.to_string(),
                        };
                        if is_default {
                            opts.default_print_quality = val.to_string();
                        }
                        parsed_quality.push(PpdOptionChoice {
                            value: val.to_string(),
                            label,
                        });
                    }
                } else if key.eq_ignore_ascii_case("Borderless") {
                    has_borderless = true;
                }
            }
        }

        if !parsed_media.is_empty() {
            opts.media_types = parsed_media;
        }
        if !parsed_quality.is_empty() {
            opts.print_qualities = parsed_quality;
        }
        opts.supports_borderless = has_borderless;
    }

    opts
}

/// Membuka properti printer secara ringan di Linux:
/// Uses `system-config-printer` if available, or opens the CUPS web interface via `xdg-open`.
#[allow(dead_code)]
pub fn open_printer_properties(printer: &str) -> Result<(), String> {
    if std::path::Path::new("/usr/bin/system-config-printer").exists()
        || Command::new("which")
            .arg("system-config-printer")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    {
        Command::new("system-config-printer")
            .spawn()
            .map_err(|e| format!("Gagal membuka system-config-printer: {e}"))?;
        return Ok(());
    }

    // Fallback: CUPS web interface
    let url = if !printer.is_empty() {
        format!("http://localhost:631/printers/{printer}")
    } else {
        "http://localhost:631/printers/".to_string()
    };
    Command::new("xdg-open")
        .arg(&url)
        .spawn()
        .map_err(|e| format!("Gagal membuka CUPS web interface: {e}"))?;
    Ok(())
}

/// Prints the canvas sheet directly to the selected printer via CUPS `lp` with full configuration.
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
    if printer.is_empty() || printer == "(Tidak ada printer ditemukan)" {
        return Err("Tidak ada printer yang dipilih".into());
    }

    check_printer_ready(printer)?;

    // If grayscale, convert sheet pixels to grayscale luma
    let tmp_path = std::env::temp_dir().join(format!("kayeprint_job_{}.pdf", std::process::id()));

    if is_grayscale {
        let mut gray_sheet = sheet.clone();
        for px in gray_sheet.pixels_mut() {
            let lum = ((px[0] as f32 * 0.299) + (px[1] as f32 * 0.587) + (px[2] as f32 * 0.114))
                .round() as u8;
            px[0] = lum;
            px[1] = lum;
            px[2] = lum;
        }
        crate::canvas::geometry::save_cropped_sheet_to_pdf(
            &gray_sheet,
            page_w_mm,
            page_h_mm,
            crop_x_mm,
            crop_y_mm,
            crop_w_mm,
            crop_h_mm,
            0.0,
            0.0,
            &tmp_path,
        )?;
    } else {
        crate::canvas::geometry::save_cropped_sheet_to_pdf(
            sheet, page_w_mm, page_h_mm, crop_x_mm, crop_y_mm, crop_w_mm, crop_h_mm, 0.0, 0.0,
            &tmp_path,
        )?;
    }

    let width_mm = page_w_mm;
    let height_mm = page_h_mm;
    let media = match paper_preset {
        PaperPreset::A4 => "A4".to_string(),
        PaperPreset::F4 => "8.5x13in".to_string(),
        PaperPreset::FourR => "4x6in".to_string(),
        PaperPreset::FiveR => "5x7in".to_string(),
        PaperPreset::Letter => "Letter".to_string(),
        PaperPreset::Custom => format!(
            "Custom.{}x{}mm",
            width_mm.min(height_mm).round() as i32,
            width_mm.max(height_mm).round() as i32
        ),
    };

    let copies = config.copies.clamp(1, 999);
    let mut cmd = Command::new("lp");
    cmd.args([
        "-d",
        printer,
        "-t",
        "kayePrint Document",
        "-n",
        &copies.to_string(),
        "-o",
        &format!("PageSize={media}"),
        "-o",
        &format!("media={media}"),
        "-o",
        "print-scaling=none",
        "-o",
        "fit-to-page=false",
        "-o",
        "nofit-to-page",
        "-o",
        "fitplot=false",
        "-o",
        "scaling=100",
        "-o",
        "ReduceEnlarge=Off",
    ]);

    // MediaType option
    if !config.media_type.is_empty() {
        cmd.args(["-o", &format!("MediaType={}", config.media_type)]);
    }

    // PrintQuality option
    if !config.print_quality.is_empty() {
        cmd.args(["-o", &format!("PrintQuality={}", config.print_quality)]);
    }

    // Opsi Warna vs Grayscale
    if is_grayscale {
        cmd.args([
            "-o",
            "ColorModel=Gray",
            "-o",
            "Color=Grayscale",
            "-o",
            "Ink=MONO",
        ]);
    } else {
        cmd.args([
            "-o",
            "ColorModel=RGB",
            "-o",
            "Color=Color",
            "-o",
            "Ink=COLOR",
        ]);
    }

    cmd.arg(&tmp_path);
    let output = cmd.output();
    let _ = std::fs::remove_file(&tmp_path);

    let output = output.map_err(|e| format!("Gagal menjalankan CUPS `lp`: {e}"))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Gagal mencetak via CUPS: {}", err.trim()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linux_get_printers_does_not_panic() {
        let (printers, default) = get_printers();
        if !default.is_empty() {
            assert!(printers.contains(&default));
        }
    }

    #[test]
    fn test_linux_check_printer_ready_validation() {
        // Empty or dummy string must return Err
        assert!(check_printer_ready("").is_err());
        assert!(check_printer_ready("(Tidak ada printer ditemukan)").is_err());
        assert!(check_printer_ready("printer-pasti-tidak-ada-123456").is_err());
    }

    #[test]
    fn test_linux_get_hardware_options() {
        let opts = get_printer_hardware_options("L3210-Series");
        assert!(!opts.media_types.is_empty());
        assert!(!opts.print_qualities.is_empty());
    }
}
