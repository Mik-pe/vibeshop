import { readFileSync, writeFileSync } from 'node:fs';
function section(path, from, to, replacement) {
  const text = readFileSync(path, 'utf8');
  const start = text.indexOf(from), end = text.indexOf(to, start);
  if (start < 0 || end < start) throw new Error(`Missing section in ${path}`);
  writeFileSync(path, text.slice(0, start) + replacement + text.slice(end));
}
section('src/document.rs', '    pub fn finish_edit(&mut self)', '    pub fn edit(', `    pub fn finish_edit(&mut self) {
        let Some(before) = self.gesture.take() else { return; };
        if before == self.document { return; }
        self.undo.push(before);
        self.redo.clear();
        while self.undo.len() > MAX_HISTORY || (source_bytes(std::iter::once(&self.document).chain(self.undo.iter())) > MAX_SOURCE_BYTES && !self.undo.is_empty()) {
            self.undo.remove(0);
        }
    }
`);
section('src/studio.rs', '                if response.drag_started() && !panning {', '                if response.drag_stopped()', `                if response.drag_started() && !panning
                    && let (Some(pointer), Some(layer)) = (ctx.input(|i| i.pointer.press_origin()), self.editor.document.layers.get(self.editor.selected)) {
                    self.move_start = Some((pointer, layer.offset));
                    self.editor.begin_edit();
                }
                if response.dragged() && !panning
                    && let (Some((start, offset)), Some(pointer)) = (self.move_start, response.interact_pointer_pos()) {
                    let delta = (pointer - start) * pixels_per_point / self.zoom;
                    let next = [(offset[0] + delta.x.round() as i32).clamp(-8192,8192), (offset[1] + delta.y.round() as i32).clamp(-8192,8192)];
                    if let Some(layer) = self.editor.document.layers.get_mut(self.editor.selected) && layer.offset != next {
                        layer.offset = next;
                        self.editor.changed();
                    }
                }
`);
const studio = readFileSync('src/studio.rs', 'utf8');
writeFileSync('src/studio.rs', studio.replaceAll('Stroke::new(1.0,', 'Stroke::new(1.0_f32,').replaceAll('Stroke::new(1.5,', 'Stroke::new(1.5_f32,'));
const readme = readFileSync('README.md', 'utf8');
writeFileSync('README.md', readme.replace('libxkbcommon-dev libwayland-dev', 'libxkbcommon-dev libxkbcommon-x11-0 libwayland-dev'));
