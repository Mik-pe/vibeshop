use std::time::{Duration, Instant};
use vibeshop::{
    document::{Document, Layer, Source},
    gpu::Engine,
    image_io,
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
    let mut pixels = Vec::with_capacity(3840 * 2160 * 4);
    for y in 0..2160 {
        for x in 0..3840 {
            pixels.extend_from_slice(&[
                (x % 256) as u8,
                (y % 256) as u8,
                ((x + y) % 256) as u8,
                if (x + y) % 2 == 0 { 128 } else { 255 },
            ]);
        }
    }
    let document = Document::new(Layer::new(
        "Generated export",
        Source::new(3840, 2160, pixels)?,
    ));
    engine.render(&document)?;
    engine.device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(Duration::from_secs(30)),
    })?;
    let directory = tempfile::tempdir_in(std::env::current_dir()?)?;
    let path = directory.path().join("export.png");
    let mut samples = Vec::new();
    for n in 0..12 {
        let start = Instant::now();
        let snapshot = engine.readback()?;
        image_io::save_png_snapshot(&path, snapshot)?;
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        println!("iteration={} elapsed_ms={:.3}", n, elapsed);
        if n >= 2 {
            samples.push(elapsed);
        }
    }
    samples.sort_by(f64::total_cmp);
    println!(
        "workload=export_pattern dimensions=3840x2160 layers=1 warmup=2 samples=10 p50_ms={:.3} p95_ms={:.3} bytes={} timing=snapshot_to_atomic_png_complete",
        samples[4],
        samples[9],
        std::fs::metadata(path)?.len()
    );
    Ok(())
}
