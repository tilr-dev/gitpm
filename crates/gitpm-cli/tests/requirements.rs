//! Mandatory product acceptance gates, deliberately separate from regression tests.
//! A green regression suite does not imply dependency replacement is implemented.
//! Hermetic: the CLI and Git run with a cleared environment, a fixture HOME/TMPDIR and
//! Git config; every path is inside a per-test temp directory removed on drop.
mod common;
use gitpm::{Manager, scaffold};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);
impl TempDir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "gitpm-cli-{name}-{}-{}",
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
        Self(dir)
    }
}
impl std::ops::Deref for TempDir {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        if std::env::var_os("GITPM_KEEP_TEST_ARTIFACTS").is_none() {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

/// A process with nothing from the developer's environment except PATH.
fn hermetic(dir: &Path, program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    command
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", dir.join("home"))
        .env("TMPDIR", dir.join("tmp"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", dir.join("home/.gitconfig"));
    command
}
fn git(dir: &Path, repo: &Path, args: &[&str]) {
    let out = hermetic(dir, "git")
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

#[test]
fn required_dependent_plugin_installation() {
    let dir = TempDir::new("required-dependencies");
    let base = scaffold::init(&dir.join("base"), "base").unwrap().repo;
    let consumer = scaffold::init(&dir.join("consumer"), "consumer")
        .unwrap()
        .repo;
    let m = Manager::open(dir.join("workspace")).unwrap();
    m.install(base.to_str().unwrap()).unwrap();
    let base_id = common::manifest_id(&base);
    let consumer_id = common::manifest_id(&consumer);
    fs::write(
        consumer.join("plugin.json"),
        serde_json::json!({"id":consumer_id, "name":"consumer", "version":"0.1.1", "dependencies":{(base_id):"0.1.0"}}).to_string(),
    )
    .unwrap();
    for args in [
        &["add", "."][..],
        &["commit", "-m", "dependent consumer"][..],
        &["tag", "v0.1.1"][..],
    ] {
        git(&dir, &consumer, args);
    }
    let result = m.install(consumer.to_str().unwrap());
    assert!(
        result.is_ok(),
        "REQUIRED CAPABILITY MISSING: install a consumer of an already installed compatible module: {result:?}. Fork replacement, dependent update and switch-back cannot be accepted until this prerequisite works. Artifacts: {}",
        dir.display()
    );
    assert!(m.state().unwrap().packages.contains_key(&consumer_id));
    // Further replacement behavior is mandatory in ATQ UC-053..056; this gate alone
    // must never be reported as proof that replacement works.
}

fn pm(dir: &Path, args: &[&str]) -> std::process::Output {
    hermetic(dir, env!("CARGO_BIN_EXE_gitpm"))
        .current_dir(dir)
        .arg("--root")
        .arg(dir.join("workspace"))
        .arg("--catalog")
        .arg(dir.join("catalog"))
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap()
}
fn release(dir: &Path, repo: &Path, manifest: serde_json::Value) {
    fs::write(repo.join("plugin.json"), manifest.to_string()).unwrap();
    let id = manifest["id"].as_str().unwrap();
    let version = manifest["version"].as_str().unwrap();
    fs::write(repo.join("dist/index.js"), format!("{id} {version}")).unwrap();
    let tag = format!("v{version}");
    for args in [
        &["add", "--all"][..],
        &["commit", "-q", "-m", version][..],
        &["tag", &tag][..],
    ] {
        git(dir, repo, args);
    }
}

#[test]
fn required_global_fork_replacement_through_the_cli() {
    let dir = TempDir::new("required-replace");
    let catalog = dir.join("catalog");
    let author = |id: &str, deps: serde_json::Value, extra: serde_json::Value| {
        let repo = dir.join("authors").join(id);
        fs::create_dir_all(repo.join("dist")).unwrap();
        git(&dir, &repo, &["init", "-q", "-b", "main"]);
        let mut manifest = serde_json::json!({"id": id, "name": "fixture", "version": "1.0.0", "dependencies": deps});
        for (k, v) in extra.as_object().unwrap() {
            manifest[k] = v.clone();
        }
        release(&dir, &repo, manifest);
        gitpm::catalog::register(catalog.to_str().unwrap(), repo.to_str().unwrap(), "v1.0.0")
            .unwrap();
        repo
    };
    author(
        "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        serde_json::json!({}),
        serde_json::json!({}),
    );
    author(
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        serde_json::json!({"c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9": "^1.0"}),
        serde_json::json!({}),
    );
    author(
        "f5984d44-92bd-51ac-8e8f-d911dd824ef3",
        serde_json::json!({"c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9": ">=0.9.0"}),
        serde_json::json!({}),
    );
    author(
        "db33e43e-e652-5767-8e21-27fb3cef9723",
        serde_json::json!({}),
        serde_json::json!({"replaces": {"id": "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "version": "1.0.0"}}),
    );
    author(
        "4e02af40-ea75-510c-b524-5f21e6de2d55",
        serde_json::json!({}),
        serde_json::json!({"replaces": {"id": "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "version": "0.9.0"}}),
    );
    for id in [
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "f5984d44-92bd-51ac-8e8f-d911dd824ef3",
        "db33e43e-e652-5767-8e21-27fb3cef9723",
        "4e02af40-ea75-510c-b524-5f21e6de2d55",
    ] {
        let out = pm(&dir, &["install-catalog", id]);
        assert!(
            out.status.success(),
            "{id}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let list = |dir: &Path| -> serde_json::Value {
        serde_json::from_slice(&pm(dir, &["list"]).stdout).unwrap()
    };
    assert!(
        list(&dir)["packages"]["c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"].is_object(),
        "pulled in as a dependency"
    );

    // Compatible fork: no question.
    let out = pm(
        &dir,
        &[
            "replace",
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            "--with",
            "db33e43e-e652-5767-8e21-27fb3cef9723",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let state = list(&dir);
    assert_eq!(
        state["replacements"]["c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"],
        "db33e43e-e652-5767-8e21-27fb3cef9723"
    );
    for consumer in state["resolved"].as_array().unwrap().iter().filter(|p| {
        [
            "9692fc07-23c7-5f20-8779-f09374f1a571",
            "f5984d44-92bd-51ac-8e8f-d911dd824ef3",
        ]
        .contains(&p["id"].as_str().unwrap())
    }) {
        assert_eq!(
            consumer["dependencies"][0]["provider"],
            "db33e43e-e652-5767-8e21-27fb3cef9723"
        );
    }
    // Incompatible fork refuses without changes; force is rejected too.
    let before = fs::read(dir.join("workspace/state.json")).unwrap();
    let out = pm(
        &dir,
        &[
            "replace",
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            "--with",
            "4e02af40-ea75-510c-b524-5f21e6de2d55",
        ],
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(
            "9692fc07-23c7-5f20-8779-f09374f1a571 needs c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 ^1.0"
        ) && !stderr.contains("f5984d44-92bd-51ac-8e8f-d911dd824ef3 needs"),
        "{stderr}"
    );
    assert_eq!(fs::read(dir.join("workspace/state.json")).unwrap(), before);
    let out = pm(
        &dir,
        &[
            "replace",
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            "--with",
            "4e02af40-ea75-510c-b524-5f21e6de2d55",
            "--force",
        ],
    );
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--force is only valid"));
    assert_eq!(fs::read(dir.join("workspace/state.json")).unwrap(), before);
    assert_eq!(
        list(&dir)["replacements"]["c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"],
        "db33e43e-e652-5767-8e21-27fb3cef9723"
    );
    // The active provider is guarded while the original cannot take its consumers
    // over (here: disabled); switching back frees it.
    assert!(
        pm(&dir, &["disable", "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"])
            .status
            .success()
    );
    let out = pm(&dir, &["disable", "db33e43e-e652-5767-8e21-27fb3cef9723"]);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("9692fc07-23c7-5f20-8779-f09374f1a571, f5984d44-92bd-51ac-8e8f-d911dd824ef3")
    );
    assert!(
        pm(&dir, &["enable", "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"])
            .status
            .success()
    );
    let out = pm(
        &dir,
        &[
            "replace",
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            "--with",
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(list(&dir).get("replacements").is_none());
    let out = pm(&dir, &["uninstall", "4e02af40-ea75-510c-b524-5f21e6de2d55"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn direct_install_without_catalog_names_the_missing_dependency() {
    let dir = TempDir::new("no-catalog");
    let consumer = scaffold::init(&dir.join("consumer"), "consumer")
        .unwrap()
        .repo;
    let consumer_id = common::manifest_id(&consumer);
    release(
        &dir,
        &consumer,
        serde_json::json!({"id": consumer_id, "name": "fixture", "version": "0.2.0", "dependencies": {"757fd5ca-2b6a-5b2f-8a5d-60980b028665": "^1.0"}}),
    );
    // No --catalog and no GITPM_CATALOG: the missing dependency cannot be fetched.
    let out = hermetic(&dir, env!("CARGO_BIN_EXE_gitpm"))
        .current_dir(&*dir)
        .arg("--root")
        .arg(dir.join("workspace"))
        .args(["install", consumer.to_str().unwrap()])
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{stderr}");
    for needle in [
        "757fd5ca-2b6a-5b2f-8a5d-60980b028665",
        "no catalog configured",
        "Error kind: dependency_conflict",
    ] {
        assert!(stderr.contains(needle), "missing {needle:?}: {stderr}");
    }
    // A catalog location that does not exist is refused, never read as "no catalog".
    let out = pm(&dir, &["install", consumer.to_str().unwrap()]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{stderr}");
    assert!(stderr.contains("does not exist"), "{stderr}");
    assert!(stderr.contains("Error kind: invalid_argument"), "{stderr}");
}

/// `download-catalog` prints one line per package, `enable` refuses the disabled
/// dependency with a hint, `enable --force` enables only the named package.
#[test]
fn download_then_enable_through_the_cli() {
    let dir = TempDir::new("download-enable");
    let catalog = dir.join("catalog");
    for (id, deps) in [
        (
            "757fd5ca-2b6a-5b2f-8a5d-60980b028665",
            serde_json::json!({}),
        ),
        (
            "9692fc07-23c7-5f20-8779-f09374f1a571",
            serde_json::json!({"757fd5ca-2b6a-5b2f-8a5d-60980b028665": "^1"}),
        ),
    ] {
        let repo = dir.join("authors").join(id);
        fs::create_dir_all(repo.join("dist")).unwrap();
        git(&dir, &repo, &["init", "-q", "-b", "main"]);
        release(
            &dir,
            &repo,
            serde_json::json!({"id": id, "name": "fixture", "version": "1.0.0", "dependencies": deps}),
        );
        gitpm::catalog::register(catalog.to_str().unwrap(), repo.to_str().unwrap(), "v1.0.0")
            .unwrap();
    }
    let out = pm(
        &dir,
        &["download-catalog", "9692fc07-23c7-5f20-8779-f09374f1a571"],
    );
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 2, "{stdout}");
    assert!(
        lines[0].starts_with("downloaded 9692fc07-23c7-5f20-8779-f09374f1a571@1.0.0 from ")
            && lines[0].ends_with(" (disabled)"),
        "{stdout}"
    );
    assert!(
        lines[1].starts_with("downloaded 757fd5ca-2b6a-5b2f-8a5d-60980b028665@1.0.0 from ")
            && lines[1].ends_with(" (disabled)"),
        "{stdout}"
    );
    assert!(!dir.join("workspace/dist/app").exists());

    let out = pm(&dir, &["enable", "9692fc07-23c7-5f20-8779-f09374f1a571"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("757fd5ca-2b6a-5b2f-8a5d-60980b028665 is disabled; enable it"),
        "{stderr}"
    );
    assert!(
        stderr.contains("Error kind: dependency_conflict"),
        "{stderr}"
    );
    assert!(
        stderr.contains(
            "Hint: `gitpm enable 9692fc07-23c7-5f20-8779-f09374f1a571 --force` enables it anyway"
        ),
        "{stderr}"
    );
    assert!(
        !dir.join("workspace/dist/9692fc07-23c7-5f20-8779-f09374f1a571")
            .exists()
    );

    let out = pm(
        &dir,
        &["enable", "9692fc07-23c7-5f20-8779-f09374f1a571", "--force"],
    );
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "9692fc07-23c7-5f20-8779-f09374f1a571 enabled\n"
    );
    assert!(
        dir.join("workspace/dist/9692fc07-23c7-5f20-8779-f09374f1a571/index.js")
            .is_file()
    );
    assert!(
        !dir.join("workspace/dist/757fd5ca-2b6a-5b2f-8a5d-60980b028665")
            .exists()
    );

    // install over the (disabled) download: refused like enable, --force enables it.
    assert!(
        pm(&dir, &["disable", "9692fc07-23c7-5f20-8779-f09374f1a571"])
            .status
            .success()
    );
    let out = pm(
        &dir,
        &["install-catalog", "9692fc07-23c7-5f20-8779-f09374f1a571"],
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("757fd5ca-2b6a-5b2f-8a5d-60980b028665 is disabled; enable it"),
        "{stderr}"
    );
    let out = pm(
        &dir,
        &[
            "install-catalog",
            "9692fc07-23c7-5f20-8779-f09374f1a571",
            "--force",
        ],
    );
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.starts_with("installed 9692fc07-23c7-5f20-8779-f09374f1a571@1.0.0 from "),
        "{stdout}"
    );
    assert!(
        dir.join("workspace/dist/9692fc07-23c7-5f20-8779-f09374f1a571/index.js")
            .is_file()
    );

    // install-catalog --force of a new package next to the disabled 757fd5ca-2b6a-5b2f-8a5d-60980b028665.
    let repo = dir.join("authors/f5984d44-92bd-51ac-8e8f-d911dd824ef3");
    fs::create_dir_all(repo.join("dist")).unwrap();
    git(&dir, &repo, &["init", "-q", "-b", "main"]);
    release(
        &dir,
        &repo,
        serde_json::json!({"id": "f5984d44-92bd-51ac-8e8f-d911dd824ef3", "name": "fixture", "version": "1.0.0", "dependencies": {"757fd5ca-2b6a-5b2f-8a5d-60980b028665": "^1"}}),
    );
    gitpm::catalog::register(catalog.to_str().unwrap(), repo.to_str().unwrap(), "v1.0.0").unwrap();
    assert!(
        !pm(
            &dir,
            &["install-catalog", "f5984d44-92bd-51ac-8e8f-d911dd824ef3"]
        )
        .status
        .success()
    );
    let out = pm(
        &dir,
        &[
            "install-catalog",
            "f5984d44-92bd-51ac-8e8f-d911dd824ef3",
            "--force",
        ],
    );
    assert!(out.status.success(), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stdout)
            .starts_with("installed f5984d44-92bd-51ac-8e8f-d911dd824ef3@1.0.0 from ")
    );
    assert!(
        dir.join("workspace/dist/f5984d44-92bd-51ac-8e8f-d911dd824ef3/index.js")
            .is_file()
    );
    assert!(
        !dir.join("workspace/dist/757fd5ca-2b6a-5b2f-8a5d-60980b028665")
            .exists()
    );
}
