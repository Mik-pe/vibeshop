# One editor, one pixel pipeline

## Current shape

One Rust package contains `document` (editable state/history), `gpu` (wgpu resources/WGSL), `image_io` (bounded image decode/PNG encode) and `project` (editable .vibe serialization). PNG and project saves share `storage::write_atomic`. `studio` is the native egui/eframe application; its file-operation controller lives in `studio/files.rs`. Do not split this into a crate forest without a concrete boundary that needs it.

The document stores bottom-to-top raster layers sharing immutable source pixels through `Arc`. A layer has visibility, opacity, translation, blend mode and non-destructive exposure/contrast/saturation. Undo stores document metadata and shared sources; one slider/move gesture is one history entry. The UI changes its render revision only when document pixels change. Pan, zoom and idle frames do not recompose the image.

Saved-state identity is separate from render revision. Undo/redo restores prior state identities while render revisions increase. Saving marks only the snapshot actually written, so later edits remain dirty. The saved marker retains no extra source pixels. Open results check the requested revision before replacing the active document; late results require an explicit decision. Save/discard/cancel protects dirty document replacement and closing. PNG export is never an editable save. See [project format and limits](PROJECT_FORMAT.md).

Source pixels are uploaded once per active source identity. Interactive color adjustment and compositing happens in WGSL. Two RGBA16F scratch textures ping-pong through visible layers, then a final pass produces display and export textures. This is a straightforward layer loop, not a speculative graph compiler. Dirty tiles and resource budgets are the next scaling work, not a second renderer.

## Color and alpha contract

Inputs are currently 8-bit, untagged sRGB PNG/JPEG. Embedded ICC profiles are rejected rather than silently misinterpreted. EXIF orientation is applied during decode. Higher-bit-depth input is reduced to 8-bit; this is not a professional color-managed or HDR workflow. Version 1 projects preserve these decoded RGBA8 assets, not the original encoded file or discarded metadata.

WGSL decodes sRGB to linear light, applies tone adjustments, and composites premultiplied RGB/alpha into RGBA16F. Source-over normal, multiply and screen have pixel tests. Final output converts straight linear RGB to sRGB. egui receives gamma-encoded premultiplied RGBA8; PNG receives straight-alpha RGBA8. Fully transparent exported RGB is normalized to zero. Do not apply sRGB transfer functions twice or blend source layers in gamma space.

Display and PNG are produced from the same composition. Export copies that revision to a staging buffer before encoding, strips GPU row padding and uses a temporary file before replacement. Readback and image codecs run off the UI thread. No readback happens on ordinary rendering. Failed rendering invalidates export rather than returning pixels from an older revision. Project saving does not need GPU readback.

## Explicit limits, not performance claims

Limits are 8192px per dimension, 16 megapixels per image, 16 layers, 64 MiB encoded image input and 128 MiB of distinct retained source pixels. Undo retains at most 32 entries and evicts oldest entries to respect its source budget. These are not total process/GPU memory guarantees: composition targets, active IO snapshots and staging buffers add memory. GPU allocation failure/device loss and very large documents need further hardening.

At 16 megapixels, two 16F targets plus two RGBA8 outputs alone use about 384 MiB. A tiled renderer must replace whole-image targets before large-document support is claimed. Do not raise limits without measuring memory and interaction latency.

Performance targets on named reference hardware: no composition/uploads during pan or zoom; no unnecessary idle redraw loop; input-to-visible-edit p95 below 16.7ms for an agreed workload; bounded allocations and no full-image CPU copies per slider tick. These are targets, not results. Report resolution, layers, adapter, backend, build mode, warm-up, sample count, p50/p95 and peak memory.

## Scope and growth

The working loop is open → layer/color edit → undo → save editable project → reopen → export. It is not Photoshop parity. Crash recovery and trustworthy large-image/color handling take priority over expanding the tool shelf. Selections/masks, crop/affine transforms, pressure-aware painting and richer adjustments should be complete slices through the same document and compositor. PSD and browser support require an honest tested compatibility matrix, not unused alternate systems.

The editor has no service, account system, AI dependency, extension marketplace, custom event bus or autonomous runtime. Agent-first describes repository development, not sending users' photos to a model.

## Verification

Document and project tests check history, sharing, saved-state semantics, invalid input and file round trips. GPU tests execute actual wgpu compute and check pixels, alpha, blending, offsets, odd-width readback padding and upload reuse. File-controller tests use the production application/controller with an actual adapter, including keyboard save, a fresh editor on reopen and PNG pixel equality. Native file pickers are not exercised by these controller tests.

`scripts/smoke.sh` launches the actual native GUI and compares captured pixels after pointer/keyboard actions. CI uses the self-hosted runner and records the actual adapter; [runner setup](RUNNER.md) describes prerequisites and security. An unavailable adapter or missing GUI test prerequisite is a failing environment, not a skipped success. Software Vulkan may verify correctness but cannot establish physical-hardware performance.
