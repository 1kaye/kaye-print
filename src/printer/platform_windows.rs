use image::RgbaImage;
use crate::{
    printer::model::LinuxPrinterConfig,
    shared::paper::PaperPreset,
};

#[cfg(windows)]
mod win_impl {
    use super::*;
    use std::collections::HashMap;
    use std::ffi::c_void;
    use std::ptr::{null, null_mut};
    use std::sync::{LazyLock, Mutex};

    static SAVED_DEVMODES: LazyLock<Mutex<HashMap<String, Vec<u8>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    #[repr(C)]
    struct PRINTER_INFO_4W {
        p_printer_name: *mut u16,
        p_server_name: *mut u16,
        attributes: u32,
    }

    #[repr(C)]
    struct DOCINFOW {
        cb_size: i32,
        lpsz_doc_name: *const u16,
        lpsz_output: *const u16,
        lpsz_datatype: *const u16,
        fw_type: u32,
    }

    #[repr(C)]
    struct BITMAPINFOHEADER {
        bi_size: u32,
        bi_width: i32,
        bi_height: i32,
        bi_planes: u16,
        bi_bit_count: u16,
        bi_compression: u32,
        bi_size_image: u32,
        bi_x_pels_per_meter: i32,
        bi_y_pels_per_meter: i32,
        bi_clr_used: u32,
        bi_clr_important: u32,
    }

    #[link(name = "winspool")]
    unsafe extern "system" {
        fn EnumPrintersW(
            flags: u32,
            name: *const u16,
            level: u32,
            p_printer_enum: *mut u8,
            cb_buf: u32,
            pcb_needed: *mut u32,
            pc_returned: *mut u32,
        ) -> i32;
        fn GetDefaultPrinterW(psz_buffer: *mut u16, pcch_buffer: *mut u32) -> i32;
        fn OpenPrinterW(
            p_printer_name: *const u16,
            ph_printer: *mut isize,
            p_default: *const c_void,
        ) -> i32;
        fn DocumentPropertiesW(
            hwnd: isize,
            h_printer: isize,
            p_device_name: *const u16,
            p_dev_mode_output: *mut u8,
            p_dev_mode_input: *const u8,
            f_mode: u32,
        ) -> i32;
        fn DeviceCapabilitiesW(
            p_device: *const u16,
            p_port: *const u16,
            fw_capability: u16,
            p_output: *mut u8,
            p_dev_mode: *const u8,
        ) -> i32;
        fn ClosePrinter(h_printer: isize) -> i32;
    }

    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn CreateDCW(
            pwsz_driver: *const u16,
            pwsz_device: *const u16,
            psz_port: *const u16,
            pdm: *const u8,
        ) -> isize;
        fn DeleteDC(hdc: isize) -> i32;
        fn GetDeviceCaps(hdc: isize, index: i32) -> i32;
        fn StartDocW(hdc: isize, lpdi: *const DOCINFOW) -> i32;
        fn EndDoc(hdc: isize) -> i32;
        fn AbortDoc(hdc: isize) -> i32;
        fn StartPage(hdc: isize) -> i32;
        fn EndPage(hdc: isize) -> i32;
        fn SetStretchBltMode(hdc: isize, mode: i32) -> i32;
        fn SetBrushOrgEx(hdc: isize, x: i32, y: i32, lppt: *mut c_void) -> i32;
        fn StretchDIBits(
            hdc: isize,
            x_dest: i32,
            y_dest: i32,
            dest_width: i32,
            dest_height: i32,
            x_src: i32,
            y_src: i32,
            src_width: i32,
            src_height: i32,
            lp_bits: *const u8,
            lpbmi: *const BITMAPINFOHEADER,
            i_usage: u32,
            dw_rop: u32,
        ) -> i32;
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetForegroundWindow() -> isize;
    }

    unsafe fn wstr_to_string(ptr: *const u16) -> String {
        if ptr.is_null() {
            return String::new();
        }
        let mut len = 0;
        unsafe {
            while *ptr.add(len) != 0 {
                len += 1;
            }
            String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len))
        }
    }

    fn to_wstr(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn get_printers() -> (Vec<String>, String) {
        unsafe {
            let mut needed = 0u32;
            let mut returned = 0u32;
            let _ = EnumPrintersW(6, null(), 4, null_mut(), 0, &mut needed, &mut returned);
            let mut printers = Vec::new();
            if needed > 0 {
                let mut buf = vec![0u8; needed as usize];
                if EnumPrintersW(
                    6,
                    null(),
                    4,
                    buf.as_mut_ptr(),
                    needed,
                    &mut needed,
                    &mut returned,
                ) != 0
                {
                    let infos = std::slice::from_raw_parts(
                        buf.as_ptr() as *const PRINTER_INFO_4W,
                        returned as usize,
                    );
                    for info in infos {
                        let name = wstr_to_string(info.p_printer_name);
                        if !name.is_empty() {
                            printers.push(name);
                        }
                    }
                }
            }

            let mut def_len = 0u32;
            let _ = GetDefaultPrinterW(null_mut(), &mut def_len);
            let mut default_name = String::new();
            if def_len > 0 {
                let mut def_buf = vec![0u16; def_len as usize];
                if GetDefaultPrinterW(def_buf.as_mut_ptr(), &mut def_len) != 0 {
                    default_name = wstr_to_string(def_buf.as_ptr());
                }
            }
            if default_name.is_empty() && !printers.is_empty() {
                default_name = printers[0].clone();
            }
            (printers, default_name)
        }
    }

    unsafe fn match_driver_paper(
        printer_w: *const u16,
        short_mm: i16,
        long_mm: i16,
        default_code: i16,
    ) -> i16 {
        unsafe {
            let count = DeviceCapabilitiesW(printer_w, null(), 2, null_mut(), null());
            if count > 0 {
                let mut ids = vec![0u16; count as usize];
                let mut sizes = vec![0i32; (count as usize) * 2];
                if DeviceCapabilitiesW(printer_w, null(), 2, ids.as_mut_ptr() as *mut u8, null())
                    > 0
                    && DeviceCapabilitiesW(
                        printer_w,
                        null(),
                        3,
                        sizes.as_mut_ptr() as *mut u8,
                        null(),
                    ) > 0
                {
                    if default_code != 256 && ids.contains(&(default_code as u16)) {
                        return default_code;
                    }
                    let mut best_id = default_code;
                    let mut best_diff = 25i32;
                    for i in 0..(count as usize) {
                        let pw = sizes[i * 2].min(sizes[i * 2 + 1]);
                        let ph = sizes[i * 2].max(sizes[i * 2 + 1]);
                        let diff = (pw - short_mm as i32).abs() + (ph - long_mm as i32).abs();
                        if diff <= 20 && diff < best_diff {
                            best_diff = diff;
                            best_id = ids[i] as i16;
                        }
                    }
                    return best_id;
                }
            }
            default_code
        }
    }

    unsafe fn get_or_init_devmode(printer: &str, h_printer: isize, printer_w: &[u16]) -> Vec<u8> {
        if let Ok(map) = SAVED_DEVMODES.lock()
            && let Some(saved) = map.get(printer)
            && saved.len() >= 200
        {
            return saved.clone();
        }
        unsafe {
            let dm_size =
                DocumentPropertiesW(0, h_printer, printer_w.as_ptr(), null_mut(), null(), 0);
            if dm_size > 0 {
                let mut buf = vec![0u8; dm_size as usize];
                if DocumentPropertiesW(
                    0,
                    h_printer,
                    printer_w.as_ptr(),
                    buf.as_mut_ptr(),
                    null(),
                    2,
                ) >= 0
                    && buf.len() >= 200
                {
                    return buf;
                }
            }
        }
        Vec::new()
    }

    unsafe fn apply_ui_to_devmode(
        printer: &str,
        h_printer: isize,
        printer_w: &[u16],
        devmode_buf: &mut Vec<u8>,
        page_w_mm: f64,
        page_h_mm: f64,
        paper_code: i16,
        color_code: i16,
    ) {
        if devmode_buf.len() < 200 {
            return;
        }
        unsafe {
            let mut fields =
                u32::from_ne_bytes(devmode_buf[72..76].try_into().unwrap()) | 0x1 | 0x2 | 0x800;

            let orient: i16 = if page_w_mm > page_h_mm { 2 } else { 1 };
            devmode_buf[76..78].copy_from_slice(&orient.to_ne_bytes());

            let short_mm = (page_w_mm.min(page_h_mm) * 10.0).round() as i16;
            let long_mm = (page_w_mm.max(page_h_mm) * 10.0).round() as i16;
            let actual_paper =
                match_driver_paper(printer_w.as_ptr(), short_mm, long_mm, paper_code);
            devmode_buf[78..80].copy_from_slice(&actual_paper.to_ne_bytes());
            if actual_paper == 256 {
                fields |= 0x4 | 0x8;
                devmode_buf[80..82].copy_from_slice(&long_mm.to_ne_bytes());
                devmode_buf[82..84].copy_from_slice(&short_mm.to_ne_bytes());
            } else {
                fields &= !(0x4 | 0x8);
            }

            devmode_buf[92..94].copy_from_slice(&color_code.to_ne_bytes());
            devmode_buf[72..76].copy_from_slice(&fields.to_ne_bytes());

            let _ = DocumentPropertiesW(
                0,
                h_printer,
                printer_w.as_ptr(),
                devmode_buf.as_mut_ptr(),
                devmode_buf.as_ptr(),
                10,
            );
            if let Ok(mut map) = SAVED_DEVMODES.lock() {
                map.insert(printer.to_string(), devmode_buf.clone());
            }
        }
    }

    pub fn open_printer_properties(printer: &str) -> Result<(), String> {
        let printer_w = to_wstr(printer);
        unsafe {
            let mut h_printer: isize = 0;
            if OpenPrinterW(printer_w.as_ptr(), &mut h_printer, null()) == 0 || h_printer == 0 {
                return Err(format!("Gagal membuka printer '{printer}'"));
            }
            let devmode_buf = get_or_init_devmode(printer, h_printer, &printer_w);
            if devmode_buf.len() < 200 {
                ClosePrinter(h_printer);
                return Err("Gagal membaca konfigurasi DEVMODE printer".into());
            }

            let mut out_buf = devmode_buf.clone();
            let hwnd = GetForegroundWindow();
            // 14 = DM_IN_PROMPT (4) | DM_IN_BUFFER (8) | DM_OUT_BUFFER (2)
            let res = DocumentPropertiesW(
                hwnd,
                h_printer,
                printer_w.as_ptr(),
                out_buf.as_mut_ptr(),
                devmode_buf.as_ptr(),
                14,
            );
            ClosePrinter(h_printer);

            if res == 1 {
                if let Ok(mut map) = SAVED_DEVMODES.lock() {
                    map.insert(printer.to_string(), out_buf);
                }
            }
            Ok(())
        }
    }

    pub fn check_printer_ready(printer: &str) -> Result<(), String> {
        if printer.is_empty() || printer == "(Tidak ada printer ditemukan)" {
            return Err("Tidak ada printer yang dipilih".into());
        }
        Ok(())
    }

    pub fn print_sheet(
        printer: &str,
        sheet: &RgbaImage,
        width_mm: f32,
        height_mm: f32,
        is_grayscale: bool,
        paper_preset: PaperPreset,
        _config: &LinuxPrinterConfig,
    ) -> Result<(), String> {
        let (width, height) = sheet.dimensions();
        let pixel_count = (width as usize) * (height as usize);
        let mut bgra = Vec::with_capacity(pixel_count * 4);

        for px in sheet.pixels() {
            if is_grayscale {
                let lum = ((px[0] as f32 * 0.299) + (px[1] as f32 * 0.587) + (px[2] as f32 * 0.114))
                    .round() as u8;
                bgra.push(lum); // B
                bgra.push(lum); // G
                bgra.push(lum); // R
                bgra.push(px[3]); // A
            } else {
                bgra.push(px[2]); // B
                bgra.push(px[1]); // G
                bgra.push(px[0]); // R
                bgra.push(px[3]); // A
            }
        }

        let paper_code: i16 = match paper_preset {
            PaperPreset::A4 => 9,
            PaperPreset::Letter => 1,
            PaperPreset::F4 => 14,
            _ => 256,
        };
        let color_code: i16 = if is_grayscale { 1 } else { 2 };

        let driver_w = to_wstr("WINSPOOL");
        let printer_w = to_wstr(printer);
        let doc_w = to_wstr("kayePrint Document");

        unsafe {
            let mut h_printer: isize = 0;
            let mut devmode_buf: Vec<u8> = Vec::new();
            if OpenPrinterW(printer_w.as_ptr(), &mut h_printer, null()) != 0 && h_printer != 0 {
                devmode_buf = get_or_init_devmode(printer, h_printer, &printer_w);
                apply_ui_to_devmode(
                    printer,
                    h_printer,
                    &printer_w,
                    &mut devmode_buf,
                    width_mm as f64,
                    height_mm as f64,
                    paper_code,
                    color_code,
                );
                ClosePrinter(h_printer);
            }

            let pdm = if devmode_buf.is_empty() {
                null()
            } else {
                devmode_buf.as_ptr()
            };

            let hdc = CreateDCW(driver_w.as_ptr(), printer_w.as_ptr(), null(), pdm);
            if hdc == 0 {
                return Err(format!("Gagal membuka device context printer '{printer}'"));
            }

            let dpi_x = GetDeviceCaps(hdc, 88).max(72) as f64;
            let dpi_y = GetDeviceCaps(hdc, 90).max(72) as f64;
            let off_x = GetDeviceCaps(hdc, 112);
            let off_y = GetDeviceCaps(hdc, 113);

            let dest_x = -off_x;
            let dest_y = -off_y;
            let dest_w = ((width_mm as f64 * dpi_x) / 25.4).round() as i32;
            let dest_h = ((height_mm as f64 * dpi_y) / 25.4).round() as i32;

            let doc_info = DOCINFOW {
                cb_size: std::mem::size_of::<DOCINFOW>() as i32,
                lpsz_doc_name: doc_w.as_ptr(),
                lpsz_output: null(),
                lpsz_datatype: null(),
                fw_type: 0,
            };

            let bmi = BITMAPINFOHEADER {
                bi_size: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                bi_width: width as i32,
                bi_height: -(height as i32),
                bi_planes: 1,
                bi_bit_count: 32,
                bi_compression: 0,
                bi_size_image: bgra.len() as u32,
                bi_x_pels_per_meter: (dpi_x * 39.3701) as i32,
                bi_y_pels_per_meter: (dpi_y * 39.3701) as i32,
                bi_clr_used: 0,
                bi_clr_important: 0,
            };

            if StartDocW(hdc, &doc_info) <= 0 {
                DeleteDC(hdc);
                return Err("Gagal memulai dokumen cetak (StartDocW)".into());
            }

            if StartPage(hdc) <= 0 {
                AbortDoc(hdc);
                DeleteDC(hdc);
                return Err("Gagal memulai halaman cetak (StartPage)".into());
            }

            SetStretchBltMode(hdc, 4);
            SetBrushOrgEx(hdc, 0, 0, null_mut());

            let stretch_res = StretchDIBits(
                hdc,
                dest_x,
                dest_y,
                dest_w,
                dest_h,
                0,
                0,
                width as i32,
                height as i32,
                bgra.as_ptr(),
                &bmi,
                0,
                0x00CC0020,
            );

            drop(bgra);

            if stretch_res <= 0 {
                AbortDoc(hdc);
                DeleteDC(hdc);
                return Err("Gagal merender gambar ke printer (StretchDIBits)".into());
            }

            if EndPage(hdc) <= 0 {
                AbortDoc(hdc);
                DeleteDC(hdc);
                return Err("Gagal menyelesaikan halaman cetak (EndPage)".into());
            }

            EndDoc(hdc);
            DeleteDC(hdc);
            Ok(())
        }
    }

    pub fn print_cropped_sheet(
        printer: &str,
        sheet: &RgbaImage,
        page_w_mm: f32,
        page_h_mm: f32,
        _crop_x_mm: f32,
        _crop_y_mm: f32,
        _crop_w_mm: f32,
        _crop_h_mm: f32,
        is_grayscale: bool,
        paper_preset: PaperPreset,
        config: &LinuxPrinterConfig,
    ) -> Result<(), String> {
        print_sheet(
            printer,
            sheet,
            page_w_mm,
            page_h_mm,
            is_grayscale,
            paper_preset,
            config,
        )
    }
}

#[cfg(windows)]
pub use win_impl::*;

#[cfg(not(windows))]
#[allow(dead_code)]
pub fn get_printers() -> (Vec<String>, String) {
    (Vec::new(), String::new())
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub fn open_printer_properties(_printer: &str) -> Result<(), String> {
    Err("Hanya didukung di Windows".into())
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub fn check_printer_ready(printer: &str) -> Result<(), String> {
    if printer.is_empty() || printer == "(Tidak ada printer ditemukan)" {
        return Err("Tidak ada printer yang dipilih".into());
    }
    Ok(())
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub fn print_sheet(
    _printer: &str,
    _sheet: &RgbaImage,
    _width_mm: f32,
    _height_mm: f32,
    _is_grayscale: bool,
    _paper_preset: PaperPreset,
    _config: &LinuxPrinterConfig,
) -> Result<(), String> {
    Err("Hanya didukung di Windows".into())
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub fn print_cropped_sheet(
    _printer: &str,
    _sheet: &RgbaImage,
    _page_w_mm: f32,
    _page_h_mm: f32,
    _crop_x_mm: f32,
    _crop_y_mm: f32,
    _crop_w_mm: f32,
    _crop_h_mm: f32,
    _is_grayscale: bool,
    _paper_preset: PaperPreset,
    _config: &LinuxPrinterConfig,
) -> Result<(), String> {
    Err("Hanya didukung di Windows".into())
}
