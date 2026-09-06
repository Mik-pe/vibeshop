//! Icon glyphs drawn from the font set bundled with egui (Ubuntu-Light,
//! Noto Emoji, emoji-icon-font). No bitmap assets, no network fetches.
//!
//! Every constant here is verified against the bundled fonts at startup by
//! [`assert_all_render`]; a missing glyph is a build/dependency regression,
//! never a silent tofu box in the UI.

use eframe::egui::{self, Color32, Context, RichText};

pub(super) const TOOL_MOVE: char = '⬌';
pub(super) const TOOL_PAN: char = '✋';
pub(super) const OPEN: char = '📂';
pub(super) const SAVE: char = '💾';
pub(super) const EXPORT: char = '⬈';
pub(super) const UNDO: char = '↩';
pub(super) const REDO: char = '↪';
pub(super) const DUPLICATE: char = '⎘';
pub(super) const REMOVE: char = '🗑';
pub(super) const RAISE: char = '⬆';
pub(super) const LOWER: char = '⬇';
pub(super) const VISIBLE: char = '👁';
pub(super) const HIDDEN: char = '🚫';
pub(super) const EXPOSURE: char = '☀';
pub(super) const CONTRAST: char = '◑';
pub(super) const OPACITY: char = '◔';
pub(super) const SATURATION: char = '🎨';
pub(super) const FIT: char = '⛶';
pub(super) const NEW_CANVAS: char = '🖼';

const ALL: &[char] = &[
    TOOL_MOVE, TOOL_PAN, OPEN, SAVE, EXPORT, UNDO, REDO, DUPLICATE, REMOVE, RAISE, LOWER, VISIBLE,
    HIDDEN, EXPOSURE, CONTRAST, OPACITY, SATURATION, FIT,
];

/// Text color matched to the current visuals. Emoji fonts do not inherit
/// widget foreground styling reliably, so icons carry their color explicitly.
pub(super) const TEXT: Color32 = Color32::from_rgb(234, 235, 237);
pub(super) const TEXT_MUTED: Color32 = Color32::from_rgb(150, 155, 165);

/// An icon sized for buttons and section labels.
pub(super) fn glyph(ch: char, size: f32) -> RichText {
    RichText::new(ch.to_string()).size(size)
}

/// Panics naming the missing codepoints if the bundled font set cannot draw
/// the whole icon vocabulary. Runs once per application start (and in the GPU
/// harness tests, which assert through the same path).
pub(super) fn assert_all_render(ctx: &Context) {
    let missing: Vec<char> = ALL
        .iter()
        .copied()
        .filter(|&ch| !renders(ctx, ch))
        .collect();
    assert!(
        missing.is_empty(),
        "Vibeshop icons missing from the bundled font set: {missing:?}"
    );
}

fn renders(ctx: &Context, ch: char) -> bool {
    ctx.fonts_mut(|fonts| fonts.has_glyph(&egui::FontId::proportional(15.0), ch))
}
