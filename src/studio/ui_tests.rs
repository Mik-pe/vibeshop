use super::{Studio, Tool, files::Action, icons};
use accesskit::{Action as AccessKitAction, ActionRequest};
use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2, Rect};
use std::{
    collections::HashMap,
    path::Path,
    sync::mpsc,
    time::{Duration, Instant},
};
use vibeshop::{
    document::{self, Document},
    image_io, project,
};

struct Harness {
    app: Studio,
    ctx: egui::Context,
    logical_size: [u32; 2],
    scale: f32,
    time: f64,
    controls: HashMap<String, (egui::accesskit::NodeId, Rect)>,
    /// The accesskit node egui reports as holding keyboard focus.
    focused_accesskit: Option<egui::accesskit::NodeId>,
    target: wgpu::Texture,
}
impl Harness {
    fn new(size: [u32; 2], scale: f32) -> Self {
        let instance = wgpu::Instance::default();
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("UI tests require a working GPU adapter");
        eprintln!("Workspace GPU: {:?}", adapter.get_info());
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        let renderer = eframe::egui_wgpu::Renderer::new(
            &device,
            wgpu::TextureFormat::Rgba8Unorm,
            Default::default(),
        );
        let state = eframe::egui_wgpu::RenderState {
            adapter,
            available_adapters: Vec::new(),
            device,
            queue,
            target_format: wgpu::TextureFormat::Rgba8Unorm,
            renderer: std::sync::Arc::new(eframe::egui::mutex::RwLock::new(renderer)),
        };
        let ctx = egui::Context::default();
        let app = Studio::from_render_state(&ctx, state, None);
        ctx.enable_accesskit();
        let target = app.gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Actual workspace test capture"),
            size: wgpu::Extent3d {
                width: (size[0] as f32 * scale) as u32,
                height: (size[1] as f32 * scale) as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let mut h = Self {
            app,
            ctx,
            logical_size: size,
            scale,
            time: 0.0,
            controls: HashMap::new(),
            focused_accesskit: None,
            target,
        };
        h.frame(Vec::new());
        h.frame(Vec::new());
        h
    }
    fn frame(&mut self, events: Vec<Event>) {
        self.time += 1.0 / 60.0;
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(self.logical_size[0] as f32, self.logical_size[1] as f32),
            )),
            time: Some(self.time),
            events,
            focused: true,
            ..Default::default()
        };
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .native_pixels_per_point = Some(self.scale);
        let output = self.ctx.run(input, |ctx| self.app.show(ctx));
        self.controls.clear();
        self.focused_accesskit = None;
        if let Some(update) = output.platform_output.accesskit_update {
            self.focused_accesskit = Some(update.focus);
            for (id, node) in &update.nodes {
                if let (Some(label), Some(bounds)) = (node.label(), node.bounds()) {
                    self.controls.insert(
                        label.to_owned(),
                        (
                            *id,
                            Rect::from_min_max(
                                egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                                egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                            ),
                        ),
                    );
                }
            }
        }
        let paints = self.ctx.tessellate(output.shapes, output.pixels_per_point);
        let screen = eframe::egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.target.width(), self.target.height()],
            pixels_per_point: output.pixels_per_point,
        };
        let mut renderer = self.app.render_state.renderer.write();
        for (id, delta) in &output.textures_delta.set {
            renderer.update_texture(&self.app.gpu.device, &self.app.gpu.queue, *id, delta);
        }
        let mut encoder = self
            .app
            .gpu
            .device
            .create_command_encoder(&Default::default());
        let mut commands = renderer.update_buffers(
            &self.app.gpu.device,
            &self.app.gpu.queue,
            &mut encoder,
            &paints,
            &screen,
        );
        let view = self.target.create_view(&Default::default());
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Production workspace UI"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            renderer.render(&mut pass.forget_lifetime(), &paints, &screen);
        }
        commands.push(encoder.finish());
        self.app.gpu.queue.submit(commands);
        for id in &output.textures_delta.free {
            renderer.free_texture(id);
        }
    }
    fn rect(&self, name: &str) -> Rect {
        self.controls
            .get(name)
            .unwrap_or_else(|| {
                panic!(
                    "Missing accessible control {name}; present: {:?}",
                    self.controls.keys().collect::<Vec<_>>()
                )
            })
            .1
    }
    /// Focus a control exactly the way assistive technology does: an
    /// accesskit Focus action request, the same path screen readers use.
    fn focus(&mut self, name: &str) {
        let id = self
            .controls
            .get(name)
            .map(|(id, _)| *id)
            .unwrap_or_else(|| {
                panic!(
                    "Cannot focus missing control {name}; present: {:?}",
                    self.controls.keys().collect::<Vec<_>>()
                )
            });
        self.frame(vec![Event::AccessKitActionRequest(ActionRequest {
            action: AccessKitAction::Focus,
            target: id,
            data: None,
        })]);
    }
    /// Is this named control the one egui reports as holding keyboard focus?
    fn is_focused(&self, name: &str) -> bool {
        match (self.controls.get(name), self.focused_accesskit) {
            (Some((id, _)), Some(focused)) => *id == focused,
            _ => false,
        }
    }
    /// Press a key on the currently focused widget.
    fn press(&mut self, key: egui::Key) {
        self.key(key, Modifiers::NONE);
    }
    fn click(&mut self, name: &str) {
        let at = self.rect(name).center();
        self.click_at(at);
    }
    fn click_at(&mut self, at: Pos2) {
        self.frame(vec![Event::PointerMoved(at)]);
        self.frame(vec![Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        }]);
        self.frame(vec![Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }]);
        self.frame(Vec::new());
    }
    fn key(&mut self, key: egui::Key, modifiers: Modifiers) {
        self.frame(vec![Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers,
        }]);
        self.frame(vec![Event::Key {
            key,
            physical_key: Some(key),
            pressed: false,
            repeat: false,
            modifiers,
        }]);
        self.frame(vec![Event::Key {
            key: egui::Key::F,
            physical_key: Some(egui::Key::F),
            pressed: false,
            repeat: false,
            modifiers: Modifiers::NONE,
        }]);
    }
    fn drag(&mut self, start: Pos2, end: Pos2) {
        self.frame(vec![Event::PointerMoved(start)]);
        self.frame(vec![Event::PointerButton {
            pos: start,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        }]);
        self.frame(vec![Event::PointerMoved(start.lerp(end, 0.5))]);
        self.frame(vec![Event::PointerMoved(end)]);
        self.frame(vec![Event::PointerButton {
            pos: end,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }]);
        self.frame(Vec::new());
    }
    fn wait_for_file(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.app.job.is_some() {
            assert!(Instant::now() < deadline, "File operation did not complete");
            std::thread::sleep(Duration::from_millis(5));
            self.frame(Vec::new());
        }
        assert!(self.app.error.is_none(), "{:?}", self.app.error);
    }
    fn capture(&self, name: &str) {
        let width = self.target.width();
        let height = self.target.height();
        let stride = (width * 4).div_ceil(256) * 256;
        let buffer = self.app.gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("UI capture readback"),
            size: u64::from(stride) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .app
            .gpu
            .device
            .create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            self.target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let submission = self.app.gpu.queue.submit([encoder.finish()]);
        let (tx, rx) = mpsc::sync_channel(1);
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
        self.app
            .gpu
            .device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(15)),
            })
            .unwrap();
        rx.recv_timeout(Duration::from_secs(15)).unwrap().unwrap();
        let data = buffer.slice(..).get_mapped_range();
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for row in data.chunks_exact(stride as usize) {
            pixels.extend_from_slice(&row[..(width * 4) as usize]);
        }
        std::fs::create_dir_all("artifacts").unwrap();
        image_io::save_png(
            &Path::new("artifacts").join(format!("{name}.png")),
            width,
            height,
            &pixels,
        )
        .unwrap();
        drop(data);
        buffer.unmap();
    }
}

#[test]
fn workspace_controls_stay_visible_at_minimum_size_and_multiple_dpi() {
    for (size, scale, name) in [
        ([1440, 940], 1.0, "workspace-default"),
        ([1000, 640], 1.0, "workspace-small"),
        ([1000, 640], 1.5, "workspace-150"),
        ([1000, 640], 2.0, "workspace-200"),
    ] {
        let mut h = Harness::new(size, scale);
        let bounds = Rect::from_min_size(Pos2::ZERO, egui::vec2(size[0] as f32, size[1] as f32));
        for control in [
            format!("{} Open", icons::OPEN),
            format!("{} Save", icons::SAVE),
            format!("{} Export PNG", icons::EXPORT),
            "+ Image".to_owned(),
            format!("{} Duplicate", icons::DUPLICATE),
            format!("{} Remove", icons::REMOVE),
            format!("{} Raise layer", icons::RAISE),
            format!("{} Lower layer", icons::LOWER),
            "Move".to_owned(),
            "Pan".to_owned(),
        ] {
            assert!(
                bounds.contains_rect(h.rect(&control)),
                "{control} is outside {size:?} at scale {scale}: {:?}",
                h.rect(&control)
            );
        }
        h.capture(name);
        let mut layer = h.app.editor.document.layers[0].clone();
        layer.name =
            "A very long image filename that must not push controls outside the inspector.png"
                .into();
        for _ in 1..document::MAX_LAYERS {
            h.app.editor.add_layer(layer.clone()).unwrap();
        }
        h.frame(Vec::new());
        h.frame(Vec::new());
        for control in [
            format!("{} Remove", icons::REMOVE),
            format!("{} Raise layer", icons::RAISE),
            format!("{} Lower layer", icons::LOWER),
        ] {
            assert!(bounds.contains_rect(h.rect(&control)));
        }
        if scale == 1.0 {
            h.capture(&format!("{name}-many-layers"));
        }
        h.app.editor.replace(Document::blank(1920, 1080).unwrap());
        h.frame(Vec::new());
        h.frame(Vec::new());
        assert!(bounds.contains_rect(h.rect("+ Image")));
    }
}
#[test]
fn workspace_buttons_edit_layers_and_navigation_reuses_gpu_pixels() {
    let mut h = Harness::new([1000, 640], 1.0);
    h.click(&format!("{} Duplicate", icons::DUPLICATE));
    assert_eq!(h.app.editor.document.layers.len(), 2);
    assert_eq!(h.app.editor.selected, 1);
    h.click(&format!("{} Lower layer", icons::LOWER));
    assert_eq!(h.app.editor.selected, 0);
    h.click(&format!("{} Raise layer", icons::RAISE));
    assert_eq!(h.app.editor.selected, 1);
    h.click(&format!("{} Remove", icons::REMOVE));
    assert_eq!(h.app.editor.document.layers.len(), 1);
    h.key(egui::Key::Z, Modifiers::COMMAND);
    assert_eq!(h.app.editor.document.layers.len(), 2);
    h.click("Move");
    h.drag(egui::pos2(250.0, 230.0), egui::pos2(310.0, 270.0));
    assert_ne!(
        h.app.editor.document.layers[h.app.editor.selected].offset,
        [0, 0]
    );
    h.key(egui::Key::Z, Modifiers::COMMAND);
    assert_eq!(
        h.app.editor.document.layers[h.app.editor.selected].offset,
        [0, 0]
    );
    h.click("Pan");
    assert!(h.app.tool == Tool::Hand);
    let renders = h.app.gpu.renders;
    let uploads = h.app.gpu.uploads;
    h.drag(egui::pos2(250.0, 230.0), egui::pos2(290.0, 250.0));
    assert!(h.app.pan.length() > 0.0);
    let old_zoom = h.app.zoom;
    h.frame(vec![Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, 100.0),
        modifiers: Modifiers::NONE,
    }]);
    assert!(h.app.zoom > old_zoom);
    assert_eq!(h.app.gpu.renders, renders);
    assert_eq!(h.app.gpu.uploads, uploads);
    h.key(egui::Key::F, Modifiers::NONE);
    let fit_zoom = h.app.zoom;
    for _ in 0..12 {
        h.frame(Vec::new());
        assert!(h.app.fit);
        assert_eq!(h.app.zoom, fit_zoom);
        assert_eq!(h.app.pan, egui::Vec2::ZERO);
    }
    assert_eq!(h.app.gpu.renders, renders);
    assert_eq!(h.app.gpu.uploads, uploads);
}
#[test]
fn modal_cancel_discard_and_save_continue_are_real_ui_actions() {
    let mut h = Harness::new([1000, 640], 1.0);
    h.key(egui::Key::N, Modifiers::COMMAND);
    assert!(h.app.new_size.is_some());
    h.key(egui::Key::Escape, Modifiers::NONE);
    assert!(h.app.new_size.is_none());
    h.click(&format!("{} Duplicate", icons::DUPLICATE));
    let before = h.app.editor.document.clone();
    h.key(egui::Key::N, Modifiers::COMMAND);
    h.click("Create canvas");
    assert!(h.app.pending.is_some());
    h.capture("workspace-save-dialog");
    h.click(&format!("{} Duplicate", icons::DUPLICATE));
    h.key(egui::Key::Z, Modifiers::COMMAND);
    assert_eq!(h.app.editor.document, before);
    assert!(h.app.pending.is_some());
    h.click("Cancel");
    assert_eq!(h.app.editor.document, before);
    assert!(h.app.pending.is_none());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("saved.vibe");
    h.app.project_path = Some(path.clone());
    h.app.request(Action::New([600, 400]), &h.ctx);
    h.frame(Vec::new());
    h.frame(Vec::new());
    h.click("Save and continue");
    h.wait_for_file();
    assert_eq!(h.app.editor.document.width, 600);
    assert!(h.app.editor.document.layers.is_empty());
    assert_eq!(project::open(&path).unwrap().layers.len(), 2);
    h.app.request(Action::New([800, 600]), &h.ctx);
    h.frame(Vec::new());
    h.frame(Vec::new());
    h.click("Discard changes");
    assert_eq!(h.app.editor.document.width, 800);
}

#[test]
fn icons_only_buttons_carry_meaningful_accessibility_names() {
    // The icon vocabulary must render from the bundled fonts before anything
    // else; a missing glyph is a regression, not a cosmetic issue.
    let h = Harness::new([1000, 640], 1.0);
    // The first-frame assertion lives inside show(); reaching this point
    // proves every icon glyph exists in the bundled fonts.
    for name in [
        "Move".to_owned(),
        "Pan".to_owned(),
        format!("{} Fit", icons::FIT),
    ] {
        h.rect(&name);
    }
    h.capture("workspace-icons");
}

#[test]
fn keyboard_focus_and_activation_work_like_assistive_technology() {
    let mut h = Harness::new([1000, 640], 1.0);
    let window = Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 640.0));
    // Assistive technology focuses controls through the accessibility tree;
    // those requests must drive this editor, and focused controls must then
    // respond to the keyboard without any pointer interaction.
    let fit = format!("{} Fit", icons::FIT);
    let layer = "Dune study · generated demo";
    let eye = format!("{} Visible: {layer}", icons::VISIBLE);
    for target in [
        format!("{} Duplicate", icons::DUPLICATE),
        fit.clone(),
        eye.clone(),
    ] {
        h.focus(&target);
        h.frame(Vec::new());
        assert!(
            h.is_focused(&target),
            "accesskit Focus on {target} did not move keyboard focus"
        );
        let rect = h.rect(&target);
        assert!(
            window.contains_rect(rect),
            "focused {target} is outside the viewport: {rect:?}"
        );
    }
    // Space activates the focused visibility toggle.
    h.focus(&eye);
    h.frame(Vec::new());
    h.press(egui::Key::Space);
    h.frame(Vec::new());
    assert!(
        !h.app.editor.document.layers[0].visible,
        "Space on the focused visibility checkbox must hide the layer"
    );
    // Enter activates a focused button command.
    h.focus(&fit);
    h.frame(Vec::new());
    h.press(egui::Key::Enter);
    h.frame(Vec::new());
    assert!(
        h.app.fit,
        "Enter on the focused Fit button must fit the canvas"
    );
}

#[test]
fn modal_dialogs_block_shortcuts_from_behind() {
    let mut h = Harness::new([1000, 640], 1.0);
    h.click(&format!("{} Duplicate", icons::DUPLICATE));
    assert_eq!(h.app.editor.document.layers.len(), 2);
    let before = h.app.editor.document.clone();
    h.key(egui::Key::N, Modifiers::COMMAND);
    h.frame(Vec::new());
    assert!(h.app.new_size.is_some());
    h.click("Create canvas");
    assert!(h.app.pending.is_some());
    let blocked_before = h.app.editor.document.clone();
    h.key(egui::Key::Z, Modifiers::COMMAND);
    h.key(egui::Key::O, Modifiers::COMMAND);
    h.frame(Vec::new());
    assert!(h.app.job.is_none(), "no file job may start behind a modal");
    assert!(
        h.app.editor.document == blocked_before,
        "document must not change while the unsaved-work modal is up"
    );
    h.key(egui::Key::Escape, Modifiers::NONE);
    h.frame(Vec::new());
    assert!(h.app.pending.is_none());
    h.frame(Vec::new());
    h.key(egui::Key::Z, Modifiers::COMMAND);
    assert_ne!(
        h.app.editor.document, before,
        "after dismissal, undo must work again"
    );
    assert_eq!(h.app.editor.document.layers.len(), 1);
}
