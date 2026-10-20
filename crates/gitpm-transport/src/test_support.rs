use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// Unique directory under the OS temp dir, removed on drop
/// (kept when `GITPM_KEEP_TEST_ARTIFACTS` is set, as in the integration tests).
pub(crate) struct TempDir(PathBuf);
impl TempDir {
    pub(crate) fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "pm-unit-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        // Canonical path: on macOS the temp dir is behind the /var -> /private/var symlink.
        Self(fs::canonicalize(path).unwrap())
    }
    pub(crate) fn join(&self, part: impl AsRef<Path>) -> PathBuf {
        self.0.join(part)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        if std::env::var_os("GITPM_KEEP_TEST_ARTIFACTS").is_none() {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

/// Settings passed on every fixture `git` call so fixtures never depend on the
/// developer's Git configuration, even when `.cargo/config.toml` is not applied.
const HERMETIC: &[&str] = &[
    "-c",
    "user.name=Test",
    "-c",
    "user.email=test@example.invalid",
    "-c",
    "commit.gpgsign=false",
    "-c",
    "tag.gpgsign=false",
    "-c",
    "init.defaultBranch=main",
    "-c",
    "core.hooksPath=/dev/null",
];

fn run_git(repo: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(HERMETIC)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .unwrap()
}
/// Fixture `git`; panics on failure, returns trimmed stdout.
pub(crate) fn git(repo: &Path, args: &[&str]) -> String {
    let out = run_git(repo, args);
    assert!(
        out.status.success(),
        "git {args:?} in {}: {}",
        repo.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}

pub(crate) struct Author {
    pub path: PathBuf,
    id: String,
}
impl Author {
    pub fn new(path: PathBuf, id: &str) -> Self {
        fs::create_dir_all(&path).unwrap();
        git(&path, &["init", "-q", "-b", "main"]);
        let a = Self {
            path,
            id: id.into(),
        };
        a.commit("0.1.0");
        git(&a.path, &["tag", &format!("{}@0.1.0", a.id)]);
        a
    }
    pub fn commit(&self, version: &str) -> String {
        fs::write(self.path.join("version"), version).unwrap();
        git(&self.path, &["add", "--all"]);
        git(&self.path, &["commit", "-q", "-m", version]);
        self.head()
    }
    pub fn head(&self) -> String {
        git(&self.path, &["rev-parse", "HEAD"])
    }
    pub fn source(&self) -> &str {
        self.path.to_str().unwrap()
    }
}
