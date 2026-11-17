pub mod crop_view;
pub mod toolbar;
pub mod viewport_frame;
pub mod viewport_freeform;

use std::sync::Arc;
use gpui_kit::{
    Context, InteractiveElement, IntoElement, KeyDownEvent, ParentElement, RenderImage, Styled,
    Window,
    assets::IconName,
    component::{
        ActiveTheme as _, Sizable as _,
        button::{Button, ButtonVariants as _},
        h_flex, v_flex,
    },
    div, px,
};

use crate::{
    app::StudioApp,
    canvas::model::{FitMode, ImageAsset, ImageTransform, LayoutMode, ShapeKind},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CropDragHandle {
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Body,
}

#[derive(Clone)]
pub struct EditorSession {
    pub item_id: u64,
    pub working_transform: ImageTransform,
    pub preview_asset: ImageAsset,
    pub frame_width_px: f32,
    pub frame_height_px: f32,
    pub shape_kind: ShapeKind,
    pub fit_mode: FitMode,
    pub layout_mode: LayoutMode,
    #[allow(dead_code)]
    pub crop_mode: bool,
    pub drag_start: Option<(f32, f32, f32, f32)>,
    pub undo_stack: Vec<ImageTransform>,
    pub crop_page_active: bool,
    pub crop_draft_rect: [f32; 4],
    pub crop_drag: Option<(CropDragHandle, f32, f32, [f32; 4])>,
    pub crop_base_rendered: Option<Arc<RenderImage>>,
}

impl EditorSession {
    pub fn push_undo(&mut self) {
        self.undo_stack.push(self.working_transform.clone());
        if self.undo_stack.len() > 30 {
            self.undo_stack.remove(0);
        }
    }

    pub fn undo(&mut self) -> bool {
        if let Some(prev) = self.undo_stack.pop() {
            self.working_transform = prev;
            self.preview_asset.rebuild_rendered(&self.working_transform);
            true
        } else {
            false
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn start_crop(&mut self) {
        self.crop_draft_rect = self.working_transform.crop_rect;
        self.crop_drag = None;
        let mut base_tf = self.working_transform.clone();
        base_tf.crop_rect = [0.0, 0.0, 1.0, 1.0];
        let base_proc = crate::canvas::geometry::apply_image_transform(
            &self.preview_asset.preview_raw,
            &base_tf,
        );
        self.crop_base_rendered = Some(crate::canvas::geometry::dynamic_to_render_image(&base_proc));
        self.crop_page_active = true;
    }

    pub fn apply_crop(&mut self) {
        self.push_undo();
        self.working_transform.crop_rect = self.crop_draft_rect;
        self.preview_asset.rebuild_rendered(&self.working_transform);
        self.crop_page_active = false;
        self.crop_drag = None;
        self.crop_base_rendered = None;
    }

    pub fn cancel_crop(&mut self) {
        self.crop_draft_rect = self.working_transform.crop_rect;
        self.crop_page_active = false;
        self.crop_drag = None;
        self.crop_base_rendered = None;
    }

    pub fn reset_crop_draft(&mut self) {
        self.crop_draft_rect = [0.0, 0.0, 1.0, 1.0];
    }
}

pub fn render_editor_page(
    app: &mut StudioApp,
    session: EditorSession,
    window: &mut Window,
    cx: &mut Context<StudioApp>,
) -> impl IntoElement {
    // 1. If interactive crop mode is active, switch to the dedicated crop view
    if session.crop_page_active {
        return crop_view::render_crop_view(app, &session, window, cx).into_any_element();
    }

    let is_freeform = session.layout_mode == LayoutMode::Freeform;
    let zoom = session.working_transform.zoom_scale;
    let focus = app.canvas_focus.clone();
    let lang = app.state.language;

    v_flex()
        .size_full()
        .bg(cx.theme().background)
        .track_focus(&focus)
        // Global keyboard shortcuts: 'r' rotate CW, 'Shift+R' flip H, 'Ctrl+R' reset, 'c' crop, 'Ctrl+Z' undo
        .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| {
            let key = ev.keystroke.key.to_ascii_lowercase();
            let mods = &ev.keystroke.modifiers;

            if key == "r" {
                if (mods.control || mods.platform) && !mods.shift && !mods.alt {
                    // Ctrl+R / Cmd+R: Reset Transformasi
                    this.mutate_editor_session(|s| {
                        s.push_undo();
                        s.working_transform = ImageTransform::default();
                        s.preview_asset.rebuild_rendered(&s.working_transform);
                    });
                    cx.notify();
                } else if mods.shift && !mods.control && !mods.alt && !mods.platform {
                    // Shift+R: Flip Horizontal
                    this.mutate_editor_session(|s| {
                        s.push_undo();
                        s.working_transform.toggle_flip_h();
                        s.preview_asset.rebuild_rendered(&s.working_transform);
                    });
                    cx.notify();
                } else if !mods.shift && !mods.control && !mods.alt && !mods.platform {
                    // R: Rotasi Kanan (CW) 90°
                    this.mutate_editor_session(|s| {
                        s.push_undo();
                        s.working_transform.rotate_cw_90();
                        s.preview_asset.rebuild_rendered(&s.working_transform);
                    });
                    cx.notify();
                }
            } else if key == "c" && !mods.shift && !mods.control && !mods.alt && !mods.platform {
                // C: Buka Mode Crop
                this.mutate_editor_session(|s| {
                    s.start_crop();
                });
                cx.notify();
            } else if key == "z" && (mods.control || mods.platform) {
                // Ctrl+Z / Cmd+Z: Undo
                this.mutate_editor_session(|s| {
                    s.undo();
                });
                cx.notify();
            }
        }))
        // Top Toolbar (GPUI-component Toolbar: Kembali, Undo, Rotate CW, Flip H, Crop, Reset, Simpan)
        .child(toolbar::render_editor_toolbar(app, &session, cx))
        // Main Viewport (Sidebar kanan dihilangkan sepenuhnya, area penuh dan responsif)
        .child(
            div()
                .relative()
                .flex_1()
                .w_full()
                .overflow_hidden()
                .child(if is_freeform {
                    viewport_freeform::render_freeform_viewport(app, &session, window, cx)
                        .into_any_element()
                } else {
                    viewport_frame::render_frame_viewport(app, &session, window, cx)
                        .into_any_element()
                })
                // Bottom HUD Status Bar (Zoom Controls di tengah bawah, sama seperti canvas HUD)
                .child(
                    div()
                        .absolute()
                        .bottom(px(16.0))
                        .left_0()
                        .right_0()
                        .flex()
                        .justify_center()
                        .child(
                            h_flex()
                                .px(px(8.0))
                                .py(px(4.0))
                                .bg(cx.theme().background)
                                .border_1()
                                .border_color(cx.theme().border)
                                .rounded_md()
                                .shadow_md()
                                .items_center()
                                .gap(px(4.0))
                                .child(
                                    Button::new("editor-zoom-out")
                                        .ghost()
                                        .small()
                                        .icon(IconName::ZoomOut)
                                        .tooltip(lang.tr("app.zoom_out_tooltip"))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.mutate_editor_session(|s| {
                                                let cur = s.working_transform.zoom_scale;
                                                s.working_transform.zoom_scale =
                                                    (cur - 0.1).clamp(0.25, 4.0);
                                            });
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new("editor-zoom-reset")
                                        .ghost()
                                        .small()
                                        .label(format!("{:.0}%", zoom * 100.0))
                                        .tooltip(lang.tr("app.zoom_reset_tooltip"))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.mutate_editor_session(|s| {
                                                s.working_transform.zoom_scale = 1.0;
                                                s.working_transform.pan_x = 0.0;
                                                s.working_transform.pan_y = 0.0;
                                            });
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new("editor-zoom-in")
                                        .ghost()
                                        .small()
                                        .icon(IconName::ZoomIn)
                                        .tooltip(lang.tr("app.zoom_in_tooltip"))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.mutate_editor_session(|s| {
                                                let cur = s.working_transform.zoom_scale;
                                                s.working_transform.zoom_scale =
                                                    (cur + 0.1).clamp(0.25, 4.0);
                                            });
                                            cx.notify();
                                        })),
                                ),
                        ),
                ),
        )
        .into_any_element()
}
