# Editable projects (.vibe), version 1

A project is a self-contained binary file. It contains the current document and deduplicated RGBA8 source assets, not a flattened preview. Runtime source IDs are regenerated on import; layers referring to the same serialized asset share one immutable source. There are no external asset paths, embedded scripts, archive extraction or network references.

All integers and IEEE-754 float fields use little-endian byte order. Header: eight ASCII bytes `VIBESHOP`; u32 version (1); u32 canvas width; u32 canvas height; u32 asset count; u32 layer count. Each asset: u32 index, width and height; u64 byte length; exactly width × height × 4 straight-alpha sRGB RGBA8 bytes. Each bottom-to-top layer: u32 UTF-8 name byte length and name bytes; u32 asset index; u8 visibility (0 or 1); u8 blend (0 normal, 1 multiply, 2 screen); four f32 values for opacity, exposure, contrast, saturation; two i32 translation offsets.

Unknown versions, invalid dimensions/counts, duplicate/missing/unreferenced assets, non-finite/out-of-range adjustments, invalid UTF-8 and trailing/truncated data are rejected. Asset bytes are limited to 128 MiB in total, file size to that plus 1 MiB, names to 4096 bytes, and layers to 16. File lengths and pixel budgets are checked before asset allocation. Empty canvases are valid. The existing 8192px and 16 MP limits still apply.

Version 1 intentionally stores raw assets rather than adding compressed-container complexity. It performs structural validation, not checksummed corruption detection or authentication. It is an initial format, not a promise of permanent compatibility. Changing pixel depth, color semantics or the layout requires an explicit version/migration decision; never silently reinterpret old files.

Saving streams to a temporary file in the destination directory, syncs the file, and atomically replaces the destination. Encoding failure preserves the previous file. PNG and project writes use the same atomic-write helper. Crash autosave, parent-directory durability across power loss and multiple-process conflict handling are not provided yet (issue #9).

The editor tracks saved document state separately from the monotonically increasing render revision. Undo/redo can return to the saved state. A save callback marks only the snapshot actually written; edits made while saving remain dirty. Open results cannot silently replace newer edits. Save/discard/cancel protects closing and replacement; PNG export never marks an editable project saved.
