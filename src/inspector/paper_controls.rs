use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, Keystroke, ParentElement, Styled, Subscription, Window,
    assets::IconName,
    component::{
        ActiveTheme as _, IndexPath, Sizable as _, Size,
        button::{Button, ButtonGroup, ButtonVariants as _},
        group_box::{GroupBox, GroupBoxVariants as _},
        h_flex,
        kbd::Kbd,
        label::Label,
        select::{SearchableVec, Select, SelectEvent, SelectState},
        v_flex,
    },
    div,
    prelude::FluentBuilder as _,
    px,
};

use crate::{
    canvas::model::{FitMode, LayoutMode},
    inspector::{
        common::{render_number_field, ButtonSelectedExt as _},
        inspector_view::InspectorView,
    },
    shared::paper::{Orientation, PaperPreset},
};

struct PaperSelectState {
    select: Entity<SelectState<SearchableVec<&'static str>>>,
    _sub: Subscription,
}

struct PrinterSelectState {
    select: Entity<SelectState<SearchableVec<String>>>,
    cached_items: Vec<String>,
    _sub: Subscription,
}

pub fn render_paper_settings_panel(
    sidebar: &mut InspectorView,
    window: &mut Window,
    cx: &mut Context<InspectorView>,
) -> impl IntoElement {
    let app_guard = sidebar.app.read(cx);
    let lang = app_guard.state.language;
    let mode = app_guard.state.mode;
    let preset = app_guard.state.paper.preset;
    let orientation = app_guard.state.paper.orientation;
    let margin_mm = app_guard.state.paper.margin_mm;
    let is_grayscale = app_guard.state.is_grayscale;
    let active_fit = app_guard.state.full_fit_mode;
    let p_unit = app_guard.state.paper.unit;
    let custom_w_mm = app_guard.state.paper.custom_width_mm;
    let custom_h_mm = app_guard.state.paper.custom_height_mm;

    let preset_ix = match preset {
        PaperPreset::A4 => 0,
        PaperPreset::F4 => 1,
        PaperPreset::FourR => 2,
        PaperPreset::FiveR => 3,
        PaperPreset::Letter => 4,
        PaperPreset::Custom => 5,
    };

    let printer_items = if app_guard.state.available_printers.is_empty() {
        vec![lang.tr("app.no_printer_found").to_string()]
    } else {
        app_guard.state.available_printers.clone()
    };
    let printer_ix = if app_guard.state.available_printers.is_empty() {
        0
    } else {
        app_guard
            .state
            .available_printers
            .iter()
            .position(|p| p == &app_guard.state.selected_printer)
            .unwrap_or(0)
    };

    let app_for_sub = sidebar.app.clone();
    let select_state = window.use_keyed_state("paper-preset-select", cx, |window, cx| {
        let items = SearchableVec::new(
            PaperPreset::ALL
                .iter()
                .map(|p| p.label())
                .collect::<Vec<_>>(),
        );
        let select =
            cx.new(|cx| SelectState::new(items, Some(IndexPath::new(preset_ix)), window, cx));
        let sub = cx.subscribe_in(
            &select,
            window,
            move |_this: &mut PaperSelectState,
                  _select,
                  event: &SelectEvent<SearchableVec<&'static str>>,
                  _window,
                  cx| {
                if let SelectEvent::Confirm(Some(val)) = event {
                    let new_preset = PaperPreset::ALL
                        .iter()
                        .copied()
                        .find(|p| p.label() == *val)
                        .unwrap_or(PaperPreset::Custom);
                    app_for_sub.update(cx, |app, cx| {
                        app.state.paper.preset = new_preset;
                        app.state.sync_full_image_bounds();
                        app.state.reflow_id_photo_grid();
                        app.state.clamp_all_freeform_items();
                        cx.notify();
                    });
                }
            },
        );
        PaperSelectState { select, _sub: sub }
    });

    let select_entity = select_state.read(cx).select.clone();
    let current_select_ix = select_entity.read(cx).selected_index(cx);
    if current_select_ix != Some(IndexPath::new(preset_ix)) {
        select_entity.update(cx, |state, cx| {
            state.set_selected_index(Some(IndexPath::new(preset_ix)), window, cx);
        });
    }

    let app_for_printer = sidebar.app.clone();
    let printer_select_key = "printer-device-select";
    let printer_select_state = window.use_keyed_state(printer_select_key, cx, |window, cx| {
        let items = SearchableVec::new(printer_items.clone());
        let select =
            cx.new(|cx| SelectState::new(items, Some(IndexPath::new(printer_ix)), window, cx));
        let sub = cx.subscribe_in(
            &select,
            window,
            move |_this: &mut PrinterSelectState,
                  _select,
                  event: &SelectEvent<SearchableVec<String>>,
                  _window,
                  cx| {
                if let SelectEvent::Confirm(Some(val)) = event {
                    let chosen = (*val).clone();
                    app_for_printer.update(cx, |app, cx| {
                        if !app.state.available_printers.is_empty() {
                            app.state.selected_printer = chosen.clone();
                            #[cfg(not(windows))]
                            {
                                let hw = crate::printer::platform_unix::get_printer_hardware_options(
                                    &chosen,
                                );
                                if !hw.default_media_type.is_empty() {
                                    app.state.linux_printer_config.media_type =
                                        hw.default_media_type.clone();
                                }
                                if !hw.default_print_quality.is_empty() {
                                    app.state.linux_printer_config.print_quality =
                                        hw.default_print_quality.clone();
                                }
                                app.state.cached_hw_options = hw;
                            }
                            cx.notify();
                        }
                    });
                }
            },
        );
        PrinterSelectState {
            select,
            cached_items: printer_items.clone(),
            _sub: sub,
        }
    });

    let (printer_select_entity, items_changed) = {
        let pss = printer_select_state.read(cx);
        (pss.select.clone(), pss.cached_items != printer_items)
    };

    if items_changed {
        printer_select_state.update(cx, |pss, cx| {
            pss.cached_items = printer_items.clone();
            pss.select.update(cx, |state, cx| {
                state.set_items(SearchableVec::new(printer_items.clone()), window, cx);
                state.set_selected_index(Some(IndexPath::new(printer_ix)), window, cx);
            });
        });
    } else {
        let current_printer_ix = printer_select_entity.read(cx).selected_index(cx);
        if current_printer_ix != Some(IndexPath::new(printer_ix)) {
            printer_select_entity.update(cx, |state, cx| {
                state.set_selected_index(Some(IndexPath::new(printer_ix)), window, cx);
            });
        }
    }

    GroupBox::new()
        .outline()
        .title(lang.tr("app.layout_and_paper"))
        // Pilih Printer Dropdown Select (Paling Atas)
        .child(
            v_flex()
                .gap(px(4.0))
                .w_full()
                .child(
                    Label::new(lang.tr("app.select_printer"))
                        .text_sm()
                        .text_color(cx.theme().muted_foreground),
                )
                .child(Select::new(&printer_select_entity).w_full()),
        )
        // Mode Warna (Warna vs Hitam Putih) ButtonGroup
        .child(
            v_flex()
                .gap(px(4.0))
                .w_full()
                .child(
                    Label::new(lang.tr("app.color_mode"))
                        .text_sm()
                        .text_color(cx.theme().muted_foreground),
                )
                .child(
                    ButtonGroup::new("color-mode-group")
                        .outline()
                        .with_size(Size::Medium)
                        .w_full()
                        .child(
                            Button::new("btn-col-color")
                                .flex_1()
                                .label(lang.tr("app.color"))
                                .neutral_selected(!is_grayscale, cx),
                        )
                        .child(
                            Button::new("btn-col-gray")
                                .flex_1()
                                .label(lang.tr("app.grayscale"))
                                .neutral_selected(is_grayscale, cx),
                        )
                        .on_click(cx.listener(|this, clicks: &Vec<usize>, _, cx| {
                            if let Some(&first) = clicks.first() {
                                this.app.update(cx, |app, cx| {
                                    app.state.is_grayscale = first == 1;
                                    cx.notify();
                                });
                            }
                        })),
                ),
        )
        // Paper Preset Select
        .child(
            v_flex()
                .gap(px(4.0))
                .w_full()
                .child(
                    Label::new(lang.tr("app.paper_size"))
                        .text_sm()
                        .text_color(cx.theme().muted_foreground),
                )
                .child(Select::new(&select_entity).w_full()),
        )
        // Custom dimensions input when paper preset is Custom
        .when(preset == PaperPreset::Custom, |col| {
            col.child(
                h_flex()
                    .gap_2()
                    .w_full()
                    .child(
                        div().flex_1().child(render_number_field(
                            "custom-paper-w",
                            lang.tr("app.width"),
                            custom_w_mm,
                            1.0,
                            20.0,
                            1200.0,
                            1,
                            p_unit.label(),
                            &sidebar.app,
                            window,
                            cx,
                            |val, app, cx| {
                                app.state.paper.custom_width_mm = val;
                                app.state.sync_full_image_bounds();
                                app.state.reflow_id_photo_grid();
                                app.state.clamp_all_freeform_items();
                                cx.notify();
                            },
                        )),
                    )
                    .child(
                        div().flex_1().child(render_number_field(
                            "custom-paper-h",
                            lang.tr("app.height"),
                            custom_h_mm,
                            1.0,
                            20.0,
                            1200.0,
                            1,
                            p_unit.label(),
                            &sidebar.app,
                            window,
                            cx,
                            |val, app, cx| {
                                app.state.paper.custom_height_mm = val;
                                app.state.sync_full_image_bounds();
                                app.state.reflow_id_photo_grid();
                                app.state.clamp_all_freeform_items();
                                cx.notify();
                            },
                        )),
                    ),
            )
        })
        // Orientation ButtonGroup
        .child(
            v_flex()
                .gap(px(4.0))
                .w_full()
                .child(
                    Label::new(lang.tr("app.orientation"))
                        .text_sm()
                        .text_color(cx.theme().muted_foreground),
                )
                .child(
                    ButtonGroup::new("orientation-group")
                        .outline()
                        .with_size(Size::Medium)
                        .w_full()
                        .child(
                            Button::new("btn-portrait")
                                .flex_1()
                                .label(lang.tr("app.portrait"))
                                .neutral_selected(orientation == Orientation::Portrait, cx),
                        )
                        .child(
                            Button::new("btn-landscape")
                                .flex_1()
                                .label(lang.tr("app.landscape"))
                                .neutral_selected(orientation == Orientation::Landscape, cx),
                        )
                        .on_click(cx.listener(|this, clicks: &Vec<usize>, _, cx| {
                            if let Some(&first) = clicks.first() {
                                let new_orient = match first {
                                    0 => Orientation::Portrait,
                                    _ => Orientation::Landscape,
                                };
                                this.app.update(cx, |app, cx| {
                                    app.state.paper.orientation = new_orient;
                                    app.state.sync_full_image_bounds();
                                    app.state.reflow_id_photo_grid();
                                    app.state.clamp_all_freeform_items();
                                    cx.notify();
                                });
                            }
                        })),
                ),
        )
        // Margin Aman
        .child(
            v_flex()
                .w_full()
                .child(render_number_field(
                    "paper-margin-mm",
                    lang.tr("app.safe_margin"),
                    margin_mm,
                    0.5,
                    0.0,
                    50.0,
                    1,
                    "mm",
                    &sidebar.app,
                    window,
                    cx,
                    |val, app, cx| {
                        app.state.paper.margin_mm = val;
                        app.state.sync_full_image_bounds();
                        app.state.reflow_id_photo_grid();
                        app.state.clamp_all_freeform_items();
                        cx.notify();
                    },
                )),
        )
        // Opsi Khusus Mode 1: Full Image
        .when(mode == LayoutMode::FullImage, |col| {
            col.child(
                v_flex()
                    .gap(px(4.0))
                    .w_full()
                    .child(
                        Label::new(lang.tr("app.image_fit"))
                            .text_sm()
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(
                        ButtonGroup::new("full-fit-mode-group")
                            .outline()
                            .with_size(Size::Medium)
                            .w_full()
                            .children(FitMode::ALL.iter().copied().map(|fit| {
                                Button::new(("fit-mode", fit as usize))
                                    .flex_1()
                                    .label(fit.label_for(lang))
                                    .neutral_selected(active_fit == fit, cx)
                            }))
                            .on_click(cx.listener(|this, clicks: &Vec<usize>, _, cx| {
                                if let Some(&first) = clicks.first()
                                    && let Some(&fit) = FitMode::ALL.get(first)
                                {
                                    this.app.update(cx, |app, cx| {
                                        app.state.full_fit_mode = fit;
                                        app.state.sync_full_image_bounds();
                                        cx.notify();
                                    });
                                }
                            })),
                    ),
            )
        })
        // Advanced printer properties button (bottom-most in all modes)
        .child(
            Button::new("btn-open-printer-properties")
                .secondary()
                .w_full()
                .icon(IconName::Settings)
                .child(
                    h_flex()
                        .w_full()
                        .justify_between()
                        .items_center()
                        .child(div().child(lang.tr("app.printer_settings")))
                        .child(
                            #[cfg(target_os = "macos")]
                            Kbd::new(Keystroke::parse("cmd-p").unwrap()),
                            #[cfg(not(target_os = "macos"))]
                            Kbd::new(Keystroke::parse("ctrl-p").unwrap()),
                        ),
                )
                .on_click(cx.listener(|this, _, window: &mut Window, cx| {
                    this.app.update(cx, |app, cx| {
                        app.trigger_open_printer_properties(window, cx);
                    });
                })),
        )
}
