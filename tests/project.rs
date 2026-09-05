use std::io::Cursor;
use std::sync::Arc;
use vibeshop::document::{Blend, Document, Editor, Layer, Source};
use vibeshop::project;

fn fixture() -> Document {
    let source = Source::new(2, 1, vec![20, 40, 60, 0, 200, 170, 140, 255]).unwrap();
    let mut bottom = Layer::new("Original", source.clone());
    bottom.exposure = 0.5;
    let mut top = Layer::new("Maskless copy · α", source);
    top.visible = false;
    top.opacity = 0.4;
    top.contrast = 0.8;
    top.saturation = 1.2;
    top.offset = [-3, 7];
    top.blend = Blend::Screen;
    Document {
        width: 7,
        height: 3,
        layers: vec![bottom, top],
    }
}

fn encoded(document: &Document) -> Vec<u8> {
    let mut bytes = Vec::new();
    project::write_to(&mut bytes, document).unwrap();
    bytes
}

#[test]
fn all_layer_settings_and_shared_originals_round_trip() {
    let original = fixture();
    let bytes = encoded(&original);
    let loaded = project::read_from(&mut Cursor::new(&bytes)).unwrap();
    assert_eq!((loaded.width, loaded.height), (7, 3));
    assert_eq!(loaded.layers.len(), original.layers.len());
    assert!(Arc::ptr_eq(
        &loaded.layers[0].source,
        &loaded.layers[1].source
    ));
    assert_ne!(loaded.layers[0].source.id, original.layers[0].source.id);
    for (actual, expected) in loaded.layers.iter().zip(&original.layers) {
        assert_eq!(actual.source.rgba, expected.source.rgba);
        assert_eq!((actual.source.width, actual.source.height), (2, 1));
        let mut normalized = actual.clone();
        normalized.source = expected.source.clone();
        assert_eq!(&normalized, expected);
    }
    assert_eq!(encoded(&loaded), bytes);
    assert_eq!(u32::from_le_bytes(bytes[20..24].try_into().unwrap()), 1);
}

#[test]
fn an_empty_canvas_is_a_valid_editable_project() {
    let document = Document::blank(123, 456).unwrap();
    let loaded = project::read_from(&mut Cursor::new(encoded(&document))).unwrap();
    assert_eq!(loaded, document);
}

#[test]
fn every_truncated_prefix_is_rejected() {
    let bytes = encoded(&fixture());
    for length in 0..bytes.len() {
        assert!(
            project::read_from(&mut Cursor::new(&bytes[..length])).is_err(),
            "Accepted prefix {length}"
        );
    }
}

#[test]
fn malformed_headers_assets_and_layers_are_rejected() {
    let bytes = encoded(&fixture());
    let first_layer = 28 + 20 + 8;
    let name_len = "Original".len();
    let source_ref = first_layer + 4 + name_len;
    let visibility = source_ref + 4;
    let opacity = visibility + 2;
    let cases: Vec<(usize, Vec<u8>)> = vec![
        (0, vec![0]),
        (8, 2_u32.to_le_bytes().to_vec()),
        (12, u32::MAX.to_le_bytes().to_vec()),
        (20, 17_u32.to_le_bytes().to_vec()),
        (24, 17_u32.to_le_bytes().to_vec()),
        (28, 1_u32.to_le_bytes().to_vec()),
        (40, u64::MAX.to_le_bytes().to_vec()),
        (first_layer, 4097_u32.to_le_bytes().to_vec()),
        (first_layer + 4, vec![0xff]),
        (source_ref, 8_u32.to_le_bytes().to_vec()),
        (visibility, vec![2]),
        (visibility + 1, vec![3]),
        (opacity, f32::NAN.to_le_bytes().to_vec()),
        (opacity + 4, f32::INFINITY.to_le_bytes().to_vec()),
        (opacity + 16, 8193_i32.to_le_bytes().to_vec()),
    ];
    for (offset, replacement) in cases {
        let mut bad = bytes.clone();
        bad[offset..offset + replacement.len()].copy_from_slice(&replacement);
        assert!(
            project::read_from(&mut Cursor::new(bad)).is_err(),
            "Accepted invalid field at {offset}"
        );
    }
    let mut trailing = bytes;
    trailing.push(0);
    assert!(project::read_from(&mut Cursor::new(trailing)).is_err());
}

#[test]
fn duplicate_and_unreferenced_assets_are_rejected() {
    let bytes = encoded(&fixture());
    let mut duplicated = bytes[..28].to_vec();
    duplicated[20..24].copy_from_slice(&2_u32.to_le_bytes());
    duplicated.extend_from_slice(&bytes[28..56]);
    duplicated.extend_from_slice(&bytes[28..56]);
    duplicated.extend_from_slice(&bytes[56..]);
    assert!(project::read_from(&mut Cursor::new(&duplicated)).is_err());
    duplicated[56..60].copy_from_slice(&1_u32.to_le_bytes());
    assert!(project::read_from(&mut Cursor::new(duplicated)).is_err());
}

#[test]
fn oversized_files_are_rejected_before_reading_assets() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("oversized.vibe");
    std::fs::File::create(&path)
        .unwrap()
        .set_len(project::MAX_FILE_BYTES + 1)
        .unwrap();
    assert!(project::open(&path).is_err());
}

#[test]
fn failed_save_keeps_the_previous_valid_project() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("work.vibe");
    let mut document = fixture();
    project::save(&path, &document).unwrap();
    let before = std::fs::read(&path).unwrap();
    document.layers[0].name = "x".repeat(4097);
    assert!(project::save(&path, &document).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(project::open(&path).unwrap().layers[0].name, "Original");
    assert!(project::save(&directory.path().join("missing/work.vibe"), &fixture()).is_err());
}

#[test]
fn undo_and_redo_return_to_the_saved_state() {
    let mut editor = Editor::new(fixture());
    editor.edit(|document, _| document.layers[0].exposure = 1.0);
    editor.mark_saved(editor.state_id());
    assert!(!editor.dirty);
    editor.undo();
    assert!(editor.dirty);
    editor.redo();
    assert!(!editor.dirty);
    editor.edit(|document, _| document.layers[0].exposure = 2.0);
    assert!(editor.dirty);
    editor.undo();
    assert!(!editor.dirty);
}

#[test]
fn completing_an_older_save_does_not_mark_newer_edits_saved() {
    let mut editor = Editor::new(fixture());
    editor.edit(|document, _| document.layers[0].opacity = 0.5);
    let saved = editor.state_id();
    editor.edit(|document, _| document.layers[0].opacity = 0.7);
    editor.mark_saved(saved);
    assert!(editor.dirty);
    editor.undo();
    assert!(!editor.dirty);
    editor.redo();
    assert!(editor.dirty);
}

#[test]
fn no_op_drag_keeps_the_saved_state_and_creates_no_history() {
    let mut editor = Editor::new(fixture());
    let saved = editor.state_id();
    editor.begin_edit();
    editor.document.layers[0].opacity = 0.5;
    editor.changed();
    editor.document.layers[0].opacity = 1.0;
    editor.changed();
    editor.finish_edit();
    assert_eq!(editor.state_id(), saved);
    assert!(!editor.dirty);
    assert!(!editor.can_undo());
}

#[test]
fn replacement_has_a_new_state_identity_and_new_canvas_is_unsaved() {
    let mut editor = Editor::new(fixture());
    let old = editor.state_id();
    editor.replace(Document::blank(10, 10).unwrap());
    assert_ne!(editor.state_id(), old);
    editor.mark_unsaved();
    assert!(editor.dirty);
    editor.mark_saved(old);
    assert!(editor.dirty);
    editor.mark_saved(editor.state_id());
    assert!(!editor.dirty);
}
