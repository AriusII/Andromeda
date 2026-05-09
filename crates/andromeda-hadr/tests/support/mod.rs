use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process,
    time::{SystemTime, UNIX_EPOCH},
};

pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub fn new(prefix: &str) -> io::Result<Self> {
        let base = env::temp_dir();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();

        for attempt in 0..1000_u32 {
            let path = base.join(format!(
                "andromeda-hadr-{prefix}-{}-{stamp}-{attempt}",
                process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }

        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not create unique HADR test temp directory",
        ))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub fn tempdir(prefix: &str) -> io::Result<TempDir> {
    TempDir::new(prefix)
}
