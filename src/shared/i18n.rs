use std::borrow::Cow;

use gpui_kit::{
    AnyElement, App, IntoElement, ParentElement, SharedString, Styled,
    assets::IconName,
    component::{Icon, Sizable as _, Size, h_flex, select::SelectItem},
    px,
};

/// Default application language. Change this single constant to switch startup default.
pub const DEFAULT_LANGUAGE: Language = Language::Id;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Id,
    Eng,
}

impl Default for Language {
    fn default() -> Self {
        DEFAULT_LANGUAGE
    }
}

impl SelectItem for Language {
    type Value = Self;

    fn title(&self) -> SharedString {
        SharedString::new_static(self.display_name())
    }

    fn display_title(&self) -> Option<AnyElement> {
        Some(
            h_flex()
                .items_center()
                .gap(px(5.0))
                .child(Icon::new(IconName::Globe).with_size(Size::Small))
                .child(self.code())
                .into_any_element(),
        )
    }

    fn value(&self) -> &Self::Value {
        self
    }
}

fn cow_to_shared(cow: Cow<'static, str>) -> SharedString {
    match cow {
        Cow::Borrowed(s) => SharedString::new_static(s),
        Cow::Owned(s) => SharedString::from(s),
    }
}

impl Language {
    pub const ALL: &'static [Self] = &[Self::Id, Self::Eng];

    /// Locale tag matching `locales/ui.yml` and `gpui_kit::component::set_locale`.
    pub const fn locale_code(self) -> &'static str {
        match self {
            Self::Id => "id",
            Self::Eng => "en",
        }
    }

    pub const fn code(self) -> &'static str {
        match self {
            Self::Id => "ID",
            Self::Eng => "ENG",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Id => "Indonesia",
            Self::Eng => "English",
        }
    }

    pub const fn index(self) -> usize {
        match self {
            Self::Id => 0,
            Self::Eng => 1,
        }
    }

    /// Synchronizes `gpui-kit` component locale and refreshes all open windows.
    pub fn apply_locale(self, cx: &mut App) {
        gpui_kit::component::set_locale(self.locale_code());
        cx.refresh_windows();
    }

    /// Look up a static translation key from `locales/ui.yml`.
    pub fn tr(self, key: &'static str) -> SharedString {
        cow_to_shared(rust_i18n::t!(key, locale = self.locale_code()))
    }

    pub fn fallback_photo(self, index: usize) -> SharedString {
        let idx = index.to_string();
        cow_to_shared(rust_i18n::t!(
            "app.fallback_photo",
            locale = self.locale_code(),
            index = idx
        ))
    }

    pub fn fallback_text(self, index: usize) -> SharedString {
        let idx = index.to_string();
        cow_to_shared(rust_i18n::t!(
            "app.fallback_text",
            locale = self.locale_code(),
            index = idx
        ))
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn open_printer_props_err(self, err: &str) -> SharedString {
        cow_to_shared(rust_i18n::t!(
            "app.open_printer_props_err",
            locale = self.locale_code(),
            err = err
        ))
    }

    pub fn processing_doc(self, printer: &str) -> SharedString {
        cow_to_shared(rust_i18n::t!(
            "app.processing_doc",
            locale = self.locale_code(),
            printer = printer
        ))
    }

    pub fn print_sent_status(self, printer: &str) -> SharedString {
        cow_to_shared(rust_i18n::t!(
            "app.print_sent_status",
            locale = self.locale_code(),
            printer = printer
        ))
    }

    pub fn print_sent_notif(self, printer: &str) -> SharedString {
        cow_to_shared(rust_i18n::t!(
            "app.print_sent_notif",
            locale = self.locale_code(),
            printer = printer
        ))
    }

    pub fn print_failed_status(self, err: &str) -> SharedString {
        cow_to_shared(rust_i18n::t!(
            "app.print_failed_status",
            locale = self.locale_code(),
            err = err
        ))
    }

    pub fn print_failed_notif(self, err: &str) -> SharedString {
        cow_to_shared(rust_i18n::t!(
            "app.print_failed_notif",
            locale = self.locale_code(),
            err = err
        ))
    }

    pub fn saved_status(self, path: &str) -> SharedString {
        cow_to_shared(rust_i18n::t!(
            "app.saved_status",
            locale = self.locale_code(),
            path = path
        ))
    }

    pub fn pdf_saved_notif(self, path: &str) -> SharedString {
        cow_to_shared(rust_i18n::t!(
            "app.pdf_saved_notif",
            locale = self.locale_code(),
            path = path
        ))
    }

    pub fn loaded_photo(self, name: &str) -> SharedString {
        cow_to_shared(rust_i18n::t!(
            "app.loaded_photo",
            locale = self.locale_code(),
            name = name
        ))
    }

    pub fn switched_mode(self, mode_label: &str) -> SharedString {
        cow_to_shared(rust_i18n::t!(
            "app.switched_mode",
            locale = self.locale_code(),
            mode = mode_label
        ))
    }

    pub fn added_id_photos(self, qty: u32, w_cm: f32, h_cm: f32) -> SharedString {
        let qty_s = qty.to_string();
        let w_s = format!("{w_cm:.1}");
        let h_s = format!("{h_cm:.1}");
        cow_to_shared(rust_i18n::t!(
            "app.added_id_photos",
            locale = self.locale_code(),
            qty = qty_s,
            w = w_s,
            h = h_s
        ))
    }

    pub fn added_freeform_photos(self, count: usize) -> SharedString {
        let count_s = count.to_string();
        cow_to_shared(rust_i18n::t!(
            "app.added_freeform_photos",
            locale = self.locale_code(),
            count = count_s
        ))
    }

    pub fn device_config(self, printer: &str) -> SharedString {
        cow_to_shared(rust_i18n::t!(
            "app.device_config",
            locale = self.locale_code(),
            printer = printer
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_language_and_locales_resolve() {
        assert_eq!(DEFAULT_LANGUAGE, Language::Id);
        assert_eq!(Language::default(), Language::Id);
        assert_eq!(Language::Id.locale_code(), "id");
        assert_eq!(Language::Eng.locale_code(), "en");
        assert_eq!(Language::Id.code(), "ID");
        assert_eq!(Language::Eng.code(), "ENG");

        for lang in Language::ALL {
            assert_eq!(lang.index(), match lang {
                Language::Id => 0,
                Language::Eng => 1,
            });
            let ready = lang.tr("app.status_ready");
            assert!(!ready.is_empty());
            assert_ne!(ready.as_ref(), "app.status_ready");
        }

        assert_eq!(Language::Id.tr("app.cancel").as_ref(), "Batal");
        assert_eq!(Language::Eng.tr("app.cancel").as_ref(), "Cancel");
        assert_eq!(Language::Id.tr("app.dimensions").as_ref(), "Ukuran Foto");
        assert_eq!(Language::Id.tr("app.mode_full_image").as_ref(), "Halaman Penuh");
        assert_eq!(Language::Eng.tr("app.mode_full_image").as_ref(), "Full Page");
        assert_eq!(Language::Id.tr("app.fit_cover").as_ref(), "Isi Penuh");
        assert_eq!(Language::Eng.tr("app.fit_cover").as_ref(), "Fill");
        assert_eq!(Language::Eng.tr("app.fit_contain").as_ref(), "Fit");
        assert_eq!(
            Language::Id.tr("app.ctx_rotate_cw").as_ref(),
            Language::Id.tr("app.editor_rotate_cw").as_ref()
        );
        assert_eq!(Language::Id.tr("app.move_up_tooltip").as_ref(), "Naikkan Lapisan");
        assert_eq!(Language::Eng.tr("app.move_up_tooltip").as_ref(), "Move Layer Up");
        assert_eq!(Language::Id.tr("app.custom_options").as_ref(), "Opsi Tampilan Frame");
        assert_eq!(Language::Eng.tr("app.custom_options").as_ref(), "Frame Appearance");
        assert_eq!(Language::Id.tr("app.bg_color").as_ref(), "Warna Latar");
        assert_eq!(Language::Id.tr("app.crop_apply").as_ref(), "Terapkan Potongan");
        assert_eq!(Language::Id.tr("app.print_success_title").as_ref(), "Perintah Cetak Terkirim");
        assert_eq!(
            Language::Id.processing_doc("EPSON").as_ref(),
            "Memproses dokumen ke 'EPSON'..."
        );
        assert_eq!(
            Language::Eng.added_id_photos(2, 3.0, 4.0).as_ref(),
            "+2 ID photo(s) (3.0×4.0 cm)"
        );
    }
}
