use super::{Studio, png_destination};
use anyhow::{Context, Result, ensure};
use eframe::egui;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, TryRecvError};
use vibeshop::{document::Document, image_io, project};

pub(super) struct Loaded {
    document: Document,
    path: Option<PathBuf>,
}

pub(super) enum Action {
    Open(Option<PathBuf>, bool),
    Replace(Loaded),
    New([u32; 2]),
    Exit,
}

pub(super) enum Job {
    Opened(Loaded, bool, u64),
    Saved(PathBuf, u64),
    Exported(PathBuf),
    Cancelled,
}

impl Studio {
    pub(super) fn request(&mut self, action: Action, ctx: &egui::Context) {
        if self.pending.is_some() || self.error.is_some() || self.new_size.is_some() {
            return;
        }
        if self.job.is_some() {
            self.status = "Finish the current file operation first".into();
            return;
        }
        self.editor.finish_edit();
        if self.editor.dirty && !matches!(action, Action::Open(_, true)) {
            self.pending = Some(action);
        } else {
            self.execute(action, ctx);
        }
    }

    fn execute(&mut self, action: Action, ctx: &egui::Context) {
        match action {
            Action::Exit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Action::Replace(loaded) => self.replace_loaded(loaded),
            Action::New([width, height]) => match Document::blank(width, height) {
                Ok(document) => {
                    self.replace_loaded(Loaded { document, path: None });
                    self.editor.mark_unsaved();
                    self.status = "New transparent canvas · add an image layer to start".into();
                }
                Err(error) => self.error = Some(error.to_string()),
            },
            Action::Open(path, add) => {
                let revision = self.editor.revision;
                self.status = "Opening locally…".into();
                self.start_job(ctx, move || {
                    let path = path.or_else(|| {
                        let dialog = rfd::AsyncFileDialog::new();
                        let dialog = if add {
                            dialog.add_filter("Images", &["png", "jpg", "jpeg"])
                        } else {
                            dialog.add_filter("Projects and images", &["vibe", "png", "jpg", "jpeg"])
                        };
                        pollster::block_on(dialog.pick_file()).map(|file| file.path().to_path_buf())
                    });
                    match path {
                        Some(path) => Ok(Job::Opened(load(&path, add)?, add, revision)),
                        None => Ok(Job::Cancelled),
                    }
                });
            }
        }
    }

    pub(super) fn save_project(&mut self, save_as: bool, ctx: &egui::Context) {
        if self.job.is_some() || self.error.is_some() {
            return;
        }
        self.editor.finish_edit();
        let document = self.editor.document.clone();
        let state = self.editor.state_id();
        let current_path = if save_as { None } else { self.project_path.clone() };
        self.status = "Saving editable project…".into();
        self.start_job(ctx, move || {
            let path = current_path.or_else(|| {
                pollster::block_on(rfd::AsyncFileDialog::new().add_filter("Vibeshop project", &["vibe"]).set_file_name("Untitled.vibe").save_file())
                    .map(|file| file.path().to_path_buf())
            });
            let Some(path) = path else { return Ok(Job::Cancelled); };
            ensure!(has_extension(&path, "vibe"), "Choose a filename ending in .vibe. The selected path will not be silently changed.");
            project::save(&path, &document)?;
            Ok(Job::Saved(path, state))
        });
    }

    pub(super) fn export(&mut self, ctx: &egui::Context) {
        if self.job.is_some() || self.pending.is_some() || self.error.is_some() || self.new_size.is_some() || !self.render() {
            return;
        }
        let snapshot = match self.gpu.readback() {
            Ok(snapshot) => snapshot,
            Err(error) => { self.error = Some(error.to_string()); return; }
        };
        self.status = "Exporting flattened PNG…".into();
        self.start_job(ctx, move || {
            let Some(file) = pollster::block_on(rfd::AsyncFileDialog::new().add_filter("PNG", &["png"]).set_file_name("vibeshop.png").save_file()) else {
                return Ok(Job::Cancelled);
            };
            let path = png_destination(file.path())?;
            let (width, height) = (snapshot.width, snapshot.height);
            let pixels = snapshot.finish()?;
            image_io::save_png(&path, width, height, &pixels)?;
            Ok(Job::Exported(path))
        });
    }

    fn start_job(&mut self, ctx: &egui::Context, work: impl FnOnce() -> Result<Job> + Send + 'static) {
        let (tx, rx) = mpsc::sync_channel(1);
        self.job = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            // A failed worker must wake the event-driven UI, including on a panic.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
                .unwrap_or_else(|_| Err(anyhow::anyhow!("File operation stopped unexpectedly")));
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    pub(super) fn poll_job(&mut self, ctx: &egui::Context) {
        let result = match self.job.as_ref().map(|job| job.try_recv()) {
            Some(Ok(result)) => result,
            Some(Err(TryRecvError::Disconnected)) => Err(anyhow::anyhow!("File worker disconnected")),
            _ => return,
        };
        self.job = None;
        match result {
            Ok(Job::Opened(mut loaded, add, revision)) => {
                let limit = self.gpu.device.limits().max_texture_dimension_2d;
                let document = &loaded.document;
                if document.width > limit || document.height > limit || document.layers.iter().any(|layer| layer.source.width > limit || layer.source.height > limit) {
                    self.error = Some(format!("Project exceeds this GPU's {limit}px texture limit"));
                    return;
                }
                if add {
                    match loaded.document.layers.pop().context("The imported image has no layer").and_then(|layer| self.editor.add_layer(layer)) {
                        Ok(()) => self.status = "Image added as an editable layer".into(),
                        Err(error) => self.error = Some(error.to_string()),
                    }
                } else if self.editor.revision != revision {
                    self.pending = Some(Action::Replace(loaded));
                    self.status = "Your work changed while opening · save or discard before replacing it".into();
                } else {
                    self.replace_loaded(loaded);
                }
            }
            Ok(Job::Saved(path, state)) => {
                self.editor.mark_saved(state);
                self.project_path = Some(path);
                if self.editor.dirty {
                    self.status = "Snapshot saved · newer changes are still unsaved".into();
                } else {
                    self.status = "Project saved · all layers remain editable".into();
                    if let Some(action) = self.pending.take() {
                        self.execute(action, ctx);
                    }
                }
            }
            Ok(Job::Exported(path)) => self.status = format!("Exported {} · project save state is unchanged", path.file_name().unwrap_or_default().to_string_lossy()),
            Ok(Job::Cancelled) => self.status = "File operation cancelled · your work is unchanged".into(),
            Err(error) => {
                self.status = "File operation failed · your work remains open".into();
                self.error = Some(format!("{error:#}"));
            }
        }
    }

    fn replace_loaded(&mut self, loaded: Loaded) {
        self.editor.replace(loaded.document);
        self.project_path = loaded.path;
        self.fit = true;
        self.move_start = None;
        self.status = if self.project_path.is_some() { "Editable project opened locally" } else { "Image opened locally · save a project to keep your edits" }.into();
    }

    pub(super) fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some(mut size) = self.new_size {
            let mut create = false;
            let mut cancel = false;
            egui::Modal::new(egui::Id::new("new-canvas")).show(ctx, |ui| {
                ui.heading("New canvas");
                ui.horizontal(|ui| {
                    ui.label("Width");
                    ui.add(egui::DragValue::new(&mut size[0]).range(1..=8192));
                    ui.label("Height");
                    ui.add(egui::DragValue::new(&mut size[1]).range(1..=8192));
                });
                let valid = vibeshop::document::validate_size(size[0], size[1]);
                if let Err(error) = &valid { ui.label(error.to_string()); }
                ui.horizontal(|ui| {
                    create = ui.add_enabled(valid.is_ok(), egui::Button::new("Create canvas")).clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });
            self.new_size = Some(size);
            if cancel || create { self.new_size = None; }
            if create { self.request(Action::New(size), ctx); }
        }
        if self.pending.is_some() {
            let mut save = false;
            let mut discard = false;
            let mut cancel = false;
            egui::Modal::new(egui::Id::new("unsaved-work")).show(ctx, |ui| {
                ui.set_max_width(420.0);
                ui.heading("Save your changes?");
                ui.label("Save a .vibe project to preserve all layers and adjustments. PNG export is a flattened copy, not an editable project.");
                ui.add_space(14.0);
                if self.job.is_some() { ui.label("Saving project…"); }
                ui.add_enabled_ui(self.job.is_none(), |ui| {
                    ui.horizontal(|ui| {
                        save = ui.button("Save and continue").clicked();
                        discard = ui.button("Discard changes").clicked();
                        cancel = ui.button("Cancel").clicked();
                    });
                });
            });
            if cancel { self.pending = None; }
            if discard && let Some(action) = self.pending.take() { self.execute(action, ctx); }
            if save { self.save_project(false, ctx); }
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
    }
}

fn has_extension(path: &Path, extension: &str) -> bool {
    path.extension().and_then(|value| value.to_str()).is_some_and(|value| value.eq_ignore_ascii_case(extension))
}

fn load(path: &Path, add: bool) -> Result<Loaded> {
    if has_extension(path, "vibe") {
        ensure!(!add, "Open a project directly; adding an entire project as one layer is not supported");
        Ok(Loaded { document: project::open(path)?, path: Some(path.to_path_buf()) })
    } else {
        Ok(Loaded { document: Document::new(image_io::open(path)?), path: None })
    }
}
