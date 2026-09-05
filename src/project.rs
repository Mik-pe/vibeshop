use anyhow::{Context, Result, bail, ensure};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Arc;

use crate::document::{
    Blend, Document, Layer, MAX_LAYERS, MAX_SOURCE_BYTES, Source, validate_size,
};

const MAGIC: &[u8; 8] = b"VIBESHOP";
const VERSION: u32 = 1;
const MAX_NAME: usize = 4096;
pub const MAX_FILE_BYTES: u64 = MAX_SOURCE_BYTES as u64 + 1024 * 1024;

pub fn open(path: &Path) -> Result<Document> {
    let file = File::open(path).context("Could not open project")?;
    read_from(&mut BufReader::new(file)).context("Invalid or unsupported Vibeshop project")
}

pub fn save(path: &Path, document: &Document) -> Result<()> {
    crate::storage::write_atomic(path, |file| write_to(file, document))
}

pub fn write_to(writer: &mut impl Write, document: &Document) -> Result<()> {
    document.validate()?;
    let mut indices = HashMap::new();
    let mut sources: Vec<&Arc<Source>> = Vec::new();
    for layer in &document.layers {
        ensure!(
            layer.name.len() <= MAX_NAME,
            "Layer name exceeds 4096 bytes"
        );
        if let Some(&index) = indices.get(&layer.source.id) {
            ensure!(
                Arc::ptr_eq(sources[index], &layer.source),
                "Conflicting source identities"
            );
        } else {
            indices.insert(layer.source.id, sources.len());
            sources.push(&layer.source);
        }
    }
    writer.write_all(MAGIC)?;
    for number in [
        VERSION,
        document.width,
        document.height,
        sources.len() as u32,
        document.layers.len() as u32,
    ] {
        writer.write_all(&number.to_le_bytes())?;
    }
    for (index, source) in sources.iter().enumerate() {
        for number in [index as u32, source.width, source.height] {
            writer.write_all(&number.to_le_bytes())?;
        }
        writer.write_all(&(source.rgba.len() as u64).to_le_bytes())?;
        writer.write_all(&source.rgba)?;
    }
    for layer in &document.layers {
        writer.write_all(&(layer.name.len() as u32).to_le_bytes())?;
        writer.write_all(layer.name.as_bytes())?;
        writer.write_all(&(indices[&layer.source.id] as u32).to_le_bytes())?;
        writer.write_all(&[u8::from(layer.visible), layer.blend as u8])?;
        for value in [
            layer.opacity,
            layer.exposure,
            layer.contrast,
            layer.saturation,
        ] {
            writer.write_all(&value.to_le_bytes())?;
        }
        for offset in layer.offset {
            writer.write_all(&offset.to_le_bytes())?;
        }
    }
    Ok(())
}

pub fn read_from(reader: &mut (impl Read + Seek)) -> Result<Document> {
    let length = reader.seek(SeekFrom::End(0))?;
    ensure!(
        (28..=MAX_FILE_BYTES).contains(&length),
        "Project file exceeds limits or is truncated"
    );
    reader.rewind()?;
    ensure!(&read_array::<8>(reader)? == MAGIC, "Not a Vibeshop project");
    ensure!(read_u32(reader)? == VERSION, "Unsupported project version");
    let width = read_u32(reader)?;
    let height = read_u32(reader)?;
    validate_size(width, height)?;
    let source_count = read_u32(reader)? as usize;
    let layer_count = read_u32(reader)? as usize;
    ensure!(
        source_count <= layer_count && layer_count <= MAX_LAYERS,
        "Invalid source or layer count"
    );
    let mut sources = vec![None; source_count];
    let mut retained_bytes = 0_u64;
    for _ in 0..source_count {
        let index = read_u32(reader)? as usize;
        ensure!(
            index < source_count && sources[index].is_none(),
            "Duplicate or invalid asset index"
        );
        let w = read_u32(reader)?;
        let h = read_u32(reader)?;
        validate_size(w, h)?;
        let bytes = u64::from_le_bytes(read_array(reader)?);
        ensure!(
            bytes == u64::from(w) * u64::from(h) * 4,
            "Invalid asset byte count"
        );
        retained_bytes += bytes;
        ensure!(
            retained_bytes <= MAX_SOURCE_BYTES as u64,
            "Project source budget exceeded"
        );
        ensure!(
            bytes <= length.saturating_sub(reader.stream_position()?),
            "Truncated asset"
        );
        let mut rgba = Vec::new();
        rgba.try_reserve_exact(bytes as usize)
            .context("Not enough memory for project asset")?;
        rgba.resize(bytes as usize, 0);
        reader.read_exact(&mut rgba)?;
        sources[index] = Some(Source::new(w, h, rgba)?);
    }
    let mut used = vec![false; source_count];
    let mut layers = Vec::with_capacity(layer_count);
    for _ in 0..layer_count {
        let name_len = read_u32(reader)? as usize;
        ensure!(name_len <= MAX_NAME, "Layer name exceeds 4096 bytes");
        let mut name = vec![0; name_len];
        reader.read_exact(&mut name)?;
        let name = String::from_utf8(name).context("Layer name is not UTF-8")?;
        let index = read_u32(reader)? as usize;
        let source = sources
            .get(index)
            .and_then(Option::as_ref)
            .context("Missing layer asset")?;
        used[index] = true;
        let [visible, blend] = read_array(reader)?;
        ensure!(visible <= 1, "Invalid visibility value");
        let blend = match blend {
            0 => Blend::Normal,
            1 => Blend::Multiply,
            2 => Blend::Screen,
            _ => bail!("Unsupported blend mode"),
        };
        layers.push(Layer {
            name,
            source: source.clone(),
            visible: visible != 0,
            blend,
            opacity: f32::from_le_bytes(read_array(reader)?),
            exposure: f32::from_le_bytes(read_array(reader)?),
            contrast: f32::from_le_bytes(read_array(reader)?),
            saturation: f32::from_le_bytes(read_array(reader)?),
            offset: [
                i32::from_le_bytes(read_array(reader)?),
                i32::from_le_bytes(read_array(reader)?),
            ],
        });
    }
    ensure!(used.iter().all(|used| *used), "Unused project asset");
    ensure!(
        reader.stream_position()? == length,
        "Unexpected trailing project data"
    );
    let document = Document {
        width,
        height,
        layers,
    };
    document.validate()?;
    Ok(document)
}

fn read_array<const N: usize>(reader: &mut impl Read) -> Result<[u8; N]> {
    let mut bytes = [0; N];
    reader.read_exact(&mut bytes).context("Truncated project")?;
    Ok(bytes)
}

fn read_u32(reader: &mut impl Read) -> Result<u32> {
    Ok(u32::from_le_bytes(read_array(reader)?))
}
