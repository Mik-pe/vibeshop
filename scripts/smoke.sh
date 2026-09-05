#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ -z "${DISPLAY:-}" ]]; then
    command -v xvfb-run >/dev/null || { echo 'Install xvfb or run inside an X11 graphical session' >&2; exit 1; }
    exec xvfb-run -a -s '-screen 0 1600x1100x24' scripts/smoke.sh
fi
for tool in xdotool import; do
    command -v "$tool" >/dev/null || { echo 'Install xdotool and imagemagick for native window capture' >&2; exit 1; }
done
cargo build --locked
mkdir -p artifacts
rm -f artifacts/studio.png
unset EFRAME_SCREENSHOT_TO
target/debug/vibeshop >artifacts/studio.log 2>&1 &
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
# Let the first GPU composition reach the window; this is not a timing benchmark.
sleep 2
kill -0 "$pid" 2>/dev/null || { cat artifacts/studio.log; exit 1; }
import -window "$window" artifacts/studio.png
test -s artifacts/studio.png
printf 'Actual native editor capture: artifacts/studio.png\n'
