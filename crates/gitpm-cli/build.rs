//! Marks development builds: `gitpm --version` gets `-dev+<short hash>` when the crate is
//! built from a Git checkout of its repository whose HEAD is not at a release tag.
//! Never fails: without `git`, outside a checkout (crates.io, `cargo package`
//! verification) or with untracked sources the version stays `CARGO_PKG_VERSION`.
use std::path::Path;
use std::process::Command;

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let Some(dir) = std::env::var_os("CARGO_MANIFEST_DIR") else {
        return;
    };
    let dir = Path::new(&dir);
    // Only a checkout that tracks this crate counts, not an unrelated repository the
    // sources happen to sit in (an unpacked crate inside a home directory under Git).
    if git(dir, &["ls-files", "--error-unmatch", "build.rs"]).is_none() {
        return;
    }
    // Rebuild when HEAD moves or tags change. `--git-path` resolves per-worktree files
    // (HEAD, logs/HEAD) and shared ones (refs, packed-refs) also when `.git` is a file
    // pointing elsewhere. The branch's loose ref is watched even when it does not exist
    // (packed refs): a missing path always counts as changed, so the script reruns.
    let watch = |path: &str| {
        if let Some(file) = git(dir, &["rev-parse", "--git-path", path]) {
            println!("cargo:rerun-if-changed={}", dir.join(file).display());
        }
    };
    for path in ["HEAD", "logs/HEAD", "packed-refs", "refs/tags"] {
        watch(path);
    }
    if let Some(head) = git(dir, &["symbolic-ref", "-q", "HEAD"]) {
        watch(&head);
    }
    // Release tags only (vX.Y.Z); package tags such as `399f4c31-829e-5063-a806-47d92cd40867@1.0` do not count.
    if git(
        dir,
        &[
            "describe",
            "--exact-match",
            "--tags",
            "--match",
            "v[0-9]*",
            "HEAD",
        ],
    )
    .is_some()
    {
        return;
    }
    if let Some(hash) = git(dir, &["rev-parse", "--short", "HEAD"])
        && !hash.is_empty()
    {
        println!("cargo:rustc-env=GITPM_BUILD_SUFFIX=-dev+{hash}");
    }
}
