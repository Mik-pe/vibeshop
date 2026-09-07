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
                                    )
                                    .shortcut_text("Ctrl/Cmd+N"),
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
        self.tone_panel(ui, &mut layer);
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
    fn tone_panel(&mut self, ui: &mut egui::Ui, layer: &mut document::Layer) {
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
                    self.curve_handle = Some(16);
                }
            }
        });
        ui.add_space(4.0);
        let curve = &mut layer.curves[self.curve_channel];
        let response = self.curve_editor(ui, curve);
        if response.has_focus() {
            ui.memory_mut(|memory| {
                memory.set_focus_lock_filter(
                    response.id,
                    egui::EventFilter {
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: true,
                        ..Default::default()
                    },
                )
            });
            if ui.input(|i| {
                [
                    egui::Key::ArrowLeft,
                    egui::Key::ArrowRight,
                    egui::Key::ArrowUp,
                    egui::Key::ArrowDown,
                ]
                .iter()
                .any(|key| i.key_pressed(*key))
            }) {
                // The first key after focus can precede egui installing its filter.
                ui.memory_mut(|memory| memory.move_focus(egui::FocusDirection::None));
            }
            let mut handle = self.curve_handle.unwrap_or(16);
            ui.input_mut(|input| {
                if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft) {
                    handle = handle.saturating_sub(1);
                }
                if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight) {
                    handle = (handle + 1).min(32);
                }
                let step = if input.modifiers.shift {
                    10.0 / 255.0
                } else {
                    1.0 / 255.0
                };
                let modifiers = input.modifiers;
                let up = input.consume_key(modifiers, egui::Key::ArrowUp);
                let down = input.consume_key(modifiers, egui::Key::ArrowDown);
                if up || down {
                    let _ = curve.set(
                        handle,
                        curve.eval(handle as f32 / 32.0)
                            + (i32::from(up) - i32::from(down)) as f32 * step,
                    );
                }
                if input.consume_key(egui::Modifiers::NONE, egui::Key::Delete) {
                    let _ = curve.reset_point(handle);
                }
            });
            self.curve_handle = Some(handle);
        }
        ui.label(
            RichText::new("←/→ select · ↑/↓ output · Delete release")
                .size(10.0)
                .color(MUTED),
        );
        ui.label(
            RichText::new("Drag a point · Esc cancels drag")
                .size(10.0)
                .color(MUTED),
        );
        ui.horizontal(|ui| {
            let mut handle = self.curve_handle.unwrap_or(16);
            let input_label = ui.label("Input step");
            ui.add(egui::DragValue::new(&mut handle).range(0..=32))
                .labelled_by(input_label.id);
            self.curve_handle = Some(handle);
            let mut value = curve.eval(handle as f32 / 32.0);
            let output_label = ui.label("Output");
            if ui
                .add(
                    egui::DragValue::new(&mut value)
                        .range(0.0..=1.0)
                        .speed(0.001)
                        .fixed_decimals(3),
                )
                .labelled_by(output_label.id)
                .changed()
            {
                let _ = curve.set(handle, value);
            }
        });
        if ui.small_button("Reset channel curve").clicked() {
            curve.reset();
        }
        ui.add_space(6.0);
        let levels = &mut layer.levels;
        control(
            ui,
            "Black point",
            &mut levels.black,
            0.0..=(levels.white - 0.01).max(0.0),
            "",
        );
        control(ui, "Gamma", &mut levels.gamma, 0.2..=5.0, "");
        control(
            ui,
            "White point",
            &mut levels.white,
            (levels.black + 0.01).min(1.0)..=1.0,
            "",
        );
        ui.add_space(6.0);
        // Histogram of the final composition, computed on the GPU. The area
        // is reserved at full height whether or not data has arrived, so the
        // first readback never shifts the layout (a scrollbar appearing here
        // would resize the canvas mid-session).
        ui.label(
            RichText::new("HISTOGRAM · linear composite")
                .size(10.0)
                .color(MUTED),
        );
        ui.add_space(4.0);
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 138.0), Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 2.0, Color32::from_black_alpha(60));
        if let Some(rows) = &self.histogram_rows
            && self.histogram_revision == self.gpu.renders
        {
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
        ui.label(
            RichText::new("Transparent pixels excluded · before sRGB encoding")
                .size(10.0)
                .color(MUTED),
        );
        let endpoints = self
            .histogram_rows
            .as_ref()
            .filter(|_| self.histogram_revision == self.gpu.renders)
            .map(|rows| format!("Luma end bins: {} / {}", rows[0][0], rows[0][255]))
            .unwrap_or_else(|| "Luma end bins: measuring…".into());
        ui.label(RichText::new(endpoints).size(10.0)).on_hover_text("Counts in bins 0 and 255; these include endpoint values and do not prove clipping before tone adjustments.");
        ui.add_space(6.0);
        // Before/after toggles a render-time bypass;
        // the document is untouched and no history entry is created.
        let compare = ui
            .add(
                egui::Button::new(
                    RichText::new(if self.compare {
                        "Show edited"
                    } else {
                        "Show before"
                    })
                    .size(12.0),
                )
                .selected(self.compare),
            )
            .on_hover_text(
                "Toggle all tonal adjustments for preview. Export always uses edited pixels.",
            );
        if compare.clicked() {
            self.compare = !self.compare;
            self.gpu.set_compare(self.compare);
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
        if response.drag_started() {
            response.request_focus();
            self.curve_drag = Some(curve.clone());
            self.curve_cancelled = false;
            if let Some(pointer) = response.interact_pointer_pos() {
                self.curve_handle = grid_index((pointer.x - rect.left()) / rect.width());
            }
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) && self.curve_drag.is_some() {
            *curve = self.curve_drag.take().unwrap();
            self.curve_cancelled = true;
        }
        if (response.dragged() || response.clicked())
            && !self.curve_cancelled
            && let Some(pointer) = response.interact_pointer_pos()
        {
            response.request_focus();
            let index =
                grid_index(((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0)).unwrap();
            let value = (1.0 - (pointer.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
            if response.dragged()
                && let Some(old) = self.curve_handle
                && old != index
            {
                let _ = curve.reset_point(old);
            }
            let _ = curve.set(index, value);
            self.curve_handle = Some(index);
        }
        if response.double_clicked()
            && let Some(index) = self.curve_handle
        {
            let _ = curve.reset_point(index);
        }
        if !ui.input(|i| i.pointer.any_down()) {
            self.curve_drag = None;
            self.curve_cancelled = false;
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
