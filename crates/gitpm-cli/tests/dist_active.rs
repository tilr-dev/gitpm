//! End-to-end tests of the `dist/` rule: `dist/<id>` exists exactly for the active
//! packages (installed, enabled, not replaced), whatever command changed the state.
//! Hermetic: the CLI and Git run with a cleared environment, a fixture HOME/TMPDIR and
//! Git config; every path is inside a per-test temp directory removed on drop.
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    dir: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "gitpm-cli-dist-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("home")).unwrap();
        fs::create_dir_all(dir.join("tmp")).unwrap();
        fs::write(
            dir.join("home/.gitconfig"),
            "[user]\n\tname = Test\n\temail = test@example.invalid\n[init]\n\tdefaultBranch = main\n[commit]\n\tgpgsign = false\n[tag]\n\tgpgsign = false\n",
        )
        .unwrap();
        Self { dir }
    }
    fn command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut command = Command::new(program);
        command
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", self.dir.join("home"))
            .env("TMPDIR", self.dir.join("tmp"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", self.dir.join("home/.gitconfig"));
        command
    }
    fn git(&self, repo: &Path, args: &[&str]) {
        let out = self
            .command("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    fn run(&self, args: &[&str]) -> Output {
        self.command(env!("CARGO_BIN_EXE_gitpm"))
            .current_dir(&self.dir)
            .arg("--root")
            .arg(self.root())
            .arg("--catalog")
            .arg(self.dir.join("catalog"))
            .args(args)
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap()
    }
    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }
    /// Failing command: returns stderr.
    fn fails(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(!out.status.success(), "{args:?} must fail");
        String::from_utf8(out.stderr).unwrap()
    }
    fn root(&self) -> PathBuf {
        self.dir.join("workspace")
    }
    fn path(&self, relative: &str) -> PathBuf {
        self.dir.join(relative)
    }
    fn state(&self) -> serde_json::Value {
        serde_json::from_str(&self.ok(&["list"])).unwrap()
    }
    /// `dist/<id>/index.js`, `None` when `dist/<id>` does not exist.
    fn dist(&self, id: &str) -> Option<String> {
        let folder = self.root().join("dist").join(id);
        folder
            .exists()
            .then(|| fs::read_to_string(folder.join("index.js")).unwrap())
    }
    /// Commit `contents` as `dist/index.js` of the single-package repository `repo` and
    /// release it as `version`.
    fn release(&self, repo: &str, contents: &str, version: &str) {
        fs::write(self.path(repo).join("dist/index.js"), contents).unwrap();
        self.ok(&["commit", self.path(repo).to_str().unwrap(), version]);
    }
    /// `vendor/demo` installed from `vendor/demo` (dist "vendor 1"), and its fork
    /// `dan/demo` released from `dan/demo` (dist "fork") and installed.
    fn original_and_fork(&self) {
        self.ok(&["init", "vendor/demo", "--into", "vendor/demo"]);
        self.release("vendor/demo", "vendor 1", "0.1.1");
        self.ok(&["install", self.path("vendor/demo").to_str().unwrap()]);
        self.ok(&["fork", "vendor/demo", "--ns", "dan", "--into", "dan/demo"]);
        self.release("dan/demo", "fork", "0.2.0");
        self.ok(&["install", self.path("dan/demo").to_str().unwrap()]);
        assert_eq!(self.dist("vendor/demo").as_deref(), Some("vendor 1"));
        assert_eq!(self.dist("dan/demo").as_deref(), Some("fork"));
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if std::env::var_os("GITPM_KEEP_TEST_ARTIFACTS").is_none() {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
}

#[test]
fn replace_removes_the_original_dist_and_back_restores_it_from_the_clone() {
    let f = Fixture::new();
    f.original_and_fork();
    f.ok(&["replace", "vendor/demo", "--with", "dan/demo"]);
    assert_eq!(f.dist("vendor/demo"), None);
    assert!(
        !f.root().join("dist/vendor").exists(),
        "empty namespace removed"
    );
    // The fork keeps its own dist; nothing is copied into the original's folder.
    assert_eq!(f.dist("dan/demo").as_deref(), Some("fork"));
    // list still reports the original as installed and enabled.
    let state = f.state();
    assert_eq!(state["packages"]["vendor/demo"]["enabled"], true);
    assert_eq!(state["replacements"]["vendor/demo"], "dan/demo");
    // resolved[].active is the dist rule; enabled alone is not.
    let resolved = |state: &serde_json::Value, id: &str| {
        let entry = state["resolved"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == id)
            .unwrap()
            .clone();
        (entry["enabled"].clone(), entry["active"].clone())
    };
    assert_eq!(resolved(&state, "vendor/demo"), (true.into(), false.into()));
    assert_eq!(resolved(&state, "dan/demo"), (true.into(), true.into()));
    let out = f.ok(&["replace", "vendor/demo", "--back"]);
    assert!(
        out.contains(
            "; dan/demo stays installed and enabled (disable or uninstall it if not wanted)"
        ),
        "{out}"
    );
    assert_eq!(
        resolved(&f.state(), "vendor/demo"),
        (true.into(), true.into())
    );
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 1"));
    assert_eq!(f.dist("dan/demo").as_deref(), Some("fork"));
}

#[test]
fn disable_removes_and_enable_restores_also_around_a_replacement() {
    let f = Fixture::new();
    f.original_and_fork();
    f.ok(&["disable", "vendor/demo"]);
    assert_eq!(f.dist("vendor/demo"), None);
    f.ok(&["enable", "vendor/demo"]);
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 1"));
    // Disabled and replaced: switching back keeps it out until it is enabled.
    f.ok(&["replace", "vendor/demo", "--with", "dan/demo"]);
    f.ok(&["disable", "vendor/demo"]);
    f.ok(&["replace", "vendor/demo", "--back"]);
    assert_eq!(f.dist("vendor/demo"), None);
    f.ok(&["enable", "vendor/demo"]);
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 1"));
    // Disabling the replacement would leave nothing loaded: refused without --force.
    f.ok(&["replace", "vendor/demo", "--with", "dan/demo"]);
    let error = f.fails(&["disable", "dan/demo"]);
    assert!(
        error.contains("cannot disable dan/demo: it replaces vendor/demo"),
        "{error}"
    );
    assert_eq!(f.dist("dan/demo").as_deref(), Some("fork"));
    // Forced: the original stays replaced, so neither is deployed.
    f.ok(&["disable", "dan/demo", "--force"]);
    assert_eq!(f.dist("dan/demo"), None);
    assert_eq!(f.dist("vendor/demo"), None);
    assert!(!f.root().join("dist/dan").exists());
    f.ok(&["enable", "dan/demo"]);
    assert_eq!(f.dist("dan/demo").as_deref(), Some("fork"));
    // A disabled package cannot become a replacement.
    f.ok(&["replace", "vendor/demo", "--back"]);
    f.ok(&["disable", "dan/demo"]);
    let error = f.fails(&["replace", "vendor/demo", "--with", "dan/demo"]);
    assert!(
        error.contains("dan/demo is disabled; enable it first"),
        "{error}"
    );
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 1"));
}

#[test]
fn update_and_rollback_of_a_replaced_package_leave_dist_alone_until_back() {
    let f = Fixture::new();
    f.original_and_fork();
    f.ok(&["replace", "vendor/demo", "--with", "dan/demo"]);
    f.release("vendor/demo", "vendor 2", "0.3.0");
    f.ok(&["update", "vendor/demo"]);
    assert_eq!(
        f.state()["packages"]["vendor/demo"]["current"]["version"],
        "0.3.0"
    );
    assert_eq!(f.dist("vendor/demo"), None);
    f.ok(&["replace", "vendor/demo", "--back"]);
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 2"));
    // Active again: rollback deploys as usual.
    f.ok(&["rollback", "vendor/demo"]);
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 1"));
    // Replaced: rollback only changes state and clone.
    f.ok(&["replace", "vendor/demo", "--with", "dan/demo"]);
    f.ok(&["rollback", "vendor/demo"]);
    assert_eq!(f.dist("vendor/demo"), None);
    f.ok(&["replace", "vendor/demo", "--back"]);
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 2"));
}

#[test]
fn installing_a_package_that_is_replaced_deploys_nothing() {
    let f = Fixture::new();
    f.original_and_fork();
    f.ok(&["replace", "vendor/demo", "--with", "dan/demo"]);
    // Uninstalling the original keeps the replacement binding.
    f.ok(&["uninstall", "vendor/demo"]);
    assert_eq!(f.state()["replacements"]["vendor/demo"], "dan/demo");
    f.ok(&["install", f.path("vendor/demo").to_str().unwrap()]);
    assert_eq!(f.dist("vendor/demo"), None);
    f.ok(&["replace", "vendor/demo", "--back"]);
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 1"));
}

#[test]
fn fork_replace_ends_with_only_the_fork_deployed() {
    let f = Fixture::new();
    f.ok(&["init", "vendor/demo", "--into", "vendor/demo"]);
    f.ok(&["install", f.path("vendor/demo").to_str().unwrap()]);
    let original = f.dist("vendor/demo").unwrap();
    f.ok(&[
        "fork",
        "vendor/demo",
        "--ns",
        "dan",
        "--into",
        "dan/demo",
        "--replace",
    ]);
    assert_eq!(f.dist("vendor/demo"), None);
    assert_eq!(f.dist("dan/demo").as_deref(), Some(original.as_str()));
    assert_eq!(f.state()["replacements"]["vendor/demo"], "dan/demo");
}

#[test]
fn uninstalling_or_forgetting_the_replacement_deploys_the_original_again() {
    let f = Fixture::new();
    f.original_and_fork();
    f.ok(&["replace", "vendor/demo", "--with", "dan/demo"]);
    // forget drops the replacement (and the fork's dist); restore keeps it dropped.
    f.ok(&["forget", "dan/demo"]);
    assert!(f.state().get("replacements").is_none());
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 1"));
    assert_eq!(f.dist("dan/demo"), None);
    f.ok(&["restore", "dan/demo"]);
    assert_eq!(f.dist("dan/demo").as_deref(), Some("fork"));
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 1"));
    // uninstall drops the replacement too.
    f.ok(&["replace", "vendor/demo", "--with", "dan/demo"]);
    assert_eq!(f.dist("vendor/demo"), None);
    f.ok(&["uninstall", "dan/demo"]);
    assert!(f.state().get("replacements").is_none());
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 1"));
    assert!(!f.root().join("dist/dan").exists());
}

/// Repository `mono` with packages `tilr/store` and `tilr/theme` (dist "NAME VERSION").
fn mono(f: &Fixture) -> PathBuf {
    let mono = f.path("mono");
    fs::create_dir_all(&mono).unwrap();
    f.git(&mono, &["init", "-q", "-b", "main"]);
    fs::write(mono.join("gitpm.json"), r#"{"packages": ["packages/*"]}"#).unwrap();
    for name in ["store", "theme"] {
        write_package(&mono, name, "1.0.0", &format!("{name} 1.0.0"));
    }
    f.git(&mono, &["add", "--all"]);
    f.git(&mono, &["commit", "-q", "-m", "first"]);
    f.git(&mono, &["tag", "tilr/store@1.0.0"]);
    f.git(&mono, &["tag", "tilr/theme@1.0.0"]);
    mono
}
fn write_package(mono: &Path, name: &str, version: &str, contents: &str) {
    let folder = mono.join("packages").join(name);
    fs::create_dir_all(folder.join("dist")).unwrap();
    fs::write(
        folder.join("plugin.json"),
        format!(r#"{{"id":"tilr/{name}","version":"{version}"}}"#),
    )
    .unwrap();
    fs::write(folder.join("dist/index.js"), contents).unwrap();
}
fn release_package(f: &Fixture, mono: &Path, name: &str, version: &str) {
    write_package(mono, name, version, &format!("{name} {version}"));
    f.git(mono, &["add", "--all"]);
    f.git(mono, &["commit", "-q", "-m", &format!("{name} {version}")]);
    f.git(mono, &["tag", &format!("tilr/{name}@{version}")]);
}

#[test]
fn a_package_of_a_shared_clone_is_deployed_from_its_own_commit() {
    let f = Fixture::new();
    let mono = mono(&f);
    let source = mono.to_str().unwrap();
    f.ok(&["install", source, "tilr/store"]);
    f.ok(&["install", source, "tilr/theme"]);
    f.ok(&[
        "fork",
        "tilr/theme",
        "--ns",
        "me",
        "--into",
        "forks/me",
        "--replace",
    ]);
    assert_eq!(f.dist("tilr/theme"), None);
    assert_eq!(f.dist("me/theme").as_deref(), Some("theme 1.0.0"));
    assert_eq!(f.dist("tilr/store").as_deref(), Some("store 1.0.0"));
    // theme 1.1.0 is installed while replaced; then store moves the shared clone's HEAD
    // to a commit whose theme files differ from theme's installed revision.
    release_package(&f, &mono, "theme", "1.1.0");
    f.ok(&["update", "tilr/theme"]);
    assert_eq!(f.dist("tilr/theme"), None);
    fs::write(
        mono.join("packages/theme/dist/index.js"),
        "theme unreleased",
    )
    .unwrap();
    release_package(&f, &mono, "store", "1.1.0");
    f.ok(&["update", "tilr/store"]);
    assert_eq!(f.dist("tilr/store").as_deref(), Some("store 1.1.0"));
    f.ok(&["replace", "tilr/theme", "--back"]);
    assert_eq!(f.dist("tilr/theme").as_deref(), Some("theme 1.1.0"));
    assert_eq!(f.dist("me/theme").as_deref(), Some("theme 1.0.0"));
    f.ok(&["disable", "tilr/theme"]);
    assert_eq!(f.dist("tilr/theme"), None);
    f.ok(&["enable", "tilr/theme"]);
    assert_eq!(f.dist("tilr/theme").as_deref(), Some("theme 1.1.0"));
}

#[test]
fn sync_dist_repairs_a_tree_written_by_an_older_gitpm() {
    let f = Fixture::new();
    f.original_and_fork();
    f.ok(&["replace", "vendor/demo", "--with", "dan/demo"]);
    // As gitpm 0.4 left it: the replaced original deployed, the fork's dist missing,
    // plus a folder of no known package.
    let dist = f.root().join("dist");
    fs::create_dir_all(dist.join("vendor/demo")).unwrap();
    fs::write(dist.join("vendor/demo/index.js"), "stale").unwrap();
    fs::remove_dir_all(dist.join("dan")).unwrap();
    fs::create_dir_all(dist.join("stray")).unwrap();
    // Read-only commands never rewrite dist.
    f.state();
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("stale"));
    let out = f.run(&["sync-dist", "--dry-run"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("dan/demo would be deployed to dist/dan/demo"),
        "{stdout}"
    );
    assert!(
        stdout.contains("vendor/demo would be removed from dist/vendor/demo"),
        "{stdout}"
    );
    assert!(stdout.contains("Nothing was changed"), "{stdout}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("dist/stray belongs to no known package"),
        "{stderr}"
    );
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("stale"));
    assert_eq!(f.dist("dan/demo"), None);
    let out = f.ok(&["sync-dist"]);
    assert!(out.contains("dan/demo deployed to dist/dan/demo"), "{out}");
    assert!(
        out.contains("vendor/demo removed from dist/vendor/demo"),
        "{out}"
    );
    assert_eq!(f.dist("vendor/demo"), None);
    assert!(!dist.join("vendor").exists());
    assert_eq!(f.dist("dan/demo").as_deref(), Some("fork"));
    assert!(
        dist.join("stray").is_dir(),
        "unmanaged folders are left alone"
    );
    let out = f.ok(&["sync-dist"]);
    assert!(
        out.contains("already holds exactly the active packages"),
        "{out}"
    );
    // A disabled package's leftover dist is removed as well.
    f.ok(&["replace", "vendor/demo", "--back"]);
    f.ok(&["disable", "vendor/demo"]);
    fs::create_dir_all(dist.join("vendor/demo")).unwrap();
    let out = f.ok(&["sync-dist"]);
    assert!(
        out.contains("vendor/demo removed from dist/vendor/demo"),
        "{out}"
    );
    assert_eq!(f.dist("vendor/demo"), None);
    // Other options are refused.
    assert!(!f.run(&["sync-dist", "--force"]).status.success());
    assert!(!f.run(&["sync-dist", "vendor/demo"]).status.success());
}

#[test]
fn any_mutating_command_reconciles_the_whole_dist() {
    let f = Fixture::new();
    f.original_and_fork();
    f.ok(&["disable", "dan/demo"]);
    // Drift no command of this test touches: an active package's dist missing, a
    // disabled package deployed, a folder of no known package.
    let dist = f.root().join("dist");
    fs::remove_dir_all(dist.join("vendor")).unwrap();
    fs::create_dir_all(dist.join("dan/demo")).unwrap();
    fs::write(dist.join("dan/demo/index.js"), "stale").unwrap();
    fs::create_dir_all(dist.join("stray")).unwrap();
    f.state();
    assert_eq!(f.dist("vendor/demo"), None, "list never changes dist");
    // An unrelated install fixes all of it.
    f.ok(&["init", "other/pkg", "--into", "other/pkg"]);
    let out = f.run(&["install", f.path("other/pkg").to_str().unwrap()]);
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("dist/stray belongs to no known package"),
        "{stderr}"
    );
    assert!(f.dist("other/pkg").is_some());
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 1"));
    assert_eq!(f.dist("dan/demo"), None);
    assert!(!dist.join("dan").exists());
    assert!(
        dist.join("stray").is_dir(),
        "unmanaged folders are left alone"
    );
    let out = f.ok(&["sync-dist", "--dry-run"]);
    assert!(
        out.contains("already holds exactly the active packages"),
        "{out}"
    );
}

#[test]
fn a_namespace_path_that_is_not_a_folder_blocks_nothing_else() {
    let f = Fixture::new();
    f.original_and_fork();
    // dist/dan becomes a symlink to a folder outside the workspace.
    let dist = f.root().join("dist");
    let outside = f.path("outside");
    fs::rename(dist.join("dan"), &outside).unwrap();
    std::os::unix::fs::symlink(&outside, dist.join("dan")).unwrap();
    fs::remove_dir_all(dist.join("vendor")).unwrap();
    f.ok(&["init", "other/pkg", "--into", "other/pkg"]);
    let out = f.run(&["install", f.path("other/pkg").to_str().unwrap()]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(
        stderr.contains("dist/dan belongs to no known package or is not a folder"),
        "{stderr}"
    );
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 1"));
    assert!(
        fs::symlink_metadata(dist.join("dan"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_to_string(outside.join("demo/index.js")).unwrap(),
        "fork",
        "never written through"
    );
    // sync-dist reports it and syncs the rest.
    fs::remove_dir_all(dist.join("vendor")).unwrap();
    let out = f.run(&["sync-dist"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(
        stderr.contains("dist/dan belongs to no known package or is not a folder"),
        "{stderr}"
    );
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 1"));
    // A plain file there behaves the same.
    fs::remove_file(dist.join("dan")).unwrap();
    fs::write(dist.join("dan"), "file").unwrap();
    f.ok(&["disable", "vendor/demo"]);
    assert_eq!(f.dist("vendor/demo"), None);
    assert_eq!(fs::read_to_string(dist.join("dan")).unwrap(), "file");
}

#[test]
fn a_missing_clone_is_reported_and_blocks_only_its_own_dist() {
    let f = Fixture::new();
    f.original_and_fork();
    fs::remove_dir_all(f.root().join("packages/vendor")).unwrap();
    fs::remove_dir_all(f.root().join("dist/vendor")).unwrap();
    f.ok(&["init", "other/pkg", "--into", "other/pkg"]);
    let out = f.run(&["install", f.path("other/pkg").to_str().unwrap()]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(
        stderr.contains(
            "Warning: vendor/demo: dist not deployed (clone missing); run `gitpm sync-dist`"
        ),
        "{stderr}"
    );
    assert!(f.dist("other/pkg").is_some());
    // sync-dist removes stale dists anyway, then fails naming the package.
    f.ok(&["disable", "other/pkg"]);
    fs::create_dir_all(f.root().join("dist/other/pkg")).unwrap();
    let out = f.run(&["sync-dist"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(
        stderr.contains("sync-dist: 1 package(s) not deployed: vendor/demo"),
        "{stderr}"
    );
    assert_eq!(f.dist("other/pkg"), None);
    assert_eq!(f.dist("dan/demo").as_deref(), Some("fork"));
}

#[test]
fn every_deploy_copies_the_committed_tree_with_nested_files_and_exec_bits() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    f.original_and_fork();
    let repo = f.path("vendor/demo");
    fs::create_dir_all(repo.join("dist/bin")).unwrap();
    fs::create_dir_all(repo.join("dist/a/b")).unwrap();
    fs::write(repo.join("dist/bin/run.sh"), "#!/bin/sh\n").unwrap();
    fs::set_permissions(
        repo.join("dist/bin/run.sh"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    fs::write(repo.join("dist/a/b/c.txt"), "nested").unwrap();
    f.release("vendor/demo", "vendor 2", "0.3.0");
    let check = |when: &str| {
        let dist = f.root().join("dist/vendor/demo");
        assert_eq!(
            fs::read_to_string(dist.join("a/b/c.txt")).unwrap(),
            "nested",
            "{when}"
        );
        let mode = fs::metadata(dist.join("bin/run.sh"))
            .unwrap()
            .permissions()
            .mode();
        assert_ne!(mode & 0o111, 0, "{when}: executable bit");
        let mode = fs::metadata(dist.join("index.js"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0, "{when}: plain file");
    };
    f.ok(&["update", "vendor/demo"]);
    check("update");
    // Uncommitted edits in the clone are never deployed.
    let clone = f.root().join("packages/vendor/demo");
    fs::write(clone.join("dist/index.js"), "edited").unwrap();
    f.ok(&["replace", "vendor/demo", "--with", "dan/demo"]);
    f.ok(&["replace", "vendor/demo", "--back"]);
    check("--back");
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 2"));
    f.ok(&["disable", "vendor/demo"]);
    f.ok(&["enable", "vendor/demo"]);
    check("enable");
    fs::write(clone.join("dist/index.js"), "vendor 2").unwrap();
    f.ok(&["rollback", "vendor/demo"]);
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 1"));
    assert!(!f.root().join("dist/vendor/demo/bin").exists());
}
