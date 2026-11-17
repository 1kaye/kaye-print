use std::path::{Path, PathBuf};

use rfd::AsyncFileDialog;

use crate::shared::i18n::Language;

const SUPPORTED_IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "webp", "JPG", "JPEG", "PNG", "WEBP", "Jpg", "Jpeg", "Png", "Webp",
];

pub fn is_supported_image(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            ext.eq_ignore_ascii_case("jpg")
                || ext.eq_ignore_ascii_case("jpeg")
                || ext.eq_ignore_ascii_case("png")
                || ext.eq_ignore_ascii_case("webp")
        })
}

pub fn filter_supported_images(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths
        .into_iter()
        .filter(|p| is_supported_image(p))
        .collect()
}

pub fn prompt_open_images(
    multiple: bool,
    lang: Language,
) -> impl std::future::Future<Output = Option<Vec<PathBuf>>> {
    async move {
        let dialog = AsyncFileDialog::new()
            .set_title(if multiple {
                lang.tr("app.dialog_pick_images")
            } else {
                lang.tr("app.dialog_pick_single_image")
            })
            .add_filter(lang.tr("app.dialog_all_images"), SUPPORTED_IMAGE_EXTENSIONS)
            .add_filter(
                "JPEG / JPG (*.jpg, *.jpeg)",
                &["jpg", "jpeg", "JPG", "JPEG", "Jpg", "Jpeg"],
            )
            .add_filter("PNG (*.png)", &["png", "PNG", "Png"])
            .add_filter("WEBP (*.webp)", &["webp", "WEBP", "Webp"])
            .add_filter(lang.tr("app.dialog_all_files"), &["*"]);

        if multiple {
            let files = dialog.pick_files().await?;
            Some(files.into_iter().map(|f| f.path().to_path_buf()).collect())
        } else {
            let file = dialog.pick_file().await?;
            Some(vec![file.path().to_path_buf()])
        }
    }
}

pub fn prompt_save_export(
    suggested_name: &str,
    lang: Language,
) -> impl std::future::Future<Output = Option<PathBuf>> {
    let name = suggested_name.to_string();
    async move {
        let is_pdf = name.ends_with(".pdf");
        let dialog = AsyncFileDialog::new()
            .set_title(lang.tr("app.dialog_save_title"))
            .set_file_name(&name)
            .add_filter(
                if is_pdf {
                    lang.tr("app.dialog_pdf_filter")
                } else {
                    lang.tr("app.dialog_png_filter")
                },
                if is_pdf {
                    &["pdf", "PDF"]
                } else {
                    &["png", "PNG"]
                },
            );

        let file = dialog.save_file().await?;
        Some(file.path().to_path_buf())
    }
}
