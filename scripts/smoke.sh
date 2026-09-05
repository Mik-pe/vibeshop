#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p artifacts
cargo build --locked
export EFRAME_SCREENSHOT_TO="$PWD/artifacts/studio.png"
rm -f "$EFRAME_SCREENSHOT_TO"
if [[ -z "${DISPLAY:-}" ]]; then
    command -v xvfb-run >/dev/null || { echo 'Install xvfb or run inside a graphical session' >&2; exit 1; }
    timeout 60s xvfb-run -a -s '-screen 0 1600x1100x24' target/debug/vibeshop
else
    timeout 60s target/debug/vibeshop
fi
test -s "$EFRAME_SCREENSHOT_TO"
printf 'Actual editor capture: %s\n' "$EFRAME_SCREENSHOT_TO"
