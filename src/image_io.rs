use crate::document::{Layer, MAX_DIMENSION, MAX_SOURCE_BYTES, Source, validate_size};
use anyhow::{Context, Result, ensure};
use image::ImageDecoder;
use std::{
    fs::File,
    io::{BufReader, Write},
    path::Path,
};

pub fn open(path: &Path) -> Result<Layer> {
    let file = File::open(path).context("Could not open image")?;
    ensure!(
        file.metadata()?.len() <= 64 * 1024 * 1024,
        "Encoded image exceeds 64 MiB import limit"
    );
    let mut reader = image::ImageReader::new(BufReader::new(file)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_SOURCE_BYTES as u64);
    reader.limits(limits);
    let mut decoder = reader.into_decoder()?;
    let (w, h) = decoder.dimensions();
    validate_size(w, h)?;
    // Do not silently reinterpret a tagged wide-gamut image as sRGB.
    ensure!(
        decoder.icc_profile()?.is_none(),
        "Embedded ICC profiles are not supported yet. Convert a copy to untagged sRGB before importing."
    );
    let orientation = decoder.orientation()?;
    let mut image = image::DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    let rgba = image.to_rgba8();
    Ok(Layer::new(
        path.file_name().unwrap_or_default().to_string_lossy(),
        Source::new(rgba.width(), rgba.height(), rgba.into_raw())?,
    ))
}

pub fn save_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<()> {
    validate_size(width, height)?;
    ensure!(
        rgba.len() as u64 == u64::from(width) * u64::from(height) * 4,
        "Invalid export pixels"
    );
    write_png(path, width, height, |stream| Ok(stream.write_all(rgba)?))
}

/// Encode a frozen GPU revision without collecting full-image CPU pixels.
/// Call on an IO worker, not the UI thread.
pub fn save_png_snapshot(path: &Path, snapshot: crate::gpu::Readback) -> Result<()> {
    let (width, height) = snapshot.dimensions();
    write_png(path, width, height, |stream| snapshot.write_to(stream))
}

fn write_png(
    path: &Path,
    width: u32,
    height: u32,
    write: impl FnOnce(&mut png::StreamWriter<'_, &mut File>) -> Result<()>,
) -> Result<()> {
    validate_size(width, height)?;
    crate::storage::write_atomic(path, |file| {
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        encoder.set_filter(png::Filter::Adaptive);
        let mut writer = encoder.write_header()?;
        let mut stream = writer.stream_writer()?;
        write(&mut stream)?;
        stream.finish()?;
        writer.finish()?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interrupted_stream_preserves_destination_and_removes_partial_png() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("output.png");
        save_png(&path, 1, 1, &[1, 2, 3, 255]).unwrap();
        let previous = std::fs::read(&path).unwrap();
        let result = write_png(&path, 13, 700, |stream| {
            stream.write_all(&[90, 80, 70, 128].repeat(13 * 300))?;
            anyhow::bail!("simulated transfer failure");
        });
        assert!(result.unwrap_err().to_string().contains("transfer failure"));
        assert_eq!(std::fs::read(&path).unwrap(), previous);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        // An incomplete stream must also fail during explicit finalization.
        assert!(write_png(&path, 13, 700, |stream| { Ok(stream.write_all(&[0; 52])?) }).is_err());
        assert_eq!(std::fs::read(path).unwrap(), previous);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
