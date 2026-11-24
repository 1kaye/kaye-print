use gpui_kit::{
    App, AppContext as _, Entity, ParentElement,
    Styled, Subscription, Window,
    component::{
        ActiveTheme as _, IndexPath, Sizable as _, Size, WindowExt as _,
        button::{Button, ButtonGroup, ButtonVariants as _},
        dialog::{
            DialogDescription, DialogEntrance, DialogFooter, DialogHeader, DialogTitle,
        },
        input::{InputEvent, InputState, NumberInput},
        label::Label,
        select::{SearchableVec, Select, SelectEvent, SelectState},
        v_flex,
    },
    px,
};

use crate::{
    app::StudioApp,
    inspector::ButtonSelectedExt as _,
    printer::model::PrinterModalDraft,
    shared::{
        i18n::Language,
        paper::{Orientation, PaperPreset},
    },
};

/// Controller entity for the printer settings dialog.
/// Menyimpan draft state dan persistent input/select entities selama dialog terbuka
/// sehingga proses rendering dialog bersifat murni, idempotent, dan mulus.
pub struct PrinterModalController {
    pub app_entity: Entity<StudioApp>,
    pub draft: PrinterModalDraft,
    pub printer_name: String,
    pub language: Language,
    pub paper_select: Entity<SelectState<SearchableVec<&'static str>>>,
    pub media_select: Entity<SelectState<SearchableVec<String>>>,
    pub copies_input: Entity<InputState>,
    _sub_paper: Subscription,
    _sub_media: Subscription,
    _sub_copies: Subscription,
}

/// Opens the CUPS printer settings dialog following the official gpui-kit dialog guide.
/// Uses a persistent controller entity so entrance and exit animations
/// are not cut off or dismissed abruptly.
pub fn open_printer_settings_dialog(
    app_entity: Entity<StudioApp>,
    initial_draft: PrinterModalDraft,
    printer_name: String,
    language: Language,
    window: &mut Window,
    cx: &mut App,
) {
    if window.has_active_dialog(cx) {
        return;
    }

    // 1. Inisialisasi Paper Preset Select
    let preset_ix = match initial_draft.paper_preset {
        PaperPreset::A4 => 0,
        PaperPreset::F4 => 1,
        PaperPreset::FourR => 2,
        PaperPreset::FiveR => 3,
        PaperPreset::Letter => 4,
        PaperPreset::Custom => 5,
    };
    let paper_items = SearchableVec::new(
        PaperPreset::ALL
            .iter()
            .map(|p| p.label())
            .collect::<Vec<_>>(),
    );
    let paper_select = cx.new(|cx| SelectState::new(paper_items, Some(IndexPath::new(preset_ix)), window, cx));

    // 2. Inisialisasi Media Type Select
    let media_labels: Vec<String> = if initial_draft.media_choices.is_empty() {
        vec!["Plain Paper".into(), "Photo Paper".into()]
    } else {
        initial_draft.media_choices.iter().map(|c| c.label.clone()).collect()
    };
    let active_media_ix = initial_draft
        .media_choices
        .iter()
        .position(|c| c.value.eq_ignore_ascii_case(&initial_draft.media_type))
        .unwrap_or(0);
    let media_items = SearchableVec::new(media_labels);
    let media_select = cx.new(|cx| SelectState::new(media_items, Some(IndexPath::new(active_media_ix)), window, cx));

    // 3. Inisialisasi Copies Input
    let initial_copies = initial_draft.copies.clamp(1, 999);
    let copies_input = cx.new(|cx| {
        InputState::new(window, cx)
            .default_value(format!("{initial_copies}"))
            .step(1.0)
            .min(1.0)
            .max(999.0)
    });

    // 4. Create Controller Entity with Subscriptions
    let controller = cx.new(|cx| {
        let sub_paper = cx.subscribe_in(
            &paper_select,
            window,
            |this: &mut PrinterModalController, _select, event: &SelectEvent<SearchableVec<&'static str>>, _window, cx| {
                if let SelectEvent::Confirm(Some(val)) = event {
                    let new_preset = PaperPreset::ALL
                        .iter()
                        .copied()
                        .find(|p| p.label() == *val)
                        .unwrap_or(PaperPreset::Custom);
                    this.draft.paper_preset = new_preset;
                    cx.notify();
                }
            },
        );

        let choices_for_sub = initial_draft.media_choices.clone();
        let sub_media = cx.subscribe_in(
            &media_select,
            window,
            move |this: &mut PrinterModalController, _select, event: &SelectEvent<SearchableVec<String>>, _window, cx| {
                if let SelectEvent::Confirm(Some(val)) = event {
                    let chosen_val = if choices_for_sub.is_empty() {
                        val.clone()
                    } else if let Some(matched) = choices_for_sub.iter().find(|c| c.label == *val) {
                        matched.value.clone()
                    } else {
                        val.clone()
                    };
                    this.draft.media_type = chosen_val;
                    cx.notify();
                }
            },
        );

        let sub_copies = cx.subscribe_in(
            &copies_input,
            window,
            |this: &mut PrinterModalController, input, event: &InputEvent, _window, cx| {
                if let InputEvent::Change = event {
                    let text = input.read(cx).value();
                    if let Ok(parsed) = text.parse::<f32>() {
                        let clamped = (parsed.clamp(1.0, 999.0)) as u32;
                        this.draft.copies = clamped;
                        cx.notify();
                    }
                }
            },
        );

        PrinterModalController {
            app_entity: app_entity.clone(),
            draft: initial_draft,
            printer_name,
            language,
            paper_select,
            media_select,
            copies_input,
            _sub_paper: sub_paper,
            _sub_media: sub_media,
            _sub_copies: sub_copies,
        }
    });

    let app_for_close = app_entity.clone();
    let ctrl_for_dialog = controller.clone();

    // 5. Open Dialog using the declarative API from gpui-kit
    window.open_dialog(cx, move |dialog, _window, _cx| {
        let app_for_close_cleanup = app_for_close.clone();
        let ctrl_handle = ctrl_for_dialog.clone();

        dialog
            .entrance(DialogEntrance::FadeSlide)
            .w(px(400.0))
            .centered()
            .on_close(move |_, _, cx| {
                // Called after the exit animation finishes completely
                app_for_close_cleanup.update(cx, |app, cx| {
                    app.state.printer_modal = None;
                    cx.notify();
                });
            })
            .content(move |content, _window, cx| {
                let ctrl = ctrl_handle.read(cx);
                let draft = &ctrl.draft;
                let printer_name = ctrl.printer_name.clone();
                let lang = ctrl.language;

                let orientation = draft.orientation;
                let is_grayscale = draft.is_grayscale;
                let quality = draft.print_quality.clone();

                let is_draft = quality.eq_ignore_ascii_case("draft") || quality.eq_ignore_ascii_case("fast");
                let is_high = quality.eq_ignore_ascii_case("high")
                    || quality.eq_ignore_ascii_case("best")
                    || quality.eq_ignore_ascii_case("photo");
                let is_standard = !is_draft && !is_high;

                let paper_select_entity = ctrl.paper_select.clone();
                let media_select_entity = ctrl.media_select.clone();
                let copies_input_entity = ctrl.copies_input.clone();

                let ctrl_for_orient = ctrl_handle.clone();
                let ctrl_for_qual = ctrl_handle.clone();
                let ctrl_for_color = ctrl_handle.clone();
                let ctrl_for_save = ctrl_handle.clone();

                content
                    .child(
                        DialogHeader::new()
                            .child(DialogTitle::new().child(lang.tr("app.printer_settings")))
                            .child(DialogDescription::new().child(lang.device_config(&printer_name))),
                    )
                    .child(
                        v_flex()
                            .gap(px(12.0))
                            .py(px(4.0))
                            // 1. Ukuran Kertas
                            .child(
                                v_flex()
                                    .gap(px(4.0))
                                    .child(Label::new(lang.tr("app.paper_size")).text_sm().text_color(cx.theme().muted_foreground))
                                    .child(Select::new(&paper_select_entity).w_full()),
                            )
                            // 2. Orientasi Cetak
                            .child(
                                v_flex()
                                    .gap(px(4.0))
                                    .child(Label::new(lang.tr("app.orientation")).text_sm().text_color(cx.theme().muted_foreground))
                                    .child(
                                        ButtonGroup::new("dialog-orient-group")
                                            .outline()
                                            .with_size(Size::Medium)
                                            .w_full()
                                            .child(
                                                Button::new("dialog-orient-portrait")
                                                    .flex_1()
                                                    .label(lang.tr("app.portrait_full"))
                                                    .neutral_selected(orientation == Orientation::Portrait, cx),
                                            )
                                            .child(
                                                Button::new("dialog-orient-landscape")
                                                    .flex_1()
                                                    .label(lang.tr("app.landscape_full"))
                                                    .neutral_selected(orientation == Orientation::Landscape, cx),
                                            )
                                            .on_click(move |clicks: &Vec<usize>, _window: &mut Window, cx: &mut App| {
                                                if let Some(&first) = clicks.first() {
                                                    ctrl_for_orient.update(cx, |ctrl, cx| {
                                                        ctrl.draft.orientation = match first {
                                                            0 => Orientation::Portrait,
                                                            _ => Orientation::Landscape,
                                                        };
                                                        cx.notify();
                                                    });
                                                }
                                            }),
                                    ),
                            )
                            // 3. Jenis Kertas (Media Type)
                            .child(
                                v_flex()
                                    .gap(px(4.0))
                                    .child(Label::new(lang.tr("app.media_type")).text_sm().text_color(cx.theme().muted_foreground))
                                    .child(Select::new(&media_select_entity).w_full()),
                            )
                            // 4. Kualitas Cetak (Print Quality)
                            .child(
                                v_flex()
                                    .gap(px(4.0))
                                    .child(Label::new(lang.tr("app.print_quality")).text_sm().text_color(cx.theme().muted_foreground))
                                    .child(
                                        ButtonGroup::new("dialog-quality-group")
                                            .outline()
                                            .with_size(Size::Medium)
                                            .w_full()
                                            .child(
                                                Button::new("dialog-qual-draft")
                                                    .flex_1()
                                                    .label(lang.tr("app.quality_draft"))
                                                    .neutral_selected(is_draft, cx),
                                            )
                                            .child(
                                                Button::new("dialog-qual-std")
                                                    .flex_1()
                                                    .label(lang.tr("app.quality_standard"))
                                                    .neutral_selected(is_standard, cx),
                                            )
                                            .child(
                                                Button::new("dialog-qual-high")
                                                    .flex_1()
                                                    .label(lang.tr("app.quality_high"))
                                                    .neutral_selected(is_high, cx),
                                            )
                                            .on_click(move |clicks: &Vec<usize>, _window: &mut Window, cx: &mut App| {
                                                if let Some(&first) = clicks.first() {
                                                    ctrl_for_qual.update(cx, |ctrl, cx| {
                                                        ctrl.draft.print_quality = match first {
                                                            0 => "Draft".into(),
                                                            2 => "High".into(),
                                                            _ => "Standard".into(),
                                                        };
                                                        cx.notify();
                                                    });
                                                }
                                            }),
                                    ),
                            )
                            // 5. Mode Warna vs Grayscale
                            .child(
                                v_flex()
                                    .gap(px(4.0))
                                    .child(Label::new(lang.tr("app.color_mode")).text_sm().text_color(cx.theme().muted_foreground))
                                    .child(
                                        ButtonGroup::new("dialog-color-group")
                                            .outline()
                                            .with_size(Size::Medium)
                                            .w_full()
                                            .child(
                                                Button::new("dialog-col-color")
                                                    .flex_1()
                                                    .label(lang.tr("app.color"))
                                                    .neutral_selected(!is_grayscale, cx),
                                            )
                                            .child(
                                                Button::new("dialog-col-gray")
                                                    .flex_1()
                                                    .label(lang.tr("app.grayscale_full"))
                                                    .neutral_selected(is_grayscale, cx),
                                            )
                                            .on_click(move |clicks: &Vec<usize>, _window: &mut Window, cx: &mut App| {
                                                if let Some(&first) = clicks.first() {
                                                    ctrl_for_color.update(cx, |ctrl, cx| {
                                                        ctrl.draft.is_grayscale = first == 1;
                                                        cx.notify();
                                                    });
                                                }
                                            }),
                                    ),
                            )
                            // 6. Jumlah Salinan (Copies)
                            .child(
                                v_flex()
                                    .gap(px(4.0))
                                    .child(Label::new(lang.tr("app.copies_label")).text_sm().text_color(cx.theme().muted_foreground))
                                    .child(NumberInput::new(&copies_input_entity).w_full()),
                            ),
                    )
                    .child(
                        DialogFooter::new()
                            .child(
                                Button::new("btn-dialog-cancel")
                                    .label(lang.tr("app.cancel"))
                                    .on_click(|_, window: &mut Window, cx| {
                                        window.close_dialog(cx);
                                    }),
                            )
                            .child(
                                Button::new("btn-dialog-save")
                                    .primary()
                                    .label(lang.tr("app.apply"))
                                    .on_click(move |_, window: &mut Window, cx: &mut App| {
                                        let updated_draft = ctrl_for_save.read(cx).draft.clone();
                                        let app_entity = ctrl_for_save.read(cx).app_entity.clone();
                                        app_entity.update(cx, |app, cx| {
                                            app.state.printer_modal = Some(updated_draft);
                                            app.state.save_and_apply_printer_modal();
                                            cx.notify();
                                        });
                                        window.close_dialog(cx);
                                    }),
                            ),
                    )
            })
    });
}
