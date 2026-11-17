use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement,
    Render, ScrollDelta, ScrollEvent, Styled, Subscription, Window,
    assets::IconName,
    component::{
        ActiveTheme as _, Icon, IndexPath, Sizable as _, Size, Theme, ThemeMode, WindowExt as _,
        button::{Button, ButtonVariants as _},
        h_flex,
        select::{SearchableVec, Select, SelectEvent, SelectState},
        status_bar::StatusBar,
        toolbar::Toolbar,
        v_flex,
    },
    div,
    prelude::FluentBuilder as _,
    px, rgb,
};

use crate::{
    app::{ActiveRoute, StudioApp, StudioState},
    canvas::{
        item::render_canvas_item,
        model::{CanvasDrag, ContextMenuState, ImageTransform},
    },
    shared::i18n::Language,
};

struct LanguageSelectState {
    select: Entity<SelectState<SearchableVec<Language>>>,
    _sub: Subscription,
}

pub struct CanvasView {
    pub app: Entity<StudioApp>,
    pub zoom: f32,
    pub pan: (f32, f32),
    pub drag_state: CanvasDrag,
    pub pending_text_click: Option<(u64, f32, f32)>,
    prev_editing_text_id: Option<u64>,
    pub context_menu: Option<ContextMenuState>,
    _subscriptions: Vec<Subscription>,
}

impl CanvasView {
    pub fn new(app: Entity<StudioApp>, cx: &mut Context<Self>) -> Self {
        let _subscriptions = vec![
            cx.observe(&app, |_, _, cx| {
                cx.notify();
            }),
        ];

        Self {
            app,
            zoom: 1.0,
            pan: (0.0, 0.0),
            drag_state: CanvasDrag::None,
            pending_text_click: None,
            prev_editing_text_id: None,
            context_menu: None,
            _subscriptions,
        }
    }

    pub fn display_scale(&self, paper: &crate::shared::paper::PaperConfig) -> f32 {
        let (paper_w_px, paper_h_px) = paper.dimensions_px();
        (580.0 / paper_h_px.max(paper_w_px).max(1.0)) * self.zoom
    }
}

impl Render for CanvasView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (
            paper,
            sidebar_open,
            selected_id,
            editing_text_id,
            mode,
            id_photo_free_move,
            canvas_focus,
            lang,
        ) = {
            let app = self.app.read(cx);
            (
                app.state.paper.clone(),
                app.state.sidebar_open,
                app.state.selected_id,
                app.state.editing_text_id,
                app.state.mode,
                app.state.id_photo_free_move,
                app.canvas_focus.clone(),
                app.state.language,
            )
        };

        let app_for_lang = self.app.clone();
        let lang_select_state = window.use_keyed_state("canvas-language-select", cx, |window, cx| {
            let items = SearchableVec::new(Language::ALL.to_vec());
            let select = cx.new(|cx| {
                SelectState::new(items, Some(IndexPath::new(lang.index())), window, cx)
            });
            let sub = cx.subscribe_in(
                &select,
                window,
                move |_this: &mut LanguageSelectState,
                      _select,
                      event: &SelectEvent<SearchableVec<Language>>,
                      _window,
                      cx| {
                    if let SelectEvent::Confirm(Some(chosen)) = event {
                        let chosen_lang = *chosen;
                        app_for_lang.update(cx, |app, cx| {
                            app.state.language = chosen_lang;
                            chosen_lang.apply_locale(cx);
                            cx.notify();
                        });
                    }
                },
            );
            LanguageSelectState { select, _sub: sub }
        });

        let lang_select_entity = lang_select_state.read(cx).select.clone();
        if lang_select_entity.read(cx).selected_index(cx) != Some(IndexPath::new(lang.index())) {
            lang_select_entity.update(cx, |state, cx| {
                state.set_selected_index(Some(IndexPath::new(lang.index())), window, cx);
            });
        }

        let (paper_w_px, paper_h_px) = paper.dimensions_px();
        let margin_px = paper.margin_px();
        let zoom = self.zoom;
        let (pan_x, pan_y) = self.pan;

        let vp = window.viewport_size();
        let win_w = f32::from(vp.width);
        let win_h = f32::from(vp.height);
        let side_w = if sidebar_open { 320.0 } else { 0.0 };
        let surf_w = (win_w - side_w).max(100.0);
        let surf_h = (win_h - 28.0 - 40.0).max(100.0);
        let surf_center_x = surf_w * 0.5;
        let surf_center_y = surf_h * 0.5;

        let display_scale = self.display_scale(&paper);
        let disp_paper_w = (paper_w_px * display_scale).max(80.0);
        let disp_paper_h = (paper_h_px * display_scale).max(80.0);
        let paper_left = surf_center_x - disp_paper_w * 0.5 + pan_x;
        let paper_top = surf_center_y - disp_paper_h * 0.5 + pan_y;

        let focus_inline_id = editing_text_id.filter(|&id| self.prev_editing_text_id != Some(id));
        self.prev_editing_text_id = editing_text_id;

        let items = self.app.read(cx).state.active_items().to_vec();
        let mut rendered_items: Vec<AnyElement> = Vec::with_capacity(items.len());
        for item in &items {
            rendered_items.push(render_canvas_item(
                item,
                display_scale,
                paper.dpi,
                selected_id,
                editing_text_id,
                focus_inline_id == Some(item.id),
                mode,
                id_photo_free_move,
                lang,
                self.app.clone(),
                canvas_focus.clone(),
                window,
                cx,
            ));
        }

        v_flex()
            .flex_1()
            .h_full()
            .bg(cx.theme().background)
            .justify_between()
            // Top Toolbar
            .child(
                div()
                    .w_full()
                    .h(px(40.0))
                    .px_2()
                    .flex()
                    .items_center()
                    .justify_between()
                    .bg(cx.theme().title_bar)
                    .border_b_1()
                    .border_color(cx.theme().title_bar_border)
                    .child(
                        Toolbar::new("canvas-top-toolbar")
                            .with_size(Size::Medium)
                            .child(
                                Button::new("btn-toggle-sidebar")
                                    .ghost()
                                    .with_size(Size::Medium)
                                    .icon(
                                        Icon::new(if sidebar_open {
                                            IconName::PanelLeftClose
                                        } else {
                                            IconName::PanelLeft
                                        })
                                        .with_size(Size::Medium),
                                    )
                                    .tooltip(if sidebar_open {
                                        lang.tr("app.panel_close_tooltip")
                                    } else {
                                        lang.tr("app.panel_open_tooltip")
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.app.update(cx, |app, cx| {
                                            app.state.sidebar_open = !app.state.sidebar_open;
                                            cx.notify();
                                        });
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        Toolbar::new("canvas-theme-toolbar")
                            .with_size(Size::Medium)
                            .child(
                                Select::new(&lang_select_entity)
                                    .small()
                                    .w(px(86.0))
                                    .menu_width(px(140.0)),
                            )
                            .child(
                                Button::new("btn-toggle-theme")
                                    .ghost()
                                    .with_size(Size::Medium)
                                    .icon(
                                        Icon::new(if cx.theme().is_dark() {
                                            IconName::Sun
                                        } else {
                                            IconName::Moon
                                        })
                                        .with_size(Size::Medium),
                                    )
                                    .tooltip(if cx.theme().is_dark() {
                                        lang.tr("app.theme_light_tooltip")
                                    } else {
                                        lang.tr("app.theme_dark_tooltip")
                                    })
                                    .on_click(cx.listener(|_this, _, _, cx| {
                                        let new_mode = if cx.theme().is_dark() {
                                            ThemeMode::Light
                                        } else {
                                            ThemeMode::Dark
                                        };
                                        Theme::change(new_mode, None, cx);
                                    })),
                            ),
                    ),
            )
            // Main Interactive Canvas Surface
            .child(
                div()
                    .id("interactive-canvas-surface")
                    .track_focus(&canvas_focus)
                    .relative()
                    .flex_1()
                    .w_full()
                    .overflow_hidden()
                    .bg(if cx.theme().is_dark() {
                        cx.theme().background
                    } else {
                        cx.theme().muted
                    })
                    .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window: &mut Window, cx| {
                        let (is_workspace, no_modal_state, selected_id) = {
                            let app = this.app.read(cx);
                            (
                                matches!(app.state.route, ActiveRoute::Workspace),
                                app.state.printer_modal.is_none()
                                    && app.state.editing_layer_id.is_none()
                                    && app.state.editing_text_id.is_none(),
                                app.state.selected_id,
                            )
                        };
                        if !is_workspace || !no_modal_state || window.has_active_dialog(cx) {
                            return;
                        }

                        let Some(id) = selected_id else {
                            return;
                        };
                        let key = ev.keystroke.key.to_ascii_lowercase();
                        let mods = &ev.keystroke.modifiers;

                        if key == "delete" && !mods.control && !mods.alt && !mods.platform {
                            this.app.update(cx, |app, cx| {
                                app.state.delete_item(id);
                                cx.notify();
                            });
                        } else if key == "c"
                            && (mods.control || mods.platform)
                            && !mods.alt
                            && !mods.shift
                        {
                            this.app.update(cx, |app, cx| {
                                app.state.duplicate_item(id);
                                cx.notify();
                            });
                        } else if key == "r" {
                            if (mods.control || mods.platform) && !mods.shift && !mods.alt {
                                this.app.update(cx, |app, cx| {
                                    if app.state.reset_item_transform(id) {
                                        cx.notify();
                                    }
                                });
                            } else if mods.shift && !mods.control && !mods.alt && !mods.platform {
                                this.app.update(cx, |app, cx| {
                                    if app
                                        .state
                                        .transform_item_with(id, ImageTransform::toggle_flip_h)
                                    {
                                        cx.notify();
                                    }
                                });
                            } else if !mods.shift && !mods.control && !mods.alt && !mods.platform {
                                this.app.update(cx, |app, cx| {
                                    if app
                                        .state
                                        .transform_item_with(id, ImageTransform::rotate_cw_90)
                                    {
                                        cx.notify();
                                    }
                                });
                            }
                        }
                    }))
                    .when(
                        matches!(self.drag_state, CanvasDrag::PanCanvas { .. }),
                        |d| d.cursor_grabbing(),
                    )
                    .on_scroll(
                        cx.listener(move |this, ev: &ScrollEvent, window: &mut Window, cx| {
                            let dy = match ev.delta {
                                ScrollDelta::Pixels(p) => f32::from(p.y),
                                ScrollDelta::Lines(l) => l.y * 20.0,
                            };
                            let old_zoom = this.zoom;
                            let factor = if dy > 0.0 { 0.15 } else { -0.15 };
                            let new_zoom = (old_zoom * (1.0 + factor)).clamp(0.5, 50.0);
                            if (new_zoom - old_zoom).abs() > 1e-4 {
                                let ratio = new_zoom / old_zoom;
                                let mx = f32::from(ev.position.x);
                                let my = f32::from(ev.position.y);

                                let vp = window.viewport_size();
                                let cur_win_w = f32::from(vp.width);
                                let cur_win_h = f32::from(vp.height);
                                let cur_side_w = if this.app.read(cx).state.sidebar_open { 320.0 } else { 0.0 };
                                let cur_surf_cx = (cur_win_w - cur_side_w).max(100.0) * 0.5;
                                let cur_surf_cy = 40.0 + (cur_win_h - 28.0 - 40.0).max(100.0) * 0.5;

                                let paper_cx = cur_side_w + cur_surf_cx + this.pan.0;
                                let paper_cy = cur_surf_cy + this.pan.1;
                                let dx = mx - paper_cx;
                                let dy = my - paper_cy;

                                this.zoom = new_zoom;
                                this.pan.0 = (this.pan.0 + dx * (1.0 - ratio)).round();
                                this.pan.1 = (this.pan.1 + dy * (1.0 - ratio)).round();
                                cx.notify();
                            }
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, ev: &MouseDownEvent, window: &mut Window, cx| {
                            let focus_handle = this.app.read(cx).canvas_focus.clone();
                            focus_handle.focus(window, cx);
                            this.context_menu = None;
                            this.pending_text_click = None;
                            this.app.update(cx, |app, cx| {
                                app.state.context_menu = None;
                                app.state.selected_id = None;
                                app.state.editing_text_id = None;
                                cx.notify();
                            });
                            this.drag_state = CanvasDrag::PanCanvas {
                                start_mx: f32::from(ev.position.x),
                                start_my: f32::from(ev.position.y),
                                orig_pan_x: this.pan.0,
                                orig_pan_y: this.pan.1,
                            };
                            cx.notify();
                        }),
                    )
                    .on_mouse_move(cx.listener(move |this, ev: &MouseMoveEvent, _, cx| {
                        if matches!(this.drag_state, CanvasDrag::None) {
                            return;
                        }
                        let mx = f32::from(ev.position.x);
                        let my = f32::from(ev.position.y);
                        match this.drag_state {
                            CanvasDrag::None => {}
                            CanvasDrag::PanCanvas {
                                start_mx,
                                start_my,
                                orig_pan_x,
                                orig_pan_y,
                            } => {
                                this.pan.0 = orig_pan_x + (mx - start_mx);
                                this.pan.1 = orig_pan_y + (my - start_my);
                                cx.notify();
                            }
                            CanvasDrag::MoveItem {
                                id,
                                start_mx,
                                start_my,
                                orig_x,
                                orig_y,
                            } => {
                                let paper = this.app.read(cx).state.paper.clone();
                                let scale = this.display_scale(&paper).max(0.01);
                                let mut nx = orig_x + (mx - start_mx) / scale;
                                let mut ny = orig_y + (my - start_my) / scale;
                                let snap = this.app.read(cx).state.snap_to_grid;
                                if snap {
                                    nx = (nx / 10.0).round() * 10.0;
                                    ny = (ny / 10.0).round() * 10.0;
                                }
                                this.app.update(cx, |app, _| {
                                    if let Some(item) = app.state.find_item_mut(id) {
                                        item.x_px = nx;
                                        item.y_px = ny;
                                        StudioState::clamp_item_to_safe_margin(&paper, item);
                                    }
                                });
                                cx.notify();
                            }
                            CanvasDrag::ResizeItem {
                                id,
                                start_mx,
                                start_my,
                                orig_w,
                                orig_h,
                            } => {
                                let paper = this.app.read(cx).state.paper.clone();
                                let scale = this.display_scale(&paper).max(0.01);
                                let dw = (mx - start_mx) / scale;
                                let dh = (my - start_my) / scale;
                                this.app.update(cx, |app, _| {
                                    if let Some(item) = app.state.find_item_mut(id) {
                                        let aspect = orig_h / orig_w.max(1.0);
                                        let nw = (orig_w + dw).max(32.0);
                                        let nh = if item.lock_aspect {
                                            (nw * aspect).max(32.0)
                                        } else {
                                            (orig_h + dh).max(32.0)
                                        };
                                        item.width_px = nw;
                                        item.height_px = nh;
                                        if let Some(tb) = &mut item.text {
                                            tb.manual_size = true;
                                        }
                                        StudioState::clamp_item_to_safe_margin(&paper, item);
                                    }
                                });
                                cx.notify();
                            }
                        }
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, ev: &MouseUpEvent, _, cx| {
                            let was_item_mutation = matches!(
                                this.drag_state,
                                CanvasDrag::MoveItem { .. } | CanvasDrag::ResizeItem { .. }
                            );
                            this.drag_state = CanvasDrag::None;
                            if let Some((clicked_id, down_x, down_y)) =
                                this.pending_text_click.take()
                            {
                                let dist = (ev.position.x.as_f32() - down_x)
                                    .hypot(ev.position.y.as_f32() - down_y);
                                if dist < 4.0 {
                                    this.app.update(cx, |app, cx| {
                                        if app.state.selected_id == Some(clicked_id)
                                            && app
                                                .state
                                                .find_item(clicked_id)
                                                .is_some_and(|i| i.is_text())
                                        {
                                            app.state.editing_text_id = Some(clicked_id);
                                            cx.notify();
                                        }
                                    });
                                }
                            }
                            if was_item_mutation {
                                this.app.update(cx, |_, app_cx| app_cx.notify());
                            }
                            cx.notify();
                        }),
                    )
                    // Paper Sheet Representation
                    .child(
                        div()
                            .id("canvas-paper")
                            .absolute()
                            .left(px(paper_left))
                            .top(px(paper_top))
                            .w(px(disp_paper_w))
                            .h(px(disp_paper_h))
                            .bg(rgb(0xF8F8FA))
                            .border_1()
                            .border_color(cx.theme().border)
                            .shadow_lg()
                            // Safe Print Margin Boundary Box
                            .child(
                                div()
                                    .absolute()
                                    .left(px(margin_px * display_scale))
                                    .top(px(margin_px * display_scale))
                                    .w(px((paper_w_px - margin_px * 2.0).max(0.0) * display_scale))
                                    .h(px((paper_h_px - margin_px * 2.0).max(0.0) * display_scale))
                                    .border_1()
                                    .border_dashed()
                                    .border_color(rgb(0x94A3B8).opacity(0.55)),
                            )
                            // Render all CanvasItems on the sheet
                            .children(rendered_items),
                    )
            )
            // Standard StatusBar component
            .child(
                StatusBar::new().child(
                    h_flex()
                        .w_full()
                        .justify_center()
                        .items_center()
                        .gap(px(4.0))
                        .child(
                            Button::new("hud-zoom-out")
                                .ghost()
                                .small()
                                .icon(IconName::ZoomOut)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let cur = this.zoom;
                                    let step = if cur > 10.0 {
                                        2.5
                                    } else if cur > 5.0 {
                                        1.0
                                    } else if cur > 2.0 {
                                        0.5
                                    } else {
                                        0.25
                                    };
                                    this.zoom = (cur - step).clamp(0.5, 50.0);
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("hud-zoom-reset")
                                .ghost()
                                .small()
                                .label(format!("{:.0}%", zoom * 100.0))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.zoom = 1.0;
                                    this.pan = (0.0, 0.0);
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("hud-zoom-in")
                                .ghost()
                                .small()
                                .icon(IconName::ZoomIn)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let cur = this.zoom;
                                    let step = if cur >= 10.0 {
                                        2.5
                                    } else if cur >= 5.0 {
                                        1.0
                                    } else if cur >= 2.0 {
                                        0.5
                                    } else {
                                        0.25
                                    };
                                    this.zoom = (cur + step).clamp(0.5, 50.0);
                                    cx.notify();
                                })),
                        ),
                ),
            )
    }
}
