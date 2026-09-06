use super::{
    ACCENT, MUTED, PANEL, Studio, Tool,
    files::Action,
    icons,
    icons::{TEXT, TEXT_MUTED},
};
use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use vibeshop::curves::{Channel, Curve};
use vibeshop::document::{self, Blend};

impl Studio {
    pub(super) fn top_bar(&mut self, ctx: &egui::Context) {
        let blocked = self.pending.is_some() || self.error.is_some() || self.new_size.is_some();
        egui::TopBottomPanel::top("toolbar")
            .exact_height(52.0)
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(16, 9)),
            )
            .show(ctx, |ui| {
                ui.add_enabled_ui(!blocked, |ui| {
                    ui.horizontal_centered(|ui| {
                        ui.label(RichText::new("vibeshop").size(21.0).strong().color(ACCENT));
                        ui.add_space(16.0);
                        ui.menu_button("File", |ui| {
                            if ui
                                .add_enabled(
                                    self.job.is_none(),
                                    egui::Button::new(
                                        RichText::new(format!("{} New canvas…", icons::NEW_CANVAS))
                                            .color(TEXT),
                                    ),
                                )
                                .clicked()
                            {
                                self.new_size = Some([1920, 1080]);
                                ui.close();
                            }
                            if ui
                                .add_enabled(
                                    self.job.is_none(),
                                    egui::Button::new(format!("{} Open…", icons::OPEN))
                                        .shortcut_text("Ctrl/Cmd+O"),
                                )
                                .clicked()
                            {
                                self.request(Action::Open(None, false), ctx);
                                ui.close();
                            }
                            ui.separator();
                            if ui
                                .add_enabled(
                                    self.job.is_none(),
                                    egui::Button::new(format!("{} Save project", icons::SAVE))
                                        .shortcut_text("Ctrl/Cmd+S"),
                                )
                                .clicked()
                            {
                                self.save_project(false, ctx);
                                ui.close();
                            }
                            if ui
                                .add_enabled(
                                    self.job.is_none(),
                                    egui::Button::new(format!("{} Save project as…", icons::SAVE))
                                        .shortcut_text("Ctrl/Cmd+Shift+S"),
                                )
                                .clicked()
                            {
                                self.save_project(true, ctx);
                                ui.close();
                            }
                            if ui
                                .add_enabled(
                                    self.job.is_none(),
                                    egui::Button::new(format!("{} Export PNG…", icons::EXPORT))
                                        .shortcut_text("Ctrl/Cmd+Shift+E"),
                                )
                                .clicked()
                            {
                                self.export(ctx);
                                ui.close();
                            }
                        });
                        ui.menu_button("View", |ui| {
                            if ui.button("Fit canvas    F").clicked() {
                                self.fit = true;
                                ui.close();
                            }
                            if ui.button("Actual pixels    100%").clicked() {
                                self.actual_pixels();
                                ui.close();
                            }
                        });
                        ui.separator();
                        if ui
                            .add_enabled(
                                self.job.is_none(),
                                egui::Button::new(RichText::new(format!("{} Open", icons::OPEN)))
                                    .min_size(egui::vec2(84.0, 28.0)),
                            )
                            .on_hover_text("Open an image or editable project · Ctrl/Cmd+O")
                            .clicked()
                        {
                            self.request(Action::Open(None, false), ctx);
                        }
                        if ui
                            .add_enabled(
                                self.job.is_none(),
                                egui::Button::new(RichText::new(format!("{} Save", icons::SAVE)))
                                    .min_size(egui::vec2(84.0, 28.0)),
                            )
                            .on_hover_text("Save editable layers as a .vibe project · Ctrl/Cmd+S")
                            .clicked()
                        {
                            self.save_project(false, ctx);
                        }
                        ui.add_space(8.0);
                        if ui
                            .add_enabled(
                                self.editor.can_undo(),
                                egui::Button::new(RichText::new(format!("{} Undo", icons::UNDO)))
                                    .min_size(egui::vec2(84.0, 28.0)),
                            )
                            .on_hover_text("Ctrl/Cmd+Z")
                            .clicked()
                        {
                            self.editor.undo();
                        }
                        if ui
                            .add_enabled(
                                self.editor.can_redo(),
                                egui::Button::new(RichText::new(format!("{} Redo", icons::REDO)))
                                    .min_size(egui::vec2(84.0, 28.0)),
                            )
                            .on_hover_text("Ctrl/Cmd+Shift+Z")
                            .clicked()
                        {
                            self.editor.redo();
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add_enabled(
                                    self.job.is_none(),
                                    egui::Button::new(
                                        RichText::new(format!("{} Export PNG", icons::EXPORT))
                                            .strong()
                                            .color(Color32::from_rgb(24, 31, 19)),
                                    )
                                    .fill(ACCENT)
                                    .min_size(egui::vec2(140.0, 32.0)),
                                )
                                .on_hover_text("Export a flattened copy · Ctrl/Cmd+Shift+E")
                                .clicked()
                            {
                                self.export(ctx);
                            }
                        });
                    });
                });
            });
        egui::TopBottomPanel::top("document")
            .exact_height(36.0)
            .frame(egui::Frame::new().fill(Color32::from_rgb(23, 25, 29)).inner_margin(egui::Margin::symmetric(16, 7)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let state = if self.editor.dirty { "Unsaved changes" } else if self.project_path.is_some() { "Saved" } else { "Untitled project" };
                    ui.label(RichText::new(state).size(11.0).color(if self.editor.dirty { ACCENT } else { MUTED }));
                    ui.separator();
                    let title = self.project_path.as_ref().and_then(|path| path.file_name()).map(|name| name.to_string_lossy().into_owned())
                        .or_else(|| self.editor.document.layers.first().map(|layer| layer.name.clone())).unwrap_or_else(|| "New canvas".into());
                    let title_width = (ui.available_width() - 210.0).max(80.0);
                    ui.add_sized([title_width, 20.0], egui::Label::new(RichText::new(&title).size(12.0)).truncate()).on_hover_text(&title);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(format!("{} × {} px", self.editor.document.width, self.editor.document.height)).size(11.0).color(MUTED));
                        ui.label(RichText::new("sRGB · 8-bit source").size(10.0).color(MUTED)).on_hover_text("Linear-light 16F composition. Embedded ICC profiles and 16-bit source preservation are not supported yet.");
                    });
                });
            });
    }

    pub(super) fn inspector(&mut self, ctx: &egui::Context) {
        let blocked = self.pending.is_some() || self.error.is_some() || self.new_size.is_some();
        egui::SidePanel::right("inspector")
            .default_width(304.0)
            .min_width(280.0)
            .max_width(380.0)
            .resizable(true)
            .frame(egui::Frame::new().fill(PANEL).inner_margin(14))
            .show(ctx, |ui| {
                ui.add_enabled_ui(!blocked, |ui| {
                    let layer_height = (ui.available_height() * 0.38).clamp(208.0, 280.0);
                    egui::TopBottomPanel::bottom("layer-section")
                        .exact_height(layer_height)
                        .resizable(false)
                        .frame(egui::Frame::new().fill(PANEL))
                        .show_inside(ui, |ui| self.layers(ui, ctx));
                    egui::ScrollArea::vertical()
                        .id_salt("layer-properties")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            self.properties(ui);
                        });
                });
            });
    }

    fn properties(&mut self, ui: &mut egui::Ui) {
        section(ui, "PROPERTIES");
        ui.add_space(8.0);
        let selected = self.editor.selected;
        let Some(original) = self.editor.document.layers.get(selected).cloned() else {
            ui.label(RichText::new("No layer selected").color(MUTED));
            ui.label("Add an image to start editing.");
            return;
        };
        let mut layer = original.clone();
        ui.add(egui::Label::new(RichText::new(&layer.name).strong()).truncate())
            .on_hover_text(&layer.name);
        ui.label(
            RichText::new(format!(
                "Raster layer · {} × {} px",
                layer.source.width, layer.source.height
            ))
            .size(11.0)
            .color(MUTED),
        );
        ui.add_space(12.0);
        let exposure = ui.label(icons::glyph(icons::EXPOSURE, 15.0).color(TEXT_MUTED));
        control(ui, "Exposure", &mut layer.exposure, -5.0..=5.0, " EV").labelled_by(exposure.id);
        let contrast = ui.label(icons::glyph(icons::CONTRAST, 15.0).color(TEXT_MUTED));
        control(ui, "Contrast", &mut layer.contrast, 0.0..=2.0, "×").labelled_by(contrast.id);
        let saturation = ui.label(icons::glyph(icons::SATURATION, 15.0).color(TEXT_MUTED));
        control(ui, "Saturation", &mut layer.saturation, 0.0..=2.0, "×").labelled_by(saturation.id);
        ui.add_space(8.0);
        self.tone_panel(ui);
        ui.add_space(8.0);
        if ui.small_button("Reset color adjustments").clicked() {
            layer.reset_adjustments();
        }
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);
        let opacity = ui.label(icons::glyph(icons::OPACITY, 15.0).color(TEXT_MUTED));
        ui.horizontal(|ui| {
            let label = ui.label("Blend");
            egui::ComboBox::from_id_salt("blend")
                .selected_text(layer.blend.name())
                .width((ui.available_width() - 6.0).max(100.0))
                .show_ui(ui, |ui| {
                    for blend in Blend::ALL {
                        ui.selectable_value(&mut layer.blend, blend, blend.name());
                    }
                })
                .response
                .labelled_by(label.id);
        });
        ui.add_space(6.0);
        control(ui, "Opacity", &mut layer.opacity, 0.0..=1.0, "").labelled_by(opacity.id);
        ui.horizontal(|ui| {
            let x = ui.label("X");
            ui.add(
                egui::DragValue::new(&mut layer.offset[0])
                    .range(-8192..=8192)
                    .suffix(" px"),
            )
            .labelled_by(x.id);
            let y = ui.label("Y");
            ui.add(
                egui::DragValue::new(&mut layer.offset[1])
                    .range(-8192..=8192)
                    .suffix(" px"),
            )
            .labelled_by(y.id);
        });
        if layer != original {
            self.editor.begin_edit();
            self.editor.document.layers[selected] = layer;
            self.editor.changed();
        }
        ui.add_space(12.0);
    }

    fn layers(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.separator();
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            section(
                ui,
                &format!("LAYERS  ·  {}", self.editor.document.layers.len()),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        self.job.is_none()
                            && self.editor.document.layers.len() < document::MAX_LAYERS,
                        egui::Button::new("+ Image"),
                    )
                    .on_hover_text("Add an image layer · Ctrl/Cmd+Shift+O")
                    .clicked()
                {
                    self.request(Action::Open(None, true), ctx);
                }
            });
        });
        ui.add_space(8.0);
        egui::TopBottomPanel::bottom("layer-actions")
            .exact_height(76.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(0, 8)),
            )
            .show_inside(ui, |ui| {
                let count = self.editor.document.layers.len();
                let selected = self.editor.selected;
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            count > 0 && count < document::MAX_LAYERS,
                            egui::Button::new(format!("{} Duplicate", icons::DUPLICATE)),
                        )
                        .on_hover_text("Duplicate the selected layer")
                        .clicked()
                    {
                        let mut layer = self.editor.document.layers[selected].clone();
                        layer.name = format!("{} copy", layer.name);
                        if let Err(error) = self.editor.add_layer(layer) {
                            self.error = Some(error.to_string());
                        }
                    }
                    if ui
                        .add_enabled(
                            count > 0,
                            egui::Button::new(format!("{} Remove", icons::REMOVE)),
                        )
                        .on_hover_text("Remove selected layer · Undo restores it")
                        .clicked()
                    {
                        self.editor.edit(|document, selected| {
                            document.layers.remove(*selected);
                        });
                    }
                });
                let count = self.editor.document.layers.len();
                let selected = self.editor.selected;
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            count > 0 && selected + 1 < count,
                            egui::Button::new(format!("{} Raise layer", icons::RAISE)),
                        )
                        .on_hover_text("Move the selected layer up in the stack")
                        .clicked()
                    {
                        self.editor.edit(|document, selected| {
                            document.layers.swap(*selected, *selected + 1);
                            *selected += 1;
                        });
                    }
                    if ui
                        .add_enabled(
                            count > 0 && selected > 0,
                            egui::Button::new(format!("{} Lower layer", icons::LOWER)),
                        )
                        .on_hover_text("Move the selected layer down in the stack")
                        .clicked()
                    {
                        self.editor.edit(|document, selected| {
                            document.layers.swap(*selected, *selected - 1);
                            *selected -= 1;
                        });
                    }
                });
            });
        egui::ScrollArea::vertical()
            .id_salt("layer-stack")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.editor.document.layers.is_empty() {
                    ui.label(RichText::new("No layers yet").color(MUTED));
                }
                for index in (0..self.editor.document.layers.len()).rev() {
                    let layer = &self.editor.document.layers[index];
                    let mut visible = layer.visible;
                    let name = layer.name.clone();
                    let selected = index == self.editor.selected;
                    egui::Frame::new()
                        .fill(if selected {
                            Color32::from_rgb(48, 57, 43)
                        } else {
                            Color32::from_rgb(36, 39, 43)
                        })
                        .corner_radius(5)
                        .inner_margin(8)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let visibility =
                                    ui.checkbox(&mut visible, "").on_hover_text(if visible {
                                        "Hide layer"
                                    } else {
                                        "Show layer"
                                    });
                                visibility.widget_info(|| {
                                    egui::WidgetInfo::selected(
                                        egui::WidgetType::Checkbox,
                                        true,
                                        visible,
                                        format!(
                                            "{} Visible: {name}",
                                            if visible {
                                                icons::VISIBLE
                                            } else {
                                                icons::HIDDEN
                                            }
                                        ),
                                    )
                                });
                                if visibility.changed() {
                                    self.editor.edit(|document, _| {
                                        document.layers[index].visible = visible
                                    });
                                }
                                let color = if visible {
                                    Color32::from_rgb(234, 235, 237)
                                } else {
                                    MUTED
                                };
                                if ui
                                    .add_sized(
                                        [ui.available_width(), 26.0],
                                        egui::Button::new(
                                            RichText::new(&name).size(12.0).color(color),
                                        )
                                        .selected(selected)
                                        .frame(false)
                                        .truncate(),
                                    )
                                    .on_hover_text(&name)
                                    .clicked()
                                {
                                    self.editor.finish_edit();
                                    self.editor.selected = index;
                                }
                            });
                        });
                    ui.add_space(4.0);
                }
            });
    }

    pub(super) fn tool_bar(&mut self, ctx: &egui::Context) {
        let blocked = self.pending.is_some() || self.error.is_some() || self.new_size.is_some();
        egui::SidePanel::left("tools")
            .exact_width(68.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(PANEL).inner_margin(8))
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.add_enabled_ui(!blocked, |ui| {
                    for (tool, icon, label, hint) in [
                        (
                            Tool::Move,
                            icons::TOOL_MOVE,
                            "Move",
                            "Move selected layer · V",
                        ),
                        (
                            Tool::Hand,
                            icons::TOOL_PAN,
                            "Pan",
                            "Pan canvas · H or hold Space",
                        ),
                    ] {
                        let response = ui
                            .add_sized(
                                [52.0, 36.0],
                                egui::Button::new(
                                    RichText::new(icon.to_string())
                                        .size(21.0)
                                        .color(if self.tool == tool { ACCENT } else { TEXT }),
                                )
                                .selected(self.tool == tool),
                            )
                            .on_hover_text(hint);
                        response.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::WidgetType::Button,
                                true,
                                self.tool == tool,
                                label,
                            )
                        });
                        if response.clicked() {
                            self.editor.finish_edit();
                            self.move_start = None;
                            self.tool = tool;
                        }
                        ui.vertical_centered(|ui| {
                            ui.label(RichText::new(label).size(10.0).color(MUTED));
                        });
                        ui.add_space(6.0);
                    }
                });
            });
    }
    pub(super) fn status_bar(&mut self, ctx: &egui::Context) {
        let blocked = self.pending.is_some() || self.error.is_some() || self.new_size.is_some();
        egui::TopBottomPanel::bottom("status")
            .exact_height(32.0)
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(16, 5)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let width = (ui.available_width() - 266.0).max(80.0);
                    ui.add_sized(
                        [width, 22.0],
                        egui::Label::new(RichText::new(&self.status).size(11.0).color(MUTED))
                            .truncate(),
                    )
                    .on_hover_text(&self.status);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_enabled_ui(!blocked, |ui| {
                            if ui
                                .small_button(format!("{} Fit", icons::FIT))
                                .on_hover_text("Fit canvas · F")
                                .clicked()
                            {
                                self.fit = true;
                            }
                            if ui
                                .small_button(format!("{:.0}%", self.zoom * 100.0))
                                .on_hover_text("Click for actual pixels (100%)")
                                .clicked()
                            {
                                self.actual_pixels();
                            }
                        });
                        ui.label(RichText::new("GPU").size(10.0).color(ACCENT))
                            .on_hover_text(format!(
                                "{}\n{} compositions · {} source uploads",
                                self.adapter, self.gpu.renders, self.gpu.uploads
                            ));
                        ui.label(
                            RichText::new(if self.tool == Tool::Hand {
                                "Pan · H"
                            } else {
                                "Move · V"
                            })
                            .size(11.0)
                            .color(MUTED),
                        );
                    });
                });
            });
    }
    fn actual_pixels(&mut self) {
        self.zoom = 1.0;
        self.pan = Vec2::ZERO;
        self.fit = false;
    }
}
fn section(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(10.0).strong().color(MUTED));
}

/// Master levels, per-channel curves with a draggable editor, the GPU
/// histogram of the final composition, and a before/after compare toggle.
impl Studio {
    fn tone_panel(&mut self, ui: &mut egui::Ui) {
        section(ui, "CURVES");
        ui.add_space(6.0);
        // Channel picker as a selectable row of named tabs.
        ui.horizontal(|ui| {
            for (index, channel) in Channel::ALL.iter().enumerate() {
                let selected = self.curve_channel == index;
                if ui
                    .add(
                        egui::Button::new(RichText::new(channel.name()).size(11.0))
                            .selected(selected)
                            .min_size(egui::vec2(0.0, 22.0)),
                    )
                    .clicked()
                {
                    self.curve_channel = index;
                    self.curve_handle = None;
                }
            }
        });
        ui.add_space(4.0);
        let selected = self.editor.selected;
        let Some(original) = self.editor.document.layers.get(selected).cloned() else {
            return;
        };
        let mut layer = original.clone();
        let curve = &mut layer.curves[self.curve_channel];
        // The editor mutates the layer copy; commit happens below.
        let response = self.curve_editor(ui, curve);
        // Keyboard editing of the active handle: arrows move in 1/255 steps,
        // Shift = 10x, Delete removes the point, Escape drops the handle.
        if let Some(handle) = self.curve_handle {
            if response.has_focus() {
                let step = ui.input(|input| {
                    let modifier = if input.modifiers.shift {
                        10.0 / 255.0
                    } else {
                        1.0 / 255.0
                    };
                    let vertical = input.key_pressed(egui::Key::ArrowUp) as i8
                        - input.key_pressed(egui::Key::ArrowDown) as i8;
                    let horizontal = input.key_pressed(egui::Key::ArrowRight) as i8
                        - input.key_pressed(egui::Key::ArrowLeft) as i8;
                    (vertical, horizontal, modifier)
                });
                let (vertical, horizontal, modifier) = step;
                if vertical != 0 || horizontal != 0 {
                    let x = (handle as f32 / 32.0 + horizontal as f32 / 255.0).clamp(0.0, 1.0);
                    let _ = curve.set(handle, curve.eval(handle as f32 / 32.0)); // keep value
                    let value = curve.get(handle).unwrap_or_default() + vertical as f32 * modifier;
                    if let Some(index) = grid_index(x) {
                        let _ = curve.set(index, value.clamp(0.0, 1.0));
                    }
                    self.editor.changed();
                }
                if ui.input(|input| input.key_pressed(egui::Key::Delete)) {
                    let _ = curve.reset_point(handle);
                    self.curve_handle = None;
                    self.editor.changed();
                }
            }
            if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                self.curve_handle = None;
            }
        }
        ui.add_space(6.0);
        let levels = &mut layer.levels;
        control(ui, "Black point", &mut levels.black, 0.0..=0.49, "");
        control(ui, "Gamma", &mut levels.gamma, 0.2..=5.0, "");
        control(ui, "White point", &mut levels.white, 0.51..=1.0, "");
        ui.add_space(6.0);
        // Histogram of the final composition, computed on the GPU. The area
        // is reserved at full height whether or not data has arrived, so the
        // first readback never shifts the layout (a scrollbar appearing here
        // would resize the canvas mid-session).
        ui.label(
            RichText::new("HISTOGRAM · final pixels")
                .size(10.0)
                .color(MUTED),
        );
        ui.add_space(4.0);
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 110.0), Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 2.0, Color32::from_black_alpha(60));
        if let Some(rows) = &self.histogram_rows {
            let peak = rows[0].iter().copied().max().unwrap_or(1).max(1) as f32;
            let luma = Rect::from_min_max(rect.min, egui::pos2(rect.right(), rect.top() + 56.0));
            let bar = luma.width() / 256.0;
            for (bin, count) in rows[0].iter().enumerate() {
                let h = luma.height() * (*count as f32 / peak).clamp(0.004, 1.0);
                painter.rect_filled(
                    Rect::from_min_size(
                        egui::pos2(luma.left() + bin as f32 * bar, luma.bottom() - h),
                        egui::vec2(bar.max(1.0), h),
                    ),
                    0.0,
                    Color32::from_rgb(228, 229, 234).gamma_multiply(0.85),
                );
            }
            for (row, color) in [
                (1, Color32::from_rgb(224, 67, 67)),
                (2, Color32::from_rgb(96, 200, 96)),
                (3, Color32::from_rgb(88, 122, 236)),
            ] {
                let peak = rows[row].iter().copied().max().unwrap_or(1).max(1) as f32;
                let band = Rect::from_min_max(
                    egui::pos2(rect.left(), luma.bottom() + (row - 1) as f32 * 27.0),
                    egui::pos2(rect.right(), luma.bottom() + (row - 1) as f32 * 27.0 + 26.0),
                );
                let bar = band.width() / 256.0;
                for (bin, count) in rows[row].iter().enumerate() {
                    let h = band.height() * (*count as f32 / peak).clamp(0.004, 1.0);
                    painter.rect_filled(
                        Rect::from_min_size(
                            egui::pos2(band.left() + bin as f32 * bar, band.bottom() - h),
                            egui::vec2(bar.max(1.0), h),
                        ),
                        0.0,
                        color.gamma_multiply(0.85),
                    );
                }
            }
        } else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "measuring…",
                egui::FontId::proportional(11.0),
                MUTED,
            );
        }
        ui.add_space(6.0);
        // Before/after: hold to see unadjusted pixels. Render-time bypass;
        // the document is untouched and no history entry is created.
        let compare = ui
            .add(
                egui::Button::new(
                    RichText::new(if self.compare {
                        "⬉ After"
                    } else {
                        "⬉ Before"
                    })
                    .size(12.0),
                )
                .selected(self.compare),
            )
            .on_hover_text("Hold to compare against the unadjusted image");
        if compare.clicked() {
            self.compare = !self.compare;
            self.gpu.set_compare(self.compare);
        }
        if layer != original {
            self.editor.begin_edit();
            self.editor.document.layers[selected] = layer;
            self.editor.changed();
        }
    }

    /// Draggable curve editor on the 33-point grid. Click/drag to bend the
    /// curve, double-click a point to release it, click the canvas to grab
    /// the nearest point. Handles are part of keyboard focus order.
    fn curve_editor(&mut self, ui: &mut egui::Ui, curve: &mut Curve) -> egui::Response {
        // A square-ish editor: inside a ScrollArea the available height is
        // the visible viewport, so cap by width and keep it fully visible.
        let height = ui.available_width().min(150.0).min(110.0);
        let size = egui::vec2(ui.available_width(), height);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Other,
                true,
                format!(
                    "Tone curve editor, {}",
                    Channel::ALL[self.curve_channel].name()
                ),
            )
        });
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 3.0, Color32::from_rgb(24, 26, 30));
        // Diagonal identity reference.
        painter.line_segment(
            [rect.left_bottom(), rect.right_top()],
            Stroke::new(1.0_f32, Color32::from_gray(70)),
        );
        // The curve itself.
        let points: Vec<Pos2> = (0..=64)
            .map(|step| {
                let x = step as f32 / 64.0;
                egui::pos2(
                    rect.left() + rect.width() * x,
                    rect.bottom() - rect.height() * curve.eval(x),
                )
            })
            .collect();
        painter.add(egui::Shape::line(points, Stroke::new(2.0_f32, ACCENT)));
        // Control points: defined ones are solid, others small dots.
        for index in 0..vibeshop::curves::CURVE_POINTS {
            let x = index as f32 / (vibeshop::curves::CURVE_POINTS - 1) as f32;
            let at = egui::pos2(
                rect.left() + rect.width() * x,
                rect.bottom() - rect.height() * curve.eval(x),
            );
            let defined = curve.get(index).is_some();
            let active = self.curve_handle == Some(index);
            let color = if active {
                Color32::WHITE
            } else if defined {
                ACCENT
            } else {
                Color32::from_gray(90)
            };
            painter.circle_filled(at, if active || defined { 4.0 } else { 2.0 }, color);
        }
        // Pointer interaction: press grabs the nearest point on the curve.
        if response.drag_started() {
            eprintln!(
                "[curve] START interact={:?} dragged={}",
                response.interact_pointer_pos().map(|p| (p.x, p.y)),
                response.dragged()
            );
        }
        if response.drag_started()
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let tx = ((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            let index = grid_index(tx).unwrap_or(0);
            self.curve_handle = Some(index);
        }
        if response.dragged()
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let x = ((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            let y = 1.0 - ((pointer.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
            if let Some(index) = grid_index(x) {
                let _ = curve.set(index, y);
                self.curve_handle = Some(index);
            }
        }
        if response.drag_stopped() {
            self.curve_handle = self.curve_handle.take();
        }
        // Double-click releases a point back to the interpolated path.
        if response.double_clicked()
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let tx = ((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            if let Some(index) = grid_index(tx) {
                let _ = curve.reset_point(index);
                self.curve_handle = None;
            }
        }
        if response.changed() {
            self.editor.changed();
        }
        response
    }
}

/// Nearest grid index for an x position on the curve editor.
fn grid_index(x: f32) -> Option<usize> {
    let index = (x * (vibeshop::curves::CURVE_POINTS - 1) as f32).round();
    usize::try_from(index as isize)
        .ok()
        .filter(|i| *i < vibeshop::curves::CURVE_POINTS)
}
fn control(
    ui: &mut egui::Ui,
    text: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
) -> egui::Response {
    let response = ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.spacing_mut().interact_size.y = 20.0;
        let mut label = egui::Id::NULL;
        ui.horizontal(|ui| {
            label = ui.label(RichText::new(text).size(12.0)).id;
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(
                    egui::DragValue::new(value)
                        .range(range.clone())
                        .speed(0.01)
                        .fixed_decimals(2)
                        .suffix(suffix),
                )
                .labelled_by(label);
            });
        });
        ui.spacing_mut().slider_width = ui.available_width();
        ui.add(egui::Slider::new(value, range).show_value(false))
            .labelled_by(label)
    });
    ui.add_space(6.0);
    response.inner
}
