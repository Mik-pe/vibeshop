use std::{fs::File, io::BufReader, path::Path};
use anyhow::{Context, Result, ensure};
use image::{ImageDecoder, ImageEncoder};
use crate::document::{Layer, Source, MAX_DIMENSION, MAX_SOURCE_BYTES, validate_size};

pub fn open(path: &Path) -> Result<Layer> {
    let file = File::open(path).context("Could not open image")?;
    ensure!(file.metadata()?.len() <= 64 * 1024 * 1024, "Encoded image exceeds 64 MiB import limit");
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
    ensure!(decoder.icc_profile()?.is_none(), "Embedded ICC profiles are not supported yet. Convert a copy to untagged sRGB before importing.");
    let orientation = decoder.orientation()?;
    let mut image = image::DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    let rgba = image.to_rgba8();
    Ok(Layer::new(path.file_name().unwrap_or_default().to_string_lossy(), Source::new(rgba.width(), rgba.height(), rgba.into_raw())?))
}

pub fn save_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<()> {
    validate_size(width, height)?;
    ensure!(rgba.len() as u64 == u64::from(width) * u64::from(height) * 4, "Invalid export pixels");
    crate::storage::write_atomic(path, |file| {
        image::codecs::png::PngEncoder::new(file).write_image(rgba, width, height, image::ExtendedColorType::Rgba8)?;
        Ok(())
    })
}
