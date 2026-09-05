use std::sync::Arc;
use vibeshop::document::*;
fn editor() -> Editor { Editor::new(Document::new(Layer::new("test", Source::new(1, 1, vec![100, 90, 80, 255]).unwrap()))) }
#[test] fn rejects_invalid_images() {
    for (w,h) in [(0,1),(1,0),(u32::MAX,u32::MAX),(8192,8192)] { assert!(validate_size(w,h).is_err()); }
    assert!(Source::new(2,2,vec![0;15]).is_err());
}
#[test] fn one_drag_is_one_undo_step_and_shares_pixels() {
    let mut e = editor(); let source = e.document.layers[0].source.clone();
    e.begin_edit();
    for i in 1..20 { e.document.layers[0].exposure = i as f32 / 10.0; e.changed(); }
    e.finish_edit(); e.undo();
    assert_eq!(e.document.layers[0].exposure,0.0); assert!(!e.can_undo());
    assert!(Arc::ptr_eq(&source,&e.document.layers[0].source));
    e.redo(); assert_eq!(e.document.layers[0].exposure,1.9);
}
#[test] fn edit_after_undo_discards_redo() {
    let mut e = editor(); e.edit(|d,_| d.layers[0].opacity=0.5); e.undo();
    e.edit(|d,_| d.layers[0].saturation=0.0); assert!(!e.can_redo());
}
#[test] fn no_op_does_not_create_history() { let mut e=editor(); e.edit(|_,_|{}); assert!(!e.can_undo()); assert!(!e.dirty); }
#[test] fn undo_restores_deleted_layer_and_selection_is_valid() {
    let mut e=editor(); e.edit(|d,_| {d.layers.clear();}); assert!(e.document.layers.is_empty());
    e.undo(); assert_eq!(e.document.layers.len(),1); assert_eq!(e.selected,0);
}
#[test] fn nonfinite_adjustments_are_rejected() { let mut e=editor(); e.document.layers[0].opacity=f32::NAN; assert!(e.document.validate().is_err()); }
#[test] fn layer_limit_is_transactional() {
    let mut e=editor(); let l=e.document.layers[0].clone();
    for _ in 1..MAX_LAYERS { e.add_layer(l.clone()).unwrap(); }
    assert!(e.add_layer(l).is_err()); assert_eq!(e.document.layers.len(),MAX_LAYERS);
}
#[test] fn png_roundtrip_and_failed_export_preserves_destination() {
    let dir=tempfile::tempdir().unwrap(); let path=dir.path().join("photo.png");
    let pixels=[20,40,60,128,200,170,140,255];
    vibeshop::image_io::save_png(&path,2,1,&pixels).unwrap();
    let source=vibeshop::image_io::open(&path).unwrap().source; assert_eq!(source.rgba,pixels);
    let before=std::fs::read(&path).unwrap();
    assert!(vibeshop::image_io::save_png(&path,2,1,&[0]).is_err()); assert_eq!(std::fs::read(path).unwrap(),before);
}
