use std::path::PathBuf;

use gpui_kit::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement, Render, SharedString, Styled, Task, Window,
    component::{ActiveTheme as _, h_flex, notification::Notification, WindowExt as _},
    div,
    prelude::FluentBuilder as _,
    px,
};

use crate::{
    canvas::{
        geometry::load_dynamic_image,
        model::{
            CanvasItem, ContextMenuState, FitMode, FreeformShapeKind, ImageAsset, ImageTransform,
            LayoutMode, IdPhotoPreset, PrintRenderSnapshot, ShapeBoxData, ShapeKind, TextAlignMode,
            TextBoxData,
        },
        CanvasView,
    },
    editor::{render_editor_page, EditorSession},
    inspector::InspectorView,
    printer::{
        model::{LinuxPrinterConfig, PrinterHardwareOptions, PrinterModalDraft},
        open_printer_settings_dialog,
    },
    shared::{
        dialog::{filter_supported_images, prompt_open_images, prompt_save_export},
        i18n::{DEFAULT_LANGUAGE, Language},
        paper::PaperConfig,
        units::{cm_to_px, compute_auto_flow_grid, mm_to_px, PhysicalUnit},
    },
};

gpui_kit::actions!([OpenPrinterSettings]);

#[derive(Clone)]
pub enum ActiveRoute {
    Workspace,
    ImageEditor(EditorSession),
}

pub struct StudioApp {
    pub state: StudioState,
    pub canvas_focus: FocusHandle,
    pub canvas: Entity<CanvasView>,
    pub inspector_view: Entity<InspectorView>,
    pub _printer_poll_task: Task<()>,
}

impl StudioApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        // Background auto-refresh loop: automatically detects printer online/offline status every 5 seconds
        let printer_poll_task = cx.spawn(async move |this, cx| {
            loop {
                let (printers, default_p) = cx
                    .background_executor()
                    .spawn(async move { crate::printer::get_printers() })
                    .await;

                let update_res = this.update(cx, |this, cx| {
                    if this.state.available_printers != printers {
                        this.state.available_printers = printers;
                        if !this
                            .state
                            .available_printers
                            .contains(&this.state.selected_printer)
                        {
                            this.state.selected_printer = if !default_p.is_empty() {
                                default_p
                            } else {
                                this.state.available_printers.first().cloned().unwrap_or_default()
                            };
                        }
                        cx.notify();
                    }
                });

                if update_res.is_err() {
                    break;
                }

                cx.background_executor()
                    .timer(std::time::Duration::from_secs(5))
                    .await;
            }
        });

        let app_handle = cx.entity().clone();
        let canvas = cx.new(|cx| CanvasView::new(app_handle.clone(), cx));
        let inspector_view = cx.new(|cx| InspectorView::new(app_handle, cx));

        Self {
            state: StudioState::new(),
            canvas_focus: cx.focus_handle(),
            canvas,
            inspector_view,
            _printer_poll_task: printer_poll_task,
        }
    }

    pub fn prompt_single_image_for_item(&mut self, item_id: u64, cx: &mut Context<Self>) {
        if self.state.is_opening_file_dialog {
            return;
        }
        self.state.is_opening_file_dialog = true;

        let fut = prompt_open_images(false, self.state.language);
        let tf = if self.state.mode == LayoutMode::Freeform {
            ImageTransform::default()
        } else {
            self.state
                .find_item(item_id)
                .map(|i| i.transform.clone())
                .unwrap_or_default()
        };
        cx.spawn(async move |this, cx| {
            let paths_opt = fut.await;
            if let Some(paths) = paths_opt {
                let valid = filter_supported_images(paths);
                if let Some(first) = valid.into_iter().next() {
                    let loaded = cx
                        .background_executor()
                        .spawn(async move {
                            let raw = load_dynamic_image(&first)?;
                            let name = first
                                .file_name()
                                .and_then(|s| s.to_str())
                                .unwrap_or("foto")
                                .to_string();
                            Ok::<ImageAsset, String>(ImageAsset::from_dynamic(
                                name,
                                Some(first),
                                raw,
                                &tf,
                            ))
                        })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        this.state.is_opening_file_dialog = false;
                        match loaded {
                            Ok(asset) => this.state.assign_loaded_asset_to_item(item_id, asset),
                            Err(e) => this.state.status_message = SharedString::from(e),
                        }
                        cx.notify();
                    });
                    return;
                }
            }
            let _ = this.update(cx, |this, _| {
                this.state.is_opening_file_dialog = false;
            });
        })
        .detach();
    }

    pub fn prompt_multi_images_freeform(&mut self, cx: &mut Context<Self>) {
        if self.state.is_opening_file_dialog {
            return;
        }
        self.state.is_opening_file_dialog = true;

        let fut = prompt_open_images(true, self.state.language);
        cx.spawn(async move |this, cx| {
            let paths_opt = fut.await;
            if let Some(paths) = paths_opt {
                let valid = filter_supported_images(paths);
                if !valid.is_empty() {
                    let assets = cx
                        .background_executor()
                        .spawn(async move {
                            let mut out = Vec::new();
                            let tf = ImageTransform::default();
                            for p in valid {
                                if let Ok(raw) = load_dynamic_image(&p) {
                                    let name = p
                                        .file_name()
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("foto")
                                        .to_string();
                                    out.push(ImageAsset::from_dynamic(name, Some(p), raw, &tf));
                                }
                            }
                            out
                        })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        this.state.is_opening_file_dialog = false;
                        this.state.insert_loaded_freeform_assets(assets);
                        cx.notify();
                    });
                    return;
                }
            }
            let _ = this.update(cx, |this, _| {
                this.state.is_opening_file_dialog = false;
            });
        })
        .detach();
    }

    pub fn trigger_open_printer_properties(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }

        #[cfg(not(windows))]
        {
            let hw = &self.state.cached_hw_options;
            let cur_media = if self.state.linux_printer_config.media_type.is_empty() {
                hw.default_media_type.clone()
            } else {
                self.state.linux_printer_config.media_type.clone()
            };
            let cur_quality = if self.state.linux_printer_config.print_quality.is_empty() {
                hw.default_print_quality.clone()
            } else {
                self.state.linux_printer_config.print_quality.clone()
            };

            let draft = PrinterModalDraft {
                paper_preset: self.state.paper.preset,
                orientation: self.state.paper.orientation,
                media_type: cur_media,
                print_quality: cur_quality,
                is_grayscale: self.state.is_grayscale,
                copies: self.state.linux_printer_config.copies.max(1),
                media_choices: hw.media_types.clone(),
            };
            self.state.printer_modal = Some(draft.clone());
            let lang = self.state.language;
            let printer_name = if self.state.selected_printer.is_empty() {
                lang.tr("app.no_printer_bracket").to_string()
            } else {
                self.state.selected_printer.clone()
            };
            cx.notify();

            open_printer_settings_dialog(
                cx.entity().clone(),
                draft,
                printer_name,
                lang,
                window,
                cx,
            );
        }
        #[cfg(windows)]
        {
            let printer = self.state.selected_printer.clone();
            let lang = self.state.language;
            if let Err(e) = crate::printer::open_printer_properties(&printer) {
                self.state.status_message = lang.open_printer_props_err(&e.to_string());
                cx.notify();
            }
        }
    }

    pub fn refresh_printers(&mut self, cx: &mut Context<Self>) {
        let (printers, default_p) = crate::printer::get_printers();
        self.state.available_printers = printers;
        if !self
            .state
            .available_printers
            .contains(&self.state.selected_printer)
        {
            self.state.selected_printer = if !default_p.is_empty() {
                default_p
            } else {
                self.state.available_printers.first().cloned().unwrap_or_default()
            };
        }
        cx.notify();
    }

    pub fn trigger_direct_print(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.is_printing {
            return;
        }

        let lang = self.state.language;
        let printer = self.state.selected_printer.clone();
        if printer.is_empty() || !self.state.available_printers.contains(&printer) {
            window.push_notification(
                Notification::warning(lang.tr("app.warn_no_printer"))
                    .title(lang.tr("app.warn_title")),
                cx,
            );
            return;
        }

        // Validasi fisik real-time: pastikan printer menyala dan terhubung
        if let Err(e) = crate::printer::check_printer_ready(&printer) {
            window.push_notification(
                Notification::warning(e).title(lang.tr("app.warn_printer_not_ready")),
                cx,
            );
            self.refresh_printers(cx);
            return;
        }

        if !self.state.current_mode_has_images() {
            window.push_notification(
                Notification::warning(lang.tr("app.warn_empty_canvas"))
                    .title(lang.tr("app.warn_empty_canvas_title")),
                cx,
            );
            return;
        }

        self.state.is_printing = true;
        self.state.status_message = lang.processing_doc(&printer);
        cx.notify();

        let snapshot = self.state.create_print_snapshot();
        let (w_mm, h_mm) = self.state.paper.dimensions_mm();
        let is_grayscale = self.state.is_grayscale;
        let preset = self.state.paper.preset;
        let config = self.state.linux_printer_config.clone();

        cx.spawn_in(window, async move |this, cx| {
            let printer_target = printer.clone();
            let res = cx
                .background_executor()
                .spawn(async move {
                    let cropped = snapshot.render_cropped_sheet();
                    let print_res = crate::printer::print_cropped_sheet(
                        &printer_target,
                        &cropped.sheet,
                        w_mm,
                        h_mm,
                        cropped.crop_x_mm,
                        cropped.crop_y_mm,
                        cropped.crop_w_mm,
                        cropped.crop_h_mm,
                        is_grayscale,
                        preset,
                        &config,
                    );
                    drop(cropped);
                    print_res
                })
                .await;

            let _ = this.update_in(cx, |this, window, cx| {
                this.state.is_printing = false;
                let lang = this.state.language;
                match res {
                    Ok(()) => {
                        this.state.status_message = lang.print_sent_status(&printer);
                        window.push_notification(
                            Notification::success(lang.print_sent_notif(&printer))
                                .title(lang.tr("app.print_success_title")),
                            cx,
                        );
                    }
                    Err(e) => {
                        this.state.status_message = lang.print_failed_status(&e);
                        window.push_notification(
                            Notification::error(lang.print_failed_notif(&e))
                                .title(lang.tr("app.print_error_title")),
                            cx,
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn trigger_export_pdf(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.is_exporting || self.state.is_printing {
            return;
        }
        self.state.is_exporting = true;
        cx.notify();

        let fut = prompt_save_export("cetak_foto.pdf", self.state.language);
        let snapshot = self.state.create_print_snapshot();
        let (w_mm, h_mm) = self.state.paper.dimensions_mm();

        cx.spawn_in(window, async move |this, cx| {
            let path_opt = fut.await;
            if let Some(path_buf) = path_opt {
                let path_clone = path_buf.clone();
                let res = cx
                    .background_executor()
                    .spawn(async move {
                        let cropped = snapshot.render_cropped_sheet();
                        let save_res = crate::canvas::geometry::save_cropped_sheet_to_pdf(
                            &cropped.sheet,
                            w_mm,
                            h_mm,
                            cropped.crop_x_mm,
                            cropped.crop_y_mm,
                            cropped.crop_w_mm,
                            cropped.crop_h_mm,
                            0.0,
                            0.0,
                            path_clone.as_path(),
                        );
                        drop(cropped);
                        save_res
                    })
                    .await;

                let _ = this.update_in(cx, |this, window, cx| {
                    this.state.is_exporting = false;
                    let lang = this.state.language;
                    let path_disp = path_buf.display().to_string();
                    match res {
                        Ok(()) => {
                            this.state.status_message = lang.saved_status(&path_disp);
                            window.push_notification(
                                Notification::success(lang.pdf_saved_notif(&path_disp))
                                    .title(lang.tr("app.export_success_title")),
                                cx,
                            );
                        }
                        Err(e) => {
                            this.state.status_message = SharedString::from(e.clone());
                            window.push_notification(
                                Notification::error(e).title(lang.tr("app.export_error_title")),
                                cx,
                            );
                        }
                    }
                    cx.notify();
                });
            } else {
                let _ = this.update_in(cx, |this, _, cx| {
                    this.state.is_exporting = false;
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub fn mutate_editor_session(&mut self, f: impl FnOnce(&mut EditorSession)) {
        if let ActiveRoute::ImageEditor(ref mut session) = self.state.route {
            f(session);
        }
    }
}

impl Render for StudioApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let route = self.state.route.clone();

        div()
            .size_full()
            .relative()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .on_action(cx.listener(|this, _: &OpenPrinterSettings, window, cx| {
                this.trigger_open_printer_properties(window, cx);
            }))
            .child(match route {
                ActiveRoute::Workspace => h_flex()
                    .items_stretch()
                    .size_full()
                    .when(self.state.sidebar_open, |row| {
                        row.child(
                            div()
                                .w(px(320.0))
                                .h_full()
                                .flex_shrink_0()
                                .child(
                                    self.inspector_view
                                        .clone()
                                        .cached(gpui_kit::StyleRefinement::default().size_full()),
                                ),
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .child(self.canvas.clone()),
                    )
                    .into_any_element(),
                ActiveRoute::ImageEditor(session) => {
                    render_editor_page(self, session, _window, cx).into_any_element()
                }
            })
    }
}

pub struct StudioState {
    pub route: ActiveRoute,
    pub language: Language,
    pub mode: LayoutMode,
    pub sidebar_open: bool,
    pub paper: PaperConfig,

    // Mode 1: Full Image
    pub full_item: CanvasItem,
    pub full_fit_mode: FitMode,
    pub full_margin_offset_mm: f32,

    // Mode 2: Pas Foto (Grid & Clipping Shape)
    pub id_photo_items: Vec<CanvasItem>,
    pub id_photo_shape_kind: ShapeKind,
    pub id_photo_preset: IdPhotoPreset,
    pub id_photo_custom_w_cm: f32,
    pub id_photo_custom_h_cm: f32,
    pub id_photo_unit: PhysicalUnit,
    pub id_photo_quantity: u32,
    pub id_photo_h_gap_px: f32,
    pub id_photo_v_gap_px: f32,
    pub id_photo_cut_line: bool,
    pub id_photo_free_move: bool,

    // Mode 3: Freeform Collage
    pub free_items: Vec<CanvasItem>,
    pub snap_to_grid: bool,

    // Selection & Canvas Viewport
    pub selected_id: Option<u64>,
    pub editing_layer_id: Option<u64>,
    pub editing_text_id: Option<u64>,
    pub context_menu: Option<ContextMenuState>,
    pub next_id: u64,
    pub status_message: SharedString,

    // Printer Configuration
    pub available_printers: Vec<String>,
    pub selected_printer: String,
    pub is_grayscale: bool,
    pub linux_printer_config: LinuxPrinterConfig,
    #[cfg(not(windows))]
    pub cached_hw_options: PrinterHardwareOptions,
    pub printer_modal: Option<PrinterModalDraft>,
    pub is_printing: bool,
    pub is_exporting: bool,
    pub is_opening_file_dialog: bool,
}

impl StudioState {
    pub fn new() -> Self {
        let paper = PaperConfig::default();
        let default_tf = ImageTransform::default();

        let (ml, mt, pw, ph) = paper.printable_bounds_px();
        let full_item = CanvasItem {
            id: 1,
            x_px: ml,
            y_px: mt,
            width_px: pw,
            height_px: ph,
            rotation_deg: 0.0,
            lock_aspect: true,
            shape_kind: ShapeKind::Rectangle,
            fit_mode: FitMode::Cover,
            cut_line: false,
            cut_line_width: 1.0,
            preset: None,
            image: None,
            text: None,
            shape: None,
            transform: default_tf.clone(),
            corner_radius: 0.0,
            bg_color: None,
            custom_name: None,
        };

        let initial_config = LinuxPrinterConfig::default();

        let mut state = Self {
            route: ActiveRoute::Workspace,
            language: DEFAULT_LANGUAGE,
            mode: LayoutMode::IdPhoto,
            sidebar_open: true,
            paper,
            full_item,
            full_fit_mode: FitMode::Cover,
            full_margin_offset_mm: 0.0,
            id_photo_items: Vec::new(),
            id_photo_shape_kind: ShapeKind::Rectangle,
            id_photo_preset: IdPhotoPreset::ThreeByFour,
            id_photo_custom_w_cm: 3.0,
            id_photo_custom_h_cm: 4.0,
            id_photo_unit: PhysicalUnit::Cm,
            id_photo_quantity: 1,
            id_photo_h_gap_px: 5.0,
            id_photo_v_gap_px: 5.0,
            id_photo_cut_line: true,
            id_photo_free_move: false,
            free_items: Vec::new(),
            snap_to_grid: false,
            selected_id: None,
            editing_layer_id: None,
            editing_text_id: None,
            context_menu: None,
            next_id: 2,
            status_message: DEFAULT_LANGUAGE.tr("app.status_ready"),
            available_printers: Vec::new(),
            selected_printer: String::new(),
            is_grayscale: false,
            linux_printer_config: initial_config,
            #[cfg(not(windows))]
            cached_hw_options: PrinterHardwareOptions::default(),
            printer_modal: None,
            is_printing: false,
            is_exporting: false,
            is_opening_file_dialog: false,
        };

        state.add_id_photo_frames();

        state
    }

    pub fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn active_items(&self) -> &[CanvasItem] {
        match self.mode {
            LayoutMode::FullImage => std::slice::from_ref(&self.full_item),
            LayoutMode::IdPhoto => &self.id_photo_items,
            LayoutMode::Freeform => &self.free_items,
        }
    }

    pub fn active_items_mut(&mut self) -> &mut [CanvasItem] {
        match self.mode {
            LayoutMode::FullImage => std::slice::from_mut(&mut self.full_item),
            LayoutMode::IdPhoto => &mut self.id_photo_items,
            LayoutMode::Freeform => &mut self.free_items,
        }
    }

    pub fn find_item(&self, id: u64) -> Option<&CanvasItem> {
        self.active_items().iter().find(|i| i.id == id)
    }

    pub fn find_item_mut(&mut self, id: u64) -> Option<&mut CanvasItem> {
        self.active_items_mut().iter_mut().find(|i| i.id == id)
    }

    pub fn selected_item(&self) -> Option<&CanvasItem> {
        let id = self.selected_id?;
        self.find_item(id)
    }

    fn natural_freeform_size(paper: &PaperConfig, asset: &ImageAsset) -> (f32, f32) {
        let (_, _, pw, ph) = paper.printable_bounds_px();
        let orig_w = asset.orig_width.max(1) as f32;
        let orig_h = asset.orig_height.max(1) as f32;
        // Use natural photo dimensions without default size; scale proportionally only if exceeding printable area
        let fit_scale = (pw / orig_w).min(ph / orig_h).min(1.0);
        (orig_w * fit_scale, orig_h * fit_scale)
    }

    pub fn assign_loaded_asset_to_item(&mut self, target_id: u64, asset: ImageAsset) {
        let name = asset.label.clone();
        let mode = self.mode;
        let paper = self.paper.clone();
        if let Some(item) = self.find_item_mut(target_id) {
            if mode == LayoutMode::Freeform {
                let (nat_w, nat_h) = Self::natural_freeform_size(&paper, &asset);
                item.transform = ImageTransform::default();
                item.rotation_deg = 0.0;
                item.lock_aspect = true;
                item.width_px = nat_w;
                item.height_px = nat_h;
                item.assign_image(asset);
                Self::clamp_item_to_safe_margin(&paper, item);
            } else {
                item.assign_image(asset);
            }
            self.selected_id = Some(target_id);
            self.status_message = self.language.loaded_photo(&name);
        }
    }

    #[allow(dead_code)]
    pub fn load_image_into_item(&mut self, target_id: u64, path: PathBuf) {
        if let Ok(raw) = load_dynamic_image(&path) {
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("image")
                .to_string();
            let tf = if self.mode == LayoutMode::Freeform {
                ImageTransform::default()
            } else {
                self.find_item(target_id)
                    .map(|i| i.transform.clone())
                    .unwrap_or_default()
            };
            let asset = ImageAsset::from_dynamic(name, Some(path), raw, &tf);
            self.assign_loaded_asset_to_item(target_id, asset);
        }
    }

    pub fn duplicate_item(&mut self, id: u64) {
        match self.mode {
            LayoutMode::FullImage => {}
            LayoutMode::IdPhoto => {
                if let Some(item) = self.id_photo_items.iter().find(|i| i.id == id).cloned() {
                    let new_id = self.alloc_id();
                    let mut copy = item;
                    copy.id = new_id;
                    if self.id_photo_free_move {
                        copy.x_px += 20.0;
                        copy.y_px += 20.0;
                        Self::clamp_item_to_safe_margin(&self.paper, &mut copy);
                    }
                    self.id_photo_items.push(copy);
                    self.selected_id = Some(new_id);
                    if !self.id_photo_free_move {
                        self.reflow_id_photo_grid();
                    }
                }
            }
            LayoutMode::Freeform => {
                if let Some(item) = self.free_items.iter().find(|i| i.id == id).cloned() {
                    let new_id = self.alloc_id();
                    let mut copy = item;
                    copy.id = new_id;
                    copy.x_px += 30.0;
                    copy.y_px += 30.0;
                    Self::clamp_item_to_safe_margin(&self.paper, &mut copy);
                    self.free_items.push(copy);
                    self.selected_id = Some(new_id);
                }
            }
        }
    }

    pub fn delete_item(&mut self, id: u64) {
        if self.context_menu.as_ref().map(|c| c.item_id) == Some(id) {
            self.context_menu = None;
        }
        if self.editing_text_id == Some(id) {
            self.editing_text_id = None;
        }
        match self.mode {
            LayoutMode::FullImage => {
                self.full_item.image = None;
                self.full_item.transform = ImageTransform::default();
                self.status_message = self.language.tr("app.status_photo_deleted");
            }
            LayoutMode::IdPhoto => {
                self.id_photo_items.retain(|i| i.id != id);
                if self.selected_id == Some(id) {
                    self.selected_id = self.id_photo_items.last().map(|i| i.id);
                }
                if self.id_photo_items.is_empty() {
                    self.id_photo_items.shrink_to_fit();
                }
                if !self.id_photo_free_move {
                    self.reflow_id_photo_grid();
                }
            }
            LayoutMode::Freeform => {
                self.free_items.retain(|i| i.id != id);
                if self.selected_id == Some(id) {
                    self.selected_id = self.free_items.last().map(|i| i.id);
                }
                if self.free_items.is_empty() {
                    self.free_items.shrink_to_fit();
                }
            }
        }
    }

    pub fn current_mode_has_images(&self) -> bool {
        match self.mode {
            LayoutMode::FullImage => self.full_item.image.is_some(),
            LayoutMode::IdPhoto => self.id_photo_items.iter().any(|i| i.image.is_some()),
            LayoutMode::Freeform => self
                .free_items
                .iter()
                .any(|i| i.image.is_some() || i.text.is_some() || i.shape.is_some()),
        }
    }

    pub fn reset_images_and_switch_mode(&mut self, new_mode: LayoutMode) {
        if self.mode == new_mode {
            return;
        }
        match self.mode {
            LayoutMode::FullImage => {
                self.full_item.image = None;
                self.full_item.transform = ImageTransform::default();
            }
            LayoutMode::IdPhoto => {
                for item in &mut self.id_photo_items {
                    item.image = None;
                    item.transform = ImageTransform::default();
                }
            }
            LayoutMode::Freeform => {
                self.free_items.clear();
            }
        }
        self.mode = new_mode;
        self.selected_id = None;
        self.editing_text_id = None;
        self.status_message = self
            .language
            .switched_mode(new_mode.label_for(self.language).as_ref());

        #[cfg(target_os = "linux")]
        unsafe {
            let mut epoch = 1u64;
            let sz = std::mem::size_of::<u64>();
            let _ = tikv_jemalloc_sys::mallctl(
                b"epoch\0".as_ptr() as *const _,
                &mut epoch as *mut _ as *mut _,
                &sz as *const _ as *mut _,
                &epoch as *const _ as *mut _,
                sz,
            );
            let _ = tikv_jemalloc_sys::mallctl(
                b"arena.4096.purge\0".as_ptr() as *const _,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
            );

            unsafe extern "C" {
                fn malloc_trim(pad: usize) -> i32;
            }
            malloc_trim(0);
        }
    }

    pub fn save_and_apply_printer_modal(&mut self) {
        if let Some(draft) = self.printer_modal.take() {
            let paper_changed = self.paper.preset != draft.paper_preset
                || self.paper.orientation != draft.orientation;
            self.paper.preset = draft.paper_preset;
            self.paper.orientation = draft.orientation;
            self.is_grayscale = draft.is_grayscale;
            self.linux_printer_config.media_type = draft.media_type;
            self.linux_printer_config.print_quality = draft.print_quality;
            self.linux_printer_config.copies = draft.copies;

            if paper_changed {
                self.sync_full_image_bounds();
                self.reflow_id_photo_grid();
                self.clamp_all_freeform_items();
            }
            self.status_message = self.language.tr("app.status_printer_applied");
        }
    }

    pub fn sync_full_image_bounds(&mut self) {
        let (ml, mt, pw, ph) = self.paper.printable_bounds_px();
        let offset_px = mm_to_px(self.full_margin_offset_mm, self.paper.dpi);
        self.full_item.x_px = ml + offset_px;
        self.full_item.y_px = mt + offset_px;
        self.full_item.width_px = (pw - offset_px * 2.0).max(20.0);
        self.full_item.height_px = (ph - offset_px * 2.0).max(20.0);
        self.full_item.fit_mode = self.full_fit_mode;
    }

    pub fn reflow_id_photo_grid(&mut self) {
        let (ml, mt, pw, _) = self.paper.printable_bounds_px();
        let sizes: Vec<(f32, f32)> = self
            .id_photo_items
            .iter()
            .map(|item| (item.width_px, item.height_px))
            .collect();
        let positions =
            compute_auto_flow_grid(&sizes, ml, mt, pw, self.id_photo_h_gap_px, self.id_photo_v_gap_px);
        for (item, (x, y)) in self.id_photo_items.iter_mut().zip(positions) {
            item.x_px = x;
            item.y_px = y;
        }
    }

    pub fn add_id_photo_frames(&mut self) {
        let (w_cm, h_cm) = self
            .id_photo_preset
            .dimensions_cm(self.id_photo_custom_w_cm, self.id_photo_custom_h_cm);
        let w_px = cm_to_px(w_cm, self.paper.dpi).round().max(24.0);
        let h_px = cm_to_px(h_cm, self.paper.dpi).round().max(24.0);
        let qty = self.id_photo_quantity.clamp(1, 100);

        let mut last_id = None;
        for _ in 0..qty {
            let id = self.alloc_id();
            last_id = Some(id);
            self.id_photo_items.push(CanvasItem {
                id,
                x_px: 0.0,
                y_px: 0.0,
                width_px: w_px,
                height_px: h_px,
                rotation_deg: 0.0,
                lock_aspect: true,
                shape_kind: self.id_photo_shape_kind,
                fit_mode: FitMode::Cover,
                cut_line: self.id_photo_cut_line,
                cut_line_width: 1.0,
                preset: Some(self.id_photo_preset),
                image: None,
                text: None,
                shape: None,
                transform: ImageTransform::default(),
                corner_radius: 0.0,
                bg_color: None,
                custom_name: None,
            });
        }
        self.selected_id = last_id.or(self.selected_id);
        self.reflow_id_photo_grid();
        self.status_message = self.language.added_id_photos(qty, w_cm, h_cm);
    }

    pub fn clamp_item_to_safe_margin(paper: &PaperConfig, item: &mut CanvasItem) {
        let (ml, mt, pw, ph) = paper.printable_bounds_px();
        if item.lock_aspect && item.width_px > 0.0 && item.height_px > 0.0 {
            let scale = (pw / item.width_px).min(ph / item.height_px).min(1.0);
            if scale < 1.0 {
                item.width_px *= scale;
                item.height_px *= scale;
            }
            item.width_px = item.width_px.max(1.0).min(pw);
            item.height_px = item.height_px.max(1.0).min(ph);
        } else {
            item.width_px = item.width_px.clamp(32.0, pw);
            item.height_px = item.height_px.clamp(32.0, ph);
        }

        let max_x = (ml + pw - item.width_px).max(ml);
        let max_y = (mt + ph - item.height_px).max(mt);
        item.x_px = item.x_px.clamp(ml, max_x);
        item.y_px = item.y_px.clamp(mt, max_y);
    }

    pub fn clamp_all_freeform_items(&mut self) {
        let paper = self.paper.clone();
        for item in &mut self.free_items {
            Self::clamp_item_to_safe_margin(&paper, item);
        }
    }

    pub fn insert_loaded_freeform_assets(&mut self, assets: Vec<ImageAsset>) {
        let (ml, mt, _, _) = self.paper.printable_bounds_px();
        let paper = self.paper.clone();
        let count = assets.len();
        for (idx, asset) in assets.into_iter().enumerate() {
            let (nat_w, nat_h) = Self::natural_freeform_size(&paper, &asset);
            let id = self.alloc_id();
            let offset = (self.free_items.len() + idx) as f32 * 32.0;
            let mut item = CanvasItem {
                id,
                x_px: ml + 20.0 + offset,
                y_px: mt + 20.0 + offset,
                width_px: nat_w,
                height_px: nat_h,
                rotation_deg: 0.0,
                lock_aspect: true,
                shape_kind: ShapeKind::Rectangle,
                fit_mode: FitMode::Cover,
                cut_line: false,
                cut_line_width: 1.0,
                preset: None,
                image: Some(asset),
                text: None,
                shape: None,
                transform: ImageTransform::default(),
                corner_radius: 0.0,
                bg_color: None,
                custom_name: None,
            };
            Self::clamp_item_to_safe_margin(&paper, &mut item);
            self.free_items.push(item);
            self.selected_id = Some(id);
        }
        if count > 0 {
            self.status_message = self.language.added_freeform_photos(count);
        }
    }

    pub fn sync_text_item_size(&mut self, id: u64) {
        let paper = self.paper.clone();
        let (_, _, pw, ph) = paper.printable_bounds_px();
        let fallback = self.language.tr("app.text_placeholder").to_string();
        if let Some(item) = self.free_items.iter_mut().find(|i| i.id == id)
            && let Some(tb) = &item.text
        {
            let fixed_w = if tb.manual_size {
                Some(item.width_px)
            } else {
                None
            };
            let (measured_w, measured_h) = crate::canvas::geometry::measure_text_box_size_px(
                &tb.content,
                &fallback,
                tb.font_size_pt,
                &tb.font_family,
                paper.dpi,
                fixed_w,
                pw,
            );
            if tb.manual_size {
                item.height_px = item.height_px.max(measured_h).min(ph);
            } else {
                item.width_px = measured_w.min(pw);
                item.height_px = measured_h.min(ph);
            }
            Self::clamp_item_to_safe_margin(&paper, item);
        }
    }

    pub fn add_freeform_text_box(&mut self, default_font_family: impl Into<String>) {
        if self.mode != LayoutMode::Freeform {
            return;
        }
        let (ml, mt, _, _) = self.paper.printable_bounds_px();
        let id = self.alloc_id();
        let offset = self.free_items.len() as f32 * 32.0;
        let item = CanvasItem {
            id,
            x_px: ml + 20.0 + offset,
            y_px: mt + 20.0 + offset,
            width_px: 100.0,
            height_px: 60.0,
            rotation_deg: 0.0,
            lock_aspect: false,
            shape_kind: ShapeKind::Rectangle,
            fit_mode: FitMode::Cover,
            cut_line: false,
            cut_line_width: 1.0,
            preset: None,
            image: None,
            text: Some(TextBoxData {
                content: self.language.tr("app.default_text_content").to_string(),
                font_size_pt: 24.0,
                font_family: default_font_family.into(),
                color: gpui_kit::rgb(0x111827).into(),
                align: TextAlignMode::Left,
                manual_size: false,
            }),
            shape: None,
            transform: ImageTransform::default(),
            corner_radius: 0.0,
            bg_color: None,
            custom_name: None,
        };
        self.free_items.push(item);
        self.sync_text_item_size(id);
        self.selected_id = Some(id);
        self.editing_text_id = None;
        self.status_message = self.language.tr("app.added_freeform_text");
    }

    pub fn add_freeform_shape(&mut self, kind: FreeformShapeKind) {
        if self.mode != LayoutMode::Freeform {
            return;
        }
        let (ml, mt, _, _) = self.paper.printable_bounds_px();
        let paper = self.paper.clone();
        let id = self.alloc_id();
        let offset = self.free_items.len() as f32 * 32.0;
        let mut item = CanvasItem {
            id,
            x_px: ml + 20.0 + offset,
            y_px: mt + 20.0 + offset,
            width_px: 300.0,
            height_px: 300.0,
            rotation_deg: 0.0,
            lock_aspect: false,
            shape_kind: ShapeKind::Rectangle,
            fit_mode: FitMode::Cover,
            cut_line: false,
            cut_line_width: 1.0,
            preset: None,
            image: None,
            text: None,
            shape: Some(ShapeBoxData {
                kind,
                fill_color: gpui_kit::rgb(0x3B82F6).into(),
            }),
            transform: ImageTransform::default(),
            corner_radius: 0.0,
            bg_color: None,
            custom_name: None,
        };
        Self::clamp_item_to_safe_margin(&paper, &mut item);
        self.free_items.push(item);
        self.selected_id = Some(id);
        self.editing_text_id = None;
        self.status_message = self.language.tr("app.added_freeform_shape");
    }

    #[allow(dead_code)]
    pub fn bring_to_front(&mut self, id: u64) {
        if self.mode == LayoutMode::Freeform
            && let Some(pos) = self.free_items.iter().position(|i| i.id == id)
        {
            let item = self.free_items.remove(pos);
            self.free_items.push(item);
        }
    }

    #[allow(dead_code)]
    pub fn send_to_back(&mut self, id: u64) {
        if self.mode == LayoutMode::Freeform
            && let Some(pos) = self.free_items.iter().position(|i| i.id == id)
        {
            let item = self.free_items.remove(pos);
            self.free_items.insert(0, item);
        }
    }

    pub fn reorder_free_item(&mut self, src_id: u64, target_id: u64) {
        if self.mode != LayoutMode::Freeform || src_id == target_id {
            return;
        }
        let src_pos = self.free_items.iter().position(|i| i.id == src_id);
        let target_pos = self.free_items.iter().position(|i| i.id == target_id);
        if let (Some(src_idx), Some(target_idx)) = (src_pos, target_pos) {
            let item = self.free_items.remove(src_idx);
            self.free_items.insert(target_idx, item);
        }
    }

    pub fn move_free_item_up(&mut self, id: u64) {
        if self.mode == LayoutMode::Freeform
            && let Some(pos) = self.free_items.iter().position(|i| i.id == id)
        {
            if pos + 1 < self.free_items.len() {
                self.free_items.swap(pos, pos + 1);
            }
        }
    }

    pub fn move_free_item_down(&mut self, id: u64) {
        if self.mode == LayoutMode::Freeform
            && let Some(pos) = self.free_items.iter().position(|i| i.id == id)
        {
            if pos > 0 {
                self.free_items.swap(pos, pos - 1);
            }
        }
    }

    pub fn rename_item(&mut self, id: u64, new_name: impl Into<String>) {
        if let Some(item) = self.find_item_mut(id) {
            let name = new_name.into();
            let trimmed = name.trim();
            if trimmed.is_empty() {
                item.custom_name = None;
            } else {
                item.custom_name = Some(trimmed.to_string());
            }
        }
    }

    pub fn align_selected(&mut self, align: &'static str) {
        let (ml, mt, pw, ph) = self.paper.printable_bounds_px();
        let paper = self.paper.clone();
        if self.mode != LayoutMode::Freeform {
            return;
        }
        if align == "distribute" {
            let count = self.free_items.len();
            if count < 3 {
                return;
            }
            self.free_items
                .sort_by(|a, b| a.x_px.partial_cmp(&b.x_px).unwrap());
            let first_x = self.free_items.first().unwrap().x_px;
            let last_item = self.free_items.last().unwrap();
            let total_span = (last_item.x_px + last_item.width_px) - first_x;
            let total_w: f32 = self.free_items.iter().map(|i| i.width_px).sum();
            let free_space = (total_span - total_w).max(0.0);
            let gap = free_space / (count - 1) as f32;
            let mut cur_x = first_x;
            for item in &mut self.free_items {
                item.x_px = cur_x;
                cur_x += item.width_px + gap;
                Self::clamp_item_to_safe_margin(&paper, item);
            }
            return;
        }
        let Some(id) = self.selected_id else {
            return;
        };
        if let Some(item) = self.free_items.iter_mut().find(|i| i.id == id) {
            match align {
                "left" => item.x_px = ml,
                "center" => item.x_px = ml + (pw - item.width_px) * 0.5,
                "right" => item.x_px = ml + pw - item.width_px,
                "middle" => item.y_px = mt + (ph - item.height_px) * 0.5,
                _ => {}
            }
            Self::clamp_item_to_safe_margin(&paper, item);
        }
    }

    pub fn create_print_snapshot(&self) -> PrintRenderSnapshot {
        PrintRenderSnapshot {
            paper: self.paper.clone(),
            mode: self.mode,
            items: self.active_items().to_vec(),
        }
    }

    #[allow(dead_code)]
    pub fn render_print_sheet(&self) -> image::RgbaImage {
        self.create_print_snapshot().render_sheet()
    }

    pub fn apply_item_transform(&mut self, id: u64, mut new_tf: ImageTransform) {
        let mode = self.mode;
        let paper = self.paper.clone();
        if let Some(item) = self.find_item_mut(id) {
            if mode == LayoutMode::Freeform {
                new_tf.pan_x = 0.0;
                new_tf.pan_y = 0.0;
                if let Some(asset) = &item.image {
                    let (old_w, old_h) = asset.display_dimensions(&item.transform);
                    let (new_w, new_h) = asset.display_dimensions(&new_tf);
                    let scale_x = item.width_px / old_w.max(1.0);
                    let scale_y = item.height_px / old_h.max(1.0);
                    let scale = ((scale_x + scale_y) * 0.5).max(0.01);
                    item.width_px = new_w * scale;
                    item.height_px = new_h * scale;
                    item.lock_aspect = true;
                    Self::clamp_item_to_safe_margin(&paper, item);
                }
            }
            item.update_transform(new_tf);
        }
    }

    /// Applies an image transform mutation to the selected item if it has an image.
    pub fn transform_item_with(&mut self, id: u64, f: impl FnOnce(&mut ImageTransform)) -> bool {
        let Some(item) = self.find_item(id) else {
            return false;
        };
        if item.image.is_none() {
            return false;
        }
        let mut tf = item.transform.clone();
        f(&mut tf);
        self.apply_item_transform(id, tf);
        true
    }

    /// Resets all image transforms to defaults if the selected item has a modified image.
    pub fn reset_item_transform(&mut self, id: u64) -> bool {
        let Some(item) = self.find_item(id) else {
            return false;
        };
        if item.image.is_none() || !item.transform.is_modified() {
            return false;
        }
        self.apply_item_transform(id, ImageTransform::default());
        true
    }

    pub fn open_editor_for_item(&mut self, id: u64) {
        self.context_menu = None;
        let mode = self.mode;
        if let Some(item) = self.find_item_mut(id) {
            if let Some(mut asset) = item.image.clone() {
                asset.rebuild_rendered(&item.transform);
                let session = EditorSession {
                    item_id: item.id,
                    working_transform: item.transform.clone(),
                    preview_asset: asset,
                    frame_width_px: item.width_px,
                    frame_height_px: item.height_px,
                    shape_kind: item.shape_kind,
                    fit_mode: item.fit_mode,
                    layout_mode: mode,
                    crop_mode: false,
                    drag_start: None,
                    undo_stack: Vec::new(),
                    crop_page_active: false,
                    crop_draft_rect: item.transform.crop_rect,
                    crop_drag: None,
                    crop_base_rendered: None,
                };
                self.selected_id = Some(id);
                self.route = ActiveRoute::ImageEditor(session);
            }
        }
    }

    pub fn open_editor_crop_for_item(&mut self, id: u64) {
        self.open_editor_for_item(id);
        if let ActiveRoute::ImageEditor(ref mut session) = self.route {
            session.start_crop();
        }
    }

    pub fn commit_editor_session(&mut self) {
        let ActiveRoute::ImageEditor(session) = self.route.clone() else {
            return;
        };
        self.apply_item_transform(session.item_id, session.working_transform);
        self.status_message = self.language.tr("app.status_editor_saved");
        self.route = ActiveRoute::Workspace;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::paper::{Orientation, PaperPreset};

    #[test]
    fn test_studio_state_id_photo_and_editor_workflow() {
        let mut st = StudioState::new();
        assert_eq!(st.mode, LayoutMode::IdPhoto);
        assert_eq!(st.id_photo_items.len(), 1);

        let id = st.id_photo_items[0].id;
        let raw = image::DynamicImage::new_rgb8(100, 100);
        st.id_photo_items[0].assign_image(ImageAsset::from_dynamic(
            "test.png",
            None,
            raw,
            &ImageTransform::default(),
        ));
        st.open_editor_for_item(id);

        match st.route {
            ActiveRoute::ImageEditor(ref sess) => {
                assert_eq!(sess.item_id, id);
                assert_eq!(sess.working_transform.quarter_turns_cw, 0);
            }
            _ => panic!("Expected ImageEditor route"),
        }

        if let ActiveRoute::ImageEditor(ref mut sess) = st.route {
            sess.push_undo();
            sess.working_transform.quarter_turns_cw = 1;
            sess.working_transform.zoom_scale = 1.25;
        }

        st.commit_editor_session();

        let item = st.find_item(id).expect("Item exists");
        assert_eq!(item.transform.quarter_turns_cw, 1);
        assert_eq!(item.transform.zoom_scale, 1.25);
    }

    #[test]
    fn test_editor_session_undo_and_crop() {
        let mut st = StudioState::new();
        let id = st.id_photo_items[0].id;
        let raw = image::DynamicImage::new_rgb8(100, 100);
        st.id_photo_items[0].assign_image(ImageAsset::from_dynamic(
            "test.png",
            None,
            raw,
            &ImageTransform::default(),
        ));
        st.open_editor_for_item(id);

        let ActiveRoute::ImageEditor(ref mut sess) = st.route else {
            panic!("Expected ImageEditor route");
        };

        assert!(!sess.can_undo());
        assert!(!sess.crop_page_active);

        // 1. Transform: Rotate & Flip with Undo
        sess.push_undo();
        sess.working_transform.quarter_turns_cw = 1;
        assert!(sess.can_undo());

        sess.push_undo();
        sess.working_transform.flip_h = true;

        // Undo flip_h
        assert!(sess.undo());
        assert!(!sess.working_transform.flip_h);
        assert_eq!(sess.working_transform.quarter_turns_cw, 1);

        // Undo rotate
        assert!(sess.undo());
        assert_eq!(sess.working_transform.quarter_turns_cw, 0);
        assert!(!sess.can_undo());

        // 2. Crop workflow
        sess.start_crop();
        assert!(sess.crop_page_active);
        sess.crop_draft_rect = [0.1, 0.1, 0.8, 0.8];
        sess.apply_crop();
        assert!(!sess.crop_page_active);
        assert_eq!(sess.working_transform.crop_rect, [0.1, 0.1, 0.8, 0.8]);
        assert!(sess.can_undo());

        // 3. Rotasi setelah crop: crop_rect harus ikut berputar konsisten
        sess.working_transform.rotate_cw_90();
        assert_eq!(sess.working_transform.quarter_turns_cw, 1);
        assert!((sess.working_transform.crop_rect[0] - 0.1).abs() < 1e-4);
        assert!((sess.working_transform.crop_rect[1] - 0.1).abs() < 1e-4);

        // Rotate 3 more times (360° total) should return to initial orientation
        sess.working_transform.rotate_cw_90();
        sess.working_transform.rotate_cw_90();
        sess.working_transform.rotate_cw_90();
        assert_eq!(sess.working_transform.quarter_turns_cw, 0);
        assert!((sess.working_transform.crop_rect[0] - 0.1).abs() < 1e-4);
        assert!((sess.working_transform.crop_rect[1] - 0.1).abs() < 1e-4);

        // Undo crop
        assert!(sess.undo());
        assert_eq!(sess.working_transform.crop_rect, [0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn test_mode_switch_clears_images_and_detects_content() {
        let mut st = StudioState::new();
        assert!(!st.current_mode_has_images());

        let raw = image::DynamicImage::new_rgb8(10, 10);
        let asset = ImageAsset::from_dynamic("a.png", None, raw, &ImageTransform::default());
        let id = st.id_photo_items[0].id;
        st.assign_loaded_asset_to_item(id, asset);
        assert!(st.current_mode_has_images());

        st.reset_images_and_switch_mode(LayoutMode::Freeform);
        assert_eq!(st.mode, LayoutMode::Freeform);
        assert!(!st.current_mode_has_images());
    }

    #[test]
    fn test_delete_item_releases_memory_and_cleans_selection() {
        let mut st = StudioState::new();
        let id = st.id_photo_items[0].id;
        st.selected_id = Some(id);

        st.delete_item(id);
        assert_eq!(st.id_photo_items.len(), 0);
        assert_eq!(st.selected_id, None);
    }

    #[test]
    fn test_freeform_safe_margin_clamp() {
        let paper = PaperConfig::default();
        let (ml, mt, pw, ph) = paper.printable_bounds_px();

        let mut item = CanvasItem {
            id: 10,
            x_px: -50.0,
            y_px: -50.0,
            width_px: pw + 200.0,
            height_px: ph + 200.0,
            rotation_deg: 0.0,
            lock_aspect: false,
            shape_kind: ShapeKind::Rectangle,
            fit_mode: FitMode::Cover,
            cut_line: false,
            cut_line_width: 1.0,
            preset: None,
            image: None,
            text: None,
            shape: None,
            transform: ImageTransform::default(),
            corner_radius: 0.0,
            bg_color: None,
            custom_name: None,
        };

        StudioState::clamp_item_to_safe_margin(&paper, &mut item);

        assert!(item.x_px >= ml);
        assert!(item.y_px >= mt);
        assert!(item.width_px <= pw);
        assert!(item.height_px <= ph);
    }

    #[test]
    fn test_freeform_layer_reordering() {
        let mut st = StudioState::new();
        st.mode = LayoutMode::Freeform;

        let raw = image::DynamicImage::new_rgb8(10, 10);
        let assets = vec![
            ImageAsset::from_dynamic("1.png", None, raw.clone(), &ImageTransform::default()),
            ImageAsset::from_dynamic("2.png", None, raw.clone(), &ImageTransform::default()),
            ImageAsset::from_dynamic("3.png", None, raw, &ImageTransform::default()),
        ];
        st.insert_loaded_freeform_assets(assets);

        assert_eq!(st.free_items.len(), 3);
        let id0 = st.free_items[0].id;
        let id1 = st.free_items[1].id;
        let id2 = st.free_items[2].id;

        st.move_free_item_up(id0);
        assert_eq!(st.free_items[1].id, id0);
        assert_eq!(st.free_items[0].id, id1);

        st.move_free_item_down(id0);
        assert_eq!(st.free_items[0].id, id0);

        st.reorder_free_item(id0, id2);
        assert_eq!(st.free_items[2].id, id0);
    }

    #[test]
    fn test_freeform_editor_resets_pan_and_exports_unclipped() {
        let mut st = StudioState::new();
        st.mode = LayoutMode::Freeform;

        let raw = image::DynamicImage::new_rgb8(120, 80);
        let assets = vec![ImageAsset::from_dynamic(
            "portrait.png",
            None,
            raw,
            &ImageTransform::default(),
        )];
        st.insert_loaded_freeform_assets(assets);

        let id = st.free_items[0].id;
        st.open_editor_for_item(id);

        if let ActiveRoute::ImageEditor(ref mut sess) = st.route {
            sess.working_transform.pan_x = 0.5;
            sess.working_transform.pan_y = -0.5;
        }
        st.commit_editor_session();

        assert_eq!(st.free_items[0].transform.pan_x, 0.0);
        assert_eq!(st.free_items[0].transform.pan_y, 0.0);

        let prev_w = st.free_items[0].width_px;
        let prev_h = st.free_items[0].height_px;
        st.open_editor_for_item(id);
        if let ActiveRoute::ImageEditor(ref mut sess) = st.route {
            sess.working_transform.quarter_turns_cw = 1;
        }
        st.commit_editor_session();
        assert!((st.free_items[0].width_px - prev_h).abs() < 1e-2);
        assert!((st.free_items[0].height_px - prev_w).abs() < 1e-2);

        let sheet = st.render_print_sheet();
        assert!(sheet.width() > 100);
    }

    #[test]
    fn test_image_transform_is_modified() {
        let mut tf = ImageTransform::default();
        assert!(!tf.is_modified());

        tf.quarter_turns_cw = 1;
        assert!(tf.is_modified());

        tf = ImageTransform::default();
        tf.flip_h = true;
        assert!(tf.is_modified());

        tf = ImageTransform::default();
        tf.zoom_scale = 1.2;
        assert!(tf.is_modified());

        tf = ImageTransform::default();
        assert!(!tf.is_modified());
    }

    #[test]
    fn test_id_photo_item_individual_resize_and_cutline() {
        let mut st = StudioState::new();
        assert_eq!(st.id_photo_items.len(), 1);
        let id = st.id_photo_items[0].id;

        assert!(st.id_photo_items[0].cut_line);
        assert_eq!(st.id_photo_items[0].cut_line_width, 1.0);

        if let Some(it) = st.find_item_mut(id) {
            it.width_px = 300.0;
            it.height_px = 400.0;
            it.cut_line = false;
            it.cut_line_width = 3.0;
        }

        let item = st.find_item(id).unwrap();
        assert_eq!(item.width_px, 300.0);
        assert_eq!(item.height_px, 400.0);
        assert!(!item.cut_line);
        assert_eq!(item.cut_line_width, 3.0);
    }

    #[test]
    fn test_id_photo_cutline_per_shape_independence() {
        let mut st = StudioState::new();
        assert_eq!(st.id_photo_items.len(), 1);
        let first_id = st.id_photo_items[0].id;
        assert!(st.id_photo_items[0].cut_line);

        // Template cut_line turned off for new shapes
        st.id_photo_cut_line = false;
        // Shape lama tetap mempertahankan cut_line miliknya sendiri
        assert!(st.find_item(first_id).unwrap().cut_line);

        // Newly added shape uses the updated template setting (false)
        st.add_id_photo_frames();
        assert_eq!(st.id_photo_items.len(), 2);
        let second_id = st.id_photo_items[1].id;
        assert!(!st.find_item(second_id).unwrap().cut_line);
        assert!(st.find_item(first_id).unwrap().cut_line);
    }

    #[test]
    fn test_id_photo_enable_moving_and_reflow_on_disable() {
        let mut st = StudioState::new();
        st.id_photo_quantity = 2;
        st.add_id_photo_frames();
        assert_eq!(st.id_photo_items.len(), 3);

        st.id_photo_free_move = true;
        let id1 = st.id_photo_items[1].id;
        if let Some(it) = st.find_item_mut(id1) {
            it.x_px = 500.0;
            it.y_px = 500.0;
        }
        assert_eq!(st.find_item(id1).unwrap().x_px, 500.0);

        st.id_photo_free_move = false;
        st.reflow_id_photo_grid();
        assert!(st.find_item(id1).unwrap().x_px < 500.0);
    }

    #[test]
    fn test_printer_modal_draft_and_apply() {
        let mut st = StudioState::new();
        st.printer_modal = Some(PrinterModalDraft {
            paper_preset: PaperPreset::FourR,
            orientation: Orientation::Landscape,
            media_type: "GLOSSY".into(),
            print_quality: "High".into(),
            is_grayscale: true,
            copies: 3,
            media_choices: Vec::new(),
        });

        st.save_and_apply_printer_modal();

        assert_eq!(st.paper.preset, PaperPreset::FourR);
        assert_eq!(st.paper.orientation, Orientation::Landscape);
        assert_eq!(st.linux_printer_config.media_type, "GLOSSY");
        assert_eq!(st.linux_printer_config.print_quality, "High");
        assert_eq!(st.linux_printer_config.copies, 3);
        assert!(st.is_grayscale);
        assert!(st.printer_modal.is_none());
    }

    #[test]
    fn test_cropped_sheet_respects_cutline_padding() {
        let mut st = StudioState::new();
        st.add_id_photo_frames();
        assert_eq!(st.id_photo_items.len(), 2);

        let raw1 = image::DynamicImage::new_rgb8(100, 100);
        let asset1 = ImageAsset::from_dynamic("f1.png", None, raw1, &ImageTransform::default());
        let id1 = st.id_photo_items[0].id;
        st.assign_loaded_asset_to_item(id1, asset1);

        st.id_photo_items[0].cut_line = true;
        st.id_photo_items[0].cut_line_width = 2.0;

        let snapshot = st.create_print_snapshot();
        let cropped = snapshot.render_cropped_sheet();
        assert!(cropped.crop_w_mm > 0.0);
        assert!(cropped.crop_h_mm > 0.0);
        assert!(cropped.sheet.width() > 0);
        assert!(cropped.sheet.height() > 0);
    }

    #[test]
    fn test_freeform_layer_uses_original_filename_and_custom_rename() {
        let mut st = StudioState::new();
        st.mode = LayoutMode::Freeform;

        let raw = image::DynamicImage::new_rgb8(40, 30);
        let assets = vec![ImageAsset::from_dynamic(
            "DSC_0042.JPG",
            None,
            raw,
            &ImageTransform::default(),
        )];
        st.insert_loaded_freeform_assets(assets);

        assert_eq!(st.free_items[0].layer_name(0), "DSC_0042.JPG");

        let id = st.free_items[0].id;
        st.rename_item(id, "Foto Keluarga");
        assert_eq!(st.free_items[0].layer_name(0), "Foto Keluarga");

        st.rename_item(id, "   ");
        assert_eq!(st.free_items[0].layer_name(0), "DSC_0042.JPG");
    }

    #[test]
    fn test_freeform_replace_photo_resets_frame_and_transform_keeps_properties() {
        let mut st = StudioState::new();
        st.mode = LayoutMode::Freeform;

        // 1. Foto awal lebar (200x100) + diberi transform dan sudut lengkung 24px
        let wide = image::DynamicImage::new_rgb8(200, 100);
        let asset1 = ImageAsset::from_dynamic("wide.png", None, wide, &ImageTransform::default());
        st.insert_loaded_freeform_assets(vec![asset1]);
        let id = st.free_items[0].id;

        st.free_items[0].corner_radius = 24.0;
        let mut edited_tf = ImageTransform::default();
        edited_tf.quarter_turns_cw = 1;
        edited_tf.flip_h = true;
        edited_tf.crop_rect = [0.1, 0.1, 0.8, 0.8];
        st.free_items[0].update_transform(edited_tf);
        let old_w = st.free_items[0].width_px;
        let old_h = st.free_items[0].height_px;
        assert!(old_w > old_h);

        // 2. Replace with a narrower photo (100x200)
        let slim = image::DynamicImage::new_rgb8(100, 200);
        let asset2 = ImageAsset::from_dynamic("slim.png", None, slim, &ImageTransform::default());
        st.assign_loaded_asset_to_item(id, asset2);

        let item = st.find_item(id).expect("Item exists");
        // Frame and transform are reset to the new photo's natural dimensions (100x200, no default size)
        assert!((item.width_px - 100.0).abs() < 1e-3);
        assert!((item.height_px - 200.0).abs() < 1e-3);
        assert_eq!(item.transform.quarter_turns_cw, 0);
        assert!(!item.transform.flip_h);
        assert_eq!(item.transform.crop_rect, [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(item.layer_name(0), "slim.png");
        // Properti item seperti sudut lengkung tetap dipertahankan
        assert!((item.corner_radius - 24.0).abs() < 1e-4);

        // 3. Replace with a very wide photo exceeding paper width: original aspect ratio must be preserved
        let pano = image::DynamicImage::new_rgb8(4000, 1000);
        let asset3 = ImageAsset::from_dynamic("pano.png", None, pano, &ImageTransform::default());
        st.assign_loaded_asset_to_item(id, asset3);

        let item_pano = st.find_item(id).expect("Item exists");
        assert!((item_pano.width_px / item_pano.height_px - 4.0).abs() < 1e-3);
        assert!((item_pano.corner_radius - 24.0).abs() < 1e-4);
    }

    #[test]
    fn test_crop_and_rotate_preserves_aspect_and_reopens_rotated_across_modes() {
        let raw = image::DynamicImage::new_rgb8(200, 100);

        // 1. Mode Bebas: crop -> simpan -> rotate via apply_item_transform -> buka edit lagi
        let mut st = StudioState::new();
        st.mode = LayoutMode::Freeform;
        st.insert_loaded_freeform_assets(vec![ImageAsset::from_dynamic(
            "foto.png",
            None,
            raw.clone(),
            &ImageTransform::default(),
        )]);
        let id = st.free_items[0].id;

        st.open_editor_for_item(id);
        if let ActiveRoute::ImageEditor(ref mut sess) = st.route {
            sess.crop_draft_rect = [0.0, 0.0, 0.5, 1.0]; // 100x100
            sess.apply_crop();
        }
        st.commit_editor_session();
        let cropped = st.find_item(id).unwrap();
        assert!((cropped.width_px - 100.0).abs() < 1e-2);
        assert!((cropped.height_px - 100.0).abs() < 1e-2);

        // Narrow crop (40x100) then rotate 90° CW -> frame must become 100x40 (no stretching)
        st.open_editor_for_item(id);
        if let ActiveRoute::ImageEditor(ref mut sess) = st.route {
            sess.crop_draft_rect = [0.0, 0.0, 0.2, 1.0]; // 40x100 on 200x100
            sess.apply_crop();
        }
        st.commit_editor_session();
        let slim = st.find_item(id).unwrap();
        assert!((slim.width_px / slim.height_px - 0.4).abs() < 1e-2);

        let mut tf = slim.transform.clone();
        tf.rotate_cw_90();
        st.apply_item_transform(id, tf);
        let rotated = st.find_item(id).unwrap();
        assert!((rotated.width_px / rotated.height_px - 2.5).abs() < 1e-2);
        assert_eq!(rotated.transform.quarter_turns_cw, 1);

        st.open_editor_for_item(id);
        let ActiveRoute::ImageEditor(ref sess) = st.route else {
            panic!("Expected ImageEditor");
        };
        assert_eq!(sess.working_transform.quarter_turns_cw, 1);
        let (dw, dh) = sess.preview_asset.display_dimensions(&sess.working_transform);
        assert!((dw / dh - 2.5).abs() < 1e-2);

        // 2. IdPhoto & FullImage modes: rotating then entering edit mode must preserve rotation
        for mode in [LayoutMode::IdPhoto, LayoutMode::FullImage] {
            let mut m = StudioState::new();
            m.reset_images_and_switch_mode(mode);
            let tid = m.active_items()[0].id;
            m.assign_loaded_asset_to_item(
                tid,
                ImageAsset::from_dynamic("m.png", None, raw.clone(), &ImageTransform::default()),
            );
            let mut mtf = m.find_item(tid).unwrap().transform.clone();
            mtf.rotate_cw_90();
            m.apply_item_transform(tid, mtf);
            m.open_editor_for_item(tid);
            let ActiveRoute::ImageEditor(ref msess) = m.route else {
                panic!("Expected ImageEditor");
            };
            assert_eq!(msess.working_transform.quarter_turns_cw, 1);
            let (mw, mh) = msess.preview_asset.display_dimensions(&msess.working_transform);
            assert!((mw - 100.0).abs() < 1e-2);
            assert!((mh - 200.0).abs() < 1e-2);
        }
    }

    #[test]
    fn test_workspace_transform_shortcuts_and_mode_guards() {
        let raw = image::DynamicImage::new_rgb8(200, 100);

        // 1. IdPhoto: empty frame stays unchanged; filled frame rotates/flips/resets without changing frame size
        let mut st = StudioState::new();
        assert_eq!(st.mode, LayoutMode::IdPhoto);
        let pas_id = st.id_photo_items[0].id;
        let orig_fw = st.id_photo_items[0].width_px;
        let orig_fh = st.id_photo_items[0].height_px;
        assert!(!st.transform_item_with(pas_id, ImageTransform::rotate_cw_90));
        assert_eq!(st.id_photo_items[0].transform.quarter_turns_cw, 0);

        st.assign_loaded_asset_to_item(
            pas_id,
            ImageAsset::from_dynamic("pas.png", None, raw.clone(), &ImageTransform::default()),
        );
        assert!(st.transform_item_with(pas_id, ImageTransform::rotate_cw_90));
        assert!(st.transform_item_with(pas_id, ImageTransform::toggle_flip_h));
        let pas_item = st.find_item(pas_id).unwrap();
        assert_eq!(pas_item.transform.quarter_turns_cw, 1);
        assert!(pas_item.transform.flip_h);
        assert_eq!(pas_item.width_px, orig_fw);
        assert_eq!(pas_item.height_px, orig_fh);

        assert!(st.reset_item_transform(pas_id));
        assert!(!st.find_item(pas_id).unwrap().transform.is_modified());
        assert!(!st.reset_item_transform(pas_id));

        // 2. FullImage: transform works when photo exists and delete clears transform
        st.reset_images_and_switch_mode(LayoutMode::FullImage);
        let full_id = st.full_item.id;
        assert!(!st.transform_item_with(full_id, ImageTransform::rotate_cw_90));
        st.assign_loaded_asset_to_item(
            full_id,
            ImageAsset::from_dynamic("full.png", None, raw.clone(), &ImageTransform::default()),
        );
        assert!(st.transform_item_with(full_id, ImageTransform::rotate_cw_90));
        assert_eq!(st.full_item.transform.quarter_turns_cw, 1);
        st.delete_item(full_id);
        assert!(st.full_item.image.is_none());
        assert!(!st.full_item.transform.is_modified());

        // 3. Freeform: rotate CW updates frame aspect and reset restores initial size
        st.reset_images_and_switch_mode(LayoutMode::Freeform);
        st.insert_loaded_freeform_assets(vec![ImageAsset::from_dynamic(
            "free.png",
            None,
            raw,
            &ImageTransform::default(),
        )]);
        let free_id = st.free_items[0].id;
        assert!(st.transform_item_with(free_id, ImageTransform::rotate_cw_90));
        let rotated = st.find_item(free_id).unwrap();
        assert!((rotated.width_px - 100.0).abs() < 1e-2);
        assert!((rotated.height_px - 200.0).abs() < 1e-2);
        assert!(st.reset_item_transform(free_id));
        let reset = st.find_item(free_id).unwrap();
        assert!((reset.width_px - 200.0).abs() < 1e-2);
        assert!((reset.height_px - 100.0).abs() < 1e-2);

        // 4. Directly enter crop edit mode
        st.open_editor_crop_for_item(free_id);
        let ActiveRoute::ImageEditor(ref sess) = st.route else {
            panic!("Expected ImageEditor");
        };
        assert!(sess.crop_page_active);
    }

    #[test]
    fn test_freeform_text_box_workflow_and_snapshot() {
        let mut st = StudioState::new();
        st.reset_images_and_switch_mode(LayoutMode::Freeform);
        assert!(!st.current_mode_has_images());

        st.add_freeform_text_box("");
        assert_eq!(st.free_items.len(), 1);
        assert!(st.current_mode_has_images());

        let id = st.free_items[0].id;
        assert_eq!(st.selected_id, Some(id));
        assert!(st.free_items[0].is_text());
        assert!(!st.free_items[0].lock_aspect);
        let initial_w = st.free_items[0].width_px;

        // Text item ignores image transform shortcuts cleanly
        assert!(!st.transform_item_with(id, ImageTransform::rotate_cw_90));
        assert!(!st.reset_item_transform(id));

        // Typing longer text while manual_size=false auto-widens frame
        if let Some(txt) = &mut st.find_item_mut(id).unwrap().text {
            txt.content = "Judul Acara Sangat Panjang".to_string();
        }
        st.sync_text_item_size(id);
        let widened_w = st.find_item(id).unwrap().width_px;
        assert!(widened_w > initial_w);

        // Once user manually resizes frame, width stays fixed and text wraps vertically
        let manual_w = 240.0;
        {
            let it = st.find_item_mut(id).unwrap();
            it.width_px = manual_w;
            it.height_px = 60.0;
            if let Some(txt) = &mut it.text {
                txt.manual_size = true;
                txt.font_size_pt = 36.0;
                txt.content = "Judul Acara\nBaris Kedua Panjang Sekali".to_string();
            }
        }
        st.sync_text_item_size(id);
        let after_manual = st.find_item(id).unwrap();
        assert!((after_manual.width_px - manual_w).abs() < 1e-2);
        assert!(after_manual.height_px > 60.0);
        assert_eq!(
            after_manual.layer_name_for(0, Language::Id).as_ref(),
            "Judul Acara"
        );

        let snap = st.create_print_snapshot();
        let cropped = snap.render_cropped_sheet();
        assert!(cropped.sheet.width() > 1 && cropped.sheet.height() > 1);
    }

    #[test]
    fn test_freeform_shape_workflow_and_snapshot() {
        let mut st = StudioState::new();
        st.reset_images_and_switch_mode(LayoutMode::Freeform);
        st.add_freeform_shape(FreeformShapeKind::Star);
        assert_eq!(st.free_items.len(), 1);
        assert!(st.current_mode_has_images());
        assert!(st.free_items[0].is_shape());
        let snap = st.create_print_snapshot();
        let cropped = snap.render_cropped_sheet();
        assert!(cropped.sheet.width() > 1 && cropped.sheet.height() > 1);
    }
}
