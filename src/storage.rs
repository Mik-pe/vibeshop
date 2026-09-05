use anyhow::{Context, Result};
use std::fs::File;
use std::io::Write;
use std::path::Path;

pub(crate) fn write_atomic(path: &Path, encode: impl FnOnce(&mut File) -> Result<()>) -> Result<()> {
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent).context("Could not create a file in the destination directory")?;
    encode(temporary.as_file_mut())?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).context("Could not replace the destination; the previous file was not truncated")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_encoder_failure_preserves_the_previous_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("work.vibe");
        std::fs::write(&path, b"last good save").unwrap();
        let result = write_atomic(&path, |file| {
            file.write_all(b"partial new content")?;
            anyhow::bail!("simulated encoder failure");
        });
        assert!(result.is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"last good save");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
        write_atomic(&path, |file| Ok(file.write_all(b"complete save")?)).unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"complete save");
    }
}
