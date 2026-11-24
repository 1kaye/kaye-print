use gpui_kit::{
    AnyElement, AppContext as _, Bounds, Context, Entity, InteractiveElement as _, IntoElement,
    KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement,
    PathBuilder, Pixels, Render, ScrollDelta, ScrollEvent, Styled, Subscription, Window,
    assets::IconName,
    canvas,
    component::{
        ActiveTheme as _, Disableable as _, Icon, IndexPath, Sizable as _, Size, Theme, ThemeMode,
        WindowExt as _,
        button::{Button, ButtonVariants as _},
        h_flex,
        select::{SearchableVec, Select, SelectEvent, SelectState},
        separator::Separator,
        status_bar::StatusBar,
        toolbar::Toolbar,
        v_flex,
    },
    div, point,
    prelude::FluentBuilder as _,
    px, rgb,
};

use crate::{
    app::{ActiveRoute, StudioApp, StudioState},
    canvas::{
        item::render_canvas_item,
        model::{CanvasDrag, ContextMenuState, ImageTransform, LayoutMode, ResizeHandle},
    },
    inspector::ButtonSelectedExt as _,
    shared::{i18n::Language, units::mm_to_px},
};

#[derive(Clone, Copy, Debug)]
pub enum SnapGuide {
    Vertical(f32),
    Horizontal(f32),
}

struct LanguageSelectState {
    select: Entity<SelectState<SearchableVec<Language>>>,
    _sub: Subscription,
}

pub struct CanvasView {
    pub app: Entity<StudioApp>,
    pub zoom: f32,
    pub pan: (f32, f32),
    pub drag_state: CanvasDrag,
    pub drag_undo_pushed: bool,
    pub active_guides: Vec<SnapGuide>,
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
            drag_undo_pushed: false,
            active_guides: Vec::new(),
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

fn paint_paper_grid(
    bounds: Bounds<Pixels>,
    margin_disp: f32,
    step_disp: f32,
    window: &mut Window,
) {
    let ox = f32::from(bounds.origin.x);
    let oy = f32::from(bounds.origin.y);
    let bw = f32::from(bounds.size.width);
    let bh = f32::from(bounds.size.height);
    if step_disp < 6.0 || bw <= margin_disp * 2.0 || bh <= margin_disp * 2.0 {
        return;
    }
    let x0 = ox + margin_disp;
    let y0 = oy + margin_disp;
    let x1 = ox + bw - margin_disp;
    let y1 = oy + bh - margin_disp;
    let mut builder = PathBuilder::stroke(px(0.75));
    let mut x = x0 + step_disp;
    while x < x1 - 1.0 {
        builder.move_to(point(px(x.round()), px(y0)));
        builder.line_to(point(px(x.round()), px(y1)));
        x += step_disp;
    }
    let mut y = y0 + step_disp;
    while y < y1 - 1.0 {
        builder.move_to(point(px(x0), px(y.round())));
        builder.line_to(point(px(x1), px(y.round())));
        y += step_disp;
    }
    if let Ok(path) = builder.build() {
        window.paint_path(path, rgb(0x94A3B8).opacity(0.24));
    }
}

fn paint_snap_guides(
    bounds: Bounds<Pixels>,
    guides: &[SnapGuide],
    display_scale: f32,
    color: gpui_kit::Hsla,
    window: &mut Window,
) {
    if guides.is_empty() {
        return;
    }
    let ox = f32::from(bounds.origin.x);
    let oy = f32::from(bounds.origin.y);
    let bw = f32::from(bounds.size.width);
    let bh = f32::from(bounds.size.height);
    let mut builder = PathBuilder::stroke(px(1.0)).dash_array(&[px(4.0), px(3.0)]);
    for guide in guides {
        match *guide {
            SnapGuide::Vertical(gx_sheet) => {
                let x = ox + (gx_sheet * display_scale).round();
                builder.move_to(point(px(x), px(oy)));
                builder.line_to(point(px(x), px(oy + bh)));
            }
            SnapGuide::Horizontal(gy_sheet) => {
                let y = oy + (gy_sheet * display_scale).round();
                builder.move_to(point(px(ox), px(y)));
                builder.line_to(point(px(ox + bw), px(y)));
            }
        }
    }
    if let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }
}

impl Render for CanvasView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (
            paper,
            sidebar_open,
            selected_id,
            selected_ids,
            editing_text_id,
            mode,
            id_photo_free_move,
            snap_to_grid,
            can_undo,
            can_redo,
            canvas_focus,
            lang,
        ) = {
            let app = self.app.read(cx);
            (
                app.state.paper.clone(),
                app.state.sidebar_open,
                app.state.selected_id,
                app.state.selected_ids.clone(),
                app.state.editing_text_id,
                app.state.mode,
                app.state.id_photo_free_move,
                app.state.snap_to_grid,
                app.state.can_undo_freeform(),
                app.state.can_redo_freeform(),
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
        let disp_paper_w = (paper_w_px * display_scale).round().max(80.0);
        let disp_paper_h = (paper_h_px * display_scale).round().max(80.0);
        let paper_left = (surf_center_x - disp_paper_w * 0.5 + pan_x).round();
        let paper_top = (surf_center_y - disp_paper_h * 0.5 + pan_y).round();

        let focus_inline_id = editing_text_id.filter(|&id| self.prev_editing_text_id != Some(id));
        self.prev_editing_text_id = editing_text_id;

        let items = self.app.read(cx).state.active_items().to_vec();
        let mut rendered_items: Vec<AnyElement> = Vec::with_capacity(items.len());
        for item in &items {
            if mode == LayoutMode::Freeform && !item.visible {
                continue;
            }
            rendered_items.push(render_canvas_item(
                item,
                display_scale,
                paper.dpi,
                selected_id,
                &selected_ids,
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

        let is_freeform = mode == LayoutMode::Freeform;
        let active_guides = self.active_guides.clone();
        let guide_color = cx.theme().primary;
        let grid_step_disp = mm_to_px(5.0, paper.dpi) * display_scale;
        let margin_disp = (margin_px * display_scale).round();

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
                            )
                            .when(is_freeform, |tb| {
                                tb.content(Separator::vertical().h(px(16.0)).mx_1())
                                    .child(
                                        Button::new("btn-freeform-undo")
                                            .ghost()
                                            .with_size(Size::Medium)
                                            .disabled(!can_undo)
                                            .icon(Icon::new(IconName::Undo2).with_size(Size::Medium))
                                            .tooltip(lang.tr("app.undo_tooltip"))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.app.update(cx, |app, cx| {
                                                    if app.state.undo_freeform() {
                                                        cx.notify();
                                                    }
                                                });
                                            })),
                                    )
                                    .child(
                                        Button::new("btn-freeform-redo")
                                            .ghost()
                                            .with_size(Size::Medium)
                                            .disabled(!can_redo)
                                            .icon(Icon::new(IconName::Redo2).with_size(Size::Medium))
                                            .tooltip(lang.tr("app.redo_tooltip"))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.app.update(cx, |app, cx| {
                                                    if app.state.redo_freeform() {
                                                        cx.notify();
                                                    }
                                                });
                                            })),
                                    )
                                    .content(Separator::vertical().h(px(16.0)).mx_1())
                                    .child(
                                        Button::new("btn-freeform-grid")
                                            .ghost()
                                            .with_size(Size::Medium)
                                            .neutral_selected(snap_to_grid, cx)
                                            .icon(
                                                Icon::new(IconName::LayoutDashboard)
                                                    .with_size(Size::Medium),
                                            )
                                            .tooltip(lang.tr("app.snap_grid_tooltip"))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.app.update(cx, |app, cx| {
                                                    app.state.snap_to_grid = !app.state.snap_to_grid;
                                                    cx.notify();
                                                });
                                            })),
                                    )
                            }),
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
                        let (is_workspace, no_modal_state, selected_id, cur_mode, dpi) = {
                            let app = this.app.read(cx);
                            (
                                matches!(app.state.route, ActiveRoute::Workspace),
                                app.state.printer_modal.is_none()
                                    && app.state.editing_layer_id.is_none()
                                    && app.state.editing_text_id.is_none(),
                                app.state.selected_id,
                                app.state.mode,
                                app.state.paper.dpi,
                            )
                        };
                        if !is_workspace || !no_modal_state || window.has_active_dialog(cx) {
                            return;
                        }

                        let key = ev.keystroke.key.to_ascii_lowercase();
                        let mods = &ev.keystroke.modifiers;
                        let accel = mods.control || mods.platform;

                        // Global Freeform shortcuts that work even without active selection
                        if cur_mode == LayoutMode::Freeform {
                            if key == "z" && accel && !mods.alt {
                                this.app.update(cx, |app, cx| {
                                    let changed = if mods.shift {
                                        app.state.redo_freeform()
                                    } else {
                                        app.state.undo_freeform()
                                    };
                                    if changed {
                                        cx.notify();
                                    }
                                });
                                return;
                            }
                            if key == "y" && accel && !mods.alt && !mods.shift {
                                this.app.update(cx, |app, cx| {
                                    if app.state.redo_freeform() {
                                        cx.notify();
                                    }
                                });
                                return;
                            }
                            if key == "a" && accel && !mods.alt && !mods.shift {
                                this.app.update(cx, |app, cx| {
                                    app.state.select_all_freeform();
                                    cx.notify();
                                });
                                return;
                            }
                        }

                        if key == "escape" && !accel && !mods.alt {
                            this.app.update(cx, |app, cx| {
                                app.state.select_single(None);
                                cx.notify();
                            });
                            return;
                        }

                        let Some(id) = selected_id else {
                            return;
                        };

                        // Arrow key nudging (1mm default, 5mm with Shift)
                        let nudge_step = mm_to_px(if mods.shift { 5.0 } else { 1.0 }, dpi);
                        if !accel && !mods.alt {
                            let delta = match key.as_str() {
                                "up" => Some((0.0, -nudge_step)),
                                "down" => Some((0.0, nudge_step)),
                                "left" => Some((-nudge_step, 0.0)),
                                "right" => Some((nudge_step, 0.0)),
                                _ => None,
                            };
                            if let Some((dx, dy)) = delta {
                                this.app.update(cx, |app, cx| {
                                    if app.state.nudge_selected(dx, dy) {
                                        cx.notify();
                                    }
                                });
                                return;
                            }
                        }

                        if (key == "delete" || key == "backspace")
                            && !accel
                            && !mods.alt
                        {
                            this.app.update(cx, |app, cx| {
                                app.state.delete_selected();
                                cx.notify();
                            });
                        } else if (key == "c" || key == "d") && accel && !mods.alt && !mods.shift {
                            this.app.update(cx, |app, cx| {
                                app.state.duplicate_selected();
                                cx.notify();
                            });
                        } else if key == "r" {
                            if accel && !mods.shift && !mods.alt {
                                this.app.update(cx, |app, cx| {
                                    if app.state.reset_item_transform(id) {
                                        cx.notify();
                                    }
                                });
                            } else if mods.shift && !accel && !mods.alt {
                                this.app.update(cx, |app, cx| {
                                    if app
                                        .state
                                        .transform_item_with(id, ImageTransform::toggle_flip_h)
                                    {
                                        cx.notify();
                                    }
                                });
                            } else if !mods.shift && !accel && !mods.alt {
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
                                app.state.select_single(None);
                                app.state.editing_text_id = None;
                                cx.notify();
                            });
                            this.active_guides.clear();
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
                        let drag_snapshot = this.drag_state.clone();
                        match drag_snapshot {
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
                            CanvasDrag::MoveItems {
                                anchor_id,
                                start_mx,
                                start_my,
                                orig_positions,
                            } => {
                                if (mx - start_mx).hypot(my - start_my) < 1.0 {
                                    return;
                                }
                                let (paper, snap_grid, cur_mode) = {
                                    let st = &this.app.read(cx).state;
                                    (st.paper.clone(), st.snap_to_grid, st.mode)
                                };
                                let scale = this.display_scale(&paper).max(0.01);
                                let mut dx = (mx - start_mx) / scale;
                                let mut dy = (my - start_my) / scale;

                                let mut guides = Vec::new();
                                if cur_mode == LayoutMode::Freeform
                                    && let Some(&(_, ax0, ay0)) =
                                        orig_positions.iter().find(|(id, _, _)| *id == anchor_id)
                                {
                                    if snap_grid && !ev.modifiers.alt {
                                        let step = mm_to_px(5.0, paper.dpi).max(10.0);
                                        let snapped_x = ((ax0 + dx) / step).round() * step;
                                        let snapped_y = ((ay0 + dy) / step).round() * step;
                                        dx = snapped_x - ax0;
                                        dy = snapped_y - ay0;
                                    }

                                    // Smart alignment guides against paper safe bounds & other visible items
                                    if !ev.modifiers.alt {
                                        let st = &this.app.read(cx).state;
                                        if let Some(anchor) =
                                            st.free_items.iter().find(|i| i.id == anchor_id)
                                        {
                                            let aw = anchor.width_px;
                                            let ah = anchor.height_px;
                                            let cand_x = ax0 + dx;
                                            let cand_y = ay0 + dy;
                                            let thresh = 6.0 / scale;
                                            let (ml, mt, pw, ph) = paper.printable_bounds_px();

                                            let mut x_targets = vec![ml, ml + pw * 0.5, ml + pw];
                                            let mut y_targets = vec![mt, mt + ph * 0.5, mt + ph];
                                            for other in &st.free_items {
                                                if !other.visible
                                                    || orig_positions
                                                        .iter()
                                                        .any(|(pid, _, _)| *pid == other.id)
                                                {
                                                    continue;
                                                }
                                                x_targets.push(other.x_px);
                                                x_targets.push(other.x_px + other.width_px * 0.5);
                                                x_targets.push(other.x_px + other.width_px);
                                                y_targets.push(other.y_px);
                                                y_targets.push(other.y_px + other.height_px * 0.5);
                                                y_targets.push(other.y_px + other.height_px);
                                            }

                                            let anchor_xs = [cand_x, cand_x + aw * 0.5, cand_x + aw];
                                            let mut best_dx: Option<(f32, f32)> = None;
                                            for &ax in &anchor_xs {
                                                for &tx in &x_targets {
                                                    let diff = tx - ax;
                                                    if diff.abs() <= thresh
                                                        && best_dx
                                                            .is_none_or(|(bd, _)| diff.abs() < bd.abs())
                                                    {
                                                        best_dx = Some((diff, tx));
                                                    }
                                                }
                                            }
                                            if let Some((snap_dx, guide_x)) = best_dx {
                                                dx += snap_dx;
                                                guides.push(SnapGuide::Vertical(guide_x));
                                            }

                                            let anchor_ys = [cand_y, cand_y + ah * 0.5, cand_y + ah];
                                            let mut best_dy: Option<(f32, f32)> = None;
                                            for &ay in &anchor_ys {
                                                for &ty in &y_targets {
                                                    let diff = ty - ay;
                                                    if diff.abs() <= thresh
                                                        && best_dy
                                                            .is_none_or(|(bd, _)| diff.abs() < bd.abs())
                                                    {
                                                        best_dy = Some((diff, ty));
                                                    }
                                                }
                                            }
                                            if let Some((snap_dy, guide_y)) = best_dy {
                                                dy += snap_dy;
                                                guides.push(SnapGuide::Horizontal(guide_y));
                                            }
                                        }
                                    }
                                }
                                this.active_guides = guides;

                                let push_undo = !this.drag_undo_pushed;
                                if push_undo {
                                    this.drag_undo_pushed = true;
                                }
                                this.app.update(cx, |app, _| {
                                    if push_undo {
                                        app.state.push_freeform_undo();
                                    }
                                    for &(id, ox, oy) in &orig_positions {
                                        if let Some(item) = app.state.find_item_mut(id) {
                                            item.x_px = ox + dx;
                                            item.y_px = oy + dy;
                                            StudioState::clamp_item_to_safe_margin(&paper, item);
                                        }
                                    }
                                });
                                cx.notify();
                            }
                            CanvasDrag::ResizeItem {
                                id,
                                handle,
                                start_mx,
                                start_my,
                                orig_x,
                                orig_y,
                                orig_w,
                                orig_h,
                            } => {
                                let paper = this.app.read(cx).state.paper.clone();
                                let scale = this.display_scale(&paper).max(0.01);
                                let dw = (mx - start_mx) / scale;
                                let dh = (my - start_my) / scale;
                                let shift_lock = ev.modifiers.shift;
                                let push_undo = !this.drag_undo_pushed;
                                if push_undo {
                                    this.drag_undo_pushed = true;
                                }
                                this.app.update(cx, |app, _| {
                                    if push_undo {
                                        app.state.push_freeform_undo();
                                    }
                                    if let Some(item) = app.state.find_item_mut(id) {
                                        let min_sz = 32.0f32;
                                        let aspect = orig_h / orig_w.max(1.0);
                                        let is_corner = matches!(
                                            handle,
                                            ResizeHandle::TopLeft
                                                | ResizeHandle::TopRight
                                                | ResizeHandle::BottomLeft
                                                | ResizeHandle::BottomRight
                                        );
                                        let keep_aspect = is_corner && (item.lock_aspect || shift_lock);

                                        let mut nx = orig_x;
                                        let mut ny = orig_y;
                                        let mut nw = orig_w;
                                        let mut nh = orig_h;

                                        match handle {
                                            ResizeHandle::Right => {
                                                nw = (orig_w + dw).max(min_sz);
                                            }
                                            ResizeHandle::Left => {
                                                nw = (orig_w - dw).max(min_sz);
                                                nx = orig_x + (orig_w - nw);
                                            }
                                            ResizeHandle::Bottom => {
                                                nh = (orig_h + dh).max(min_sz);
                                            }
                                            ResizeHandle::Top => {
                                                nh = (orig_h - dh).max(min_sz);
                                                ny = orig_y + (orig_h - nh);
                                            }
                                            ResizeHandle::BottomRight => {
                                                nw = (orig_w + dw).max(min_sz);
                                                nh = if keep_aspect {
                                                    (nw * aspect).max(min_sz)
                                                } else {
                                                    (orig_h + dh).max(min_sz)
                                                };
                                            }
                                            ResizeHandle::TopLeft => {
                                                nw = (orig_w - dw).max(min_sz);
                                                nh = if keep_aspect {
                                                    (nw * aspect).max(min_sz)
                                                } else {
                                                    (orig_h - dh).max(min_sz)
                                                };
                                                nx = orig_x + (orig_w - nw);
                                                ny = orig_y + (orig_h - nh);
                                            }
                                            ResizeHandle::TopRight => {
                                                nw = (orig_w + dw).max(min_sz);
                                                nh = if keep_aspect {
                                                    (nw * aspect).max(min_sz)
                                                } else {
                                                    (orig_h - dh).max(min_sz)
                                                };
                                                ny = orig_y + (orig_h - nh);
                                            }
                                            ResizeHandle::BottomLeft => {
                                                nw = (orig_w - dw).max(min_sz);
                                                nh = if keep_aspect {
                                                    (nw * aspect).max(min_sz)
                                                } else {
                                                    (orig_h + dh).max(min_sz)
                                                };
                                                nx = orig_x + (orig_w - nw);
                                            }
                                        }

                                        item.x_px = nx;
                                        item.y_px = ny;
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
                                CanvasDrag::MoveItems { .. } | CanvasDrag::ResizeItem { .. }
                            );
                            this.drag_state = CanvasDrag::None;
                            this.drag_undo_pushed = false;
                            this.active_guides.clear();
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
                                                .is_some_and(|i| i.is_text() && !i.locked)
                                        {
                                            app.state.push_freeform_undo();
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
                            // Optional 5mm Freeform Snap Grid
                            .when(is_freeform && snap_to_grid, |d| {
                                d.child(
                                    canvas(
                                        |_, _, _| {},
                                        move |bounds, (), window, _| {
                                            paint_paper_grid(
                                                bounds,
                                                margin_disp,
                                                grid_step_disp,
                                                window,
                                            );
                                        },
                                    )
                                    .absolute()
                                    .inset_0(),
                                )
                            })
                            // Safe Print Margin Boundary Box
                            .child(
                                div()
                                    .absolute()
                                    .left(px(margin_disp))
                                    .top(px(margin_disp))
                                    .w(px((disp_paper_w - margin_disp * 2.0).max(0.0)))
                                    .h(px((disp_paper_h - margin_disp * 2.0).max(0.0)))
                                    .border_1()
                                    .border_dashed()
                                    .border_color(rgb(0x94A3B8).opacity(0.55)),
                            )
                            // Render all CanvasItems on the sheet
                            .children(rendered_items)
                            // Smart Alignment Guides overlay during drag
                            .when(!active_guides.is_empty(), |d| {
                                d.child(
                                    canvas(
                                        |_, _, _| {},
                                        move |bounds, (), window, _| {
                                            paint_snap_guides(
                                                bounds,
                                                &active_guides,
                                                display_scale,
                                                guide_color,
                                                window,
                                            );
                                        },
                                    )
                                    .absolute()
                                    .inset_0(),
                                )
                            }),
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
