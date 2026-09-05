# Vibeshop

A local-first photo editor built in **Rust + wgpu**. A charcoal workspace, a large canvas, and GPU image operations with editable layers.

## Run

Install Rust through rustup, then:

```sh
cargo run --locked --release
# Open an image or editable project directly:
cargo run --locked --release -- /path/to/work.vibe
```

On Ubuntu/Debian, install the native build/runtime dependencies first:

```sh
sudo apt-get install build-essential pkg-config libxkbcommon-dev libxkbcommon-x11-0 libwayland-dev libvulkan1 mesa-vulkan-drivers
```

A working Vulkan, Metal, or DirectX 12 adapter is required. Linux file dialogs use the desktop's XDG portal. The application starts with an original generated dune study; no sample download, account, API key, or network service is needed. Linux is the initial validation platform; macOS and Windows still need the runtime verification in issue #10.

## Edit and keep your work

Open PNG/JPEG, drop an image or .vibe project onto the canvas, or create a transparent canvas from File → New canvas. Add, duplicate, delete, reorder, hide, move and blend raster layers. Adjust exposure, contrast, saturation and opacity without modifying source pixels. Pan and cursor-anchored zoom reuse the rendered GPU image. Undo/redo coalesces a drag into one edit.

**Save project** writes a self-contained `.vibe` file that preserves source pixels, shared assets, layer settings and canvas dimensions. Reopen it to continue editing. **Export PNG** writes a flattened copy and does not mark the project saved. Closing or replacing an edited document offers save/discard/cancel. A failed save leaves the previous file intact; edits made while saving remain unsaved. Save regularly: automatic crash recovery is not implemented.

`Ctrl/Cmd+N` creates a canvas. `Ctrl/Cmd+O` opens a project or image; `Ctrl/Cmd+Shift+O` adds an image layer. `Ctrl/Cmd+S` saves the project, `Ctrl/Cmd+Shift+S` saves as, and `Ctrl/Cmd+Shift+E` exports PNG. `Ctrl/Cmd+Z` undoes and `Ctrl/Cmd+Shift+Z` redoes. `H` selects pan, `V` moves the selected layer, and Space temporarily pans. Scroll zooms; `F` or double-click fits the image. Click the zoom percentage for 100% physical-pixel display.

**This is an early editor, not Photoshop parity.** Masks, brushes, crop/rotation, PSD, RAW, ICC color management, automatic recovery and a browser build are not implemented. Tagged ICC images are rejected. Input is reduced to RGBA8 and interpreted as sRGB. Limits are 16 megapixels, 8192px per side, 16 layers and 128 MiB of distinct retained source pixels. Project version 1 stores raw assets rather than compressed archives. Keep original files and read [the architecture](docs/ARCHITECTURE.md) and [project format](docs/PROJECT_FORMAT.md).

## Verify

```sh
scripts/check.sh
# Linux native interaction checks in an isolated X11 display:
sudo apt-get install xvfb xdotool imagemagick
scripts/smoke.sh
```

Tests execute the production GPU shaders, compare pixels, round-trip project assets/settings and exercise the actual file controller's open → edit → keyboard save → fresh-editor reopen → PNG path. They also cover malformed/truncated projects, interrupted writes, saved-state undo/redo and late or cancelled file results. Native file-picker interaction is separate from these controller tests.

The smoke script launches the real editor, moves a layer with native input, checks undo/redo against captured canvas pixels, changes exposure and verifies undo again. It writes captures to `artifacts/`. Inspect them and exercise changed controls; the fixed-layout smoke test is not comprehensive UI coverage. Missing GPU or GUI prerequisites fail validation rather than becoming skipped passing tests. Hardware correctness is not a performance benchmark.

CI uses the dedicated self-hosted runner for trusted branch pushes. Read [runner setup and the trust boundary](docs/RUNNER.md) before connecting a machine. External PRs are not automatically scheduled on that runner by this workflow.

## Agent-only development

Start at [AGENTS.md](AGENTS.md). GitHub issues are the backlog. Existing scheduled coding agents can run either instruction:

```text
Read AGENTS.md and execute .agents/skills/issue-worker/SKILL.md.
```

```text
Read AGENTS.md and execute .agents/skills/pr-triage/SKILL.md.
```

The issue worker ships a tested vertical slice as a PR. PR triage independently reviews/merges ready work and repairs rejected, drifted or failing PRs. Expiring Git-backed per-task leases coordinate agents across machines. There is no extra scheduler, database, model API dependency or lock service. Schedules and credentials belong to the chosen runner. Server-side branch protection and independent review identities are tracked in issue #11 and require owner configuration.
