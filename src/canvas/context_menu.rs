use gpui_kit::{
    Entity, IntoElement, Keystroke, ParentElement as _, SharedString, Styled as _,
    assets::IconName,
    component::{
        h_flex,
        kbd::Kbd,
        menu::{PopupMenu, PopupMenuItem},
    },
};

use crate::{
    app::StudioApp,
    canvas::model::{ImageTransform, LayoutMode},
    shared::i18n::Language,
};

fn menu_item_with_kbd(
    label: SharedString,
    icon: IconName,
    shortcut: &'static str,
) -> PopupMenuItem {
    let kbd = Kbd::new(Keystroke::parse(shortcut).unwrap()).appearance(false);
    PopupMenuItem::element(move |_, _| {
        h_flex()
            .w_full()
            .items_center()
            .justify_between()
            .gap_4()
            .child(label.clone())
            .child(kbd.clone().into_any_element())
    })
    .icon(icon)
}

pub(crate) fn build_item_popup_menu(
    menu: PopupMenu,
    item_id: u64,
    mode: LayoutMode,
    is_text: bool,
    has_image: bool,
    has_edit: bool,
    lang: Language,
    app_entity: Entity<StudioApp>,
) -> PopupMenu {
    let mut menu = menu;
    let mut has_top_section = false;
    if !is_text {
        has_top_section = true;
        menu = menu.item(
            PopupMenuItem::new(lang.tr("app.ctx_replace_photo"))
                .icon(IconName::Replace)
                .on_click({
                    let app = app_entity.clone();
                    move |_, window, cx| {
                        app.update(cx, |app, cx| {
                            app.canvas_focus.focus(window, cx);
                            app.prompt_single_image_for_item(item_id, cx);
                            cx.notify();
                        });
                    }
                }),
        );
    }

    if !is_text && has_image {
        has_top_section = true;
        menu = menu
            .item(
                PopupMenuItem::new(lang.tr("app.ctx_edit_photo"))
                    .icon(IconName::Image)
                    .on_click({
                        let app = app_entity.clone();
                        move |_, _, cx| {
                            app.update(cx, |app, cx| {
                                app.state.open_editor_for_item(item_id);
                                cx.notify();
                            });
                        }
                    }),
            )
            .item(
                PopupMenuItem::new(lang.tr("app.ctx_crop"))
                    .icon(IconName::Crop)
                    .on_click({
                        let app = app_entity.clone();
                        move |_, _, cx| {
                            app.update(cx, |app, cx| {
                                app.state.open_editor_crop_for_item(item_id);
                                cx.notify();
                            });
                        }
                    }),
            )
            .separator()
            .item(
                menu_item_with_kbd(lang.tr("app.ctx_rotate_cw"), IconName::RotateCw, "r").on_click({
                    let app = app_entity.clone();
                    move |_, window, cx| {
                        app.update(cx, |app, cx| {
                            app.canvas_focus.focus(window, cx);
                            if app
                                .state
                                .transform_item_with(item_id, ImageTransform::rotate_cw_90)
                            {
                                cx.notify();
                            }
                        });
                    }
                }),
            )
            .item(
                menu_item_with_kbd(lang.tr("app.ctx_flip_h"), IconName::FlipHorizontal2, "shift-r")
                    .on_click({
                        let app = app_entity.clone();
                        move |_, window, cx| {
                            app.update(cx, |app, cx| {
                                app.canvas_focus.focus(window, cx);
                                if app
                                    .state
                                    .transform_item_with(item_id, ImageTransform::toggle_flip_h)
                                {
                                    cx.notify();
                                }
                            });
                        }
                    }),
            );

        if has_edit {
            menu = menu.item(
                menu_item_with_kbd(lang.tr("app.ctx_reset_transform"), IconName::RefreshCw, "ctrl-r").on_click(
                    {
                        let app = app_entity.clone();
                        move |_, window, cx| {
                            app.update(cx, |app, cx| {
                                app.canvas_focus.focus(window, cx);
                                if app.state.reset_item_transform(item_id) {
                                    cx.notify();
                                }
                            });
                        }
                    },
                ),
            );
        }
    }

    let can_duplicate = mode != LayoutMode::FullImage;
    let can_delete = mode != LayoutMode::FullImage || has_image;
    if has_top_section && (can_duplicate || can_delete) {
        menu = menu.separator();
    }
    if can_duplicate {
        menu = menu.item(
            menu_item_with_kbd(lang.tr("app.ctx_duplicate"), IconName::Copy, "ctrl-c").on_click({
                let app = app_entity.clone();
                move |_, window, cx| {
                    app.update(cx, |app, cx| {
                        app.canvas_focus.focus(window, cx);
                        app.state.duplicate_item(item_id);
                        cx.notify();
                    });
                }
            }),
        );
    }
    if can_delete {
        menu = menu.item(
            menu_item_with_kbd(lang.tr("app.ctx_delete"), IconName::Trash, "delete").on_click({
                let app = app_entity;
                move |_, window, cx| {
                    app.update(cx, |app, cx| {
                        app.canvas_focus.focus(window, cx);
                        app.state.delete_item(item_id);
                        cx.notify();
                    });
                }
            }),
        );
    }

    menu
}
