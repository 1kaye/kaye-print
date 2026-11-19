use gpui_kit::{
    AppContext as _, Context, Entity, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit, ParentElement, Styled,
    StyledImage as _, Subscription, Window, hsla,
    assets::IconName,
    component::{
        ActiveTheme as _, IndexPath, Sizable as _,
        button::{Button, ButtonVariants as _},
        h_flex,
        select::{SearchableVec, Select, SelectEvent, SelectState},
        separator::Separator,
        toolbar::Toolbar,
        v_flex,
    },
    div, img,
    prelude::FluentBuilder as _,
    px,
};

use crate::{
    app::StudioApp,
    canvas::geometry::{apply_image_transform, dynamic_to_render_image},
    editor::{CropDragHandle, EditorSession},
    shared::CropUnit,
};

struct CropUnitSelectState {
    select: Entity<SelectState<SearchableVec<CropUnit>>>,
    _sub: Subscription,
}

pub fn render_crop_view(
    app: &mut StudioApp,
    session: &EditorSession,
    window: &mut Window,
    cx: &mut Context<StudioApp>,
) -> impl IntoElement {
    let lang = app.state.language;
    let crop_unit = app.state.crop_unit;
    let paper_dpi = app.state.paper.dpi;
    let app_for_unit = cx.entity();

    let unit_select_state = window.use_keyed_state("crop-unit-select", cx, |window, cx| {
        let items = SearchableVec::new(CropUnit::ALL.to_vec());
        let select = cx.new(|cx| {
            SelectState::new(items, Some(IndexPath::new(crop_unit.index())), window, cx)
        });
        let sub = cx.subscribe_in(
            &select,
            window,
            move |_this: &mut CropUnitSelectState,
                  _select,
                  event: &SelectEvent<SearchableVec<CropUnit>>,
                  _window,
                  cx| {
                if let SelectEvent::Confirm(Some(chosen)) = event {
                    let chosen_unit = *chosen;
                    app_for_unit.update(cx, |app, cx| {
                        app.state.crop_unit = chosen_unit;
                        cx.notify();
                    });
                }
            },
        );
        CropUnitSelectState { select, _sub: sub }
    });

    let unit_select_entity = unit_select_state.read(cx).select.clone();
    if unit_select_entity.read(cx).selected_index(cx) != Some(IndexPath::new(crop_unit.index())) {
        unit_select_entity.update(cx, |state, cx| {
            state.set_selected_index(Some(IndexPath::new(crop_unit.index())), window, cx);
        });
    }
    // 1. Use the uncropped image cached during start_crop() for 0-lag rendering
    let base_rendered = session.crop_base_rendered.clone().unwrap_or_else(|| {
        let mut base_tf = session.working_transform.clone();
        base_tf.crop_rect = [0.0, 0.0, 1.0, 1.0];
        let base_processed = apply_image_transform(&session.preview_asset.preview_raw, &base_tf);
        dynamic_to_render_image(&base_processed)
    });

    let is_rotated_90 = session.working_transform.quarter_turns_cw.rem_euclid(2) != 0;
    let orig_w = session.preview_asset.orig_width.max(1) as f32;
    let orig_h = session.preview_asset.orig_height.max(1) as f32;
    let (cur_w, cur_h) = if is_rotated_90 {
        (orig_h, orig_w)
    } else {
        (orig_w, orig_h)
    };

    let vp = window.viewport_size();
    let win_w = f32::from(vp.width);
    let win_h = f32::from(vp.height);

    let max_disp_w = (win_w - 60.0).clamp(240.0, 720.0);
    let max_disp_h = (win_h - 140.0).clamp(200.0, 520.0);
    let fit_scale = (max_disp_w / cur_w).min(max_disp_h / cur_h);
    let disp_w = (cur_w * fit_scale).round().max(24.0);
    let disp_h = (cur_h * fit_scale).round().max(24.0);

    // Pixel snapping: round to integer pixels to prevent subpixel gap artifacts / phantom lines
    let [cx_norm, cy_norm, cw_norm, ch_norm] = session.crop_draft_rect;
    let crop_x = (disp_w * cx_norm).round().clamp(0.0, disp_w - 20.0);
    let crop_y = (disp_h * cy_norm).round().clamp(0.0, disp_h - 20.0);
    let crop_w = (disp_w * cw_norm).round().clamp(20.0, disp_w - crop_x);
    let crop_h = (disp_h * ch_norm).round().clamp(20.0, disp_h - crop_y);

    let shadow_color = hsla(0.0, 0.0, 0.0, 0.65);
    let handle_bg = cx.theme().background;
    let primary_color = cx.theme().primary;

    let dim_bottom_h = disp_h - (crop_y + crop_h);
    let dim_right_w = disp_w - (crop_x + crop_w);

    v_flex()
        .size_full()
        .bg(cx.theme().background)
        // Top Navigation Bar Crop
        .child(
            h_flex()
                .w_full()
                .h(px(48.0))
                .px(px(14.0))
                .items_center()
                .justify_between()
                .bg(cx.theme().sidebar)
                .border_b_1()
                .border_color(cx.theme().border)
                .child(
                    Toolbar::new("crop-header-toolbar")
                        .small()
                        .child(
                            Button::new("crop-btn-cancel")
                                .ghost()
                                .small()
                                .icon(IconName::ArrowLeft)
                                .label(lang.tr("app.cancel"))
                                .tooltip(lang.tr("app.crop_cancel_tooltip"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.mutate_editor_session(|s| {
                                        s.cancel_crop();
                                    });
                                    cx.notify();
                                })),
                        )
                        .content(Separator::vertical().h(px(18.0)).mx(px(6.0)))
                        .child(
                            Button::new("crop-btn-reset")
                                .ghost()
                                .small()
                                .icon(IconName::RefreshCw)
                                .label(lang.tr("app.crop_reset_area"))
                                .tooltip(lang.tr("app.crop_reset_area_tooltip"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.mutate_editor_session(|s| {
                                        s.reset_crop_draft();
                                    });
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    Button::new("crop-btn-apply")
                        .primary()
                        .small()
                        .icon(IconName::Check)
                        .label(lang.tr("app.crop_apply"))
                        .tooltip(lang.tr("app.crop_apply_tooltip"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.mutate_editor_session(|s| {
                                s.apply_crop();
                            });
                            cx.notify();
                        })),
                ),
        )
        // Crop Canvas Surface
        .child(
            v_flex()
                .flex_1()
                .w_full()
                .overflow_hidden()
                .items_center()
                .justify_center()
                .p(px(20.0))
                .on_mouse_move(cx.listener(move |this, ev: &MouseMoveEvent, _, cx| {
                    let mx = f32::from(ev.position.x);
                    let my = f32::from(ev.position.y);
                    let mut changed = false;
                    this.mutate_editor_session(|s| {
                        if let Some((handle, sx, sy, init_rect)) = s.crop_drag {
                            let dx = (mx - sx) / disp_w.max(1.0);
                            let dy = (my - sy) / disp_h.max(1.0);
                            let [ix, iy, iw, ih] = init_rect;
                            let min_s = 0.05_f32;

                            match handle {
                                CropDragHandle::Left => {
                                    let new_x = (ix + dx).clamp(0.0, ix + iw - min_s);
                                    let new_w = iw - (new_x - ix);
                                    s.crop_draft_rect[0] = new_x;
                                    s.crop_draft_rect[2] = new_w;
                                }
                                CropDragHandle::Right => {
                                    let new_w = (iw + dx).clamp(min_s, 1.0 - ix);
                                    s.crop_draft_rect[2] = new_w;
                                }
                                CropDragHandle::Top => {
                                    let new_y = (iy + dy).clamp(0.0, iy + ih - min_s);
                                    let new_h = ih - (new_y - iy);
                                    s.crop_draft_rect[1] = new_y;
                                    s.crop_draft_rect[3] = new_h;
                                }
                                CropDragHandle::Bottom => {
                                    let new_h = (ih + dy).clamp(min_s, 1.0 - iy);
                                    s.crop_draft_rect[3] = new_h;
                                }
                                CropDragHandle::TopLeft => {
                                    let new_x = (ix + dx).clamp(0.0, ix + iw - min_s);
                                    let new_w = iw - (new_x - ix);
                                    let new_y = (iy + dy).clamp(0.0, iy + ih - min_s);
                                    let new_h = ih - (new_y - iy);
                                    s.crop_draft_rect = [new_x, new_y, new_w, new_h];
                                }
                                CropDragHandle::TopRight => {
                                    let new_w = (iw + dx).clamp(min_s, 1.0 - ix);
                                    let new_y = (iy + dy).clamp(0.0, iy + ih - min_s);
                                    let new_h = ih - (new_y - iy);
                                    s.crop_draft_rect = [ix, new_y, new_w, new_h];
                                }
                                CropDragHandle::BottomLeft => {
                                    let new_x = (ix + dx).clamp(0.0, ix + iw - min_s);
                                    let new_w = iw - (new_x - ix);
                                    let new_h = (ih + dy).clamp(min_s, 1.0 - iy);
                                    s.crop_draft_rect = [new_x, iy, new_w, new_h];
                                }
                                CropDragHandle::BottomRight => {
                                    let new_w = (iw + dx).clamp(min_s, 1.0 - ix);
                                    let new_h = (ih + dy).clamp(min_s, 1.0 - iy);
                                    s.crop_draft_rect = [ix, iy, new_w, new_h];
                                }
                                CropDragHandle::Body => {
                                    let new_x = (ix + dx).clamp(0.0, 1.0 - iw);
                                    let new_y = (iy + dy).clamp(0.0, 1.0 - ih);
                                    s.crop_draft_rect[0] = new_x;
                                    s.crop_draft_rect[1] = new_y;
                                }
                            }
                            changed = true;
                        }
                    });
                    if changed {
                        cx.notify();
                    }
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _: &MouseUpEvent, _, cx| {
                        let mut changed = false;
                        this.mutate_editor_session(|s| {
                            if s.crop_drag.is_some() {
                                s.crop_drag = None;
                                changed = true;
                            }
                        });
                        if changed {
                            cx.notify();
                        }
                    }),
                )
                // Main Stage: separate the image layer from the interaction layer so handles are never clipped
                .child(
                    div()
                        .id("crop-main-stage")
                        .relative()
                        .w(px(disp_w))
                        .h(px(disp_h))
                        .shadow_lg()
                        // Layer 1: Image and dark overlay (cleanly clipped with rounded_sm)
                        .child(
                            div()
                                .id("crop-image-backdrop")
                                .size_full()
                                .rounded_sm()
                                .overflow_hidden()
                                .relative()
                                .child(
                                    img(base_rendered)
                                        .size_full()
                                        .object_fit(ObjectFit::Fill),
                                )
                                .when(crop_y >= 1.0, |d| {
                                    d.child(
                                        div()
                                            .id("crop-dim-top")
                                            .absolute()
                                            .left_0()
                                            .top_0()
                                            .w(px(disp_w))
                                            .h(px(crop_y))
                                            .bg(shadow_color),
                                    )
                                })
                                .when(dim_bottom_h >= 1.0, |d| {
                                    d.child(
                                        div()
                                            .id("crop-dim-bottom")
                                            .absolute()
                                            .left_0()
                                            .top(px(crop_y + crop_h))
                                            .w(px(disp_w))
                                            .h(px(dim_bottom_h))
                                            .bg(shadow_color),
                                    )
                                })
                                .when(crop_x >= 1.0, |d| {
                                    d.child(
                                        div()
                                            .id("crop-dim-left")
                                            .absolute()
                                            .left_0()
                                            .top(px(crop_y))
                                            .w(px(crop_x))
                                            .h(px(crop_h))
                                            .bg(shadow_color),
                                    )
                                })
                                .when(dim_right_w >= 1.0, |d| {
                                    d.child(
                                        div()
                                            .id("crop-dim-right")
                                            .absolute()
                                            .left(px(crop_x + crop_w))
                                            .top(px(crop_y))
                                            .w(px(dim_right_w))
                                            .h(px(crop_h))
                                            .bg(shadow_color),
                                    )
                                }),
                        )
                        // Layer 2: Interactive Crop Box (clean without false internal grid lines)
                        .child({
                            let crop_px_w = (cur_w * cw_norm).round().max(1.0);
                            let crop_px_h = (cur_h * ch_norm).round().max(1.0);
                            let dim_text = crop_unit.format_dimensions(crop_px_w, crop_px_h, paper_dpi);
                            div()
                                .id("crop-interactive-overlay")
                                .absolute()
                                .inset_0()
                                .child(
                                    div()
                                        .absolute()
                                        .left(px(crop_x))
                                        .top(px(crop_y + crop_h + 8.0))
                                        .w(px(crop_w))
                                        .flex()
                                        .justify_center()
                                        .child(
                                            h_flex()
                                                .items_center()
                                                .gap(px(4.0))
                                                .px_2()
                                                .py_0p5()
                                                .rounded_sm()
                                                .bg(hsla(0.0, 0.0, 0.0, 0.72))
                                                .text_xs()
                                                .text_color(gpui_kit::rgb(0xFFFFFF))
                                                .whitespace_nowrap()
                                                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                                    cx.stop_propagation();
                                                })
                                                .child(dim_text)
                                                .child(
                                                    Select::new(&unit_select_entity)
                                                        .xsmall()
                                                        .appearance(false)
                                                        .w(px(56.0))
                                                        .menu_width(px(72.0)),
                                                ),
                                        ),
                                )
                                .child(
                                    div()
                                        .id("crop-active-box")
                                        .absolute()
                                        .left(px(crop_x))
                                        .top(px(crop_y))
                                        .w(px(crop_w))
                                        .h(px(crop_h))
                                        .border_2()
                                        .border_color(primary_color)
                                        // Center area: smoothly pan the entire crop box with grab/grabbing cursor
                                        .child(
                                            div()
                                                .id("crop-body-pan")
                                                .size_full()
                                                .cursor_grab()
                                                .when(session.crop_drag.is_some(), |d| d.cursor_grabbing())
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                                                        cx.stop_propagation();
                                                        let mx = f32::from(ev.position.x);
                                                        let my = f32::from(ev.position.y);
                                                        this.mutate_editor_session(|s| {
                                                            s.crop_drag = Some((
                                                                CropDragHandle::Body,
                                                                mx,
                                                                my,
                                                                s.crop_draft_rect,
                                                            ));
                                                        });
                                                        cx.notify();
                                                    }),
                                                ),
                                        )
                                        // Handle Garis Atas (Sensitivitas 24px)
                                        .child(
                                            div()
                                                .id("crop-edge-top")
                                                .absolute()
                                                .left(px(16.0))
                                                .right(px(16.0))
                                                .top(px(-12.0))
                                                .h(px(24.0))
                                                .cursor_ns_resize()
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                                                        cx.stop_propagation();
                                                        let mx = f32::from(ev.position.x);
                                                        let my = f32::from(ev.position.y);
                                                        this.mutate_editor_session(|s| {
                                                            s.crop_drag = Some((
                                                                CropDragHandle::Top,
                                                                mx,
                                                                my,
                                                                s.crop_draft_rect,
                                                            ));
                                                        });
                                                        cx.notify();
                                                    }),
                                                ),
                                        )
                                        // Handle Garis Bawah (Sensitivitas 24px)
                                        .child(
                                            div()
                                                .id("crop-edge-bottom")
                                                .absolute()
                                                .left(px(16.0))
                                                .right(px(16.0))
                                                .bottom(px(-12.0))
                                                .h(px(24.0))
                                                .cursor_ns_resize()
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                                                        cx.stop_propagation();
                                                        let mx = f32::from(ev.position.x);
                                                        let my = f32::from(ev.position.y);
                                                        this.mutate_editor_session(|s| {
                                                            s.crop_drag = Some((
                                                                CropDragHandle::Bottom,
                                                                mx,
                                                                my,
                                                                s.crop_draft_rect,
                                                            ));
                                                        });
                                                        cx.notify();
                                                    }),
                                                ),
                                        )
                                        // Handle Garis Kiri (Sensitivitas 24px)
                                        .child(
                                            div()
                                                .id("crop-edge-left")
                                                .absolute()
                                                .top(px(16.0))
                                                .bottom(px(16.0))
                                                .left(px(-12.0))
                                                .w(px(24.0))
                                                .cursor_ew_resize()
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                                                        cx.stop_propagation();
                                                        let mx = f32::from(ev.position.x);
                                                        let my = f32::from(ev.position.y);
                                                        this.mutate_editor_session(|s| {
                                                            s.crop_drag = Some((
                                                                CropDragHandle::Left,
                                                                mx,
                                                                my,
                                                                s.crop_draft_rect,
                                                            ));
                                                        });
                                                        cx.notify();
                                                    }),
                                                ),
                                        )
                                        // Handle Garis Kanan (Sensitivitas 24px)
                                        .child(
                                            div()
                                                .id("crop-edge-right")
                                                .absolute()
                                                .top(px(16.0))
                                                .bottom(px(16.0))
                                                .right(px(-12.0))
                                                .w(px(24.0))
                                                .cursor_ew_resize()
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                                                        cx.stop_propagation();
                                                        let mx = f32::from(ev.position.x);
                                                        let my = f32::from(ev.position.y);
                                                        this.mutate_editor_session(|s| {
                                                            s.crop_drag = Some((
                                                                CropDragHandle::Right,
                                                                mx,
                                                                my,
                                                                s.crop_draft_rect,
                                                            ));
                                                        });
                                                        cx.notify();
                                                    }),
                                                ),
                                        )
                                        // Titik Sudut Atas-Kiri (Sensitivitas 28px, visual 12px)
                                        .child(
                                            div()
                                                .id("crop-corner-top-left")
                                                .absolute()
                                                .left(px(-14.0))
                                                .top(px(-14.0))
                                                .w(px(28.0))
                                                .h(px(28.0))
                                                .cursor_nwse_resize()
                                                .items_center()
                                                .justify_center()
                                                .flex()
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                                                        cx.stop_propagation();
                                                        let mx = f32::from(ev.position.x);
                                                        let my = f32::from(ev.position.y);
                                                        this.mutate_editor_session(|s| {
                                                            s.crop_drag = Some((
                                                                CropDragHandle::TopLeft,
                                                                mx,
                                                                my,
                                                                s.crop_draft_rect,
                                                            ));
                                                        });
                                                        cx.notify();
                                                    }),
                                                )
                                                .child(
                                                    div()
                                                        .w(px(12.0))
                                                        .h(px(12.0))
                                                        .bg(handle_bg)
                                                        .border_2()
                                                        .border_color(primary_color)
                                                        .rounded_sm(),
                                                ),
                                        )
                                        // Titik Sudut Atas-Kanan (Sensitivitas 28px, visual 12px)
                                        .child(
                                            div()
                                                .id("crop-corner-top-right")
                                                .absolute()
                                                .right(px(-14.0))
                                                .top(px(-14.0))
                                                .w(px(28.0))
                                                .h(px(28.0))
                                                .cursor_nesw_resize()
                                                .items_center()
                                                .justify_center()
                                                .flex()
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                                                        cx.stop_propagation();
                                                        let mx = f32::from(ev.position.x);
                                                        let my = f32::from(ev.position.y);
                                                        this.mutate_editor_session(|s| {
                                                            s.crop_drag = Some((
                                                                CropDragHandle::TopRight,
                                                                mx,
                                                                my,
                                                                s.crop_draft_rect,
                                                            ));
                                                        });
                                                        cx.notify();
                                                    }),
                                                )
                                                .child(
                                                    div()
                                                        .w(px(12.0))
                                                        .h(px(12.0))
                                                        .bg(handle_bg)
                                                        .border_2()
                                                        .border_color(primary_color)
                                                        .rounded_sm(),
                                                ),
                                        )
                                        // Titik Sudut Bawah-Kiri (Sensitivitas 28px, visual 12px)
                                        .child(
                                            div()
                                                .id("crop-corner-bottom-left")
                                                .absolute()
                                                .left(px(-14.0))
                                                .bottom(px(-14.0))
                                                .w(px(28.0))
                                                .h(px(28.0))
                                                .cursor_nesw_resize()
                                                .items_center()
                                                .justify_center()
                                                .flex()
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                                                        cx.stop_propagation();
                                                        let mx = f32::from(ev.position.x);
                                                        let my = f32::from(ev.position.y);
                                                        this.mutate_editor_session(|s| {
                                                            s.crop_drag = Some((
                                                                CropDragHandle::BottomLeft,
                                                                mx,
                                                                my,
                                                                s.crop_draft_rect,
                                                            ));
                                                        });
                                                        cx.notify();
                                                    }),
                                                )
                                                .child(
                                                    div()
                                                        .w(px(12.0))
                                                        .h(px(12.0))
                                                        .bg(handle_bg)
                                                        .border_2()
                                                        .border_color(primary_color)
                                                        .rounded_sm(),
                                                ),
                                        )
                                        // Titik Sudut Bawah-Kanan (Sensitivitas 28px, visual 12px)
                                        .child(
                                            div()
                                                .id("crop-corner-bottom-right")
                                                .absolute()
                                                .right(px(-14.0))
                                                .bottom(px(-14.0))
                                                .w(px(28.0))
                                                .h(px(28.0))
                                                .cursor_nwse_resize()
                                                .items_center()
                                                .justify_center()
                                                .flex()
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                                                        cx.stop_propagation();
                                                        let mx = f32::from(ev.position.x);
                                                        let my = f32::from(ev.position.y);
                                                        this.mutate_editor_session(|s| {
                                                            s.crop_drag = Some((
                                                                CropDragHandle::BottomRight,
                                                                mx,
                                                                my,
                                                                s.crop_draft_rect,
                                                            ));
                                                        });
                                                        cx.notify();
                                                    }),
                                                )
                                                .child(
                                                    div()
                                                        .w(px(12.0))
                                                        .h(px(12.0))
                                                        .bg(handle_bg)
                                                        .border_2()
                                                        .border_color(primary_color)
                                                        .rounded_sm(),
                                                ),
                                        ),
                                )
                        }),
                ),
        )
}
