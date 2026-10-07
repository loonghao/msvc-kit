//! Cross-process exclusion and atomic publication for local toolchain state.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;

use crate::{MsvcKitError, Result};

/// The returned file owns the OS lock. Closing it releases the lock, including after a crash.
pub fn lock_file(path: &Path) -> Result<File> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?;
    file.lock()?;
    Ok(file)
}

/// Replace a file only after its complete new contents have been written and flushed.
pub fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(contents)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|error| MsvcKitError::Io(error.error))?;
    Ok(())
}
