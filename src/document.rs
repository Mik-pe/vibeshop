use anyhow::{Result, ensure};
use std::collections::HashSet;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

pub const MAX_PIXELS: u64 = 16 * 1024 * 1024;
pub const MAX_DIMENSION: u32 = 8192;
pub const MAX_SOURCE_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_LAYERS: usize = 16;
const MAX_HISTORY: usize = 32;
static NEXT_IMAGE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, PartialEq, Eq)]
pub struct Source {
    pub id: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub fn validate_size(width: u32, height: u32) -> Result<()> {
    ensure!(width > 0 && height > 0, "Image dimensions must be nonzero");
    ensure!(
        width <= MAX_DIMENSION && height <= MAX_DIMENSION,
        "Maximum image dimension is {MAX_DIMENSION}px"
    );
    ensure!(
        u64::from(width) * u64::from(height) <= MAX_PIXELS,
        "This build supports images up to 16 megapixels; tiling is not implemented yet"
    );
    Ok(())
}

impl Source {
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Result<Arc<Self>> {
        validate_size(width, height)?;
        ensure!(
            rgba.len() as u64 == u64::from(width) * u64::from(height) * 4,
            "Invalid RGBA byte count"
        );
        Ok(Arc::new(Self {
            id: NEXT_IMAGE.fetch_add(1, Ordering::Relaxed),
            width,
            height,
            rgba,
        }))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum Blend {
    #[default]
    Normal,
    Multiply,
    Screen,
}
impl Blend {
    pub const ALL: [Self; 3] = [Self::Normal, Self::Multiply, Self::Screen];
    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Multiply => "Multiply",
            Self::Screen => "Screen",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub name: String,
    pub source: Arc<Source>,
    pub visible: bool,
    pub opacity: f32,
    pub exposure: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub blend: Blend,
    pub offset: [i32; 2],
}
impl Layer {
    pub fn new(name: impl Into<String>, source: Arc<Source>) -> Self {
        Self {
            name: name.into(),
            source,
            visible: true,
            opacity: 1.0,
            exposure: 0.0,
            contrast: 1.0,
            saturation: 1.0,
            blend: Blend::Normal,
            offset: [0, 0],
        }
    }
    pub fn reset_adjustments(&mut self) {
        self.exposure = 0.0;
        self.contrast = 1.0;
        self.saturation = 1.0;
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub width: u32,
    pub height: u32,
    // Bottom to top. Immutable sources are shared by layers and undo snapshots.
    pub layers: Vec<Layer>,
}
impl Document {
    pub fn new(layer: Layer) -> Self {
        Self {
            width: layer.source.width,
            height: layer.source.height,
            layers: vec![layer],
        }
    }
    pub fn validate(&self) -> Result<()> {
        validate_size(self.width, self.height)?;
        ensure!(
            self.layers.len() <= MAX_LAYERS,
            "This build supports at most {MAX_LAYERS} layers"
        );
        ensure!(
            source_bytes(std::iter::once(self)) <= MAX_SOURCE_BYTES,
            "Document source budget exceeded (128 MiB)"
        );
        for layer in &self.layers {
            validate_size(layer.source.width, layer.source.height)?;
            ensure!(
                layer.source.rgba.len() as u64
                    == u64::from(layer.source.width) * u64::from(layer.source.height) * 4,
                "Invalid layer pixels"
            );
            ensure!(
                (0.0..=1.0).contains(&layer.opacity)
                    && (-5.0..=5.0).contains(&layer.exposure)
                    && (0.0..=2.0).contains(&layer.contrast)
                    && (0.0..=2.0).contains(&layer.saturation),
                "Invalid adjustment values"
            );
            ensure!(
                layer.offset.iter().all(|x| (-8192..=8192).contains(x)),
                "Layer offset out of range"
            );
        }
        Ok(())
    }
}

fn source_bytes<'a>(docs: impl Iterator<Item = &'a Document>) -> usize {
    let mut seen = HashSet::new();
    docs.flat_map(|d| &d.layers)
        .filter(|l| seen.insert(l.source.id))
        .map(|l| l.source.rgba.len())
        .sum()
}

pub struct Editor {
    pub document: Document,
    pub selected: usize,
    pub revision: u64,
    pub dirty: bool,
    undo: Vec<Document>,
    redo: Vec<Document>,
    gesture: Option<Document>,
}
impl Editor {
    pub fn new(document: Document) -> Self {
        Self {
            document,
            selected: 0,
            revision: 1,
            dirty: false,
            undo: Vec::new(),
            redo: Vec::new(),
            gesture: None,
        }
    }
    pub fn begin_edit(&mut self) {
        if self.gesture.is_none() {
            self.gesture = Some(self.document.clone());
        }
    }
    pub fn changed(&mut self) {
        self.revision += 1;
        self.dirty = true;
    }
    pub fn finish_edit(&mut self) {
        let Some(before) = self.gesture.take() else {
            return;
        };
        if before == self.document {
            return;
        }
        self.undo.push(before);
        self.redo.clear();
        while self.undo.len() > MAX_HISTORY
            || (source_bytes(std::iter::once(&self.document).chain(self.undo.iter()))
                > MAX_SOURCE_BYTES
                && !self.undo.is_empty())
        {
            self.undo.remove(0);
        }
    }
    pub fn edit(&mut self, f: impl FnOnce(&mut Document, &mut usize)) {
        self.finish_edit();
        self.begin_edit();
        f(&mut self.document, &mut self.selected);
        if self.gesture.as_ref() != Some(&self.document) {
            self.changed();
        }
        self.finish_edit();
        self.clamp_selection();
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty() || self.gesture.as_ref().is_some_and(|g| g != &self.document)
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn undo(&mut self) {
        self.finish_edit();
        if let Some(previous) = self.undo.pop() {
            self.redo
                .push(std::mem::replace(&mut self.document, previous));
            self.changed();
            self.clamp_selection();
        }
    }
    pub fn redo(&mut self) {
        self.finish_edit();
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.document, next));
            self.changed();
            self.clamp_selection();
        }
    }
    pub fn add_layer(&mut self, layer: Layer) -> Result<()> {
        let mut next = self.document.clone();
        next.layers.push(layer);
        next.validate()?;
        self.edit(|doc, selected| {
            *selected = next.layers.len() - 1;
            *doc = next;
        });
        Ok(())
    }
    pub fn replace_if_revision(
        &mut self,
        document: Document,
        expected: u64,
    ) -> std::result::Result<(), Document> {
        if self.revision != expected {
            return Err(document);
        }
        self.replace(document);
        Ok(())
    }
    pub fn replace(&mut self, document: Document) {
        let revision = self.revision + 1;
        *self = Self::new(document);
        self.revision = revision;
    }
    fn clamp_selection(&mut self) {
        self.selected = self
            .selected
            .min(self.document.layers.len().saturating_sub(1));
    }
}

pub fn demo_document() -> Document {
    let (w, h) = (1440, 960);
    let mut pixels = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let u = x as f32 / w as f32;
            let v = y as f32 / h as f32;
            let grain = ((x * 73 + y * 97 + x * y * 13) % 19) as f32 / 19.0 - 0.5;
            let sun = ((u - 0.69).powi(2) + ((v - 0.3) * 0.6667).powi(2)).sqrt();
            let mut c = [30.0 + v * 104.0, 39.0 + v * 75.0, 74.0 + v * 36.0];
            if sun < 0.086 {
                c = [255.0, 207.0, 140.0];
            }
            for i in 0..5 {
                let k = i as f32;
                let ridge = 0.47 + k * 0.103 + (u * 5.2 + k * 1.7).sin() * (0.065 + k * 0.008);
                if v > ridge {
                    let light = ((v - ridge) * 4.0).min(1.0);
                    c = [
                        201.0 - k * 27.0 - light * 37.0,
                        119.0 - k * 18.0 - light * 23.0,
                        100.0 - k * 9.0 - light * 17.0,
                    ];
                }
            }
            pixels.extend([
                (c[0] + grain * 2.0).clamp(0.0, 255.0) as u8,
                (c[1] + grain * 2.0).clamp(0.0, 255.0) as u8,
                (c[2] + grain * 2.0).clamp(0.0, 255.0) as u8,
                255,
            ]);
        }
    }
    Document::new(Layer::new(
        "Dune study · generated demo",
        Source::new(w as u32, h as u32, pixels).expect("valid built-in demo"),
    ))
}
