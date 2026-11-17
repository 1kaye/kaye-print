use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, FocusHandle, Focusable as _,
    InteractiveElement as _, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, ObjectFit,
    ParentElement, Styled, StyledImage as _, Subscription, Window,
    assets::IconName,
    base::input::{InputEvent, Textarea as BaseTextarea, TextareaState},
    component::{
        ActiveTheme as _, Icon, Sizable as _, Size, v_flex,
        menu::ContextMenuExt,
    },
    div, img,
    prelude::FluentBuilder as _,
    px, relative, rgb,
};

use crate::{
    app::StudioApp,
    canvas::{
        canvas_view::CanvasView,
        context_menu::build_item_popup_menu,
        geometry::TEXT_BOX_PAD_PX,
        model::{CanvasDrag, CanvasItem, LayoutMode, ShapeKind, TextAlignMode},
    },
    shared::i18n::Language,
};

struct InlineTextState {
    textarea: Entity<TextareaState>,
    _sub: Subscription,
}

pub(crate) fn render_canvas_item(
    item: &CanvasItem,
    display_scale: f32,
    paper_dpi: u32,
    selected_id: Option<u64>,
    editing_text_id: Option<u64>,
    focus_inline: bool,
    mode: LayoutMode,
    id_photo_free_move: bool,
    lang: Language,
    app_entity: Entity<StudioApp>,
    canvas_focus: FocusHandle,
    window: &mut Window,
    cx: &mut Context<CanvasView>,
) -> AnyElement {
    let id = item.id;
    let is_selected = selected_id == Some(id);
    let orig_x = item.x_px;
    let orig_y = item.y_px;
    let orig_w = item.width_px;
    let orig_h = item.height_px;

    let x = orig_x * display_scale;
    let y = orig_y * display_scale;
    let is_freeform = mode == LayoutMode::Freeform;
    let is_text = is_freeform && item.is_text();
    let is_editing_inline = is_text && editing_text_id == Some(id);
    let has_image = item.image.is_some();
    let min_disp = if is_freeform && (has_image || is_text) {
        4.0
    } else {
        18.0
    };
    let w = (orig_w * display_scale).max(min_disp);
    let h = (orig_h * display_scale).max(min_disp);
    let can_move =
        !is_editing_inline && (is_freeform || (mode == LayoutMode::IdPhoto && id_photo_free_move));
    let is_oval = !is_freeform && item.shape_kind == ShapeKind::CircleOval;
    let cut_thickness = if item.cut_line && !is_freeform {
        (item.cut_line_width * display_scale).clamp(1.0, 16.0)
    } else {
        0.0
    };

    let inner_frame = if is_freeform {
        // Freeform mode: render photo naturally or as a text box
        if let Some(tb) = &item.text {
            let font_px = (tb.font_size_pt.clamp(6.0, 300.0) * (paper_dpi.max(72) as f32 / 72.0)
                * display_scale)
                .max(4.0);
            let pad_px = (TEXT_BOX_PAD_PX * display_scale).max(2.0);
            let is_empty = tb.content.trim().is_empty();
            let font_family = tb.font_family.clone();

            if is_editing_inline {
                let app_for_edit = app_entity.clone();
                let current_text = tb.content.clone();
                let initial_text = current_text.clone();
                let placeholder = lang.tr("app.text_placeholder");
                let primary_border = cx.theme().primary;
                let state = window.use_keyed_state(("canvas-inline-text", id), cx, |window, cx| {
                    let textarea = cx.new(|cx| {
                        TextareaState::new(window, cx)
                            .auto_grow(1, 50)
                            .default_value(initial_text)
                            .placeholder(placeholder)
                    });
                    let sub = cx.subscribe_in(
                        &textarea,
                        window,
                        move |_this: &mut InlineTextState, textarea, event: &InputEvent, _window, cx| {
                            if let InputEvent::Change = event {
                                let val = textarea.read(cx).value().to_string();
                                app_for_edit.update(cx, |app, cx| {
                                    if let Some(it) = app.state.find_item_mut(id)
                                        && let Some(txt) = &mut it.text
                                        && txt.content != val
                                    {
                                        txt.content = val;
                                        app.state.sync_text_item_size(id);
                                        cx.notify();
                                    }
                                });
                            }
                        },
                    );
                    InlineTextState {
                        textarea,
                        _sub: sub,
                    }
                });

                let textarea_entity = state.read(cx).textarea.clone();
                if focus_inline {
                    textarea_entity.read(cx).focus_handle(cx).focus(window, cx);
                }
                if textarea_entity.read(cx).value().as_ref() != current_text.as_str()
                    && !textarea_entity.read(cx).focus_handle(cx).is_focused(window)
                {
                    textarea_entity.update(cx, |st, cx| {
                        st.set_value(current_text, window, cx);
                    });
                }

                let app_for_esc = app_entity.clone();
                let focus_for_esc = canvas_focus.clone();
                div()
                    .id(("inline-text-wrap", id))
                    .size_full()
                    .p(px(pad_px))
                    .cursor_text()
                    .when(!font_family.is_empty(), |d| d.font_family(font_family))
                    .text_size(px(font_px))
                    .line_height(relative(1.25))
                    .text_color(tb.color)
                    .bg(rgb(0xFFFFFF).opacity(0.6))
                    .border_1()
                    .border_color(primary_border.opacity(0.6))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_key_down(move |ev: &KeyDownEvent, window: &mut Window, cx| {
                        if ev.keystroke.key.eq_ignore_ascii_case("escape") {
                            app_for_esc.update(cx, |app, cx| {
                                app.state.editing_text_id = None;
                                cx.notify();
                            });
                            focus_for_esc.focus(window, cx);
                        }
                    })
                    .child(BaseTextarea::new(&textarea_entity))
                    .into_any_element()
            } else {
                div()
                    .size_full()
                    .overflow_hidden()
                    .p(px(pad_px))
                    .when(!font_family.is_empty(), |d| d.font_family(font_family))
                    .text_size(px(font_px))
                    .line_height(relative(1.25))
                    .map(|d| match tb.align {
                        TextAlignMode::Left | TextAlignMode::Justified => d.text_left(),
                        TextAlignMode::Center => d.text_center(),
                        TextAlignMode::Right => d.text_right(),
                    })
                    .text_color(if is_empty {
                        cx.theme().muted_foreground
                    } else {
                        tb.color
                    })
                    .when(!is_selected, |d| {
                        d.border_1()
                            .border_dashed()
                            .border_color(rgb(0x94A3B8).opacity(0.35))
                    })
                    .child(if is_empty {
                        lang.tr("app.text_placeholder")
                    } else {
                        tb.content.clone().into()
                    })
                    .into_any_element()
            }
        } else if let Some(asset) = &item.image {
            div()
                .size_full()
                .overflow_hidden()
                .when(item.corner_radius > 0.0, |d| {
                    d.rounded(px(item.corner_radius * display_scale))
                })
                .when_some(item.bg_color, |d, bg| d.bg(bg))
                .child(
                    img(asset.rendered.clone())
                        .size_full()
                        .object_fit(ObjectFit::Fill),
                )
                .into_any_element()
        } else {
            v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .bg(cx.theme().muted)
                .border_1()
                .border_dashed()
                .border_color(cx.theme().border)
                .when(item.corner_radius > 0.0, |d| {
                    d.rounded(px(item.corner_radius * display_scale))
                })
                .child(
                    Icon::new(IconName::Plus)
                        .with_size(Size::Small)
                        .text_color(cx.theme().muted_foreground),
                )
                .into_any_element()
        }
    } else {
        // Mode Penuh & Pas Foto: Menggunakan clipping mask bentuk (persegi/oval)
        div()
            .relative()
            .size_full()
            .overflow_hidden()
            .when(is_oval, |d| d.rounded(px(w.max(h))))
            .when(!is_oval && item.corner_radius > 0.0, |d| {
                d.rounded(px(item.corner_radius * display_scale))
            })
            .when_some(item.bg_color, |d, bg| d.bg(bg))
            .when(item.bg_color.is_none(), |d| {
                if has_image {
                    d.bg(cx.theme().background)
                } else {
                    d.bg(cx.theme().muted)
                        .border_1()
                        .border_dashed()
                        .border_color(cx.theme().border)
                }
            })
            .child(if let Some(asset) = &item.image {
                let (raw_w, raw_h) = asset.display_dimensions(&item.transform);
                let (draw_w_f, draw_h_f, offset_x_f, offset_y_f) =
                    crate::canvas::geometry::compute_fitted_image_geometry(
                        w,
                        h,
                        raw_w * display_scale,
                        raw_h * display_scale,
                        item.fit_mode,
                        item.transform.zoom_scale,
                        item.transform.pan_x,
                        item.transform.pan_y,
                    );

                div()
                    .absolute()
                    .left(px(offset_x_f))
                    .top(px(offset_y_f))
                    .w(px(draw_w_f))
                    .h(px(draw_h_f))
                    .child(
                        img(asset.rendered.clone())
                            .size_full()
                            .object_fit(ObjectFit::Fill),
                    )
                    .into_any_element()
            } else {
                v_flex()
                    .size_full()
                    .items_center()
                    .justify_center()
                    .child(
                        Icon::new(IconName::Plus)
                            .with_size(Size::Small)
                            .text_color(cx.theme().muted_foreground),
                    )
                    .into_any_element()
            })
            .into_any_element()
    };

    let has_image = item.image.is_some();
    let has_edit = has_image && item.transform.is_modified();
    let app_for_menu = app_entity.clone();
    let app_for_select = app_entity.clone();

    let mut outer = div()
        .id(("canvas-item", id))
        .absolute()
        .left(px(x))
        .top(px(y))
        .w(px(w))
        .h(px(h))
        .when(is_oval, |d| d.rounded(px(w.max(h))))
        .when(!is_oval && item.corner_radius > 0.0, |d| {
            d.rounded(px(item.corner_radius * display_scale))
        })
        .when(can_move, |d| d.cursor_move())
        .on_mouse_down(
            MouseButton::Left,
            cx.listener({
                let app = app_entity.clone();
                let focus = canvas_focus;
                move |this, ev: &MouseDownEvent, window: &mut Window, cx| {
                    cx.stop_propagation();
                    this.context_menu = None;
                    if is_editing_inline {
                        return;
                    }
                    focus.focus(window, cx);

                    let was_selected = app.read(cx).state.selected_id == Some(id);
                    app.update(cx, |app, cx| {
                        app.state.editing_text_id = None;
                        if app.state.selected_id != Some(id) {
                            app.state.selected_id = Some(id);
                            cx.notify();
                        }
                    });

                    if ev.click_count >= 2 {
                        if is_text {
                            this.pending_text_click = None;
                            this.drag_state = CanvasDrag::None;
                            app.update(cx, |app, cx| {
                                app.state.editing_text_id = Some(id);
                                cx.notify();
                            });
                        } else {
                            app.update(cx, |app, cx| {
                                app.prompt_single_image_for_item(id, cx);
                                cx.notify();
                            });
                        }
                        cx.notify();
                        return;
                    }

                    this.pending_text_click = if was_selected && is_text {
                        Some((id, ev.position.x.as_f32(), ev.position.y.as_f32()))
                    } else {
                        None
                    };

                    if can_move {
                        this.drag_state = CanvasDrag::MoveItem {
                            id,
                            start_mx: ev.position.x.as_f32(),
                            start_my: ev.position.y.as_f32(),
                            orig_x,
                            orig_y,
                        };
                    }
                    cx.notify();
                }
            }),
        )
        .context_menu(move |menu, _window, cx| {
            app_for_select.update(cx, |app: &mut StudioApp, cx| {
                app.state.selected_id = Some(id);
                cx.notify();
            });
            build_item_popup_menu(
                menu,
                id,
                mode,
                is_text,
                has_image,
                has_edit,
                lang,
                app_for_menu.clone(),
            )
        })
        .child(inner_frame)
        .when(cut_thickness > 0.0, |d| {
            d.child(
                div()
                    .absolute()
                    .inset_0()
                    .border(px(cut_thickness))
                    .border_color(rgb(0x000000))
                    .when(is_oval, |d| d.rounded(px(w.max(h))))
                    .when(!is_oval && item.corner_radius > 0.0, |d| {
                        d.rounded(px(item.corner_radius * display_scale))
                    }),
            )
        });

    // Outline / Selection ring (dashed outline without shifting bounds)
    if is_selected {
        outer = outer.child(
            div()
                .absolute()
                .left(px(-2.0))
                .top(px(-2.0))
                .w(px(w + 4.0))
                .h(px(h + 4.0))
                .border_2()
                .border_dashed()
                .border_color(cx.theme().primary)
                .when(is_oval, |d| d.rounded(px((w + 4.0).max(h + 4.0))))
                .when(!is_oval && item.corner_radius > 0.0, |d| {
                    d.rounded(px(item.corner_radius * display_scale + 2.0))
                }),
        );

        if is_freeform {
            // Resize handle on bottom-right corner for Freeform mode
            let handle_sz = 14.0f32;
            outer = outer.child(
                div()
                    .id(("resize-handle", id))
                    .absolute()
                    .right(px(-handle_sz * 0.5))
                    .bottom(px(-handle_sz * 0.5))
                    .w(px(handle_sz))
                    .h(px(handle_sz))
                    .rounded_full()
                    .bg(cx.theme().primary)
                    .border_2()
                    .border_color(rgb(0xFFFFFF))
                    .cursor_nwse_resize()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            this.drag_state = CanvasDrag::ResizeItem {
                                id,
                                start_mx: ev.position.x.as_f32(),
                                start_my: ev.position.y.as_f32(),
                                orig_w,
                                orig_h,
                            };
                            cx.notify();
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.drag_state = CanvasDrag::None;
                            cx.notify();
                        }),
                    ),
            );
        }
    }

    outer.into_any_element()
}
