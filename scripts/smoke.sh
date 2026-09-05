#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ "${1:-}" != --in-xvfb ]]; then
    missing=0
    for tool in xvfb-run xauth xdotool import convert; do
        if ! command -v "$tool" >/dev/null; then echo "Missing UI test tool: $tool (see docs/RUNNER.md)" >&2; missing=1; fi
    done
    (( missing == 0 )) || exit 1
    exec xvfb-run -a -s '-screen 0 1600x1100x24' scripts/smoke.sh --in-xvfb
fi
cargo build --locked
mkdir -p artifacts
rm -f artifacts/studio.png artifacts/moved.png artifacts/undone.png artifacts/redone.png artifacts/exposure.png artifacts/restored.png
unset EFRAME_SCREENSHOT_TO WAYLAND_DISPLAY
export WINIT_X11_SCALE_FACTOR=1
# Xvfb has no DRI3. Copy presentation through Mesa's software WSI path while
# keeping the same Vulkan image operations. This is not a latency benchmark.
export MESA_VK_WSI_DEBUG=sw
"${CARGO_TARGET_DIR:-target}/debug/vibeshop" >artifacts/studio.log 2>&1 &
pid=$!
trap 'kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true' EXIT
window=''
for _ in $(seq 1 100); do
    kill -0 "$pid" 2>/dev/null || { cat artifacts/studio.log; exit 1; }
    window=$(xdotool search --onlyvisible --name '^Vibeshop$' 2>/dev/null | head -n 1 || true)
    [[ -n "$window" ]] && break
    sleep 0.1
done
[[ -n "$window" ]] || { echo 'Editor window did not appear' >&2; cat artifacts/studio.log; exit 1; }
xdotool windowsize "$window" 1440 940
xdotool windowfocus --sync "$window"
# Fixed layout and generated pixels make this a native-input correctness check.
sleep 2
capture() {
    sleep 0.8
    kill -0 "$pid" 2>/dev/null || { cat artifacts/studio.log; exit 1; }
    import -window "$window" "artifacts/$1.png"
    test -s "artifacts/$1.png"
}
canvas_hash() {
    convert "artifacts/$1.png" -crop 950x620+125+195 +repage -depth 8 RGBA:- | sha256sum | cut -d ' ' -f 1
}
capture studio
original=$(canvas_hash studio)
xdotool key --clearmodifiers v
xdotool mousemove --window "$window" 400 400
xdotool mousedown 1
sleep 0.2
xdotool mousemove --sync --window "$window" 480 430
sleep 0.2
xdotool mouseup 1
capture moved
moved=$(canvas_hash moved)
[[ "$moved" != "$original" ]] || { echo 'Moving a layer did not change the visible canvas' >&2; exit 1; }
xdotool key --clearmodifiers ctrl+z
capture undone
[[ "$(canvas_hash undone)" == "$original" ]] || { echo 'Undo did not restore the canvas pixels' >&2; exit 1; }
xdotool key --clearmodifiers ctrl+shift+z
capture redone
[[ "$(canvas_hash redone)" == "$moved" ]] || { echo 'Redo did not restore the moved layer' >&2; exit 1; }
xdotool key --clearmodifiers ctrl+z
sleep 0.3
xdotool mousemove --window "$window" 1292 268 click 1
capture exposure
[[ "$(canvas_hash exposure)" != "$original" ]] || { echo 'Exposure control did not change rendered pixels' >&2; exit 1; }
xdotool mousemove --window "$window" 600 400 click 1
xdotool key --clearmodifiers ctrl+z
capture restored
[[ "$(canvas_hash restored)" == "$original" ]] || { echo 'Undo did not restore exposure' >&2; exit 1; }
printf 'Native UI move, undo, redo and exposure checks passed. Captures: artifacts/\n'
