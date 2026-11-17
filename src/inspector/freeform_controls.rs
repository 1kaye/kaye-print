use gpui_kit::{
    AppContext as _, Context, Entity, Focusable as _, IntoElement, ParentElement, SharedString,
    Styled, Subscription, Window,
    assets::IconName,
    component::{
        ActiveTheme as _, Icon, IndexPath, Sizable as _, Size,
        button::{Button, ButtonGroup, ButtonVariants as _},
        group_box::{GroupBox, GroupBoxVariants as _},
        h_flex,
        input::{InputEvent, Textarea, TextareaState},
        label::Label,
        select::{SearchableVec, Select, SelectEvent, SelectState},
        v_flex,
    },
    prelude::FluentBuilder as _,
    px,
};

use super::{
    common::{ButtonSelectedExt as _, render_color_select, render_number_field, render_rounded_slider},
    inspector_view::InspectorView,
    layer_list::render_layer_list,
};
use crate::canvas::model::TextAlignMode;

struct TextEditorFieldState {
    textarea: Entity<TextareaState>,
    _sub: Subscription,
}

struct FontSelectState {
    select: Entity<SelectState<SearchableVec<SharedString>>>,
    _sub: Subscription,
}

fn render_text_content_editor(
    item_id: u64,
    current_text: String,
    placeholder: SharedString,
    app: &Entity<crate::app::StudioApp>,
    window: &mut Window,
    cx: &mut Context<InspectorView>,
) -> impl IntoElement {
    let app_for_sub = app.clone();
    let initial = current_text.clone();
    let state = window.use_keyed_state(("freeform-text-editor", item_id), cx, |window, cx| {
        let textarea = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(2, 6)
                .default_value(initial)
                .placeholder(placeholder)
        });
        let sub = cx.subscribe_in(
            &textarea,
            window,
            move |_this: &mut TextEditorFieldState, textarea, event: &InputEvent, _window, cx| {
                if let InputEvent::Change = event {
                    let val = textarea.read(cx).value().to_string();
                    app_for_sub.update(cx, |app, cx| {
                        if let Some(item) = app.state.find_item_mut(item_id)
                            && let Some(tb) = &mut item.text
                            && tb.content != val
                        {
                            tb.content = val;
                            app.state.sync_text_item_size(item_id);
                            cx.notify();
                        }
                    });
                }
            },
        );
        TextEditorFieldState { textarea, _sub: sub }
    });

    let textarea_entity = state.read(cx).textarea.clone();
    if textarea_entity.read(cx).value().as_ref() != current_text.as_str()
        && !textarea_entity.read(cx).focus_handle(cx).is_focused(window)
    {
        textarea_entity.update(cx, |state, cx| {
            state.set_value(current_text, window, cx);
        });
    }

    Textarea::new(&textarea_entity).small().w_full()
}

fn render_font_family_select(
    item_id: u64,
    current_family: String,
    app: &Entity<crate::app::StudioApp>,
    window: &mut Window,
    cx: &mut Context<InspectorView>,
) -> impl IntoElement {
    let fonts_entity = window.use_keyed_state("freeform-system-fonts", cx, |_, cx| {
        let mut names = cx.text_system().all_font_names();
        names.sort_by_cached_key(|n| n.to_lowercase());
        names.dedup();
        if names.is_empty() {
            names.push(".SystemUIFont".to_string());
        }
        names.into_iter().map(SharedString::from).collect::<Vec<_>>()
    });
    let font_list = fonts_entity.read(cx).clone();
    let active_family = if current_family.is_empty() {
        cx.theme().font_family.to_string()
    } else {
        current_family
    };
    let active_ix = font_list
        .iter()
        .position(|f| f.as_ref() == active_family.as_str())
        .or_else(|| {
            font_list
                .iter()
                .position(|f| f.eq_ignore_ascii_case(&active_family))
        })
        .unwrap_or(0);

    let app_for_font = app.clone();
    let select_state = window.use_keyed_state(("freeform-font-select", item_id), cx, |window, cx| {
        let items = SearchableVec::new(font_list.clone());
        let select = cx.new(|cx| {
            SelectState::new(items, Some(IndexPath::new(active_ix)), window, cx).searchable(true)
        });
        let sub = cx.subscribe_in(
            &select,
            window,
            move |_this: &mut FontSelectState,
                  _select,
                  event: &SelectEvent<SearchableVec<SharedString>>,
                  _window,
                  cx| {
                if let SelectEvent::Confirm(Some(chosen)) = event {
                    let chosen_font = chosen.to_string();
                    app_for_font.update(cx, |app, cx| {
                        if let Some(it) = app.state.find_item_mut(item_id)
                            && let Some(tb) = &mut it.text
                            && tb.font_family != chosen_font
                        {
                            tb.font_family = chosen_font;
                            app.state.sync_text_item_size(item_id);
                            cx.notify();
                        }
                    });
                }
            },
        );
        FontSelectState { select, _sub: sub }
    });

    let select_entity = select_state.read(cx).select.clone();
    if select_entity.read(cx).selected_index(cx) != Some(IndexPath::new(active_ix)) {
        select_entity.update(cx, |state, cx| {
            state.set_selected_index(Some(IndexPath::new(active_ix)), window, cx);
        });
    }

    Select::new(&select_entity).w_full()
}

pub fn render_freeform_panel(
    sidebar: &mut InspectorView,
    window: &mut Window,
    cx: &mut Context<InspectorView>,
) -> impl IntoElement {
    let (selected, lang) = {
        let app_guard = sidebar.app.read(cx);
        (app_guard.state.selected_item().cloned(), app_guard.state.language)
    };
    let default_font_family = cx.theme().font_family.to_string();

    v_flex()
        .gap(px(12.0))
        .w_full()
        // 1. Card Susun Bebas
        .child(
            GroupBox::new().outline().title(lang.tr("app.freeform_layout")).child(
                h_flex()
                    .gap_2()
                    .w_full()
                    .child(
                        Button::new("btn-freeform-add-multi")
                            .secondary()
                            .flex_1()
                            .icon(IconName::Plus)
                            .label(lang.tr("app.add_photos"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.app.update(cx, |app, cx| {
                                    app.prompt_multi_images_freeform(cx);
                                });
                            })),
                    )
                    .child(
                        Button::new("btn-freeform-add-text")
                            .secondary()
                            .flex_1()
                            .icon(IconName::Plus)
                            .label(lang.tr("app.add_text"))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                let family = default_font_family.clone();
                                this.app.update(cx, |app, cx| {
                                    app.state.add_freeform_text_box(family);
                                    app.canvas_focus.focus(window, cx);
                                    cx.notify();
                                });
                            })),
                    ),
            ),
        )
        // 2. Layers card - loaded from layer_list.rs
        .child(render_layer_list(sidebar, window, cx))
        // 3. Selected item card: show "Selected Text" for text boxes, or "Selected Image" for photos
        .when_some(selected, |col, item| {
            let id = item.id;
            if let Some(tb) = item.text {
                col.child(
                    GroupBox::new().outline().title(lang.tr("app.selected_text")).child(
                        v_flex()
                            .gap(px(10.0))
                            .w_full()
                            .child(
                                v_flex()
                                    .gap(px(4.0))
                                    .w_full()
                                    .child(
                                        Label::new(lang.tr("app.text_content"))
                                            .text_sm()
                                            .text_color(cx.theme().muted_foreground),
                                    )
                                    .child(render_text_content_editor(
                                        id,
                                        tb.content,
                                        lang.tr("app.text_placeholder"),
                                        &sidebar.app,
                                        window,
                                        cx,
                                    )),
                            )
                            .child(
                                v_flex()
                                    .gap(px(4.0))
                                    .w_full()
                                    .child(
                                        Label::new(lang.tr("app.font_family"))
                                            .text_sm()
                                            .text_color(cx.theme().muted_foreground),
                                    )
                                    .child(render_font_family_select(
                                        id,
                                        tb.font_family,
                                        &sidebar.app,
                                        window,
                                        cx,
                                    )),
                            )
                            .child(render_number_field(
                                ("freeform-text-font-size", id),
                                lang.tr("app.font_size"),
                                tb.font_size_pt,
                                1.0,
                                6.0,
                                300.0,
                                0,
                                "pt",
                                &sidebar.app,
                                window,
                                cx,
                                move |val, app, cx| {
                                    if let Some(it) = app.state.find_item_mut(id)
                                        && let Some(txt) = &mut it.text
                                    {
                                        txt.font_size_pt = val;
                                        app.state.sync_text_item_size(id);
                                        cx.notify();
                                    }
                                },
                            ))
                            .child(
                                v_flex()
                                    .gap(px(4.0))
                                    .w_full()
                                    .child(
                                        Label::new(lang.tr("app.text_align"))
                                            .text_sm()
                                            .text_color(cx.theme().muted_foreground),
                                    )
                                    .child(
                                        ButtonGroup::new(("freeform-text-align", id))
                                            .outline()
                                            .with_size(Size::Medium)
                                            .w_full()
                                            .child(
                                                Button::new("text-align-left")
                                                    .flex_1()
                                                    .icon(Icon::new(IconName::TextAlignStart).with_size(Size::Medium))
                                                    .primary_selected(tb.align == TextAlignMode::Left, cx)
                                                    .tooltip(lang.tr("app.align_left_tooltip")),
                                            )
                                            .child(
                                                Button::new("text-align-center")
                                                    .flex_1()
                                                    .icon(Icon::new(IconName::TextAlignCenter).with_size(Size::Medium))
                                                    .primary_selected(tb.align == TextAlignMode::Center, cx)
                                                    .tooltip(lang.tr("app.align_center_tooltip")),
                                            )
                                            .child(
                                                Button::new("text-align-right")
                                                    .flex_1()
                                                    .icon(Icon::new(IconName::TextAlignEnd).with_size(Size::Medium))
                                                    .primary_selected(tb.align == TextAlignMode::Right, cx)
                                                    .tooltip(lang.tr("app.align_right_tooltip")),
                                            )
                                            .child(
                                                Button::new("text-align-justify")
                                                    .flex_1()
                                                    .icon(Icon::new(IconName::TextAlignJustify).with_size(Size::Medium))
                                                    .primary_selected(tb.align == TextAlignMode::Justified, cx)
                                                    .tooltip(lang.tr("app.text_align_justify")),
                                            )
                                            .on_click(cx.listener(
                                                move |this, clicks: &Vec<usize>, _, cx| {
                                                    if let Some(&first) = clicks.first() {
                                                        let align = match first {
                                                            0 => TextAlignMode::Left,
                                                            1 => TextAlignMode::Center,
                                                            2 => TextAlignMode::Right,
                                                            _ => TextAlignMode::Justified,
                                                        };
                                                        this.app.update(cx, |app, cx| {
                                                            if let Some(it) = app.state.find_item_mut(id)
                                                                && let Some(txt) = &mut it.text
                                                            {
                                                                txt.align = align;
                                                                cx.notify();
                                                            }
                                                        });
                                                    }
                                                },
                                            )),
                                    ),
                            )
                            .child(
                                v_flex()
                                    .gap(px(4.0))
                                    .w_full()
                                    .child(
                                        Label::new(lang.tr("app.font_color"))
                                            .text_sm()
                                            .text_color(cx.theme().muted_foreground),
                                    )
                                    .child(render_color_select(
                                        ("freeform-text-color", id),
                                        Some(tb.color),
                                        tb.color,
                                        &sidebar.app,
                                        window,
                                        cx,
                                        move |color, app, cx| {
                                            if let Some(col) = color
                                                && let Some(it) = app.state.find_item_mut(id)
                                                && let Some(txt) = &mut it.text
                                            {
                                                txt.color = col;
                                                cx.notify();
                                            }
                                        },
                                    )),
                            ),
                    ),
                )
            } else {
                col.child(
                    GroupBox::new().outline().title(lang.tr("app.selected_image")).child(
                        v_flex()
                            .gap(px(10.0))
                            .w_full()
                            // Layout Perataan Posisi
                            .child(
                                v_flex()
                                    .gap(px(4.0))
                                    .child(
                                        Label::new(lang.tr("app.alignment"))
                                            .text_sm()
                                            .text_color(cx.theme().muted_foreground),
                                    )
                                    .child(
                                        ButtonGroup::new(("freeform-align-group", id))
                                            .outline()
                                            .with_size(Size::Medium)
                                            .w_full()
                                            .child(
                                                Button::new("align-left")
                                                    .flex_1()
                                                    .icon(
                                                        Icon::new(IconName::AlignStartHorizontal)
                                                            .with_size(Size::Medium),
                                                    )
                                                    .tooltip(lang.tr("app.align_left_tooltip")),
                                            )
                                            .child(
                                                Button::new("align-center")
                                                    .flex_1()
                                                    .icon(
                                                        Icon::new(IconName::AlignCenterHorizontal)
                                                            .with_size(Size::Medium),
                                                    )
                                                    .tooltip(lang.tr("app.align_center_tooltip")),
                                            )
                                            .child(
                                                Button::new("align-right")
                                                    .flex_1()
                                                    .icon(
                                                        Icon::new(IconName::AlignEndHorizontal)
                                                            .with_size(Size::Medium),
                                                    )
                                                    .tooltip(lang.tr("app.align_right_tooltip")),
                                            )
                                            .child(
                                                Button::new("align-middle")
                                                    .flex_1()
                                                    .icon(
                                                        Icon::new(IconName::AlignCenterVertical)
                                                            .with_size(Size::Medium),
                                                    )
                                                    .tooltip(lang.tr("app.align_middle_tooltip")),
                                            )
                                            .child(
                                                Button::new("align-dist")
                                                    .flex_1()
                                                    .icon(
                                                        Icon::new(
                                                            IconName::AlignHorizontalSpaceBetween,
                                                        )
                                                        .with_size(Size::Medium),
                                                    )
                                                    .tooltip(lang.tr("app.align_dist_tooltip")),
                                            )
                                            .on_click(cx.listener(
                                                |this, clicks: &Vec<usize>, _, cx| {
                                                    if let Some(&first) = clicks.first() {
                                                        this.app.update(cx, |app, cx| {
                                                            match first {
                                                                0 => app.state.align_selected("left"),
                                                                1 => app.state.align_selected("center"),
                                                                2 => app.state.align_selected("right"),
                                                                3 => app.state.align_selected("middle"),
                                                                4 => {
                                                                    app.state.align_selected("distribute")
                                                                }
                                                                _ => {}
                                                            }
                                                            cx.notify();
                                                        });
                                                    }
                                                },
                                            )),
                                    ),
                            )
                            // Slider Rounded
                            .child(
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
                                    .child(render_rounded_slider(
                                        id,
                                        item.corner_radius,
                                        &sidebar.app,
                                        window,
                                        cx,
                                    )),
                            ),
                    ),
                )
            }
        })
}
