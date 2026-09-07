use super::*;
use std::sync::Arc;
use std::time::{Duration, Instant};
use vibeshop::document::{Blend, Editor, Layer, Source};
use vibeshop::gpu::Engine;

fn studio() -> (Studio, egui::Context) {
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("File-controller tests require an actual GPU adapter");
    eprintln!("File-controller GPU: {:?}", adapter.get_info());
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let renderer = eframe::egui_wgpu::Renderer::new(
        &device,
        wgpu::TextureFormat::Rgba8Unorm,
        Default::default(),
    );
    let render_state = eframe::egui_wgpu::RenderState {
        adapter,
        available_adapters: Vec::new(),
        device: device.clone(),
        queue: queue.clone(),
        target_format: wgpu::TextureFormat::Rgba8Unorm,
        renderer: Arc::new(egui::epaint::mutex::RwLock::new(renderer)),
    };
    let ctx = egui::Context::default();
    super::super::theme(&ctx);
    (
        Studio {
            editor: Editor::new(Document::blank(13, 7).unwrap()),
            gpu: Engine::new(device, queue),
            render_state,
            texture: None,
            rendered_revision: 0,
            render_valid: false,
            tool: super::super::Tool::Hand,
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            fit: true,
            move_start: None,
            job: None,
            pending: None,
            icons_checked: true,
            allow_close: false,
            status: String::new(),
            error: None,
            adapter: String::new(),
            project_path: None,
            new_size: None,
            startup: None,
        },
        ctx,
    )
}

fn drain(app: &mut Studio, ctx: &egui::Context) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.job.is_some() {
        assert!(Instant::now() < deadline, "File operation did not finish");
        app.poll_job(ctx);
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn complete(app: &mut Studio, ctx: &egui::Context, result: Result<Job>) {
    let (tx, rx) = mpsc::sync_channel(1);
    assert!(tx.send(result).is_ok());
    app.job = Some(rx);
    app.poll_job(ctx);
}

fn pixels(app: &mut Studio) -> Vec<u8> {
    assert!(app.render(), "{:?}", app.error);
    app.gpu.readback().unwrap().finish().unwrap()
}

#[test]
fn project_open_keyboard_save_reopen_and_export_keep_the_same_pixels() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("original.png");
    let path = directory.path().join("work.vibe");
    let export = directory.path().join("export.png");
    image_io::save_png(&input, 13, 7, &[90, 140, 200, 128].repeat(13 * 7)).unwrap();
    let original_file = std::fs::read(&input).unwrap();
    project::save(&path, &Document::new(image_io::open(&input).unwrap())).unwrap();
    let (mut app, ctx) = studio();
    app.request(Action::Open(Some(path.clone()), false), &ctx);
    drain(&mut app, &ctx);
    assert!(app.error.is_none(), "{:?}", app.error);
    app.editor
        .edit(|document, _| document.layers[0].exposure = 0.5);
    let mut top = app.editor.document.layers[0].clone();
    top.blend = Blend::Multiply;
    top.opacity = 0.4;
    top.offset = [1, -1];
    app.editor.add_layer(top).unwrap();
    let before = pixels(&mut app);
    ctx.begin_pass(egui::RawInput {
        events: vec![egui::Event::Key {
            key: egui::Key::S,
            physical_key: Some(egui::Key::S),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        }],
        ..Default::default()
    });
    app.shortcuts(&ctx);
    let _ = ctx.end_pass();
    assert!(app.job.is_some());
    drain(&mut app, &ctx);
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(!app.editor.dirty);
    assert_eq!(app.project_path.as_ref(), Some(&path));
    drop(app);
    let (mut reopened, ctx) = studio();
    reopened.request(Action::Open(Some(path), false), &ctx);
    drain(&mut reopened, &ctx);
    assert!(reopened.error.is_none(), "{:?}", reopened.error);
    let after = pixels(&mut reopened);
    assert_eq!(before, after);
    assert_eq!(reopened.editor.document.layers.len(), 2);
    image_io::save_png(&export, 13, 7, &after).unwrap();
    assert_eq!(image_io::open(&export).unwrap().source.rgba, after);
    assert_eq!(std::fs::read(input).unwrap(), original_file);
}

#[test]
fn failed_and_cancelled_save_completions_keep_work_and_pending_action() {
    let directory = tempfile::tempdir().unwrap();
    let (mut app, ctx) = studio();
    app.editor.mark_unsaved();
    app.project_path = Some(directory.path().join("missing/work.vibe"));
    app.pending = Some(Action::New([20, 30]));
    let before = app.editor.document.clone();
    app.save_project(false, &ctx);
    drain(&mut app, &ctx);
    assert!(app.error.is_some());
    assert!(app.editor.dirty);
    assert!(matches!(app.pending, Some(Action::New([20, 30]))));
    assert_eq!(app.editor.document, before);
    app.error = None;
    complete(&mut app, &ctx, Ok(Job::Cancelled));
    assert!(app.editor.dirty);
    assert!(app.pending.is_some());
    assert_eq!(app.editor.document, before);
}

#[test]
fn late_open_and_save_results_do_not_discard_newer_edits() {
    let (mut app, ctx) = studio();
    let revision = app.editor.revision;
    let saved_state = app.editor.state_id();
    app.editor
        .add_layer(Layer::new(
            "New work",
            Source::new(1, 1, vec![1, 2, 3, 255]).unwrap(),
        ))
        .unwrap();
    let before = app.editor.document.clone();
    complete(
        &mut app,
        &ctx,
        Ok(Job::Opened(
            Loaded {
                document: Document::blank(50, 50).unwrap(),
                path: None,
            },
            false,
            revision,
        )),
    );
    assert!(matches!(app.pending, Some(Action::Replace(_))));
    assert_eq!(app.editor.document, before);
    complete(
        &mut app,
        &ctx,
        Ok(Job::Saved(PathBuf::from("work.vibe"), saved_state)),
    );
    assert!(app.editor.dirty);
    assert!(matches!(app.pending, Some(Action::Replace(_))));
    assert_eq!(app.editor.document, before);
}

#[test]
fn saving_mid_drag_keeps_later_motion_undoable_back_to_the_saved_state() {
    let directory = tempfile::tempdir().unwrap();
    let (mut app, ctx) = studio();
    app.project_path = Some(directory.path().join("drag.vibe"));
    app.editor
        .add_layer(Layer::new(
            "Moving layer",
            Source::new(1, 1, vec![1, 2, 3, 255]).unwrap(),
        ))
        .unwrap();
    app.editor.begin_edit();
    app.editor.document.layers[0].offset = [2, 0];
    app.editor.changed();
    ctx.begin_pass(egui::RawInput {
        events: vec![egui::Event::PointerButton {
            pos: egui::pos2(100.0, 100.0),
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
        ..Default::default()
    });
    assert!(ctx.input(|input| input.pointer.any_down()));
    app.save_project(false, &ctx);
    let _ = ctx.end_pass();
    app.editor.document.layers[0].offset = [4, 0];
    app.editor.changed();
    app.editor.finish_edit();
    drain(&mut app, &ctx);
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.editor.dirty);
    app.editor.undo();
    assert_eq!(app.editor.document.layers[0].offset, [2, 0]);
    assert!(!app.editor.dirty);
    app.editor.redo();
    assert_eq!(app.editor.document.layers[0].offset, [4, 0]);
    assert!(app.editor.dirty);
}
