import { readFileSync, writeFileSync } from 'node:fs';
function replace(path, before, after) {
  const text = readFileSync(path, 'utf8');
  if (text.split(before).length !== 2) throw new Error(`Expected exactly one patch target in ${path}: ${before}`);
  writeFileSync(path, text.replace(before, after));
}
function section(path, from, to, replacement) {
  const text = readFileSync(path, 'utf8');
  const start = text.indexOf(from), end = text.indexOf(to, start);
  if (start < 0 || end < start) throw new Error(`Missing section in ${path}`);
  writeFileSync(path, text.slice(0, start) + replacement + text.slice(end));
}
const doc = 'src/document.rs';
replace(doc, '#[derive(Debug, PartialEq)]', '#[derive(Debug, PartialEq, Eq)]');
replace(doc, '    pub fn replace(&mut self, document: Document) {', `    pub fn replace_if_revision(&mut self, document: Document, expected: u64) -> std::result::Result<(), Document> {
        if self.revision != expected { return Err(document); }
        self.replace(document);
        Ok(())
    }
    pub fn replace(&mut self, document: Document) {`);
const gpu = 'src/gpu.rs';
replace(gpu, '    pub renders: u64,', '    pub renders: u64,\n    render_valid: bool,');
replace(gpu, '            renders: 0,', '            renders: 0,\n            render_valid: false,');
replace(gpu, '    pub fn render(&mut self, document: &Document) -> Result<bool> {', '    pub fn render(&mut self, document: &Document) -> Result<bool> {\n        self.render_valid = false;');
replace(gpu, '        self.renders += 1;', '        self.renders += 1;\n        self.render_valid = true;');
replace(gpu, '    pub fn readback(&self) -> Result<Readback> {', '    pub fn readback(&self) -> Result<Readback> {\n        ensure!(self.render_valid, "The current image could not be rendered; refusing to export stale pixels");');
const studio = 'src/studio.rs';
replace(studio, '    Open(Option<PathBuf>, bool),', '    Open(Option<PathBuf>, bool),\n    Replace(Document),');
replace(studio, '    Opened(Layer, bool),', '    Opened(Layer, bool, u64),');
replace(studio, '    rendered_revision: u64,', '    rendered_revision: u64,\n    render_valid: bool,');
replace(studio, '            rendered_revision: 0,', '            rendered_revision: 0,\n            render_valid: false,');
section(studio, '    fn render(&mut self)', '    fn request(', `    fn render(&mut self) -> bool {
        if self.rendered_revision == self.editor.revision { return self.render_valid; }
        self.render_valid = false;
        match self.gpu.render(&self.editor.document) {
            Ok(resized) => {
                if let Some(view) = self.gpu.display_view() {
                    let mut renderer = self.render_state.renderer.write();
                    match self.texture {
                        Some(id) if resized => renderer.update_egui_texture_from_wgpu_texture(&self.gpu.device, view, wgpu::FilterMode::Linear, id),
                        None => self.texture = Some(renderer.register_native_texture(&self.gpu.device, view, wgpu::FilterMode::Linear)),
                        _ => {}
                    }
                    self.render_valid = true;
                }
            }
            Err(error) => {
                self.error = Some(format!("{error:#}"));
                if let Some(id) = self.texture.take() { self.render_state.renderer.write().free_texture(&id); }
            }
        }
        self.rendered_revision = self.editor.revision;
        self.render_valid
    }
`);
replace(studio, '    fn request(&mut self, action: Action, ctx: &egui::Context) {', '    fn request(&mut self, action: Action, ctx: &egui::Context) {\n        if self.pending.is_some() || self.error.is_some() { return; }');
replace(studio, '            Action::Open(path, add) => {', `            Action::Replace(document) => {
                self.editor.replace(document);
                self.fit = true;
                self.status = "Image opened locally · source pixels are preserved".into();
            }
            Action::Open(path, add) => {
                let revision = self.editor.revision;`);
replace(studio, 'Ok(Job::Opened(image_io::open(&path)?, add))', 'Ok(Job::Opened(image_io::open(&path)?, add, revision))');
replace(studio, '        self.render();\n        let snapshot', '        if !self.render() { return; }\n        let snapshot');
replace(studio, '                let path = file.path().with_extension("png");', '                let path = png_destination(file.path())?;');
replace(studio, '            Ok(Job::Opened(layer, add)) => {', '            Ok(Job::Opened(layer, add, revision)) => {');
replace(studio, `                    self.editor.replace(Document::new(layer));
                    self.fit = true;`, `                    match self.editor.replace_if_revision(Document::new(layer), revision) {
                        Ok(()) => self.fit = true,
                        Err(document) => {
                            self.pending = Some(Action::Replace(document));
                            self.status = "Your work changed while opening the image · choose whether to replace it".into();
                            return;
                        }
                    }`);
section(studio, '    fn dialogs(&mut self', '\n}\nimpl eframe::App', `    fn dialogs(&mut self, ctx: &egui::Context) {
        if self.pending.is_some() {
            let mut proceed = false;
            let mut cancel = false;
            egui::Modal::new(egui::Id::new("unsaved-work")).show(ctx, |ui| {
                ui.set_max_width(390.0);
                ui.heading("Keep your work");
                ui.label("Your editable layers exist only in memory. Export a PNG before discarding them. Export is a flattened image, not an editable project.");
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    cancel = ui.button("Keep editing").clicked();
                    proceed = ui.button("Discard layers and continue").clicked();
                });
            });
            if cancel { self.pending = None; }
            if proceed && let Some(action) = self.pending.take() { self.execute(action, ctx); }
        }
        if let Some(error) = self.error.clone() {
            let mut dismiss = false;
            egui::Modal::new(egui::Id::new("operation-error")).show(ctx, |ui| {
                ui.set_max_width(450.0);
                ui.heading("Could not complete that");
                ui.label(error);
                ui.add_space(12.0);
                dismiss = ui.button("Back to editing").clicked();
            });
            if dismiss { self.error = None; }
        }
    }`);
replace(studio, 'fn zoom_pan(pan: Vec2, anchor: Vec2, ratio: f32) -> Vec2 {', `fn png_destination(path: &std::path::Path) -> Result<PathBuf> {
    anyhow::ensure!(path.extension().and_then(|s| s.to_str()).is_some_and(|s| s.eq_ignore_ascii_case("png")), "Choose a filename ending in .png. The export path will not be silently changed.");
    Ok(path.to_path_buf())
}
fn zoom_pan(pan: Vec2, anchor: Vec2, ratio: f32) -> Vec2 {`);
replace(studio, '    fn cursor_anchored_zoom_preserves_image_coordinate() {', `    fn export_keeps_the_exact_confirmed_path() {
        let path = std::path::Path::new("Original.PNG");
        assert_eq!(png_destination(path).unwrap(), path);
        assert!(png_destination(std::path::Path::new("Original.jpg")).is_err());
        assert!(png_destination(std::path::Path::new("Original")).is_err());
    }
    #[test]
    fn cursor_anchored_zoom_preserves_image_coordinate() {`);
writeFileSync('tests/document.rs', readFileSync('tests/document.rs', 'utf8') + `
#[test]
fn a_late_open_cannot_discard_newer_edits() {
    let mut e = editor();
    let requested_revision = e.revision;
    let incoming = Document::new(Layer::new("incoming", Source::new(1, 1, vec![1, 2, 3, 255]).unwrap()));
    e.edit(|d, _| d.layers[0].exposure = 1.0);
    let pending = e.replace_if_revision(incoming, requested_revision).unwrap_err();
    assert_eq!(e.document.layers[0].name, "test");
    assert_eq!(e.document.layers[0].exposure, 1.0);
    assert!(e.can_undo());
    e.replace_if_revision(pending, e.revision).unwrap();
    assert_eq!(e.document.layers[0].name, "incoming");
    assert!(!e.can_undo());
}
`);
writeFileSync('tests/gpu.rs', readFileSync('tests/gpu.rs', 'utf8') + `
#[test]
fn failed_render_cannot_export_previous_pixels() {
    let mut e = engine();
    let mut d = Document::new(layer([80, 90, 100, 255], 1, 1));
    close(&render(&mut e, &d), &[80, 90, 100, 255]);
    d.width = 0;
    assert!(e.render(&d).is_err());
    assert!(e.readback().is_err());
    d.width = 1;
    close(&render(&mut e, &d), &[80, 90, 100, 255]);
}
`);
