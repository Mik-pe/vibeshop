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

## Tone panel and live histogram cost (2026-09-07)

Baseline `4f08c04`; tone/histogram production code `395dcd9`. Same locked release
benchmark, adapter/driver/backend, warm-up and sample count described above;
no other repository GPU tests or CI jobs ran during this matched pair.

| Workload | Main p50 / p95 | Tone + histogram p50 / p95 |
| --- | --- | --- |
| Full-canvas exposure | 11.513 / 11.872 ms | 15.752 / 17.047 ms |
| Local-layer exposure | 3.343 / 3.559 ms | 4.238 / 4.449 ms |
| Process peak RSS | 100,804 KiB | 111,028 KiB |

Both uploaded exactly two sources. This measures the cost of keeping the live
histogram current during ordinary exposure edits, including neutral curve LUTs;
it does not measure an active curve drag or input-to-visible latency. The full
edit p95 exceeds the 16.7 ms interaction target even before presentation. This is
an explicit remaining performance limitation, not Photoshop parity or a claim
that the new controls are free. Process RSS is not total GPU residency and no
per-event allocation count was measured.

The first separate histogram pass measured 32.537 ms full-edit p95. The final
implementation shares the final linear-pixel load with the existing encode pass,
processes 32×32 regions per 64-lane workgroup, and coalesces repeated bins within
each lane. Only changed tiles replace their cached bins; a small GPU reduction
sums them before the bounded 4 KiB asynchronous readback. There is no second
compositor or full-image CPU histogram. Histograms add at most 1 MiB of cached
GPU counters under the existing dimension limits. Timing this generated,
mostly uniform workload does not establish results for arbitrary photos.

An additional candidate-only run at `b3f423b` adds
`full_canvas_levels_curve`: after the two exposure scenarios, alternate the
background's gamma between 1.2/1.0 and its RGB curve midpoint between 0.4/0.5.
The same generated document, 10 warm-up / 100 samples and render-to-GPU-completion
timer include per-edit LUT regeneration/upload and active tone evaluation.
This scenario measured p50 **17.240 ms**, p95 **18.292 ms**. The same run's
exposure p95 values were 16.972 ms full and 4.410 ms local; process peak RSS across
all three scenarios was 103,828 KiB. This is a separate run, not a replacement
for the matched comparison above. Document mutation before `Engine::render`,
UI input, presentation and per-event allocation counts remain unmeasured.


## Bounded PNG export workload

`examples/export_bench.rs` measures a generated 3840×2160 raster layer whose
RGBA values are `(x%256, y%256, (x+y)%256, alternating 128/255 alpha)`. Initial
composition and its GPU wait happen before timing. Each sample freezes the
current output and finishes an atomic PNG write, including file sync/replacement.
There are two warm-up exports and ten measured exports; p50/p95 use nearest ranks
5/10 of the sorted samples. The temporary destination is on the current working
directory's filesystem. This measures export completion, not input-to-display
latency or the native file picker.

Build first, then reuse the existing Linux RSS wrapper so compiler memory is
excluded:

```sh
cargo build --locked --release --example export_bench
scripts/measure-compositor.py target/release/examples/export_bench
```

For the `4905228` baseline, use the same generated workload and timing loop,
replacing `save_png_snapshot(&path, snapshot)` with
`let rgba = snapshot.finish()?; image_io::save_png(&path, 3840, 2160, &rgba)?;`.
Both use PNG Fast compression and Adaptive filtering. Streamed IDAT chunking
can change compressed bytes without changing decoded pixels. Run binaries
sequentially on the same filesystem, with repository GPU tests/benchmarks idle.

The production path retains an immutable full-image RGBA8 GPU snapshot (up to
64 MiB under current document limits), one reusable transfer buffer of at most
1 MiB, PNG's 64 KiB output chunks and row/filter/compression workspaces. It does
not collect full-image CPU pixels. Process RSS includes document/renderer/driver
and codec memory; it is not a measurement of total GPU residency. Large-document
limits and device-allocation failure handling remain work in #3.

Measured on 2026-09-07, Intel Iris Plus Graphics (ICL GT2), Mesa 26.2.1,
Vulkan, Linux 7.1.9-arch1-2 / Omarchy 4.0.2, release builds and the same local
filesystem. Each row is a separate process with the protocol above; builds and
repository GPU tests were stopped during measurements. RSS was captured with
Python `resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss`, as in the wrapper.

| Run order / implementation | p50 ms | p95 ms | Peak RSS KiB | PNG bytes |
| --- | ---: | ---: | ---: | ---: |
| 1. Baseline `4905228` | 2023.851 | 2313.387 | 153684 | 19701416 |
| 2. Initial stream, 4 KiB chunks `105d6ea` | 3331.659 | 3758.227 | 101560 | 19759124 |
| 3. Final stream, 64 KiB chunks `39a6730` | 152.790 | 249.191 | 102092 | 19705016 |
| 4. Baseline repeat, same binary | 123.269 | 135.433 | 154232 | 19701416 |
| 5. Final stream repeat, same binary | 2341.176 | 2735.832 | 101876 | 19705016 |

The unchanged binaries vary dramatically in completion time under this desktop
and storage environment. These samples do **not** establish a speed improvement
or a stable latency regression estimate, and are not a timing gate. The closest
fast pair still shows slower streamed completion. The consistent result is lower
peak process RSS: roughly 102,000 KiB streamed versus 154,000 KiB baseline (about
34% less) on this fixture. This does not establish memory or speed behavior for
all photos. The final implementation uses bounded 64 KiB chunks to avoid the
initial stream's many small destination writes; no additional caching or
parallel encoding was introduced.
