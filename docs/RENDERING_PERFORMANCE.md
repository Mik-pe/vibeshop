# Compositor measurement

Build `cargo build --locked --release --example compositor_bench`, then run
`scripts/measure-compositor.py target/release/examples/compositor_bench` on an
otherwise idle machine. Record the exact commit, OS, adapter/driver/backend and
build mode with the output. The generated 3840×2160 workload has one full-canvas
raster background and one 256×256 translucent patch. Each workload warms up for
10 edits, then measures 100 alternating exposure changes, first to the background
and then to the patch. p50 and p95 are nearest-rank percentiles.

Timing begins before `Engine::render` and ends after the GPU queue completes.
This includes CPU submission and GPU work, but excludes input dispatch, egui,
presentation and display scanout. It is **not input-to-visible latency**. Process
peak RSS is read from Linux `getrusage` for the benchmark subprocess, excluding the
compiler. RSS is not total GPU memory; texture payload calculations also exclude
driver padding, command buffers and driver allocations. Do not compare software
Vulkan results against physical GPU speed claims.

## Tile evaluation

The compositor reuses two 512×512 RGBA16F scratch textures (4 MiB total). Each
output tile records an ordered list of intersecting layers' pixel-affecting
properties and source identities. Only tiles whose list changes are cleared,
composited and encoded. Moving a layer invalidates both its old and new bounds;
removing/hiding a layer clears its previous pixels. Renaming a layer does not
invalidate pixels. No old source assets are retained by the tile signatures.

Source textures, display/export textures and export readback remain full-image
allocations within the existing 16 MP / 8192px document limits. This is a first
slice of issue #3, not completion of its large-document memory contract. Bounded
source residency, streaming export, recoverable GPU allocation/device loss and
24 MP / larger-than-adapter workloads remain outstanding. Raising image limits
before that work would be misleading.
