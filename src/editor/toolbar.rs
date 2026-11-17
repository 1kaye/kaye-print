use gpui_kit::{
    Context, IntoElement, Keystroke, ParentElement, Styled, px,
    assets::IconName,
    component::{
        ActiveTheme as _, Icon, Sizable as _,
        button::{Button, ButtonVariants as _},
        h_flex,
        kbd::Kbd,
        menu::{DropdownMenu as _, PopupMenuItem},
        separator::Separator,
        toolbar::Toolbar,
    },
    div,
};

use crate::{
    app::{ActiveRoute, StudioApp},
    canvas::model::ImageTransform,
    editor::EditorSession,
};

pub fn render_editor_toolbar(
    app: &mut StudioApp,
    session: &EditorSession,
    cx: &mut Context<StudioApp>,
) -> impl IntoElement {
    let lang = app.state.language;
    let can_undo = session.can_undo();
    let flip_h = session.working_transform.flip_h;
    let app_ent = cx.entity().clone();

    let app_undo = app_ent.clone();
    let app_rot = app_ent.clone();
    let app_flip = app_ent.clone();
    let app_crop = app_ent.clone();
    let app_reset = app_ent.clone();

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
            Toolbar::new("editor-toolbar-commands")
                .small()
                .child(
                    Button::new("editor-btn-back")
                        .ghost()
                        .small()
                        .icon(IconName::ArrowLeft)
                        .label(lang.tr("app.editor_back"))
                        .tooltip(lang.tr("app.editor_back_tooltip"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.state.route = ActiveRoute::Workspace;
                            this.canvas_focus.focus(window, cx);
                            cx.notify();
                        })),
                )
                .content(
                    Separator::vertical()
                        .h(px(18.0))
                        .mx(px(6.0)),
                )
                // Single "Edit" text button that opens a dropdown menu
                .content(
                    Button::new("editor-dropdown-edit")
                        .ghost()
                        .small()
                        .label(lang.tr("app.editor_edit"))
                        .tooltip(lang.tr("app.editor_edit_tooltip"))
                        .dropdown_menu(move |menu, _, _| {
                            let a_undo = app_undo.clone();
                            let a_rot = app_rot.clone();
                            let a_flip = app_flip.clone();
                            let a_crop = app_crop.clone();
                            let a_reset = app_reset.clone();

                            menu
                                // 1. Undo
                                .item(
                                    PopupMenuItem::element(move |_, _| {
                                        h_flex()
                                            .w_full()
                                            .items_center()
                                            .justify_between()
                                            .gap_6()
                                            .child(
                                                h_flex()
                                                    .gap_2()
                                                    .items_center()
                                                    .child(Icon::new(IconName::Undo).xsmall())
                                                    .child(lang.tr("app.editor_undo")),
                                            )
                                            .child(Kbd::from(Keystroke::parse("ctrl-z").unwrap()))
                                    })
                                    .disabled(!can_undo)
                                    .on_click(move |_, _, cx| {
                                        a_undo.update(cx, |this, cx| {
                                            this.mutate_editor_session(|s| {
                                                s.undo();
                                            });
                                            cx.notify();
                                        });
                                    }),
                                )
                                .separator()
                                // 2. Rotasi Kanan 90°
                                .item(
                                    PopupMenuItem::element(move |_, _| {
                                        h_flex()
                                            .w_full()
                                            .items_center()
                                            .justify_between()
                                            .gap_6()
                                            .child(
                                                h_flex()
                                                    .gap_2()
                                                    .items_center()
                                                    .child(Icon::new(IconName::RotateCw).xsmall())
                                                    .child(lang.tr("app.editor_rotate_cw")),
                                            )
                                            .child(Kbd::from(Keystroke::parse("r").unwrap()))
                                    })
                                    .on_click(move |_, _, cx| {
                                        a_rot.update(cx, |this, cx| {
                                            this.mutate_editor_session(|s| {
                                                s.push_undo();
                                                s.working_transform.rotate_cw_90();
                                                s.preview_asset
                                                    .rebuild_rendered(&s.working_transform);
                                            });
                                            cx.notify();
                                        });
                                    }),
                                )
                                // 3. Balik Horizontal
                                .item(
                                    PopupMenuItem::element(move |_, _| {
                                        h_flex()
                                            .w_full()
                                            .items_center()
                                            .justify_between()
                                            .gap_6()
                                            .child(
                                                h_flex()
                                                    .gap_2()
                                                    .items_center()
                                                    .child(
                                                        Icon::new(IconName::FlipHorizontal2)
                                                            .xsmall(),
                                                    )
                                                    .child(lang.tr("app.editor_flip_h")),
                                            )
                                            .child(Kbd::from(Keystroke::parse("shift-r").unwrap()))
                                    })
                                    .checked(flip_h)
                                    .on_click(move |_, _, cx| {
                                        a_flip.update(cx, |this, cx| {
                                            this.mutate_editor_session(|s| {
                                                s.push_undo();
                                                s.working_transform.toggle_flip_h();
                                                s.preview_asset
                                                    .rebuild_rendered(&s.working_transform);
                                            });
                                            cx.notify();
                                        });
                                    }),
                                )
                                .separator()
                                // 4. Crop
                                .item(
                                    PopupMenuItem::element(move |_, _| {
                                        h_flex()
                                            .w_full()
                                            .items_center()
                                            .justify_between()
                                            .gap_6()
                                            .child(
                                                h_flex()
                                                    .gap_2()
                                                    .items_center()
                                                    .child(Icon::new(IconName::Crop).xsmall())
                                                    .child(lang.tr("app.editor_crop")),
                                            )
                                            .child(Kbd::from(Keystroke::parse("c").unwrap()))
                                    })
                                    .on_click(move |_, _, cx| {
                                        a_crop.update(cx, |this, cx| {
                                            this.mutate_editor_session(|s| {
                                                s.start_crop();
                                            });
                                            cx.notify();
                                        });
                                    }),
                                )
                                .separator()
                                // 5. Reset
                                .item(
                                    PopupMenuItem::element(move |_, _| {
                                        h_flex()
                                            .w_full()
                                            .items_center()
                                            .justify_between()
                                            .gap_6()
                                            .child(
                                                h_flex()
                                                    .gap_2()
                                                    .items_center()
                                                    .child(Icon::new(IconName::RefreshCw).xsmall())
                                                    .child(lang.tr("app.editor_reset")),
                                            )
                                            .child(Kbd::from(Keystroke::parse("ctrl-r").unwrap()))
                                    })
                                    .on_click(move |_, _, cx| {
                                        a_reset.update(cx, |this, cx| {
                                            this.mutate_editor_session(|s| {
                                                s.push_undo();
                                                s.working_transform =
                                                    ImageTransform::default();
                                                s.preview_asset
                                                    .rebuild_rendered(&s.working_transform);
                                            });
                                            cx.notify();
                                        });
                                    }),
                                )
                        }),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .child(
                    Button::new("editor-btn-save")
                        .primary()
                        .small()
                        .icon(IconName::Check)
                        .label(lang.tr("app.editor_save"))
                        .tooltip(lang.tr("app.editor_save_tooltip"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.state.commit_editor_session();
                            this.canvas_focus.focus(window, cx);
                            cx.notify();
                        })),
                ),
        )
}
