use gpui_kit::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement,
    Render, Styled, Subscription, Window,
    assets::IconName,
    component::{
        ActiveTheme as _, Disableable as _, Sizable as _, Size, WindowExt as _,
        button::{Button, ButtonGroup, ButtonVariant, ButtonVariants as _},
        dialog::DialogEntrance,
        h_flex, Icon,
        scroll::ScrollableElement as _,
        v_flex,
    },
    div, px,
};

use crate::{
    app::StudioApp,
    canvas::model::LayoutMode,
    inspector::{
        common::ButtonSelectedExt as _,
        freeform_controls::render_freeform_panel,
        id_photo_controls::render_id_photo_panel,
        layer_list::LayerListView,
        paper_controls::render_paper_settings_panel,
    },
};

pub struct InspectorView {
    pub app: Entity<StudioApp>,
    pub layer_list: Entity<LayerListView>,
    _subscriptions: Vec<Subscription>,
}

impl InspectorView {
    pub fn new(app: Entity<StudioApp>, cx: &mut Context<Self>) -> Self {
        let layer_list = cx.new(|cx| LayerListView::new(app.clone(), cx));
        let _subscriptions = vec![
            cx.observe(&app, |_, _, cx| {
                cx.notify();
            }),
        ];

        Self {
            app,
            layer_list,
            _subscriptions,
        }
    }
}

impl Render for InspectorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (is_printing, is_exporting, sidebar_open, lang) = {
            let app_guard = self.app.read(cx);
            (
                app_guard.state.is_printing,
                app_guard.state.is_exporting,
                app_guard.state.sidebar_open,
                app_guard.state.language,
            )
        };
        let is_busy = is_printing || is_exporting;

        if !sidebar_open {
            return div().id("app-inspector").hidden();
        }

        div()
            .id("app-inspector")
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().sidebar)
            .border_r_1()
            .border_color(cx.theme().sidebar_border)
            // 1. Header Inspector
            .child(
                div()
                    .h(px(40.0))
                    .px(px(14.0))
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(cx.theme().sidebar_border)
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                            .text_color(cx.theme().sidebar_foreground)
                            .child("kaye-print"),
                    ),
            )
            // 2. Scrollable Content Body (with clean GPUI visual scrollbar and safe padding so buttons are not covered)
            .child(
                div()
                    .id("inspector-content-scroll")
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_y_scrollbar()
                    .px(px(14.0))
                    .py(px(12.0))
                    .child(render_inspector_sections(self, window, cx)),
            )
            // 3. Footer Action Buttons (integrated at the bottom of inspector with a clear divider border)
            .child(
                div()
                    .id("inspector-footer-actions")
                    .w_full()
                    .p(px(12.0))
                    .bg(cx.theme().sidebar)
                    .border_t_1()
                    .border_color(cx.theme().sidebar_border)
                    .child(
                        h_flex()
                            .gap(px(8.0))
                            .w_full()
                            // Direct Print Button (Primary Orange, with loading spinner while processing job)
                            .child(
                                Button::new("btn-direct-print")
                                    .primary()
                                    .flex_1()
                                    .h(px(38.0))
                                    .disabled(is_busy)
                                    .loading(is_printing)
                                    .icon(IconName::Printer)
                                    .label(if is_printing {
                                        lang.tr("app.printing_btn")
                                    } else {
                                        lang.tr("app.print_btn")
                                    })
                                    .on_click(cx.listener(|this, _, window: &mut Window, cx| {
                                        this.app.update(cx, |app, cx| {
                                            app.trigger_direct_print(window, cx);
                                        });
                                    })),
                            )
                            // Save PDF Button (Secondary Neutral with document icon)
                            .child(
                                Button::new("btn-export-pdf")
                                    .secondary()
                                    .flex_1()
                                    .h(px(38.0))
                                    .disabled(is_busy)
                                    .loading(is_exporting)
                                    .icon(IconName::FileText)
                                    .label(if is_exporting {
                                        lang.tr("app.saving_btn")
                                    } else {
                                        lang.tr("app.save_pdf_btn")
                                    })
                                    .on_click(cx.listener(|this, _, window: &mut Window, cx| {
                                        this.app.update(cx, |app, cx| {
                                            app.trigger_export_pdf(window, cx);
                                        });
                                    })),
                            ),
                    ),
            )
    }
}

fn render_inspector_sections(
    sidebar: &mut InspectorView,
    window: &mut Window,
    cx: &mut Context<InspectorView>,
) -> impl IntoElement {
    let (mode, lang) = {
        let app = sidebar.app.read(cx);
        (app.state.mode, app.state.language)
    };

    v_flex()
        .gap(px(12.0))
        .w_full()
        // Mode Switcher ButtonGroup
        .child(
            ButtonGroup::new("layout-mode-group")
                .outline()
                .with_size(Size::Medium)
                .w_full()
                .child(
                    Button::new("mode-id-photo")
                        .flex_1()
                        .icon(Icon::new(IconName::Frame).with_size(Size::Medium))
                        .label(lang.tr("app.mode_id_photo"))
                        .neutral_selected(mode == LayoutMode::IdPhoto, cx),
                )
                .child(
                    Button::new("mode-freeform")
                        .flex_1()
                        .icon(Icon::new(IconName::Layers).with_size(Size::Medium))
                        .label(lang.tr("app.mode_freeform"))
                        .neutral_selected(mode == LayoutMode::Freeform, cx),
                )
                .child(
                    Button::new("mode-full-image")
                        .flex_1()
                        .icon(Icon::new(IconName::Maximize).with_size(Size::Medium))
                        .label(lang.tr("app.mode_full_image"))
                        .neutral_selected(mode == LayoutMode::FullImage, cx),
                )
                .on_click(cx.listener(|this, clicks: &Vec<usize>, window: &mut Window, cx| {
                    if let Some(&first) = clicks.first() {
                        let target_mode = match first {
                            0 => LayoutMode::IdPhoto,
                            1 => LayoutMode::Freeform,
                            _ => LayoutMode::FullImage,
                        };

                        let app_read = this.app.read(cx);
                        let cur_mode = app_read.state.mode;
                        let has_images = app_read.state.current_mode_has_images();
                        let cur_lang = app_read.state.language;

                        if cur_mode == target_mode {
                            return;
                        }

                        if has_images {
                            let app_entity = this.app.clone();
                            let target = target_mode;
                            window.open_alert_dialog(cx, move |alert, _, cx| {
                                let app_confirm = app_entity.clone();
                                alert
                                    .entrance(DialogEntrance::FadeSlide)
                                    .w(px(420.0))
                                    .title(cur_lang.tr("app.switch_mode_title"))
                                    .description(cur_lang.tr("app.switch_mode_desc"))
                                    .icon(
                                        Icon::new(IconName::TriangleAlert)
                                            .size_8()
                                            .text_color(cx.theme().warning),
                                    )
                                    .confirm()
                                    .cancel_text(cur_lang.tr("app.cancel"))
                                    .ok_text(cur_lang.tr("app.switch_mode_confirm"))
                                    .ok_variant(ButtonVariant::Danger)
                                    .on_ok(move |_, window, cx| {
                                        app_confirm.update(cx, |app, cx| {
                                            app.state.reset_images_and_switch_mode(target);
                                            app.canvas_focus.focus(window, cx);
                                            cx.notify();
                                        });
                                        true
                                    })
                            });
                        } else {
                            this.app.update(cx, |app, cx| {
                                app.state.reset_images_and_switch_mode(target_mode);
                                app.canvas_focus.focus(window, cx);
                                cx.notify();
                            });
                        }
                    }
                })),
        )
        // Section: Paper & Global Output Settings
        .child(render_paper_settings_panel(sidebar, window, cx))
        // Section: Dynamic Mode Configuration Panel
        .child(match mode {
            LayoutMode::FullImage => div().into_any_element(),
            LayoutMode::IdPhoto => render_id_photo_panel(sidebar, window, cx).into_any_element(),
            LayoutMode::Freeform => render_freeform_panel(sidebar, window, cx).into_any_element(),
        })
}
