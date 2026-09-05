use anyhow::{Context, Result};
use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use std::{path::PathBuf, sync::mpsc::Receiver};

mod files;
use files::{Action, Job};
use vibeshop::{
    document::{self, Blend, Editor},
    gpu::Engine,
};

const ACCENT: Color32 = Color32::from_rgb(216, 247, 153);
const PANEL: Color32 = Color32::from_rgb(29, 31, 35);
const MUTED: Color32 = Color32::from_rgb(150, 155, 165);
#[derive(Clone, Copy, PartialEq)]
enum Tool {
    Hand,
    Move,
}

pub struct Studio {
    editor: Editor,
    gpu: Engine,
    render_state: eframe::egui_wgpu::RenderState,
    texture: Option<egui::TextureId>,
    rendered_revision: u64,
    render_valid: bool,
    tool: Tool,
    zoom: f32,
    pan: Vec2,
    fit: bool,
    move_start: Option<(Pos2, [i32; 2])>,
    job: Option<Receiver<Result<Job>>>,
    pending: Option<Action>,
    allow_close: bool,
    status: String,
    error: Option<String>,
    adapter: String,
    project_path: Option<PathBuf>,
    new_size: Option<[u32; 2]>,
    startup: Option<PathBuf>,
}
impl Studio {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Result<Self> {
        theme(&cc.egui_ctx);
        let state = cc
            .wgpu_render_state
            .as_ref()
            .context("Vibeshop requires a WebGPU-compatible graphics device")?
            .clone();
        let adapter = state.adapter.get_info();
        Ok(Self {
            editor: Editor::new(document::demo_document()),
            gpu: Engine::new(state.device.clone(), state.queue.clone()),
            render_state: state,
            texture: None,
            rendered_revision: 0,
            render_valid: false,
            tool: Tool::Hand,
            zoom: 1.0,
            pan: Vec2::ZERO,
            fit: true,
            move_start: None,
            job: None,
            pending: None,
            allow_close: false,
            status: "Generated demo · open a photo to make it yours".into(),
            error: None,
            adapter: format!("{} · {:?}", adapter.name, adapter.backend),
            project_path: None,
            new_size: None,
            startup: std::env::args_os().nth(1).map(PathBuf::from),
        })
    }
    fn render(&mut self) -> bool {
        if self.rendered_revision == self.editor.revision {
            return self.render_valid;
        }
        self.render_valid = false;
        match self.gpu.render(&self.editor.document) {
            Ok(resized) => {
                if let Some(view) = self.gpu.display_view() {
                    let mut renderer = self.render_state.renderer.write();
                    match self.texture {
                        Some(id) if resized => renderer.update_egui_texture_from_wgpu_texture(
                            &self.gpu.device,
                            view,
                            wgpu::FilterMode::Linear,
                            id,
                        ),
                        None => {
                            self.texture = Some(renderer.register_native_texture(
                                &self.gpu.device,
                                view,
                                wgpu::FilterMode::Linear,
                            ))
                        }
                        _ => {}
                    }
                    self.render_valid = true;
                }
            }
            Err(error) => {
                self.error = Some(format!("{error:#}"));
                if let Some(id) = self.texture.take() {
                    self.render_state.renderer.write().free_texture(&id);
                }
            }
        }
        self.rendered_revision = self.editor.revision;
        self.render_valid
    }
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if text_editor_has_focus(ctx) || self.pending.is_some() || self.error.is_some() || self.new_size.is_some() {
            return;
        }
        let command = egui::Modifiers::COMMAND;
        let command_shift = egui::Modifiers {
            shift: true,
            ..command
        };
        if shortcut(ctx, command_shift, egui::Key::O) {
            self.request(Action::Open(None, true), ctx);
        } else if shortcut(ctx, command, egui::Key::O) {
            self.request(Action::Open(None, false), ctx);
        }
        if shortcut(ctx, command_shift, egui::Key::S) {
            self.save_project(true, ctx);
        } else if shortcut(ctx, command, egui::Key::S) {
            self.save_project(false, ctx);
        }
        if shortcut(ctx, command_shift, egui::Key::E) {
            self.export(ctx);
        }
        if shortcut(ctx, command, egui::Key::N) && self.job.is_none() {
            self.new_size = Some([1920, 1080]);
        }
        if shortcut(ctx, command_shift, egui::Key::Z) {
            self.editor.redo();
        } else if shortcut(ctx, command, egui::Key::Z) {
            self.editor.undo();
        }
        if shortcut(ctx, egui::Modifiers::NONE, egui::Key::H) {
            self.tool = Tool::Hand;
        }
        if shortcut(ctx, egui::Modifiers::NONE, egui::Key::V) {
            self.tool = Tool::Move;
        }
        if shortcut(ctx, egui::Modifiers::NONE, egui::Key::F) {
            self.fit = true;
        }
    }
    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("toolbar")
            .exact_height(66.0)
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(20, 12)),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new("vibe").size(25.0).strong().color(ACCENT));
                    ui.label(RichText::new("shop").size(25.0).strong());
                    ui.add_space(16.0);
                    ui.menu_button("File", |ui| {
                        if ui.add_enabled(self.job.is_none(), egui::Button::new("New canvas…")).clicked() {
                            self.new_size = Some([1920, 1080]);
                            ui.close();
                        }
                        if ui.add_enabled(self.job.is_none(), egui::Button::new("Save project as…")).clicked() {
                            self.save_project(true, ctx);
                            ui.close();
                        }
                    });
                    if ui
                        .add_enabled(self.job.is_none(), egui::Button::new("Open"))
                        .on_hover_text("Open project, PNG or JPEG · Ctrl/Cmd+O")
                        .clicked()
                    {
                        self.request(Action::Open(None, false), ctx);
                    }
                    if ui
                        .add_enabled(self.job.is_none(), egui::Button::new("+ Add layer"))
                        .on_hover_text("Ctrl/Cmd+Shift+O")
                        .clicked()
                    {
                        self.request(Action::Open(None, true), ctx);
                    }
                    if ui.add_enabled(self.job.is_none(), egui::Button::new("Save project")).on_hover_text("Ctrl/Cmd+S · saves editable layers").clicked() {
                        self.save_project(false, ctx);
                    }
                    ui.separator();
                    if ui
                        .add_enabled(self.editor.can_undo(), egui::Button::new("Undo"))
                        .on_hover_text("Ctrl/Cmd+Z")
                        .clicked()
                    {
                        self.editor.undo();
                    }
                    if ui
                        .add_enabled(self.editor.can_redo(), egui::Button::new("Redo"))
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
                                    RichText::new("Export PNG  ↗")
                                        .color(Color32::from_rgb(24, 31, 19))
                                        .strong(),
                                )
                                .fill(ACCENT)
                                .min_size(egui::vec2(140.0, 34.0)),
                            )
                            .on_hover_text("Export flattened pixels · Ctrl/Cmd+Shift+E")
                            .clicked()
                        {
                            self.export(ctx);
                        }
                        ui.add_space(12.0);
                        ui.label(RichText::new("LOCAL FIRST").size(10.0).color(MUTED));
                    });
                });
            });
        egui::TopBottomPanel::top("document")
            .exact_height(38.0)
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(23, 25, 29))
                    .inner_margin(egui::Margin::symmetric(20, 8)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("CANVAS").size(10.0).color(ACCENT));
                    ui.add_space(12.0);
                    if let Some(path) = &self.project_path {
                        ui.label(path.file_name().unwrap_or_default().to_string_lossy());
                        ui.separator();
                    }
                    let d = &self.editor.document;
                    ui.label(format!("{} × {} px", d.width, d.height));
                    ui.label(
                        RichText::new("sRGB input · linear 16F composition")
                            .size(11.0)
                            .color(MUTED),
                    );
                    if self.editor.dirty {
                        ui.label(RichText::new("●  Unsaved changes").color(ACCENT).size(11.0));
                    }
                });
            });
    }
    fn inspector(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("inspector").default_width(292.0).min_width(260.0).max_width(380.0).resizable(true).frame(egui::Frame::new().fill(PANEL).inner_margin(18)).show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(6.0); ui.heading("Make it yours.");
                ui.label(RichText::new("Non-destructive adjustments").color(MUTED).size(12.0)); ui.add_space(24.0);
                let selected = self.editor.selected;
                if let Some(original) = self.editor.document.layers.get(selected) {
                    let mut layer = original.clone();
                    ui.label(RichText::new("LIGHT & COLOR").color(MUTED).size(10.0).strong()); ui.add_space(12.0);
                    slider(ui, "Exposure", &mut layer.exposure, -5.0..=5.0, " EV");
                    slider(ui, "Contrast", &mut layer.contrast, 0.0..=2.0, "×");
                    slider(ui, "Saturation", &mut layer.saturation, 0.0..=2.0, "×");
                    ui.add_space(4.0);
                    if ui.button("Reset adjustments").clicked() { layer.reset_adjustments(); }
                    ui.add_space(20.0); ui.separator(); ui.add_space(14.0);
                    ui.label(RichText::new("COMPOSITION").color(MUTED).size(10.0).strong()); ui.add_space(12.0);
                    slider(ui, "Opacity", &mut layer.opacity, 0.0..=1.0, "");
                    egui::ComboBox::from_id_salt("blend").selected_text(layer.blend.name()).width(ui.available_width() - 8.0).show_ui(ui, |ui| {
                        for blend in Blend::ALL { ui.selectable_value(&mut layer.blend, blend, blend.name()); }
                    });
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.label("X"); ui.add(egui::DragValue::new(&mut layer.offset[0]).range(-8192..=8192));
                        ui.label("Y"); ui.add(egui::DragValue::new(&mut layer.offset[1]).range(-8192..=8192));
                    });
                    if &layer != original {
                        self.editor.begin_edit(); self.editor.document.layers[selected] = layer; self.editor.changed();
                    }
                } else { ui.label("Add a layer to start editing."); }
                ui.add_space(22.0); ui.separator(); ui.add_space(16.0);
                ui.horizontal(|ui| { ui.label(RichText::new("LAYERS").size(10.0).strong().color(MUTED)); ui.label(RichText::new(format!("{}", self.editor.document.layers.len())).size(10.0).color(ACCENT)); });
                ui.add_space(10.0);
                for index in (0..self.editor.document.layers.len()).rev() {
                    let layer = &self.editor.document.layers[index];
                    let mut visible = layer.visible; let name = layer.name.clone();
                    let active = index == self.editor.selected;
                    let fill = if active { Color32::from_rgb(49, 56, 46) } else { Color32::from_rgb(36, 38, 43) };
                    egui::Frame::new().fill(fill).corner_radius(7).inner_margin(10).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut visible, "").on_hover_text("Layer visibility").changed() { self.editor.edit(|d, _| d.layers[index].visible = visible); }
                            if ui.add(egui::Button::new(RichText::new(name).size(12.0)).selected(active).frame(false).wrap()).clicked() { self.editor.finish_edit(); self.editor.selected = index; }
                        });
                    }); ui.add_space(5.0);
                }
                let count = self.editor.document.layers.len(); let selected = self.editor.selected;
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(count > 0 && count < document::MAX_LAYERS, egui::Button::new("Duplicate")).clicked() {
                        let mut layer = self.editor.document.layers[selected].clone(); layer.name = format!("{} copy", layer.name);
                        if let Err(e) = self.editor.add_layer(layer) { self.error = Some(e.to_string()); }
                    }
                    if ui.add_enabled(count > 0, egui::Button::new("Delete")).clicked() { self.editor.edit(|d, s| { d.layers.remove(*s); }); }
                });
                ui.horizontal(|ui| {
                    if ui.add_enabled(selected + 1 < count, egui::Button::new("Move up")).clicked() { self.editor.edit(|d, s| { d.layers.swap(*s, *s + 1); *s += 1; }); }
                    if ui.add_enabled(count > 0 && selected > 0, egui::Button::new("Move down")).clicked() { self.editor.edit(|d, s| { d.layers.swap(*s, *s - 1); *s -= 1; }); }
                });
                ui.add_space(24.0);
                ui.label(RichText::new("Save keeps editable layers in a .vibe project. Export PNG creates a flattened copy.").size(11.0).color(MUTED));
            });
        });
    }
    fn canvas(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("tools")
            .exact_width(68.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(PANEL).inner_margin(10))
            .show(ctx, |ui| {
                ui.add_space(15.0);
                for (tool, key, label) in [(Tool::Hand, "H", "Pan"), (Tool::Move, "V", "Move")] {
                    if ui
                        .add_sized(
                            [46.0, 42.0],
                            egui::Button::new(RichText::new(key).strong().size(18.0))
                                .selected(self.tool == tool),
                        )
                        .on_hover_text(format!("{label} · {key}"))
                        .clicked()
                    {
                        self.tool = tool;
                    }
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(label).size(10.0).color(MUTED));
                    });
                    ui.add_space(18.0);
                }
            });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(16, 18, 22))
                    .inner_margin(0),
            )
            .show(ctx, |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                let pixels_per_point = ctx.pixels_per_point();
                let dimensions = egui::vec2(
                    self.editor.document.width as f32,
                    self.editor.document.height as f32,
                );
                if self.fit {
                    self.zoom = ((rect.width() - 100.0) * pixels_per_point / dimensions.x)
                        .min((rect.height() - 110.0) * pixels_per_point / dimensions.y)
                        .clamp(0.02, 16.0);
                    self.pan = Vec2::ZERO;
                }
                if response.hovered() {
                    let scroll = ctx.input(|i| i.smooth_scroll_delta.y);
                    if scroll.abs() > 0.0 {
                        let old = self.zoom;
                        self.zoom = (old * (scroll * 0.002).exp()).clamp(0.02, 16.0);
                        if let Some(pointer) = response.hover_pos() {
                            self.pan = zoom_pan(self.pan, pointer - rect.center(), self.zoom / old);
                        }
                        self.fit = false;
                    }
                }
                let panning =
                    self.tool == Tool::Hand || ctx.input(|i| i.key_down(egui::Key::Space));
                if response.dragged() && panning {
                    self.pan += response.drag_delta();
                    self.fit = false;
                }
                if response.drag_started()
                    && !panning
                    && let (Some(pointer), Some(layer)) = (
                        ctx.input(|i| i.pointer.press_origin()),
                        self.editor.document.layers.get(self.editor.selected),
                    )
                {
                    self.move_start = Some((pointer, layer.offset));
                    self.editor.begin_edit();
                }
                if response.dragged()
                    && !panning
                    && let (Some((start, offset)), Some(pointer)) =
                        (self.move_start, response.interact_pointer_pos())
                {
                    let delta = (pointer - start) * pixels_per_point / self.zoom;
                    let next = [
                        (offset[0] + delta.x.round() as i32).clamp(-8192, 8192),
                        (offset[1] + delta.y.round() as i32).clamp(-8192, 8192),
                    ];
                    if let Some(layer) = self.editor.document.layers.get_mut(self.editor.selected)
                        && layer.offset != next
                    {
                        layer.offset = next;
                        self.editor.changed();
                    }
                }
                if response.drag_stopped() {
                    self.move_start = None;
                    self.editor.finish_edit();
                }
                if response.double_clicked() {
                    self.fit = true;
                }
                response.on_hover_cursor(if panning {
                    egui::CursorIcon::Grab
                } else {
                    egui::CursorIcon::Move
                });
                self.render();
                let image = Rect::from_center_size(
                    rect.center() + self.pan,
                    dimensions * (self.zoom / pixels_per_point),
                );
                let painter = ui.painter_at(rect);
                painter.rect_filled(image.expand(6.0), 2.0, Color32::from_black_alpha(70));
                let visible = image.intersect(rect);
                if visible.is_positive() {
                    let start_x = ((visible.min.x - image.min.x) / 16.0).floor() as i32;
                    let start_y = ((visible.min.y - image.min.y) / 16.0).floor() as i32;
                    let columns = (visible.width() / 16.0).ceil() as i32 + 1;
                    let rows = (visible.height() / 16.0).ceil() as i32 + 1;
                    let checkers = ui.painter_at(visible);
                    for y in start_y..start_y + rows {
                        for x in start_x..start_x + columns {
                            let shade = if (x + y) % 2 == 0 { 48 } else { 58 };
                            checkers.rect_filled(
                                Rect::from_min_size(
                                    image.min + egui::vec2(x as f32 * 16.0, y as f32 * 16.0),
                                    egui::vec2(16.0, 16.0),
                                ),
                                0.0,
                                Color32::from_gray(shade),
                            );
                        }
                    }
                }
                if let Some(texture) = self.texture {
                    painter.image(
                        texture,
                        image,
                        Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
                painter.rect_stroke(
                    image,
                    0.0,
                    Stroke::new(1.0_f32, Color32::from_gray(76)),
                    egui::StrokeKind::Outside,
                );
                painter.text(
                    rect.left_top() + egui::vec2(24.0, 22.0),
                    egui::Align2::LEFT_TOP,
                    "YOUR NEXT GOOD IDEA STARTS HERE",
                    egui::FontId::proportional(10.0),
                    MUTED,
                );
                painter.text(
                    rect.center_bottom() - egui::vec2(0.0, 24.0),
                    egui::Align2::CENTER_BOTTOM,
                    "Scroll to zoom   ·   Space + drag to pan   ·   F to fit",
                    egui::FontId::proportional(11.0),
                    MUTED,
                );
            });
    }
}
impl eframe::App for Studio {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.poll_job(ctx);
        if let Some(path) = self.startup.take() {
            self.request(Action::Open(Some(path), false), ctx);
        }
        if ctx.input(|i| i.viewport().close_requested())
            && !self.allow_close
            && (self.editor.dirty || self.job.is_some())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.request(Action::Exit, ctx);
        }
        self.shortcuts(ctx);
        if let Some(path) = ctx.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone())) {
            self.request(Action::Open(Some(path), false), ctx);
        }
        self.top_bar(ctx);
        egui::TopBottomPanel::bottom("status").exact_height(32.0).frame(egui::Frame::new().fill(PANEL).inner_margin(egui::Margin::symmetric(16, 7))).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("●").color(ACCENT));
                ui.label(RichText::new(&self.status).size(11.0).color(MUTED));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Fit").clicked() { self.fit = true; }
                    if ui.small_button(format!("{:.0}%", self.zoom * 100.0)).on_hover_text("Click for 100% / one image pixel per physical screen pixel").clicked() { self.zoom = 1.0; self.pan = Vec2::ZERO; self.fit = false; }
                    ui.label(RichText::new(format!("UI {:.1} ms", frame.info().cpu_usage.unwrap_or(0.0) * 1000.0)).size(10.0).color(MUTED)).on_hover_text("CPU time for UI generation, not GPU latency or a performance benchmark");
                    ui.label(RichText::new("GPU").size(10.0).color(ACCENT)).on_hover_text(format!("{}\n{} compositions · {} source uploads", self.adapter, self.gpu.renders, self.gpu.uploads));
                });
            });
        });
        self.inspector(ctx);
        self.canvas(ctx);
        if !ctx.input(|i| i.pointer.any_down()) {
            self.editor.finish_edit();
        }
        self.dialogs(ctx);
    }
}
fn shortcut(ctx: &egui::Context, modifiers: egui::Modifiers, key: egui::Key) -> bool {
    ctx.input_mut(|input| input.consume_key(modifiers, key))
}
fn text_editor_has_focus(ctx: &egui::Context) -> bool {
    // Canvas and buttons can own keyboard focus without being text editors.
    ctx.memory(|memory| memory.focused())
        .is_some_and(|id| egui::TextEdit::load_state(ctx, id).is_some())
}
fn png_destination(path: &std::path::Path) -> Result<PathBuf> {
    anyhow::ensure!(
        path.extension()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("png")),
        "Choose a filename ending in .png. The export path will not be silently changed."
    );
    Ok(path.to_path_buf())
}
fn zoom_pan(pan: Vec2, anchor: Vec2, ratio: f32) -> Vec2 {
    anchor - (anchor - pan) * ratio
}
fn slider(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
) {
    ui.label(RichText::new(label).size(12.0));
    ui.add(
        egui::Slider::new(value, range)
            .suffix(suffix)
            .fixed_decimals(2),
    );
    ui.add_space(10.0);
}
fn theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = PANEL;
    visuals.window_fill = PANEL;
    visuals.override_text_color = Some(Color32::from_rgb(234, 235, 237));
    visuals.selection.bg_fill = Color32::from_rgb(74, 90, 50);
    visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(41, 44, 50);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(59, 65, 59);
    visuals.widgets.active.bg_fill = Color32::from_rgb(71, 83, 54);
    visuals.widgets.active.fg_stroke = Stroke::new(1.5_f32, ACCENT);
    ctx.set_visuals(visuals);
    ctx.style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(12.0, 7.0);
        style.spacing.slider_width = 170.0;
        style.spacing.interact_size.y = 28.0;
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn export_keeps_the_exact_confirmed_path() {
        let path = std::path::Path::new("Original.PNG");
        assert_eq!(png_destination(path).unwrap(), path);
        assert!(png_destination(std::path::Path::new("Original.jpg")).is_err());
        assert!(png_destination(std::path::Path::new("Original")).is_err());
    }
    #[test]
    fn cursor_anchored_zoom_preserves_image_coordinate() {
        let pan = egui::vec2(21.0, -17.0);
        let anchor = egui::vec2(100.0, 82.0);
        let next = zoom_pan(pan, anchor, 2.5);
        assert!(((anchor - pan) - (anchor - next) / 2.5).length() < 0.0001);
    }
    #[test]
    fn only_text_focus_suppresses_editor_shortcuts() {
        let ctx = egui::Context::default();
        assert!(!text_editor_has_focus(&ctx));
        let canvas = egui::Id::new("canvas");
        ctx.memory_mut(|memory| memory.request_focus(canvas));
        assert!(ctx.wants_keyboard_input());
        assert!(!text_editor_has_focus(&ctx));
        let text = egui::Id::new("numeric-text-entry");
        egui::TextEdit::store_state(&ctx, text, Default::default());
        ctx.memory_mut(|memory| memory.request_focus(text));
        assert!(text_editor_has_focus(&ctx));
        ctx.memory_mut(|memory| memory.surrender_focus(text));
        assert!(!text_editor_has_focus(&ctx));
    }
    #[test]
    fn a_released_modifier_does_not_erase_the_key_event_shortcut() {
        let ctx = egui::Context::default();
        ctx.begin_pass(egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Z,
                physical_key: Some(egui::Key::Z),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }],
            ..Default::default()
        });
        assert!(!ctx.input(|input| input.modifiers.command));
        assert!(shortcut(&ctx, egui::Modifiers::COMMAND, egui::Key::Z));
        assert!(!shortcut(&ctx, egui::Modifiers::COMMAND, egui::Key::Z));
        let _ = ctx.end_pass();
    }
}
