# One editor, one pixel pipeline

## Current shape

A single Rust package has three independently testable modules: `document` (editable state and history), `gpu` (wgpu resources and WGSL), and `image_io` (bounded decode and atomic PNG export). `studio` is the native egui/eframe application. Do not split this into a crate forest without a concrete boundary that needs it.

The document stores bottom-to-top raster layers sharing immutable source pixels through `Arc`. A layer has visibility, opacity, translation, blend mode, and non-destructive exposure/contrast/saturation. Undo stores document metadata and shared sources; one slider/move gesture is one history entry. The UI changes a revision only when document pixels change. Pan, zoom, and idle frames do not recompose the image.

Source pixels are uploaded once per active source identity. All interactive color adjustment and compositing happens in WGSL. Two RGBA16F scratch textures ping-pong through visible layers, then a final pass produces a display texture and export texture. This deliberately starts with a straightforward layer loop, not a speculative graph compiler. Dirty tiles and resource budgets are the next scaling work, not a second renderer.

## Color and alpha contract

Inputs are currently 8-bit, untagged sRGB PNG/JPEG. Embedded ICC profiles are rejected rather than silently misinterpreted. EXIF orientation is applied during decode. Higher-bit-depth input is currently reduced to 8-bit; this build is not a professional color-managed or HDR workflow.

WGSL decodes sRGB to linear light, applies tone adjustments, and composites premultiplied RGB/alpha into RGBA16F. Source-over normal, multiply, and screen have explicit pixel tests. Final output converts straight linear RGB to sRGB. egui receives gamma-encoded premultiplied RGBA8; PNG export receives straight-alpha RGBA8. Fully transparent RGB is normalized to zero. Do not apply sRGB transfer functions twice or blend source layers in gamma space.

Display and PNG are produced from the same composition. Export copies that revision to a staging buffer before encoding, strips the required GPU row padding, and uses a temporary file in the destination directory before replacement. Readback and image codecs must not block pointer interaction. There is no readback on ordinary rendering.

## Explicit limits, not performance claims

The initial limits are 8192px per dimension, 16 megapixels per image, 16 layers, 64 MiB encoded input, and 128 MiB of distinct retained source pixels. Undo retains at most 32 entries and evicts oldest entries to respect the source budget. These are safety limits, not total process or GPU memory guarantees: composition targets and staging buffers add memory. GPU allocation failure/device loss and very large documents still need hardened handling.

At 16 megapixels, two 16F targets plus two RGBA8 outputs alone use about 384 MiB. A tiled renderer must replace whole-image targets before we claim large-document support. Do not quietly raise limits without measuring memory and interaction latency.

Performance goals to validate on named hardware: no composition/uploads during pan or zoom; no unnecessary idle redraw loop; input-to-visible-edit p95 below 16.7ms for the agreed reference workload; bounded allocations and no full-image CPU copies per slider tick. These are targets, not results. Report resolution, layers, adapter, backend, build mode, warm-up, sample count, p50/p95, and peak memory.

## Scope and growth

The first loop is open → layer/color edit → undo → export. It is not Photoshop parity. Editable project files and crash recovery come before expanding the tool shelf. Then color management, tiled evaluation, selections/masks, crop/affine transforms, pressure-aware painting, and richer adjustments can be added as complete slices. PSD and browser support must use the same document/compositor and publish an honest compatibility matrix. Do not scaffold unused alternate systems now.

The editor has no service, account system, AI dependency, extension marketplace, custom event bus, or autonomous runtime. Agent-first describes repository development, not a requirement to send users' photos to a model.

## Verification

`tests/document.rs` checks history semantics, sharing, invalid input, and PNG round trips. `tests/gpu.rs` runs actual wgpu compute on an available adapter and checks image pixels, alpha, blend modes, bounds, odd-width readback padding, and upload reuse. CI installs Mesa Vulkan for deterministic headless availability; software rendering cannot establish hardware performance. `scripts/smoke.sh` launches the actual GUI and captures it, rather than rendering a mockup.
