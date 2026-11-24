use gpui_kit::{
    Context, IntoElement, ParentElement, Styled, Window,
    assets::IconName,
    component::{
        ActiveTheme as _, Sizable as _, Size,
        button::{Button, ButtonGroup, ButtonVariants as _},
        group_box::{GroupBox, GroupBoxVariants as _},
        h_flex,
        label::Label,
        v_flex,
    },
    div,
    prelude::FluentBuilder as _,
    px,
};

use crate::{
    canvas::model::{IdPhotoPreset, LayoutMode, ShapeKind},
    inspector::{
        common::{
            render_bg_color_select, render_number_field, render_rounded_slider,
            ButtonSelectedExt as _,
        },
        inspector_view::InspectorView,
    },
    shared::units::cm_to_px,
};

pub fn render_id_photo_panel(
    sidebar: &mut InspectorView,
    window: &mut Window,
    cx: &mut Context<InspectorView>,
) -> impl IntoElement {
    let (
        shape_kind,
        preset,
        unit,
        dpi,
        custom_w_cm,
        custom_h_cm,
        h_gap,
        v_gap,
        cut_line_enabled,
        free_move_enabled,
        selected_id_photo_item,
        lang,
    ) = {
        let app_guard = sidebar.app.read(cx);
        let s = &app_guard.state;
        (
            s.id_photo_shape_kind,
            s.id_photo_preset,
            s.id_photo_unit,
            s.paper.dpi,
            s.id_photo_custom_w_cm,
            s.id_photo_custom_h_cm,
            s.id_photo_h_gap_px,
            s.id_photo_v_gap_px,
            s.id_photo_cut_line,
            s.id_photo_free_move,
            if s.mode == LayoutMode::IdPhoto {
                s.selected_item().cloned()
            } else {
                None
            },
            s.language,
        )
    };
    let (w_cm, h_cm) = preset.dimensions_cm(custom_w_cm, custom_h_cm);
    let w_disp = unit.from_px(unit.to_px(w_cm, dpi), dpi);
    let h_disp = unit.from_px(unit.to_px(h_cm, dpi), dpi);

    v_flex()
        .gap_3()
        .w_full()
        .child(
            GroupBox::new()
                .outline()
                .title(lang.tr("app.mode_id_photo"))
                // Shape selector ButtonGroup with icons
                .child(
                    v_flex()
                        .gap(px(4.0))
                        .w_full()
                        .child(
                            Label::new(lang.tr("app.frame_shape"))
                                .text_sm()
                                .text_color(cx.theme().muted_foreground),
                        )
                        .child(
                            ButtonGroup::new("id-photo-shape-kind-group")
                                .outline()
                                .with_size(Size::Medium)
                                .w_full()
                                .children(ShapeKind::ALL.iter().copied().map(|kind| {
                                    let icon = match kind {
                                        ShapeKind::Rectangle => IconName::Square,
                                        ShapeKind::CircleOval => IconName::Circle,
                                    };
                                    Button::new(("shape-kind", kind as usize))
                                        .flex_1()
                                        .icon(icon)
                                        .label(kind.label_for(lang))
                                        .neutral_selected(shape_kind == kind, cx)
                                }))
                                .on_click(cx.listener(|this, clicks: &Vec<usize>, _, cx| {
                                    if let Some(&first) = clicks.first()
                                        && let Some(&kind) = ShapeKind::ALL.get(first)
                                    {
                                        this.app.update(cx, |app, cx| {
                                            app.state.id_photo_shape_kind = kind;
                                            cx.notify();
                                        });
                                    }
                                })),
                        ),
                )
                // Preset dimensions ButtonGroup
                .child(
                    v_flex()
                        .gap(px(4.0))
                        .w_full()
                        .child(
                            Label::new(lang.tr("app.dimensions"))
                                .text_sm()
                                .text_color(cx.theme().muted_foreground),
                        )
                        .child(
                            ButtonGroup::new("id-photo-preset-group")
                                .outline()
                                .with_size(Size::Medium)
                                .w_full()
                                .children(IdPhotoPreset::ALL.iter().copied().map(|p| {
                                    Button::new(("id-photo-preset", p as usize))
                                        .flex_1()
                                        .label(p.label_for(lang))
                                        .neutral_selected(preset == p, cx)
                                }))
                                .on_click(cx.listener(|this, clicks: &Vec<usize>, _, cx| {
                                    if let Some(&first) = clicks.first()
                                        && let Some(&p) = IdPhotoPreset::ALL.get(first)
                                    {
                                        this.app.update(cx, |app, cx| {
                                            app.state.id_photo_preset = p;
                                            cx.notify();
                                        });
                                    }
                                })),
                        ),
                )
                // Custom Dimensions (if preset == Custom)
                .when(preset == IdPhotoPreset::Custom, |col| {
                    col.child(
                        h_flex()
                            .gap_2()
                            .w_full()
                            .child(
                                div().flex_1().child(render_number_field(
                                    "id-photo-custom-w",
                                    lang.tr("app.width"),
                                    w_disp,
                                    0.1,
                                    1.0,
                                    50.0,
                                    1,
                                    unit.label(),
                                    &sidebar.app,
                                    window,
                                    cx,
                                    |val, app, cx| {
                                        app.state.id_photo_custom_w_cm = val;
                                        cx.notify();
                                    },
                                )),
                            )
                            .child(
                                div().flex_1().child(render_number_field(
                                    "id-photo-custom-h",
                                    lang.tr("app.height"),
                                    h_disp,
                                    0.1,
                                    1.0,
                                    50.0,
                                    1,
                                    unit.label(),
                                    &sidebar.app,
                                    window,
                                    cx,
                                    |val, app, cx| {
                                        app.state.id_photo_custom_h_cm = val;
                                        cx.notify();
                                    },
                                )),
                            ),
                    )
                })
                // Grid Spacing Row (Jarak Horizontal & Vertikal Antar Frame)
                .child(
                    h_flex()
                        .gap_2()
                        .w_full()
                        .child(
                            div().flex_1().child(render_number_field(
                                "id-photo-h-gap",
                                lang.tr("app.h_gap"),
                                h_gap,
                                1.0,
                                0.0,
                                100.0,
                                0,
                                "px",
                                &sidebar.app,
                                window,
                                cx,
                                |val, app, cx| {
                                    app.state.id_photo_h_gap_px = val;
                                    if !app.state.id_photo_free_move {
                                        app.state.reflow_id_photo_grid();
                                    }
                                    cx.notify();
                                },
                            )),
                        )
                        .child(
                            div().flex_1().child(render_number_field(
                                "id-photo-v-gap",
                                lang.tr("app.v_gap"),
                                v_gap,
                                1.0,
                                0.0,
                                100.0,
                                0,
                                "px",
                                &sidebar.app,
                                window,
                                cx,
                                |val, app, cx| {
                                    app.state.id_photo_v_gap_px = val;
                                    if !app.state.id_photo_free_move {
                                        app.state.reflow_id_photo_grid();
                                    }
                                    cx.notify();
                                },
                            )),
                        ),
                )
                // Frame Settings ButtonGroup (multi-select toggle group)
                .child(
                    v_flex()
                        .gap(px(4.0))
                        .w_full()
                        .child(
                            Label::new(lang.tr("app.frame_settings"))
                                .text_sm()
                                .text_color(cx.theme().muted_foreground),
                        )
                        .child(
                            ButtonGroup::new("id-photo-options-group")
                                .outline()
                                .multiple(true)
                                .with_size(Size::Medium)
                                .w_full()
                                .child(
                                    Button::new("btn-id-photo-cut-line")
                                        .flex_1()
                                        .label(lang.tr("app.cut_line"))
                                        .neutral_selected(cut_line_enabled, cx),
                                )
                                .child(
                                    Button::new("btn-id-photo-free-move")
                                        .flex_1()
                                        .label(lang.tr("app.free_move"))
                                        .neutral_selected(free_move_enabled, cx),
                                )
                                .on_click(cx.listener(|this, selected: &Vec<usize>, _, cx| {
                                    let cut_line = selected.contains(&0);
                                    let free_move = selected.contains(&1);
                                    this.app.update(cx, |app, cx| {
                                        app.state.id_photo_cut_line = cut_line;
                                        if app.state.id_photo_free_move != free_move {
                                            app.state.id_photo_free_move = free_move;
                                            if !free_move {
                                                app.state.reflow_id_photo_grid();
                                            }
                                        }
                                        cx.notify();
                                    });
                                })),
                        ),
                )
                // Tombol Tambah Pas Foto (Paling Bawah)
                .child(
                    Button::new("btn-id-photo-add-frames")
                        .primary()
                        .w_full()
                        .icon(IconName::Plus)
                        .label(lang.tr("app.add"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.app.update(cx, |app, cx| {
                                app.state.add_id_photo_frames();
                                app.canvas_focus.focus(window, cx);
                                cx.notify();
                            });
                        })),
                ),
        )
        // Edit selected individual ID Photo frame (when a shape is selected on canvas)
        .when_some(selected_id_photo_item, |col, item| {
            let id = item.id;
            let cut_line = item.cut_line;
            let cut_width = item.cut_line_width;
            let is_circle = item.shape_kind == ShapeKind::CircleOval;
            let cur_w_cm = (item.width_px / dpi as f32) * 2.54;
            let cur_h_cm = (item.height_px / dpi as f32) * 2.54;

            col.child(
                GroupBox::new()
                    .outline()
                    .title(lang.tr("app.selected_id_frame"))
                    .child(
                        v_flex()
                            .gap(px(10.0))
                            .w_full()
                            // Individual Dimensions
                            .child(
                                h_flex()
                                    .gap_2()
                                    .w_full()
                                    .child(
                                        div().flex_1().child(render_number_field(
                                            ("id-photo-item-w", id),
                                            lang.tr("app.width"),
                                            cur_w_cm,
                                            0.1,
                                            1.0,
                                            30.0,
                                            1,
                                            "cm",
                                            &sidebar.app,
                                            window,
                                            cx,
                                            move |val, app, cx| {
                                                if let Some(it) = app.state.find_item_mut(id) {
                                                    it.width_px = cm_to_px(val, dpi).round().max(24.0);
                                                    if !app.state.id_photo_free_move {
                                                        app.state.reflow_id_photo_grid();
                                                    }
                                                    cx.notify();
                                                }
                                            },
                                        )),
                                    )
                                    .child(
                                        div().flex_1().child(render_number_field(
                                            ("id-photo-item-h", id),
                                            lang.tr("app.height"),
                                            cur_h_cm,
                                            0.1,
                                            1.0,
                                            30.0,
                                            1,
                                            "cm",
                                            &sidebar.app,
                                            window,
                                            cx,
                                            move |val, app, cx| {
                                                if let Some(it) = app.state.find_item_mut(id) {
                                                    it.height_px = cm_to_px(val, dpi).round().max(24.0);
                                                    if !app.state.id_photo_free_move {
                                                        app.state.reflow_id_photo_grid();
                                                    }
                                                    cx.notify();
                                                }
                                            },
                                        )),
                                    ),
                            )
                            // Rounded Corners slider (only for rectangle/square shapes)
                            .when(!is_circle, |v| {
                                v.child(
                                    v_flex()
                                        .gap(px(4.0))
                                        .child(
                                            h_flex()
                                                .justify_between()
                                                .items_center()
                                                .child(
                                                    Label::new(lang.tr("app.corner_radius"))
                                                        .text_sm()
                                                        .text_color(cx.theme().muted_foreground),
                                                )
                                                .child(
                                                    Label::new(format!("{:.0} px", item.corner_radius))
                                                        .text_sm()
                                                        .text_color(cx.theme().muted_foreground),
                                                ),
                                        )
                                        .child(render_rounded_slider(id, item.corner_radius, &sidebar.app, window, cx)),
                                )
                            })
                            // Pengaturan Khusus Frame Terpilih (Garis Potong & Warna Background)
                            .child(
                                v_flex()
                                    .gap(px(4.0))
                                    .w_full()
                                    .child(
                                        Label::new(lang.tr("app.custom_options"))
                                            .text_sm()
                                            .text_color(cx.theme().muted_foreground),
                                    )
                                    .child(
                                        ButtonGroup::new(("pas-item-options-group", id))
                                            .outline()
                                            .multiple(true)
                                            .with_size(Size::Medium)
                                            .w_full()
                                            .child(
                                                Button::new(("btn-pas-item-cut-line", id))
                                                    .flex_1()
                                                    .label(lang.tr("app.cut_line"))
                                                    .neutral_selected(cut_line, cx),
                                            )
                                            .child(
                                                Button::new(("btn-pas-item-bg-color", id))
                                                    .flex_1()
                                                    .label(lang.tr("app.bg_color"))
                                                    .neutral_selected(item.bg_color.is_some(), cx),
                                            )
                                            .on_click(cx.listener(move |this, selected: &Vec<usize>, _, cx| {
                                                let new_cut_line = selected.contains(&0);
                                                let new_has_bg = selected.contains(&1);
                                                this.app.update(cx, |app, cx| {
                                                    if let Some(it) = app.state.find_item_mut(id) {
                                                        it.cut_line = new_cut_line;
                                                        if new_has_bg && it.bg_color.is_none() {
                                                            it.bg_color = Some(gpui_kit::rgb(0xDC2626).into());
                                                        } else if !new_has_bg && it.bg_color.is_some() {
                                                            it.bg_color = None;
                                                        }
                                                        cx.notify();
                                                    }
                                                });
                                            })),
                                    ),
                            )
                            // Line Width input shown when Cut Line is active
                            .when(cut_line, |v| {
                                v.child(render_number_field(
                                    ("pas-item-cutline-w", id),
                                    lang.tr("app.line_width"),
                                    cut_width,
                                    0.5,
                                    0.5,
                                    10.0,
                                    1,
                                    "px",
                                    &sidebar.app,
                                    window,
                                    cx,
                                    move |val, app, cx| {
                                        if let Some(it) = app.state.find_item_mut(id) {
                                            it.cut_line_width = val;
                                            cx.notify();
                                        }
                                    },
                                ))
                            })
                            // Color Picker input shown when Background Color is active
                            .when(item.bg_color.is_some(), |v| {
                                v.child(
                                    v_flex()
                                        .gap(px(4.0))
                                        .w_full()
                                        .child(
                                            Label::new(lang.tr("app.choose_bg_color"))
                                                .text_sm()
                                                .text_color(cx.theme().muted_foreground),
                                        )
                                        .child(render_bg_color_select(id, item.bg_color, &sidebar.app, window, cx)),
                                )
                            }),
                    ),
            )
        })
}
