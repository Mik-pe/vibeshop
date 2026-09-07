//! Generated 4K workload; GPU completion latency, not input-to-display latency.
use std::time::{Duration, Instant};
use vibeshop::{
    document::{Document, Layer, Source},
    gpu::Engine,
};
fn main() -> anyhow::Result<()> {
    let adapter =
        pollster::block_on(wgpu::Instance::default().request_adapter(&Default::default()))?;
    println!(
        "adapter={:?} os={} release={}",
        adapter.get_info(),
        std::env::consts::OS,
        !cfg!(debug_assertions)
    );
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))?;
    let mut engine = Engine::new(device, queue);
    let mut document = Document::new(Layer::new(
        "Generated background",
        Source::new(3840, 2160, [80, 120, 160, 255].repeat(3840 * 2160))?,
    ));
    let mut patch = Layer::new(
        "Generated patch",
        Source::new(256, 256, [200, 90, 50, 192].repeat(256 * 256))?,
    );
    patch.offset = [1700, 900];
    document.layers.push(patch);
    for (name, index) in [("full_canvas_adjustment", 0), ("local_layer_adjustment", 1)] {
        let mut samples = Vec::new();
        for n in 0..110 {
            document.layers[index].exposure = if n % 2 == 0 { 0.5 } else { 0.0 };
            let start = Instant::now();
            engine.render(&document)?;
            engine.device.poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(30)),
            })?;
            if n >= 10 {
                samples.push(start.elapsed().as_secs_f64() * 1000.0);
            }
        }
        samples.sort_by(f64::total_cmp);
        println!(
            "workload={name} dimensions=3840x2160 layers=2 warmup=10 samples=100 p50_ms={:.3} p95_ms={:.3} uploads={}",
            samples[49], samples[94], engine.uploads
        );
    }
    Ok(())
}
