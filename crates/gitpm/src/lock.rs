//! Exclusive advisory locks on lock files.
//!
//! POSIX record locks (`fcntl(F_SETLK)`) are owned by the process and are not
//! inherited by children: a git subprocess spawned by another thread can never keep
//! a released lock alive (as an inherited `flock` descriptor would in the window
//! between fork and exec). A crashed owner releases its lock with the process, so a
//! leftover lock file never blocks. Record locks do not exclude other handles of the
//! same process, so held locks are also tracked in a process-wide set.
use crate::error::{Error, Result};
use std::{
    collections::BTreeSet,
    fs::File,
    os::fd::AsRawFd,
    path::{Path, PathBuf},
    sync::Mutex,
};

static HELD: Mutex<BTreeSet<PathBuf>> = Mutex::new(BTreeSet::new());

/// Held until dropped. The lock file itself is kept: removing it would let another
/// process lock a fresh inode while a third still waits on the old one.
#[derive(Debug)]
pub(crate) struct FileLock {
    _file: File,
    key: PathBuf,
}

impl Drop for FileLock {
    fn drop(&mut self) {
        // Closing `_file` (after this) releases the record lock.
        HELD.lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.key);
    }
}

/// Lock `path` (created if missing) or fail at once with [`Error::Locked`].
pub(crate) fn try_lock(path: &Path, busy: &str) -> Result<FileLock> {
    let file = File::options()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?;
    // Same file through another spelling (symlinked parent) is the same lock.
    let key = std::fs::canonicalize(path)?;
    let mut held = HELD.lock().unwrap_or_else(|e| e.into_inner());
    if held.contains(&key) {
        return Err(Error::Locked(busy.into()));
    }
    // SAFETY: plain fcntl on an owned, open descriptor with a zeroed flock struct.
    let locked = unsafe {
        let mut request: libc::flock = std::mem::zeroed();
        request.l_type = libc::F_WRLCK as _;
        request.l_whence = libc::SEEK_SET as _;
        libc::fcntl(file.as_raw_fd(), libc::F_SETLK, &request) == 0
    };
    if !locked {
        let error = std::io::Error::last_os_error();
        return Err(match error.raw_os_error() {
            Some(libc::EAGAIN | libc::EACCES) => Error::Locked(busy.into()),
            _ => error.into(),
        });
    }
    held.insert(key.clone());
    Ok(FileLock { _file: file, key })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;
    use std::process::Command;

    #[test]
    fn second_lock_in_process_is_refused_until_drop() {
        let dir = TempDir::new("lock");
        let path = dir.join("x.lock");
        let first = try_lock(&path, "busy").unwrap();
        let error = try_lock(&path, "busy").unwrap_err();
        assert_eq!((error.kind(), error.to_string()), ("locked", "busy".into()));
        // Another spelling of the same file is the same lock.
        std::os::unix::fs::symlink(dir.path(), dir.join("alias")).unwrap();
        assert!(try_lock(&dir.join("alias/x.lock"), "busy").is_err());
        drop(first);
        drop(try_lock(&path, "busy").unwrap());
        assert!(path.exists(), "the lock file is kept");
    }

    #[test]
    fn lock_is_exclusive_across_processes_and_released_by_a_dead_owner() {
        let dir = TempDir::new("lock");
        let path = dir.join("x.lock");
        // This test binary, re-run as a helper process, holds the lock.
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "lock::tests::hold_lock_helper",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("GITPM_TEST_HOLD_LOCK", &path)
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut reader = std::io::BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        while !line.contains("PM_LOCK_HELD") {
            line.clear();
            assert_ne!(
                std::io::BufRead::read_line(&mut reader, &mut line).unwrap(),
                0,
                "helper exited"
            );
        }
        assert_eq!(try_lock(&path, "busy").unwrap_err().kind(), "locked");
        child.kill().unwrap();
        child.wait().unwrap();
        drop(try_lock(&path, "busy").expect("a killed owner releases the lock"));
    }

    #[test]
    #[ignore = "helper process for lock_is_exclusive_across_processes"]
    fn hold_lock_helper() {
        let Some(path) = std::env::var_os("GITPM_TEST_HOLD_LOCK") else {
            return;
        };
        let _lock = try_lock(Path::new(&path), "busy").unwrap();
        println!("PM_LOCK_HELD");
        std::thread::sleep(std::time::Duration::from_secs(30));
    }

    #[test]
    fn released_lock_is_free_while_other_threads_spawn_subprocesses() {
        let dir = TempDir::new("lock");
        let path = dir.join("x.lock");
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let spawners: Vec<_> = (0..4)
            .map(|_| {
                let stop = stop.clone();
                std::thread::spawn(move || {
                    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                        let _ = Command::new("true").status();
                    }
                })
            })
            .collect();
        for _ in 0..500 {
            drop(try_lock(&path, "busy").expect("no inherited lock"));
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        for spawner in spawners {
            spawner.join().unwrap();
        }
    }
}
