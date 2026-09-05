# Vibeshop

A local-first photo editor built in **Rust + wgpu**. A quiet charcoal workspace, a big canvas, and actual GPU editing—not a web mockup around a CPU image loop.

## Run

Install Rust with rustup, then:

```sh
cargo run --locked --release
```

On Ubuntu/Debian, install the native build/runtime dependencies first:

```sh
sudo apt-get install build-essential pkg-config libxkbcommon-dev libwayland-dev libvulkan1 mesa-vulkan-drivers
```

A working Vulkan, Metal, or DirectX 12 adapter is required. Linux file dialogs use your desktop's XDG portal. The native application starts with an original generated dune study; no sample download, account, API key, or network service is required.

## What this build does

Open PNG/JPEG or drop a file onto the canvas. Add, duplicate, delete, reorder, hide, move, and blend raster layers. Adjust exposure, contrast, saturation, and opacity without altering original pixels. Pan and cursor-anchored zoom reuse the existing GPU image. Undo/redo coalesces a drag into one edit. Export the composed image to PNG.

`Ctrl/Cmd+O` opens an image; `Ctrl/Cmd+Shift+O` adds a layer; `Ctrl/Cmd+S` exports PNG. `Ctrl/Cmd+Z` undoes and `Ctrl/Cmd+Shift+Z` redoes. `H` selects pan, `V` moves the selected layer, and Space temporarily pans. Scroll zooms; `F` or double-click fits the image. Click the zoom percentage for 100% physical-pixel display.

**This is an early editor, not Photoshop parity.** Editable projects, recovery, masks, brushes, crop/rotation, PSD, RAW, ICC color management, and a browser build are not implemented yet. Export is flattened; editable layers currently exist only in memory. Tagged ICC images are explicitly rejected. Inputs are reduced to RGBA8 and interpreted as sRGB. Limits are 16 megapixels, 8192px per side, and 16 layers. Keep original files and read [the architecture and limits](docs/ARCHITECTURE.md).

## Verify

```sh
scripts/check.sh
# Linux GUI smoke capture (also needs xvfb):
scripts/smoke.sh
```

The checks include real GPU pixel comparisons; no adapter means failure, not a silently skipped test. The smoke command launches the actual editor and writes `artifacts/studio.png`. Inspect it and exercise changed controls before claiming UI behavior works. Mesa CI verifies correctness, not performance leadership.

## Agent-only development

Start at [AGENTS.md](AGENTS.md). GitHub issues are the backlog. Scheduled coding agents can run either of these instructions directly:

```text
Read AGENTS.md and execute .agents/skills/issue-worker/SKILL.md.
```

```text
Read AGENTS.md and execute .agents/skills/pr-triage/SKILL.md.
```

The issue worker ships a tested vertical slice as a PR. PR triage independently reviews/merges ready work and repairs rejected, drifted, or failing PRs. Expiring Git-backed per-task leases coordinate agents across machines. There is no extra scheduler, database, model API dependency, or hidden lock service. Schedules and credentials belong to your chosen agent runner; this repository does not secretly enable them.

[GitHub issues](https://github.com/Mik-pe/vibeshop/issues) track the next complete user workflows. Unsupported features stay out of the UI until they work.
