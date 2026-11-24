use gpui_kit::{
    AnyElement, AppContext as _, Bounds, Context, Entity, FocusHandle, Focusable as _, FontWeight,
    Hsla, InteractiveElement as _, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent,
    ObjectFit, ParentElement, PathBuilder, Pixels, Point, Styled, StyledImage as _, Subscription,
    Window,
    assets::IconName,
    base::input::{InputEvent, Textarea as BaseTextarea, TextareaState},
    canvas,
    component::{
        ActiveTheme as _, Icon, Sizable as _, Size, v_flex,
        menu::ContextMenuExt,
    },
    div, img, point,
    prelude::FluentBuilder as _,
    px, relative, rgb,
};

use crate::{
    app::StudioApp,
    canvas::{
        canvas_view::CanvasView,
        context_menu::build_item_popup_menu,
        geometry::{TEXT_BOX_PAD_PX, freeform_shape_polygon},
        model::{
            CanvasDrag, CanvasItem, FreeformShapeKind, LayoutMode, ResizeHandle, ShapeBoxData,
            ShapeKind, TextAlignMode,
        },
    },
    shared::i18n::Language,
};

const KAPPA: f32 = 0.552_284_8;

#[inline]
fn rot_pt(
    ox: f32,
    oy: f32,
    cx: f32,
    cy: f32,
    x: f32,
    y: f32,
    cos_r: f32,
    sin_r: f32,
) -> Point<Pixels> {
    let dx = x - cx;
    let dy = y - cy;
    point(
        px(ox + cx + dx * cos_r - dy * sin_r),
        px(oy + cy + dx * sin_r + dy * cos_r),
    )
}

fn append_shape_geometry(
    builder: &mut PathBuilder,
    kind: FreeformShapeKind,
    ox: f32,
    oy: f32,
    w: f32,
    h: f32,
    inset: f32,
    corner_radius: f32,
    cos_r: f32,
    sin_r: f32,
) {
    let cx = w * 0.5;
    let cy = h * 0.5;
    let pt = |x: f32, y: f32| rot_pt(ox, oy, cx, cy, x, y, cos_r, sin_r);

    match kind {
        FreeformShapeKind::Line => {
            builder.move_to(pt(0.0, cy));
            builder.line_to(pt(w, cy));
        }
        FreeformShapeKind::Rectangle => {
            let x0 = inset.min(cx);
            let y0 = inset.min(cy);
            let x1 = (w - inset).max(cx);
            let y1 = (h - inset).max(cy);
            let rw = (x1 - x0).max(0.5);
            let rh = (y1 - y0).max(0.5);
            let r = (corner_radius - inset).clamp(0.0, rw.min(rh) * 0.5);
            if r <= 0.5 {
                builder.move_to(pt(x0, y0));
                builder.line_to(pt(x1, y0));
                builder.line_to(pt(x1, y1));
                builder.line_to(pt(x0, y1));
                builder.close();
            } else {
                let k = r * KAPPA;
                builder.move_to(pt(x0 + r, y0));
                builder.line_to(pt(x1 - r, y0));
                builder.cubic_bezier_to(pt(x1, y0 + r), pt(x1 - r + k, y0), pt(x1, y0 + r - k));
                builder.line_to(pt(x1, y1 - r));
                builder.cubic_bezier_to(pt(x1 - r, y1), pt(x1, y1 - r + k), pt(x1 - r + k, y1));
                builder.line_to(pt(x0 + r, y1));
                builder.cubic_bezier_to(pt(x0, y1 - r), pt(x0 + r - k, y1), pt(x0, y1 - r + k));
                builder.line_to(pt(x0, y0 + r));
                builder.cubic_bezier_to(pt(x0 + r, y0), pt(x0, y0 + r - k), pt(x0 + r - k, y0));
                builder.close();
            }
        }
        FreeformShapeKind::Circle => {
            let rx = (cx - inset).max(0.5);
            let ry = (cy - inset).max(0.5);
            let kx = rx * KAPPA;
            let ky = ry * KAPPA;
            builder.move_to(pt(cx, cy - ry));
            builder.cubic_bezier_to(
                pt(cx + rx, cy),
                pt(cx + kx, cy - ry),
                pt(cx + rx, cy - ky),
            );
            builder.cubic_bezier_to(
                pt(cx, cy + ry),
                pt(cx + rx, cy + ky),
                pt(cx + kx, cy + ry),
            );
            builder.cubic_bezier_to(
                pt(cx - rx, cy),
                pt(cx - kx, cy + ry),
                pt(cx - rx, cy + ky),
            );
            builder.cubic_bezier_to(
                pt(cx, cy - ry),
                pt(cx - rx, cy - ky),
                pt(cx - kx, cy - ry),
            );
            builder.close();
        }
        FreeformShapeKind::Triangle | FreeformShapeKind::Diamond | FreeformShapeKind::Star => {
            let verts = freeform_shape_polygon(kind, w, h);
            let sx = if w > 1.0 { ((w - inset * 2.0) / w).clamp(0.1, 1.0) } else { 1.0 };
            let sy = if h > 1.0 { ((h - inset * 2.0) / h).clamp(0.1, 1.0) } else { 1.0 };
            for (i, (vx, vy)) in verts.into_iter().enumerate() {
                let px_v = cx + (vx - cx) * sx;
                let py_v = cy + (vy - cy) * sy;
                let p = pt(px_v, py_v);
                if i == 0 {
                    builder.move_to(p);
                } else {
                    builder.line_to(p);
                }
            }
            builder.close();
        }
    }
}

fn paint_freeform_shape(
    sb: &ShapeBoxData,
    corner_radius_disp: f32,
    stroke_width_disp: f32,
    rotation_deg: f32,
    opacity: f32,
    bounds: Bounds<Pixels>,
    window: &mut Window,
) {
    let w = f32::from(bounds.size.width);
    let h = f32::from(bounds.size.height);
    let ox = f32::from(bounds.origin.x);
    let oy = f32::from(bounds.origin.y);
    if w <= 0.5 || h <= 0.5 || opacity <= 0.001 {
        return;
    }

    let rad = rotation_deg.to_radians();
    let cos_r = rad.cos();
    let sin_r = rad.sin();
    let apply_op = |c: Hsla| Hsla {
        a: (c.a * opacity).clamp(0.0, 1.0),
        ..c
    };

    if sb.kind == FreeformShapeKind::Line {
        let col = sb.stroke_color.unwrap_or(sb.fill_color);
        let sw = stroke_width_disp.max(1.5);
        let mut builder = PathBuilder::stroke(px(sw));
        append_shape_geometry(&mut builder, sb.kind, ox, oy, w, h, 0.0, 0.0, cos_r, sin_r);
        if let Ok(path) = builder.build() {
            window.paint_path(path, apply_op(col));
        }
        return;
    }

    if sb.fill_color.a > 0.0 {
        let mut fill_builder = PathBuilder::fill();
        append_shape_geometry(
            &mut fill_builder,
            sb.kind,
            ox,
            oy,
            w,
            h,
            0.0,
            corner_radius_disp,
            cos_r,
            sin_r,
        );
        if let Ok(path) = fill_builder.build() {
            window.paint_path(path, apply_op(sb.fill_color));
        }
    }

    if let Some(stroke_col) = sb.stroke_color
        && stroke_col.a > 0.0
        && stroke_width_disp > 0.1
    {
        let sw = stroke_width_disp.max(1.0);
        let mut stroke_builder = PathBuilder::stroke(px(sw));
        append_shape_geometry(
            &mut stroke_builder,
            sb.kind,
            ox,
            oy,
            w,
            h,
            sw * 0.5,
            corner_radius_disp,
            cos_r,
            sin_r,
        );
        if let Ok(path) = stroke_builder.build() {
            window.paint_path(path, apply_op(stroke_col));
        }
    }
}

fn paint_dashed_selection_outline(
    bounds: Bounds<Pixels>,
    shape_kind: Option<FreeformShapeKind>,
    corner_radius_disp: f32,
    stroke_disp: f32,
    rotation_deg: f32,
    is_oval: bool,
    color: Hsla,
    window: &mut Window,
) {
    let ox = f32::from(bounds.origin.x);
    let oy = f32::from(bounds.origin.y);
    let bw = f32::from(bounds.size.width);
    let bh = f32::from(bounds.size.height);
    if bw <= 4.0 || bh <= 4.0 {
        return;
    }

    let rad = rotation_deg.to_radians();
    let cos_r = rad.cos();
    let sin_r = rad.sin();
    let item_ox = ox + 2.0;
    let item_oy = oy + 2.0;
    let item_w = (bw - 4.0).max(1.0);
    let item_h = (bh - 4.0).max(1.0);

    // 1. Primary dashed selection frame (rotates with shape, follows oval/rounded corners/Line)
    let mut builder = PathBuilder::stroke(px(1.5)).dash_array(&[px(5.0), px(3.0)]);
    if is_oval {
        append_shape_geometry(
            &mut builder,
            FreeformShapeKind::Circle,
            ox,
            oy,
            bw,
            bh,
            1.0,
            0.0,
            cos_r,
            sin_r,
        );
    } else if shape_kind == Some(FreeformShapeKind::Line) {
        let hull_h = (stroke_disp + 10.0).max(14.0).min(bh);
        let hull_oy = oy + (bh - hull_h) * 0.5;
        append_shape_geometry(
            &mut builder,
            FreeformShapeKind::Rectangle,
            ox,
            hull_oy,
            bw,
            hull_h,
            1.0,
            4.0,
            cos_r,
            sin_r,
        );
    } else {
        let r_clamped = if corner_radius_disp > 0.0 {
            corner_radius_disp.min(item_w.min(item_h) * 0.5) + 2.0
        } else {
            0.0
        };
        append_shape_geometry(
            &mut builder,
            FreeformShapeKind::Rectangle,
            ox,
            oy,
            bw,
            bh,
            1.0,
            r_clamped,
            cos_r,
            sin_r,
        );
    }
    if let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }

    // 2. For non-rectangular vector shapes, draw a subtle contour hugging the exact shape silhouette
    if let Some(kind @ (FreeformShapeKind::Circle
        | FreeformShapeKind::Triangle
        | FreeformShapeKind::Diamond
        | FreeformShapeKind::Star)) = shape_kind
    {
        let mut contour = PathBuilder::stroke(px(1.0));
        append_shape_geometry(
            &mut contour,
            kind,
            item_ox,
            item_oy,
            item_w,
            item_h,
            0.0,
            0.0,
            cos_r,
            sin_r,
        );
        if let Ok(path) = contour.build() {
            window.paint_path(path, color.opacity(0.5));
        }
    }
}

struct InlineTextState {
    textarea: Entity<TextareaState>,
    _sub: Subscription,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_canvas_item(
    item: &CanvasItem,
    display_scale: f32,
    paper_dpi: u32,
    selected_id: Option<u64>,
    selected_ids: &[u64],
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
    let is_primary_selected = selected_id == Some(id);
    let is_selected = is_primary_selected || selected_ids.contains(&id);
    let is_locked = item.locked;
    let orig_x = item.x_px;
    let orig_y = item.y_px;
    let orig_w = item.width_px;
    let orig_h = item.height_px;

    // Pixel snapping prevents subpixel right/bottom drift between shape fill and selection outline.
    let x = (orig_x * display_scale).round();
    let y = (orig_y * display_scale).round();
    let is_freeform = mode == LayoutMode::Freeform;
    let is_text = is_freeform && item.is_text();
    let is_shape = is_freeform && item.is_shape();
    let is_editing_inline = is_text && !is_locked && editing_text_id == Some(id);
    let has_image = item.image.is_some();
    let min_disp = if is_freeform && (has_image || is_text || is_shape) {
        4.0
    } else {
        18.0
    };
    let w = (orig_w * display_scale).round().max(min_disp);
    let h = (orig_h * display_scale).round().max(min_disp);
    let can_move = !is_editing_inline
        && !is_locked
        && (is_freeform || (mode == LayoutMode::IdPhoto && id_photo_free_move));
    let is_oval = !is_freeform && item.shape_kind == ShapeKind::CircleOval;
    let cut_thickness = if item.cut_line && !is_freeform {
        (item.cut_line_width * display_scale).clamp(1.0, 16.0)
    } else {
        0.0
    };
    let item_opacity = item.opacity.clamp(0.05, 1.0);

    let inner_frame = if is_freeform {
        // Freeform mode: render photo naturally, text box, or vector shape
        if let Some(sb) = &item.shape {
            let sb_owned = sb.clone();
            let corner_disp = item.corner_radius * display_scale;
            let stroke_disp = sb.stroke_width * display_scale;
            let rot_deg = item.rotation_deg;
            canvas(
                |_, _, _| {},
                move |bounds, (), window, _| {
                    paint_freeform_shape(
                        &sb_owned,
                        corner_disp,
                        stroke_disp,
                        rot_deg,
                        item_opacity,
                        bounds,
                        window,
                    );
                },
            )
            .w(px(w))
            .h(px(h))
            .into_any_element()
        } else if let Some(tb) = &item.text {
            let font_px = (tb.font_size_pt.clamp(6.0, 300.0) * (paper_dpi.max(72) as f32 / 72.0)
                * display_scale)
                .max(4.0);
            let pad_px = (TEXT_BOX_PAD_PX * display_scale).max(2.0);
            let is_empty = tb.content.trim().is_empty();
            let font_family = tb.font_family.clone();
            let lh = tb.line_height.clamp(0.9, 2.5);

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
                    .line_height(relative(lh))
                    .when(tb.bold, |d| d.font_weight(FontWeight::BOLD))
                    .when(tb.italic, |d| d.italic())
                    .when(tb.underline, |d| d.underline())
                    .text_color(tb.color)
                    .bg(item.bg_color.unwrap_or_else(|| rgb(0xFFFFFF).opacity(0.6).into()))
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
                    .opacity(item_opacity)
                    .when_some(item.bg_color, |d, bg| d.bg(bg))
                    .when(!font_family.is_empty(), |d| d.font_family(font_family))
                    .text_size(px(font_px))
                    .line_height(relative(lh))
                    .when(tb.bold, |d| d.font_weight(FontWeight::BOLD))
                    .when(tb.italic, |d| d.italic())
                    .when(tb.underline, |d| d.underline())
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
                .opacity(item_opacity)
                .when(item.corner_radius > 0.0, |d| {
                    d.rounded(px(item.corner_radius * display_scale))
                })
                .when_some(item.bg_color, |d, bg| d.bg(bg))
                .child(
                    img(asset.rendered.clone())
                        .w(px(w))
                        .h(px(h))
                        .aspect_ratio(w / h.max(1.0))
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
                            .aspect_ratio(draw_w_f / draw_h_f.max(1.0))
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

                    let was_selected = app.read(cx).state.is_item_selected(id);
                    let shift = ev.modifiers.shift;
                    app.update(cx, |app, cx| {
                        app.state.editing_text_id = None;
                        if is_freeform && shift {
                            app.state.toggle_select_item(id);
                            cx.notify();
                        } else if !was_selected {
                            app.state.select_single(Some(id));
                            cx.notify();
                        } else if app.state.selected_id != Some(id) {
                            app.state.selected_id = Some(id);
                            cx.notify();
                        }
                    });

                    if shift && is_freeform {
                        this.pending_text_click = None;
                        this.drag_state = CanvasDrag::None;
                        cx.notify();
                        return;
                    }

                    if ev.click_count >= 2 && !is_locked {
                        if is_text {
                            this.pending_text_click = None;
                            this.drag_state = CanvasDrag::None;
                            app.update(cx, |app, cx| {
                                app.state.push_freeform_undo();
                                app.state.editing_text_id = Some(id);
                                cx.notify();
                            });
                        } else if !is_shape {
                            app.update(cx, |app, cx| {
                                app.prompt_single_image_for_item(id, cx);
                                cx.notify();
                            });
                        }
                        cx.notify();
                        return;
                    }

                    this.pending_text_click = if was_selected && is_text && !is_locked {
                        Some((id, ev.position.x.as_f32(), ev.position.y.as_f32()))
                    } else {
                        None
                    };

                    if can_move {
                        let orig_positions = if is_freeform {
                            let st = &app.read(cx).state;
                            let sel_ids = st.selected_freeform_ids();
                            let mut positions: Vec<(u64, f32, f32)> = st
                                .free_items
                                .iter()
                                .filter(|it| sel_ids.contains(&it.id) && !it.locked && it.visible)
                                .map(|it| (it.id, it.x_px, it.y_px))
                                .collect();
                            if !positions.iter().any(|(pid, _, _)| *pid == id) {
                                positions.push((id, orig_x, orig_y));
                            }
                            positions
                        } else {
                            vec![(id, orig_x, orig_y)]
                        };
                        this.drag_undo_pushed = false;
                        this.drag_state = CanvasDrag::MoveItems {
                            anchor_id: id,
                            start_mx: ev.position.x.as_f32(),
                            start_my: ev.position.y.as_f32(),
                            orig_positions,
                        };
                    }
                    cx.notify();
                }
            }),
        )
        .context_menu(move |menu, _window, cx| {
            app_for_select.update(cx, |app: &mut StudioApp, cx| {
                if !app.state.is_item_selected(id) {
                    app.state.select_single(Some(id));
                } else {
                    app.state.selected_id = Some(id);
                }
                cx.notify();
            });
            build_item_popup_menu(
                menu,
                id,
                mode,
                is_text || is_shape,
                has_image,
                has_edit,
                is_locked,
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

    // Outline / Selection ring (vector PathBuilder explicitly sized to exact integer bounds;
    // rotates with shapes, follows rounded corners / ovals / Line, and hugs non-rect contours)
    if is_selected {
        let ring_color = if is_locked {
            cx.theme().muted_foreground
        } else {
            cx.theme().primary
        };
        let shape_kind_opt = if is_freeform {
            item.shape.as_ref().map(|s| s.kind)
        } else {
            None
        };
        let corner_disp = if !is_oval && item.corner_radius > 0.0 {
            item.corner_radius * display_scale
        } else {
            0.0
        };
        let stroke_disp = item
            .shape
            .as_ref()
            .map(|s| s.stroke_width * display_scale)
            .unwrap_or(0.0);
        let rot_deg = if is_shape { item.rotation_deg } else { 0.0 };

        outer = outer.child(
            canvas(
                |_, _, _| {},
                move |bounds, (), window, _| {
                    paint_dashed_selection_outline(
                        bounds,
                        shape_kind_opt,
                        corner_disp,
                        stroke_disp,
                        rot_deg,
                        is_oval,
                        ring_color,
                        window,
                    );
                },
            )
            .absolute()
            .left(px(-2.0))
            .top(px(-2.0))
            .w(px(w + 4.0))
            .h(px(h + 4.0)),
        );

        if is_primary_selected && is_freeform && !is_locked && !is_editing_inline {
            let corner_sz = 10.0f32;
            let primary = cx.theme().primary;
            let mut handles: Vec<AnyElement> = Vec::with_capacity(8);
            let is_line = shape_kind_opt == Some(FreeformShapeKind::Line);
            let is_rotated = rot_deg.abs() > 0.01;
            let rad = rot_deg.to_radians();
            let cos_r = rad.cos();
            let sin_r = rad.sin();
            let cx_loc = w * 0.5;
            let cy_loc = h * 0.5;
            let placed_pos = |lx: f32, ly: f32, hw: f32, hh: f32| -> (f32, f32) {
                let dx = lx - cx_loc;
                let dy = ly - cy_loc;
                let rx = cx_loc + dx * cos_r - dy * sin_r;
                let ry = cy_loc + dx * sin_r + dy * cos_r;
                ((rx - hw * 0.5).round(), (ry - hh * 0.5).round())
            };

            let mut push_handle = |handle: ResizeHandle,
                                   handle_div: gpui_kit::Stateful<gpui_kit::Div>| {
                handles.push(
                    handle_div
                        .bg(primary)
                        .border_2()
                        .border_color(rgb(0xFFFFFF))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                this.drag_undo_pushed = false;
                                this.drag_state = CanvasDrag::ResizeItem {
                                    id,
                                    handle,
                                    start_mx: ev.position.x.as_f32(),
                                    start_my: ev.position.y.as_f32(),
                                    orig_x,
                                    orig_y,
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
                                this.active_guides.clear();
                                cx.notify();
                            }),
                        )
                        .into_any_element(),
                );
            };

            if is_line {
                // Line shape: 2 endpoint handles rotating with the line
                let (lx, ly) = placed_pos(0.0, cy_loc, corner_sz, corner_sz);
                push_handle(
                    ResizeHandle::Left,
                    div()
                        .id(("resize-l", id))
                        .absolute()
                        .left(px(lx))
                        .top(px(ly))
                        .w(px(corner_sz))
                        .h(px(corner_sz))
                        .rounded_full()
                        .cursor_ew_resize(),
                );
                let (rx, ry) = placed_pos(w, cy_loc, corner_sz, corner_sz);
                push_handle(
                    ResizeHandle::Right,
                    div()
                        .id(("resize-r", id))
                        .absolute()
                        .left(px(rx))
                        .top(px(ry))
                        .w(px(corner_sz))
                        .h(px(corner_sz))
                        .rounded_full()
                        .cursor_ew_resize(),
                );
            } else {
                // 4 corner handles (rotated around center when shape is rotated)
                let (tl_x, tl_y) = placed_pos(0.0, 0.0, corner_sz, corner_sz);
                push_handle(
                    ResizeHandle::TopLeft,
                    div()
                        .id(("resize-tl", id))
                        .absolute()
                        .left(px(tl_x))
                        .top(px(tl_y))
                        .w(px(corner_sz))
                        .h(px(corner_sz))
                        .rounded_full()
                        .cursor_nwse_resize(),
                );
                let (tr_x, tr_y) = placed_pos(w, 0.0, corner_sz, corner_sz);
                push_handle(
                    ResizeHandle::TopRight,
                    div()
                        .id(("resize-tr", id))
                        .absolute()
                        .left(px(tr_x))
                        .top(px(tr_y))
                        .w(px(corner_sz))
                        .h(px(corner_sz))
                        .rounded_full()
                        .cursor_nesw_resize(),
                );
                let (bl_x, bl_y) = placed_pos(0.0, h, corner_sz, corner_sz);
                push_handle(
                    ResizeHandle::BottomLeft,
                    div()
                        .id(("resize-bl", id))
                        .absolute()
                        .left(px(bl_x))
                        .top(px(bl_y))
                        .w(px(corner_sz))
                        .h(px(corner_sz))
                        .rounded_full()
                        .cursor_nesw_resize(),
                );
                let (br_x, br_y) = placed_pos(w, h, corner_sz, corner_sz);
                push_handle(
                    ResizeHandle::BottomRight,
                    div()
                        .id(("resize-br", id))
                        .absolute()
                        .left(px(br_x))
                        .top(px(br_y))
                        .w(px(corner_sz))
                        .h(px(corner_sz))
                        .rounded_full()
                        .cursor_nwse_resize(),
                );

                // 4 side handles (pills when unrotated, compact dots when rotated)
                if w >= 36.0 {
                    let (pw, ph) = if is_rotated { (8.0, 8.0) } else { (14.0, 6.0) };
                    let (tx, ty) = placed_pos(cx_loc, 0.0, pw, ph);
                    push_handle(
                        ResizeHandle::Top,
                        div()
                            .id(("resize-t", id))
                            .absolute()
                            .left(px(tx))
                            .top(px(ty))
                            .w(px(pw))
                            .h(px(ph))
                            .rounded_full()
                            .cursor_ns_resize(),
                    );
                    let (bx, by) = placed_pos(cx_loc, h, pw, ph);
                    push_handle(
                        ResizeHandle::Bottom,
                        div()
                            .id(("resize-b", id))
                            .absolute()
                            .left(px(bx))
                            .top(px(by))
                            .w(px(pw))
                            .h(px(ph))
                            .rounded_full()
                            .cursor_ns_resize(),
                    );
                }
                if h >= 36.0 {
                    let (pw, ph) = if is_rotated { (8.0, 8.0) } else { (6.0, 14.0) };
                    let (lx, ly) = placed_pos(0.0, cy_loc, pw, ph);
                    push_handle(
                        ResizeHandle::Left,
                        div()
                            .id(("resize-l", id))
                            .absolute()
                            .left(px(lx))
                            .top(px(ly))
                            .w(px(pw))
                            .h(px(ph))
                            .rounded_full()
                            .cursor_ew_resize(),
                    );
                    let (rx, ry) = placed_pos(w, cy_loc, pw, ph);
                    push_handle(
                        ResizeHandle::Right,
                        div()
                            .id(("resize-r", id))
                            .absolute()
                            .left(px(rx))
                            .top(px(ry))
                            .w(px(pw))
                            .h(px(ph))
                            .rounded_full()
                            .cursor_ew_resize(),
                    );
                }
            }
            outer = outer.children(handles);
        }
    }

    outer.into_any_element()
}
