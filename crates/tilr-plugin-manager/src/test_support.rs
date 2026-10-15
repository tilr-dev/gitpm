//! Shared fixtures for unit tests: unique temp directories, hermetic `git`, small plugin
//! repositories and a tree hash for byte-identical comparisons. Test-only.
use crate::catalog::{FileDigest, Release};
use crate::{Manifest, State};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// Unique directory under the OS temp dir, removed on drop
/// (kept when `PM_KEEP_TEST_ARTIFACTS` is set, as in the integration tests).
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
    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
    pub(crate) fn join(&self, part: impl AsRef<Path>) -> PathBuf {
        self.0.join(part)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        if std::env::var_os("PM_KEEP_TEST_ARTIFACTS").is_none() {
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

pub(crate) fn manifest(id: &str, version: &str) -> Manifest {
    Manifest {
        id: id.into(),
        version: version.into(),
        dependencies: Default::default(),
        fork_of: None,
    }
}
pub(crate) fn write_manifest(repo: &Path, m: &Manifest) {
    fs::write(
        repo.join("plugin.json"),
        crate::model::manifest_json(m).unwrap(),
    )
    .unwrap();
}
pub(crate) fn write_json(path: &Path, value: &serde_json::Value) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
pub(crate) fn read_json(path: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

/// An author's plugin repository: `plugin.json` + `dist/index.js`, branch `main`.
pub(crate) struct Author {
    pub(crate) path: PathBuf,
    pub(crate) id: String,
}
impl Author {
    /// Create, commit and tag `id@0.1.0` (`v0.1.0` for a legacy ID).
    pub(crate) fn new(path: PathBuf, id: &str) -> Self {
        let author = Self::untagged(path, id, "0.1.0");
        author.tag("0.1.0");
        author
    }
    /// Create and commit `version` without a release tag.
    pub(crate) fn untagged(path: PathBuf, id: &str, version: &str) -> Self {
        fs::create_dir_all(path.join("dist")).unwrap();
        git(&path, &["init", "-q", "-b", "main"]);
        let author = Self {
            path,
            id: id.into(),
        };
        author.write(version);
        git(&author.path, &["add", "--all"]);
        git(&author.path, &["commit", "-q", "-m", version]);
        author
    }
    fn write(&self, version: &str) {
        write_manifest(&self.path, &manifest(&self.id, version));
        fs::write(
            self.path.join("dist/index.js"),
            format!("export const v = \"{version}\";\n"),
        )
        .unwrap();
    }
    pub(crate) fn source(&self) -> &str {
        self.path.to_str().unwrap()
    }
    /// Commit a new version (manifest + dist) without tagging; returns the commit.
    pub(crate) fn commit(&self, version: &str) -> String {
        self.write(version);
        git(&self.path, &["add", "--all"]);
        git(&self.path, &["commit", "-q", "-m", version]);
        self.head()
    }
    /// Commit and tag a release; returns the commit.
    pub(crate) fn release(&self, version: &str) -> String {
        let commit = self.commit(version);
        self.tag(version);
        commit
    }
    pub(crate) fn tag(&self, version: &str) {
        git(
            &self.path,
            &["tag", &crate::identity::release_tag(&self.id, version)],
        );
    }
    pub(crate) fn head(&self) -> String {
        git(&self.path, &["rev-parse", "HEAD"])
    }
}

/// Deterministic hash of a directory tree: relative paths, entry types, file bytes and
/// symlink targets (not timestamps). Equal hashes mean a byte-identical tree.
pub(crate) fn tree_hash(root: &Path) -> String {
    fn walk(base: &Path, path: &Path, hasher: &mut Sha256) {
        let meta = fs::symlink_metadata(path).unwrap();
        let name = path
            .strip_prefix(base)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        if meta.file_type().is_symlink() {
            hasher.update(format!(
                "L {name} -> {}\0",
                fs::read_link(path).unwrap().display()
            ));
        } else if meta.is_dir() {
            hasher.update(format!("D {name}\0"));
            let mut entries: Vec<_> = fs::read_dir(path)
                .unwrap()
                .map(|e| e.unwrap().path())
                .collect();
            entries.sort();
            for entry in entries {
                walk(base, &entry, hasher);
            }
        } else {
            let bytes = fs::read(path).unwrap();
            hasher.update(format!("F {name} {}\0", bytes.len()));
            hasher.update(&bytes);
        }
    }
    let mut hasher = Sha256::new();
    walk(root, root, &mut hasher);
    format!("{:x}", hasher.finalize())
}

/// A syntactically valid catalog release for `id@version` (no Git needed).
pub(crate) fn release(id: &str, version: &str, source: &str, commit_char: char) -> Release {
    Release {
        manifest: manifest(id, version),
        source: source.into(),
        commit: commit_char.to_string().repeat(40),
        reference: crate::identity::release_tag(id, version),
        files: BTreeMap::from([(
            "index.js".to_owned(),
            FileDigest {
                sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into(),
                size: 3,
            },
        )]),
        build: None,
    }
}

/// State with one installed plugin.
pub(crate) fn state_with(id: &str, source: &str, version: &str, commit: &str) -> State {
    State {
        plugins: BTreeMap::from([(
            id.to_owned(),
            crate::Installed {
                source: source.into(),
                current: crate::Revision {
                    version: version.into(),
                    commit: commit.into(),
                },
                previous: None,
                enabled: true,
                fork_of: None,
            },
        )]),
    }
}
