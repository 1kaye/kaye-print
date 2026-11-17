use gpui_kit::{
    Context, InteractiveElement, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ObjectFit, ParentElement, ScrollDelta, ScrollEvent, Styled, StyledImage as _,
    Window,
    component::{ActiveTheme as _, v_flex},
    div, img,
    prelude::FluentBuilder as _,
    px,
};

use crate::{
    app::StudioApp,
    canvas::{geometry::compute_fitted_image_geometry, model::ShapeKind},
    editor::EditorSession,
};

pub fn render_frame_viewport(
    _app: &mut StudioApp,
    session: &EditorSession,
    _window: &mut Window,
    cx: &mut Context<StudioApp>,
) -> impl IntoElement {
    let raw_fw = session.frame_width_px.max(1.0);
    let raw_fh = session.frame_height_px.max(1.0);
    let fit_scale = (480.0 / raw_fw).min(440.0 / raw_fh);
    let frame_w = (raw_fw * fit_scale).max(24.0);
    let frame_h = (raw_fh * fit_scale).max(24.0);

    let tf = &session.working_transform;
    let is_oval = session.shape_kind == ShapeKind::CircleOval;
    let (asset_w, asset_h) = session.preview_asset.display_dimensions(tf);
    let (img_w, img_h, left_off, top_off) = compute_fitted_image_geometry(
        frame_w,
        frame_h,
        asset_w,
        asset_h,
        session.fit_mode,
        tf.zoom_scale,
        tf.pan_x,
        tf.pan_y,
    );

    v_flex()
        .flex_1()
        .h_full()
        .overflow_hidden()
        .items_center()
        .justify_center()
        .p(px(20.0))
        .on_mouse_move(cx.listener(move |this, ev: &MouseMoveEvent, _, cx| {
            let mx = f32::from(ev.position.x);
            let my = f32::from(ev.position.y);
            let mut changed = false;
            this.mutate_editor_session(|s| {
                if let Some((sx, sy, opx, opy)) = s.drag_start {
                    let dx = (mx - sx) / frame_w.max(1.0);
                    let dy = (my - sy) / frame_h.max(1.0);
                    s.working_transform.pan_x = (opx + dx).clamp(-3.0, 3.0);
                    s.working_transform.pan_y = (opy + dy).clamp(-3.0, 3.0);
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
                    if s.drag_start.is_some() {
                        s.drag_start = None;
                        changed = true;
                    }
                });
                if changed {
                    cx.notify();
                }
            }),
        )
        .child(
            div()
                .id("editor-viewport-frame")
                .relative()
                .w(px(frame_w))
                .h(px(frame_h))
                .cursor_grab()
                .when(session.drag_start.is_some(), |d| d.cursor_grabbing())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                        let mx = f32::from(ev.position.x);
                        let my = f32::from(ev.position.y);
                        this.mutate_editor_session(|s| {
                            s.drag_start = Some((
                                mx,
                                my,
                                s.working_transform.pan_x,
                                s.working_transform.pan_y,
                            ));
                        });
                        cx.notify();
                    }),
                )
                .on_scroll(cx.listener(|this, ev: &ScrollEvent, _, cx| {
                    let dy = match ev.delta {
                        ScrollDelta::Pixels(p) => f32::from(p.y),
                        ScrollDelta::Lines(l) => l.y * 20.0,
                    };
                    let factor = if dy > 0.0 { 0.1 } else { -0.1 };
                    let mut changed = false;
                    this.mutate_editor_session(|s| {
                        let old = s.working_transform.zoom_scale;
                        s.working_transform.zoom_scale = (old + factor).clamp(0.25, 4.0);
                        changed = true;
                    });
                    if changed {
                        cx.notify();
                    }
                }))
                .shadow_lg()
                .border_2()
                .border_color(cx.theme().primary)
                .when(is_oval, |d| d.rounded(px(frame_w.max(frame_h))))
                .overflow_hidden()
                .bg(cx.theme().background)
                .child(
                    div()
                        .absolute()
                        .left(px(left_off))
                        .top(px(top_off))
                        .w(px(img_w))
                        .h(px(img_h))
                        .child(
                            img(session.preview_asset.rendered.clone())
                                .size_full()
                                .object_fit(ObjectFit::Fill),
                        ),
                ),
        )
}
