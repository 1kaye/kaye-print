use gpui_kit::{
    Context, InteractiveElement, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ObjectFit, ParentElement, ScrollDelta, ScrollEvent, Styled, StyledImage as _,
    Window,
    component::ActiveTheme as _,
    div, img,
    prelude::FluentBuilder as _,
    px,
};

use crate::{app::StudioApp, editor::EditorSession};

pub fn render_freeform_viewport(
    _app: &mut StudioApp,
    session: &EditorSession,
    window: &mut Window,
    cx: &mut Context<StudioApp>,
) -> impl IntoElement {
    let tf = &session.working_transform;
    let (cur_w_px, cur_h_px) = session.preview_asset.display_dimensions(tf);
    let fit_scale = (520.0 / cur_w_px.max(1.0)).min(440.0 / cur_h_px.max(1.0));
    let free_w = (cur_w_px * fit_scale).max(24.0);
    let free_h = (cur_h_px * fit_scale).max(24.0);

    let zoom = tf.zoom_scale.clamp(0.2, 5.0);
    let cur_w = free_w * zoom;
    let cur_h = free_h * zoom;

    let vp = window.viewport_size();
    let win_w = f32::from(vp.width);
    let win_h = f32::from(vp.height);
    let surf_w = win_w.max(100.0);
    let surf_h = (win_h - 48.0).max(100.0);
    let surf_cx = surf_w * 0.5;
    let surf_cy = surf_h * 0.5;
    let img_left = surf_cx - cur_w * 0.5 + tf.pan_x * free_w;
    let img_top = surf_cy - cur_h * 0.5 + tf.pan_y * free_h;

    div()
        .id("editor-freeform-surface")
        .relative()
        .flex_1()
        .h_full()
        .overflow_hidden()
        .bg(cx.theme().background)
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
        .on_mouse_move(cx.listener(move |this, ev: &MouseMoveEvent, _, cx| {
            let mx = f32::from(ev.position.x);
            let my = f32::from(ev.position.y);
            let mut changed = false;
            this.mutate_editor_session(|s| {
                if let Some((sx, sy, opx, opy)) = s.drag_start {
                    let dx = (mx - sx) / free_w.max(1.0);
                    let dy = (my - sy) / free_h.max(1.0);
                    s.working_transform.pan_x = (opx + dx).clamp(-4.0, 4.0);
                    s.working_transform.pan_y = (opy + dy).clamp(-4.0, 4.0);
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
        .on_scroll(cx.listener(
            move |this, ev: &ScrollEvent, window: &mut Window, cx| {
                let dy = match ev.delta {
                    ScrollDelta::Pixels(p) => f32::from(p.y),
                    ScrollDelta::Lines(l) => l.y * 20.0,
                };
                let factor = if dy > 0.0 { 0.12 } else { -0.12 };
                let mx = f32::from(ev.position.x);
                let my = f32::from(ev.position.y);

                let vp = window.viewport_size();
                let cur_win_w = f32::from(vp.width);
                let cur_win_h = f32::from(vp.height);
                let cur_center_x = cur_win_w * 0.5;
                let cur_center_y = 48.0 + (cur_win_h - 48.0) * 0.5;

                let mut changed = false;
                this.mutate_editor_session(|s| {
                    let old_zoom = s.working_transform.zoom_scale;
                    let new_zoom = (old_zoom + factor).clamp(0.25, 4.0);
                    if (new_zoom - old_zoom).abs() > 1e-4 {
                        let ratio = new_zoom / old_zoom;
                        let img_cx = cur_center_x + s.working_transform.pan_x * free_w;
                        let img_cy = cur_center_y + s.working_transform.pan_y * free_h;
                        let dx = mx - img_cx;
                        let dy = my - img_cy;

                        s.working_transform.pan_x = (s.working_transform.pan_x
                            + (dx * (1.0 - ratio)) / free_w.max(1.0))
                        .clamp(-4.0, 4.0);
                        s.working_transform.pan_y = (s.working_transform.pan_y
                            + (dy * (1.0 - ratio)) / free_h.max(1.0))
                        .clamp(-4.0, 4.0);
                        s.working_transform.zoom_scale = new_zoom;
                        changed = true;
                    }
                });
                if changed {
                    cx.notify();
                }
            },
        ))
        .child(
            div()
                .id("editor-freeform-image")
                .absolute()
                .left(px(img_left))
                .top(px(img_top))
                .w(px(cur_w))
                .h(px(cur_h))
                .cursor_grab()
                .child(
                    img(session.preview_asset.rendered.clone())
                        .size_full()
                        .object_fit(ObjectFit::Fill)
                        .shadow_lg()
                        .rounded_md(),
                ),
        )
}
