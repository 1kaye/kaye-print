use std::{sync::Arc, time::Duration};

use gpui_kit::{
    App, AppContext as _, AnyElement, Context, DragMoveEvent, InteractiveElement as _, IntoElement,
    KeyDownEvent, MouseButton, ObjectFit, ParentElement, Render, RenderImage, SharedString,
    StatefulInteractiveElement as _, Styled, StyledImage as _, Window,
    assets::IconName,
    base::{Spring, spring},
    component::{
        ActiveTheme as _, Disableable as _, group_box::{GroupBox, GroupBoxVariants as _},
        h_flex, Icon, Sizable as _,
        button::{Button, ButtonVariants as _},
        input::Input,
    },
    div, img,
    prelude::FluentBuilder as _,
    px, transparent_black,
};

use super::inspector_view::InspectorView;

const LAYER_ROW_HEIGHT: f32 = 34.0;
const LAYER_ROW_GAP: f32 = 4.0;
const LAYER_ROW_STRIDE: f32 = LAYER_ROW_HEIGHT + LAYER_ROW_GAP;

pub(crate) fn compute_drag_preview_order(
    ids_top_to_bottom: &[u64],
    drag_hover: Option<(u64, usize)>,
) -> Vec<u64> {
    let mut order = ids_top_to_bottom.to_vec();
    if let Some((src_id, target_row)) = drag_hover
        && let Some(src_row) = order.iter().position(|&id| id == src_id)
        && !order.is_empty()
    {
        let item = order.remove(src_row);
        let clamped = target_row.min(order.len());
        order.insert(clamped, item);
    }
    order
}

fn render_layer_thumbnail(
    thumbnail: Option<Arc<RenderImage>>,
    is_text: bool,
    cx: &App,
) -> AnyElement {
    if let Some(rendered) = thumbnail {
        div()
            .w(px(22.0))
            .h(px(22.0))
            .flex_shrink_0()
            .rounded(px(2.0))
            .overflow_hidden()
            .child(
                img(rendered)
                    .size_full()
                    .object_fit(ObjectFit::Cover),
            )
            .into_any_element()
    } else {
        div()
            .w(px(22.0))
            .h(px(22.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(2.0))
            .bg(cx.theme().muted.opacity(0.4))
            .child(
                Icon::new(if is_text {
                    IconName::FileText
                } else {
                    IconName::Image
                })
                .small()
                .text_color(cx.theme().muted_foreground),
            )
            .into_any_element()
    }
}

#[derive(Clone)]
pub struct DragLayerPayload {
    pub item_id: u64,
    pub label: SharedString,
    pub thumbnail: Option<Arc<RenderImage>>,
    pub is_text: bool,
    pub is_selected: bool,
}

impl Render for DragLayerPayload {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(268.0))
            .px_2()
            .py_1()
            .rounded_md()
            .bg(cx.theme().background)
            .border_1()
            .border_color(if self.is_selected {
                cx.theme().primary
            } else {
                cx.theme().border
            })
            .shadow_lg()
            .opacity(0.95)
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_1p5()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .child(
                                Icon::new(IconName::GripVertical)
                                    .small()
                                    .flex_shrink_0()
                                    .text_color(cx.theme().primary),
                            )
                            .child(render_layer_thumbnail(self.thumbnail.clone(), self.is_text, cx))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .text_xs()
                                    .text_color(cx.theme().foreground)
                                    .child(self.label.clone()),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_0p5()
                            .flex_shrink_0()
                            .opacity(0.45)
                            .child(
                                Icon::new(IconName::ChevronUp)
                                    .small()
                                    .text_color(cx.theme().muted_foreground),
                            )
                            .child(
                                Icon::new(IconName::ChevronDown)
                                    .small()
                                    .text_color(cx.theme().muted_foreground),
                            ),
                    ),
            )
    }
}

pub fn render_layer_list(
    sidebar: &mut InspectorView,
    window: &mut Window,
    cx: &mut Context<InspectorView>,
) -> impl IntoElement {
    let (free_items, selected_id, editing_layer_id, lang) = {
        let app_guard = sidebar.app.read(cx);
        (
            app_guard.state.free_items.clone(),
            app_guard.state.selected_id,
            app_guard.state.editing_layer_id,
            app_guard.state.language,
        )
    };

    if free_items.is_empty() {
        sidebar.drag_hover = None;
        return div().into_any_element();
    }

    if !cx.has_active_drag() {
        sidebar.drag_hover = None;
    }

    let rename_input = sidebar
        .rename_session
        .as_ref()
        .map(|(sess_id, input, _)| (*sess_id, input.clone()));

    let total = free_items.len();
    let ids_top_to_bottom: Vec<u64> = free_items.iter().rev().map(|item| item.id).collect();
    let active_drag_hover = sidebar.drag_hover;
    let preview_order = compute_drag_preview_order(&ids_top_to_bottom, active_drag_hover);
    let dragged_src_id = active_drag_hover.map(|(src_id, _)| src_id);

    let mut layer_elements = Vec::with_capacity(total);
    for (idx, item) in free_items.iter().enumerate().rev() {
        let id = item.id;
        let is_selected = selected_id == Some(id);
        let is_drag_source = dragged_src_id == Some(id);
        let visual_row = preview_order
            .iter()
            .position(|&pid| pid == id)
            .unwrap_or(total - 1 - idx);
        let target_y = visual_row as f32 * LAYER_ROW_STRIDE;
        let animated_y = spring(
            format!("layer-y-{id}"),
            target_y,
            Spring::new(Duration::from_millis(220)).with_damping(0.84),
            window,
            cx,
        );

        let rename_entity = rename_input
            .as_ref()
            .filter(|(sess_id, _)| *sess_id == id && editing_layer_id == Some(id))
            .map(|(_, input)| input.clone());
        let is_editing = rename_entity.is_some();
        let label = item.layer_name_for(idx, lang);
        let is_text = item.is_text();
        let thumbnail = item.image.as_ref().map(|asset| asset.rendered.clone());

        let name_slot = if let Some(input_entity) = rename_entity {
            div()
                .id(("layer-rename-wrap", id))
                .flex_1()
                .min_w_0()
                .on_key_down(cx.listener(move |this, ev: &KeyDownEvent, _, cx| {
                    if ev.keystroke.key.eq_ignore_ascii_case("escape") {
                        this.cancel_layer_rename(cx);
                    }
                }))
                .child(Input::new(&input_entity).small().w_full())
                .into_any_element()
        } else {
            let click_label = label.to_string();
            div()
                .id(("layer-name", id))
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_xs()
                .px_1()
                .py_0p5()
                .rounded_sm()
                .cursor_text()
                .hover(|h| h.bg(cx.theme().muted.opacity(0.6)))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.start_layer_rename(id, click_label.clone(), window, cx);
                }))
                .child(label.clone())
                .into_any_element()
        };

        let trailing_actions = if is_editing {
            h_flex()
                .gap_0p5()
                .flex_shrink_0()
                .child(
                    Button::new(("save-layer-name", id))
                        .small()
                        .ghost()
                        .icon(IconName::Check)
                        .tooltip(lang.tr("app.save_name_tooltip"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.commit_layer_rename(id, cx);
                        })),
                )
                .child(
                    Button::new(("cancel-layer-name", id))
                        .small()
                        .ghost()
                        .icon(IconName::X)
                        .tooltip(lang.tr("app.cancel"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.cancel_layer_rename(cx);
                        })),
                )
                .into_any_element()
        } else {
            h_flex()
                .gap_0p5()
                .flex_shrink_0()
                .child(
                    Button::new(("layer-up", id))
                        .small()
                        .ghost()
                        .icon(IconName::ChevronUp)
                        .disabled(idx == total - 1)
                        .tooltip(lang.tr("app.move_up_tooltip"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.app.update(cx, |app, cx| {
                                app.state.move_free_item_up(id);
                                cx.notify();
                            });
                        })),
                )
                .child(
                    Button::new(("layer-down", id))
                        .small()
                        .ghost()
                        .icon(IconName::ChevronDown)
                        .disabled(idx == 0)
                        .tooltip(lang.tr("app.move_down_tooltip"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.app.update(cx, |app, cx| {
                                app.state.move_free_item_down(id);
                                cx.notify();
                            });
                        })),
                )
                .into_any_element()
        };

        layer_elements.push(
            div()
                .id(("layer-item", id))
                .absolute()
                .left_0()
                .right_0()
                .top(px(animated_y))
                .h(px(LAYER_ROW_HEIGHT))
                .px_2()
                .flex()
                .items_center()
                .rounded_md()
                .when(!is_editing, |d| d.cursor_pointer())
                .when(is_drag_source, |d| {
                    d.bg(cx.theme().primary.opacity(0.08))
                        .border_1()
                        .border_dashed()
                        .border_color(cx.theme().primary.opacity(0.6))
                })
                .when(!is_drag_source && (is_selected || is_editing), |d| {
                    d.bg(cx.theme().primary.opacity(0.15))
                        .border_1()
                        .border_color(cx.theme().primary)
                })
                .when(!is_drag_source && !is_selected && !is_editing, |d| {
                    d.hover(|h| h.bg(cx.theme().muted.opacity(0.4)))
                        .border_1()
                        .border_color(transparent_black())
                })
                .when(!is_editing, |d| {
                    let drag_thumb = thumbnail.clone();
                    let drag_label = label.clone();
                    d.on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window: &mut Window, cx| {
                            this.rename_session = None;
                            this.app.update(cx, |app, cx| {
                                app.canvas_focus.focus(window, cx);
                                let changed = app.state.selected_id != Some(id)
                                    || app.state.editing_layer_id.is_some();
                                app.state.selected_id = Some(id);
                                app.state.editing_layer_id = None;
                                if changed {
                                    cx.notify();
                                }
                            });
                        }),
                    )
                    .on_drag(
                        DragLayerPayload {
                            item_id: id,
                            label: drag_label,
                            thumbnail: drag_thumb,
                            is_text,
                            is_selected,
                        },
                        |payload, _, _, cx| cx.new(|_| payload.clone()),
                    )
                })
                .child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .when(is_drag_source, |row| row.opacity(0.35))
                        .child(
                            h_flex()
                                .items_center()
                                .gap_1p5()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    Icon::new(IconName::GripVertical)
                                        .small()
                                        .flex_shrink_0()
                                        .text_color(cx.theme().muted_foreground),
                                )
                                .child(render_layer_thumbnail(thumbnail, is_text, cx))
                                .child(name_slot),
                        )
                        .child(trailing_actions),
                )
                .into_any_element(),
        );
    }

    let list_height = (total as f32 * LAYER_ROW_STRIDE - LAYER_ROW_GAP).max(LAYER_ROW_HEIGHT);

    GroupBox::new()
        .outline()
        .title(lang.tr("app.layers_title"))
        .child(
            div()
                .id("layer-list-rows")
                .relative()
                .w_full()
                .h(px(list_height))
                .on_drag_move::<DragLayerPayload>(cx.listener(
                    move |this, event: &DragMoveEvent<DragLayerPayload>, _, cx| {
                        let next_hover = if event.bounds.contains(&event.event.position) {
                            let src_id = event.drag(cx).item_id;
                            let rel_y = (event.event.position.y - event.bounds.top())
                                .as_f32()
                                .max(0.0);
                            let target_row =
                                ((rel_y / LAYER_ROW_STRIDE) as usize).min(total - 1);
                            Some((src_id, target_row))
                        } else {
                            None
                        };
                        if this.drag_hover != next_hover {
                            this.drag_hover = next_hover;
                            cx.notify();
                        }
                    },
                ))
                .on_drop(cx.listener(
                    move |this, payload: &DragLayerPayload, _, cx| {
                        let drop_row = this.drag_hover.and_then(|(src_id, row)| {
                            (src_id == payload.item_id).then_some(row)
                        });
                        this.drag_hover = None;
                        if let Some(target_row) = drop_row {
                            this.app.update(cx, |app, cx| {
                                if let Some(target_item) =
                                    app.state.free_items.iter().rev().nth(target_row)
                                {
                                    let target_id = target_item.id;
                                    app.state.reorder_free_item(payload.item_id, target_id);
                                }
                                cx.notify();
                            });
                        }
                        cx.notify();
                    },
                ))
                .children(layer_elements),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::compute_drag_preview_order;

    #[test]
    fn test_drag_preview_order_shifts_other_layers() {
        // Urutan tampilan awal (atas -> bawah): [10, 20, 30]
        assert_eq!(compute_drag_preview_order(&[10, 20, 30], None), vec![10, 20, 30]);
        // Second layer (20) moved to first position (row 0) -> layer 10 shifts to row 1
        assert_eq!(
            compute_drag_preview_order(&[10, 20, 30], Some((20, 0))),
            vec![20, 10, 30]
        );
        // First layer (10) moved to bottom position (row 2) -> layers 20 & 30 move up
        assert_eq!(
            compute_drag_preview_order(&[10, 20, 30], Some((10, 2))),
            vec![20, 30, 10]
        );
    }
}
