//! Atomic file publication shared by locked catalog and workspace writers.
use crate::error::Result;
use std::{
    fs,
    io::Write,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

#[cfg(test)]
thread_local! {
    pub(crate) static CRASH_WRITE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// A directory treated as a separate mount: a rename across its boundary fails with EXDEV.
    pub(crate) static MOUNT_POINT: std::cell::RefCell<Option<std::path::PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

/// Stage at `<temp_prefix>-<pid>-<counter>.tmp`, sync, then publish with rename.
/// The caller holds the destination's writer lock and cleans stale temporaries.
pub(crate) fn write_atomically(temp_prefix: &Path, path: &Path, bytes: &[u8]) -> Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let (temporary, mut file) = loop {
        let mut name = temp_prefix.as_os_str().to_os_string();
        name.push(format!(
            "-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let temporary = std::path::PathBuf::from(name);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => break (temporary, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    };
    #[cfg(test)]
    if CRASH_WRITE.replace(false) {
        file.write_all(&bytes[..bytes.len() / 2])?;
        panic!("simulated crash during atomic write");
    }
    let result = (|| -> Result<()> {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        #[cfg(test)]
        if MOUNT_POINT.with_borrow(|mount| {
            mount
                .as_ref()
                .is_some_and(|m| temporary.starts_with(m) != path.starts_with(m))
        }) {
            return Err(std::io::Error::from(std::io::ErrorKind::CrossesDevices).into());
        }
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}
