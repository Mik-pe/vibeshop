mod studio;
fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1440.0, 940.0]).with_min_inner_size([1000.0, 640.0]),
        ..Default::default()
    };
    eframe::run_native("Vibeshop", options, Box::new(|cc| Ok(Box::new(studio::Studio::new(cc)?))))
}
