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

## Measured 4K result (2026-09-07)

Baseline compositor: `5897147`. Tiled implementation: `81d4081`.
Both used the same `examples/compositor_bench.rs` workload and the locked release
profile (thin LTO). Linux 7.1.9-arch1-2; Intel Iris Plus Graphics (ICL GT2), Intel
Mesa 26.2.1-arch1.1, Vulkan. Other repository GPU tests were paused for the pair.
These are one matched pair of 100-sample runs, not a regression threshold.

| Workload | Baseline p50 / p95 | Tiled p50 / p95 |
| --- | --- | --- |
| Full-canvas exposure | 14.161 / 15.277 ms | 11.606 / 11.902 ms |
| Local-layer exposure | 14.335 / 15.845 ms | 3.366 / 3.582 ms |
| Process peak RSS (both workloads) | 89,600 KiB | 100,884 KiB |

Both runs uploaded exactly two sources. At this resolution, scratch texture
payload falls from 126.56 MiB to 4 MiB; display/export payload remains 63.28 MiB.
Measured process RSS increases by about 11 MiB despite the smaller GPU scratch
payload. RSS does not account for all driver-managed GPU memory, and these
results do not establish a total-memory bound or peak GPU residency measurement.
The two small generated layers do not represent every professional workload.

The initial tile candidate used a render-pass clear per tile and regressed
full-canvas timing. The final shader starts the first intersecting layer with a
transparent backdrop and explicitly clears empty output tiles. The old
whole-image path and the extra clear pass are removed; there is one compositor.
