use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

static TEMP_PATH_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub(crate) fn elapsed_micros(started: Instant) -> u64 {
    match u64::try_from(started.elapsed().as_micros()) {
        Ok(micros) => micros.max(1),
        Err(_) => u64::MAX,
    }
}

#[derive(Debug)]
pub(crate) struct BenchmarkTempDir {
    path: PathBuf,
}

impl BenchmarkTempDir {
    pub(crate) fn new(prefix: &str) -> Result<Self, std::io::Error> {
        let base = std::env::temp_dir();
        let process_id = u64::from(std::process::id());

        for _ in 0..16 {
            let sequence = TEMP_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = base.join(format!("{prefix}-{process_id}-{sequence}"));
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }

        Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "could not reserve a unique benchmark temp directory",
        ))
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for BenchmarkTempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[derive(Debug)]
pub(crate) struct BenchmarkTempFile {
    path: PathBuf,
}

impl BenchmarkTempFile {
    pub(crate) fn new(prefix: &str, extension: &str) -> Result<Self, std::io::Error> {
        let base = std::env::temp_dir();
        let process_id = u64::from(std::process::id());
        let sequence = TEMP_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let filename = format!("{prefix}-{process_id}-{sequence}.{extension}");
        let path = base.join(filename);

        match std::fs::remove_file(&path) {
            Ok(()) => Ok(Self { path }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self { path }),
            Err(error) => Err(error),
        }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for BenchmarkTempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
