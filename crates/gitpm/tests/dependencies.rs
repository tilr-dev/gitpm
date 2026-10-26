//! Dependencies between packages, dependent guards, global fork replacement and the
//! single-catalog rule, through the public API (ATQ UC-053..059, UC-066).
use gitpm::{Error, InstalledPackage, Manager, Options, catalog, scaffold};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}

struct Fixture {
    dir: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "pm-dependencies-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self { dir }
    }
    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }
    /// Author repository for `id`; every call to `release` tags a new version.
    fn author(&self, id: &str) -> PathBuf {
        let repo = self.path(&format!("authors/{id}"));
        fs::create_dir_all(repo.join("dist")).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        repo
    }
    fn release(&self, repo: &Path, id: &str, version: &str, deps: &[(&str, &str)]) -> String {
        self.release_manifest(
            repo,
            json!({
                "id": id,
                "name": "fixture", "version": version,
                "dependencies": deps.iter().map(|(k, v)| (k.to_string(), json!(v))).collect::<serde_json::Map<_, _>>(),
            }),
        )
    }
    fn release_manifest(&self, repo: &Path, manifest: Value) -> String {
        let id = manifest["id"].as_str().unwrap().to_owned();
        let version = manifest["version"].as_str().unwrap().to_owned();
        fs::write(
            repo.join("plugin.json"),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        fs::write(repo.join("dist/index.js"), format!("{id} {version}")).unwrap();
        git(repo, &["add", "--all"]);
        git(repo, &["commit", "-q", "-m", &version]);
        let tag = format!("v{version}");
        git(repo, &["tag", &tag]);
        tag
    }
    /// Release and register in the catalog.
    fn publish(&self, catalog: &str, id: &str, version: &str, deps: &[(&str, &str)]) {
        let repo = self.path(&format!("authors/{id}"));
        let repo = if repo.exists() { repo } else { self.author(id) };
        let tag = self.release(&repo, id, version, deps);
        catalog::register(catalog, repo.to_str().unwrap(), &tag).unwrap();
    }
    fn catalog(&self, name: &str) -> String {
        self.path(name).to_str().unwrap().to_owned()
    }
    /// Mark a catalog verified, as its owner does by hand in `catalog.json`.
    fn verify(&self, catalog: &str) {
        let path = Path::new(catalog).join("catalog.json");
        let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        value["verified"] = json!(true);
        fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    }
    fn open(&self, catalog: &str) -> Manager {
        Manager::open_with(
            self.path("workspace"),
            Options {
                catalog: Some(catalog.into()),
                custom_ssh: false,
            },
        )
        .unwrap()
    }
    fn snapshot(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        fn walk(dir: &Path, base: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, base, out);
                } else {
                    out.insert(
                        path.strip_prefix(base).unwrap().into(),
                        fs::read(&path).unwrap(),
                    );
                }
            }
        }
        let mut out = BTreeMap::new();
        let root = self.path("workspace");
        walk(&root, &root, &mut out);
        out
    }
    /// Run a failing operation; the workspace must stay byte-identical.
    fn refuses<T: std::fmt::Debug>(
        &self,
        kind: &str,
        op: impl FnOnce() -> gitpm::Result<T>,
    ) -> Error {
        let before = self.snapshot();
        let error = op().expect_err("operation must fail");
        assert_eq!(error.kind(), kind, "{error}");
        assert!(self.snapshot() == before, "workspace changed by: {error}");
        error
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if std::env::var_os("GITPM_KEEP_TEST_ARTIFACTS").is_none() {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
}

fn plugin(m: &Manager, id: &str) -> InstalledPackage {
    m.list()
        .unwrap()
        .into_iter()
        .find(|p| p.id == id)
        .unwrap_or_else(|| panic!("{id} not installed"))
}
fn provider(m: &Manager, consumer: &str, dep: &str) -> (Option<String>, bool) {
    let p = plugin(m, consumer);
    let d = p.dependencies.iter().find(|d| d.id == dep).unwrap();
    (d.provider.clone(), d.satisfied)
}
fn ids(m: &Manager) -> Vec<String> {
    m.list().unwrap().into_iter().map(|p| p.id).collect()
}

/// 757fd5ca-2b6a-5b2f-8a5d-60980b028665 1.0.0, 1.1.0, 2.0.0; 9692fc07-23c7-5f20-8779-f09374f1a571 needs ^1.0; f5984d44-92bd-51ac-8e8f-d911dd824ef3 needs ^2.0; 1a620c0a-aa3c-533c-9960-3d49016fa231 needs ^1.
fn core_catalog(f: &Fixture) -> String {
    let cat = f.catalog("catalog");
    for v in ["1.0.0", "1.1.0", "2.0.0"] {
        f.publish(&cat, "757fd5ca-2b6a-5b2f-8a5d-60980b028665", v, &[]);
    }
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.0.0",
        &[("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^1.0")],
    );
    f.publish(
        &cat,
        "f5984d44-92bd-51ac-8e8f-d911dd824ef3",
        "1.0.0",
        &[("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^2.0")],
    );
    f.publish(
        &cat,
        "1a620c0a-aa3c-533c-9960-3d49016fa231",
        "1.0.0",
        &[("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^1")],
    );
    cat
}

/// Model the lifetime of update-server-info's packs_XXXXXX without racing a timer.
/// A filesystem clone opens the FIFO, then its writer removes the pathname before
/// closing: Git's copy_file_with_time fails when it stats the vanished source.
/// A transport clone never opens it; after the operation, a reader releases the
/// writer so cleanup also completes in that case.
fn with_transient_server_info<T>(repo: &Path, operation: impl FnOnce() -> T) -> T {
    use std::os::unix::{ffi::OsStrExt, fs::OpenOptionsExt};
    let path = repo.join(".git/objects/info/packs_controlled");
    let cpath = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    // SAFETY: cpath is a valid NUL-terminated path; mkfifo retains no pointer.
    assert_eq!(unsafe { libc::mkfifo(cpath.as_ptr(), 0o600) }, 0);
    let writer_path = path.clone();
    let writer = std::thread::spawn(move || {
        let file = fs::OpenOptions::new()
            .write(true)
            .open(&writer_path)
            .unwrap();
        fs::remove_file(writer_path).unwrap();
        drop(file);
    });
    let result = operation();
    let release = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path);
    writer.join().unwrap();
    drop(release);
    result
}

#[test]
fn local_source_clones_ignore_transient_server_info() {
    let f = Fixture::new();
    let repo = fs::canonicalize(f.author("71e491c8-518d-5990-951c-ee30686cebd0")).unwrap();
    let tag = f.release(&repo, "71e491c8-518d-5990-951c-ee30686cebd0", "1.0.0", &[]);
    let source = repo.to_str().unwrap();
    // Control: real Git must reproduce the original error with this case.
    let out = with_transient_server_info(&repo, || {
        Command::new("git")
            .args(["clone", "--no-hardlinks", "--", source])
            .arg(f.path("control"))
            .env("LC_ALL", "C")
            .output()
            .unwrap()
    });
    assert!(!out.status.success());
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(
        error.contains("failed to copy file to") && error.contains("packs_controlled"),
        "{error}"
    );
    assert!(error.contains("No such file or directory"), "{error}");

    let cat = f.catalog("catalog");
    let release =
        with_transient_server_info(&repo, || catalog::register(&cat, source, &tag)).unwrap();
    assert_eq!(release.manifest.id, "71e491c8-518d-5990-951c-ee30686cebd0");
    let sources = f.path("sources.txt");
    fs::write(&sources, source).unwrap();
    with_transient_server_info(&repo, || {
        catalog::update_sources(&cat, &sources, &mut |_| {})
    })
    .unwrap();
    let manager = f.open(&cat);
    let installed = with_transient_server_info(&repo, || {
        manager.install_from_catalog("71e491c8-518d-5990-951c-ee30686cebd0", None)
    })
    .unwrap();
    assert_eq!(installed.id, "71e491c8-518d-5990-951c-ee30686cebd0");
    let clone = f.path("workspace").join(
        manager.state().unwrap().packages["71e491c8-518d-5990-951c-ee30686cebd0"]
            .clone_folder("71e491c8-518d-5990-951c-ee30686cebd0"),
    );
    assert_eq!(git(&clone, &["remote", "get-url", "origin"]), source);
    assert!(!clone.join("objects/info/packs_controlled").exists());
    assert!(!clone.join("objects/info/alternates").exists());
    fs::remove_dir_all(repo).unwrap();
    assert_eq!(
        git(
            &clone,
            &[
                "show",
                "refs/gitpm/71e491c8-518d-5990-951c-ee30686cebd0/current:dist/index.js"
            ]
        ),
        "71e491c8-518d-5990-951c-ee30686cebd0 1.0.0"
    );
}

#[test]
fn catalog_install_resolves_dependencies_and_records_edges() {
    // An unverified catalog serves discovery only: nothing records it.
    let f = Fixture::new();
    let cat = core_catalog(&f);
    let m = f.open(&cat);
    m.install_from_catalog("9692fc07-23c7-5f20-8779-f09374f1a571", None)
        .unwrap();
    let state = m.state().unwrap();
    assert!(
        state.packages["9692fc07-23c7-5f20-8779-f09374f1a571"]
            .catalog
            .is_none()
    );
    assert!(
        state.packages["757fd5ca-2b6a-5b2f-8a5d-60980b028665"]
            .catalog
            .is_none()
    );
    drop((m, f));

    let f = Fixture::new();
    let cat = core_catalog(&f);
    f.verify(&cat);
    let m = f.open(&cat);
    let installed = m
        .install_from_catalog("9692fc07-23c7-5f20-8779-f09374f1a571", None)
        .unwrap();
    assert_eq!(
        ids(&m),
        [
            "757fd5ca-2b6a-5b2f-8a5d-60980b028665",
            "9692fc07-23c7-5f20-8779-f09374f1a571"
        ]
    );
    // The pulled dependency is enabled and deployed in the same transaction.
    assert!(installed.enabled);
    let pulled: Vec<_> = installed
        .dependencies
        .iter()
        .map(|d| (d.id.as_str(), d.current.version.as_str(), d.enabled))
        .collect();
    assert_eq!(
        pulled,
        [("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "1.1.0", true)]
    );
    assert_eq!(
        installed.dist.deployed,
        [
            "757fd5ca-2b6a-5b2f-8a5d-60980b028665",
            "9692fc07-23c7-5f20-8779-f09374f1a571"
        ]
    );
    let core = plugin(&m, "757fd5ca-2b6a-5b2f-8a5d-60980b028665");
    assert_eq!(
        core.current.as_ref().unwrap().version,
        "1.1.0",
        "highest version matching ^1.0"
    );
    assert_eq!(core.dependents, ["9692fc07-23c7-5f20-8779-f09374f1a571"]);
    let state = m.state().unwrap();
    assert_eq!(
        state.packages["9692fc07-23c7-5f20-8779-f09374f1a571"].dependencies["757fd5ca-2b6a-5b2f-8a5d-60980b028665"],
        "^1.0"
    );
    let origin = catalog::canonical_location(&cat).unwrap();
    assert_eq!(
        state.packages["757fd5ca-2b6a-5b2f-8a5d-60980b028665"]
            .catalog
            .as_deref(),
        Some(origin.as_str())
    );
    assert_eq!(
        provider(
            &m,
            "9692fc07-23c7-5f20-8779-f09374f1a571",
            "757fd5ca-2b6a-5b2f-8a5d-60980b028665"
        ),
        (Some("757fd5ca-2b6a-5b2f-8a5d-60980b028665".into()), true)
    );
    assert!(
        f.path("workspace/dist/757fd5ca-2b6a-5b2f-8a5d-60980b028665/index.js")
            .is_file()
    );
    assert!(core.enabled);

    // Missing dependency: refused with its name, nothing changes.
    f.publish(
        &cat,
        "2a8af310-7783-5cbb-8218-c875e4f8a3e6",
        "1.0.0",
        &[("0276fe29-bef5-5d0b-aae9-f9920c809729", "^1")],
    );
    let error = f.refuses("dependency_conflict", || {
        m.install_from_catalog("2a8af310-7783-5cbb-8218-c875e4f8a3e6", None)
    });
    assert!(
        error
            .to_string()
            .contains("0276fe29-bef5-5d0b-aae9-f9920c809729"),
        "{error}"
    );

    // Conflict with an installed consumer: both are named, nothing changes.
    let error = f.refuses("dependency_conflict", || {
        m.install_from_catalog("f5984d44-92bd-51ac-8e8f-d911dd824ef3", None)
    });
    let text = error.to_string();
    assert!(
        text.contains("9692fc07-23c7-5f20-8779-f09374f1a571")
            && text.contains("f5984d44-92bd-51ac-8e8f-d911dd824ef3"),
        "{text}"
    );

    // Shared dependency: one copy; uninstalling one consumer keeps it.
    m.install_from_catalog("1a620c0a-aa3c-533c-9960-3d49016fa231", None)
        .unwrap();
    assert_eq!(
        ids(&m),
        [
            "1a620c0a-aa3c-533c-9960-3d49016fa231",
            "757fd5ca-2b6a-5b2f-8a5d-60980b028665",
            "9692fc07-23c7-5f20-8779-f09374f1a571"
        ]
    );
    assert_eq!(
        plugin(&m, "757fd5ca-2b6a-5b2f-8a5d-60980b028665").dependents,
        [
            "1a620c0a-aa3c-533c-9960-3d49016fa231",
            "9692fc07-23c7-5f20-8779-f09374f1a571"
        ]
    );
    m.uninstall("9692fc07-23c7-5f20-8779-f09374f1a571", &[], false)
        .unwrap();
    assert_eq!(
        ids(&m),
        [
            "1a620c0a-aa3c-533c-9960-3d49016fa231",
            "757fd5ca-2b6a-5b2f-8a5d-60980b028665"
        ]
    );
}

/// A disabled package satisfies no dependency: install, enable and single update of an
/// enabled consumer refuse it, `update all`, `list` and `check` report it unsatisfied;
/// only a download takes it as given.
#[test]
fn disabled_packages_satisfy_no_dependency() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "757fd5ca-2b6a-5b2f-8a5d-60980b028665", "1.2.0", &[]);
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.0.0",
        &[("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^1")],
    );
    let m = f.open(&cat);
    m.install_from_catalog("757fd5ca-2b6a-5b2f-8a5d-60980b028665", None)
        .unwrap();
    m.disable("757fd5ca-2b6a-5b2f-8a5d-60980b028665").unwrap();
    // Install next to a disabled dependency: refused, naming it; nothing changes.
    let error = f.refuses("dependency_conflict", || {
        m.install_from_catalog("9692fc07-23c7-5f20-8779-f09374f1a571", None)
    });
    assert!(
        error
            .to_string()
            .contains("757fd5ca-2b6a-5b2f-8a5d-60980b028665 is disabled; enable it"),
        "{error}"
    );
    let app_repo = f.path("authors/9692fc07-23c7-5f20-8779-f09374f1a571");
    let error = f.refuses("dependency_conflict", || {
        m.install(app_repo.to_str().unwrap())
    });
    assert!(
        error
            .to_string()
            .contains("757fd5ca-2b6a-5b2f-8a5d-60980b028665 is disabled; enable it"),
        "{error}"
    );
    // A download takes the disabled version as given and enables nothing.
    let downloaded = m
        .download_from_catalog("9692fc07-23c7-5f20-8779-f09374f1a571", None)
        .unwrap();
    assert!(!downloaded.enabled && downloaded.dependencies.is_empty());
    assert!(
        !f.path("workspace/dist/9692fc07-23c7-5f20-8779-f09374f1a571")
            .exists()
    );
    // list and check report the dependency unsatisfied.
    assert_eq!(
        provider(
            &m,
            "9692fc07-23c7-5f20-8779-f09374f1a571",
            "757fd5ca-2b6a-5b2f-8a5d-60980b028665"
        ),
        (Some("757fd5ca-2b6a-5b2f-8a5d-60980b028665".into()), false)
    );
    let row = plugin(&m, "9692fc07-23c7-5f20-8779-f09374f1a571").dependencies[0].clone();
    assert_eq!(
        row.problem.as_deref(),
        Some("757fd5ca-2b6a-5b2f-8a5d-60980b028665 is disabled")
    );
    let report = gitpm::check::check(&f.path("workspace"), Some(&cat)).unwrap();
    assert_eq!(report.unsatisfied.len(), 1);
    assert_eq!(
        (
            report.unsatisfied[0].package.as_str(),
            report.unsatisfied[0].dependency.as_str(),
            report.unsatisfied[0].problem.as_deref()
        ),
        (
            "9692fc07-23c7-5f20-8779-f09374f1a571",
            "757fd5ca-2b6a-5b2f-8a5d-60980b028665",
            Some("757fd5ca-2b6a-5b2f-8a5d-60980b028665 is disabled")
        )
    );
    // enable refuses; enabling the dependency first works.
    let error = f.refuses("dependency_conflict", || {
        m.enable("9692fc07-23c7-5f20-8779-f09374f1a571")
    });
    assert!(
        error
            .to_string()
            .contains("757fd5ca-2b6a-5b2f-8a5d-60980b028665 is disabled"),
        "{error}"
    );
    m.enable("757fd5ca-2b6a-5b2f-8a5d-60980b028665").unwrap();
    m.enable("9692fc07-23c7-5f20-8779-f09374f1a571").unwrap();
    assert!(
        f.path("workspace/dist/9692fc07-23c7-5f20-8779-f09374f1a571/index.js")
            .is_file()
    );

    // Single update of an enabled consumer whose dependency was disabled by force.
    m.set_enabled_with("757fd5ca-2b6a-5b2f-8a5d-60980b028665", false, true)
        .unwrap();
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.1.0",
        &[("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^1")],
    );
    let error = f.refuses("dependency_conflict", || {
        m.update("9692fc07-23c7-5f20-8779-f09374f1a571")
    });
    assert!(
        error
            .to_string()
            .contains("757fd5ca-2b6a-5b2f-8a5d-60980b028665 is disabled; enable it"),
        "{error}"
    );
    // update all counts it unsatisfied as well: the consumer is skipped, not updated.
    let plan = m.update_all().unwrap();
    assert!(plan.updates.is_empty());
    assert_eq!(
        plan.unchanged
            .iter()
            .find(|p| p.id == "9692fc07-23c7-5f20-8779-f09374f1a571")
            .unwrap()
            .reason,
        Some(gitpm::UnchangedReason::BrokenDependency {
            dependency: "757fd5ca-2b6a-5b2f-8a5d-60980b028665".into(),
            problem: "757fd5ca-2b6a-5b2f-8a5d-60980b028665 is disabled".into(),
        })
    );
    // A disabled consumer may move: it does not become active.
    m.disable("9692fc07-23c7-5f20-8779-f09374f1a571").unwrap();
    assert!(
        m.update("9692fc07-23c7-5f20-8779-f09374f1a571")
            .unwrap()
            .changed
    );
    assert!(!plugin(&m, "9692fc07-23c7-5f20-8779-f09374f1a571").enabled);
}

/// `install --force` enables the named package over a disabled dependency; an install
/// over a download from the same source enables it (with the same check).
#[test]
fn forced_install_and_install_over_a_download() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "757fd5ca-2b6a-5b2f-8a5d-60980b028665", "1.2.0", &[]);
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.0.0",
        &[("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^1")],
    );
    let m = f.open(&cat);
    m.download_from_catalog("9692fc07-23c7-5f20-8779-f09374f1a571", None)
        .unwrap();
    // Install over the download: refused while 757fd5ca-2b6a-5b2f-8a5d-60980b028665 is disabled, nothing changes.
    let error = f.refuses("dependency_conflict", || {
        m.install_from_catalog("9692fc07-23c7-5f20-8779-f09374f1a571", None)
    });
    assert!(
        error
            .to_string()
            .contains("757fd5ca-2b6a-5b2f-8a5d-60980b028665 is disabled; enable it"),
        "{error}"
    );
    // Forced: only 9692fc07-23c7-5f20-8779-f09374f1a571 is enabled; it is no "already installed".
    let forced = m
        .install_from_catalog_with("9692fc07-23c7-5f20-8779-f09374f1a571", None, true)
        .unwrap();
    assert!(forced.enabled && !forced.already_installed, "{forced:?}");
    assert_eq!(
        forced.dist.deployed,
        ["9692fc07-23c7-5f20-8779-f09374f1a571"]
    );
    assert!(!plugin(&m, "757fd5ca-2b6a-5b2f-8a5d-60980b028665").enabled);
    assert_eq!(
        provider(
            &m,
            "9692fc07-23c7-5f20-8779-f09374f1a571",
            "757fd5ca-2b6a-5b2f-8a5d-60980b028665"
        ),
        (Some("757fd5ca-2b6a-5b2f-8a5d-60980b028665".into()), false)
    );
    // Enabled now: installing again is "already installed".
    assert!(
        m.install_from_catalog("9692fc07-23c7-5f20-8779-f09374f1a571", None)
            .unwrap()
            .already_installed
    );
    // An install over a download with active dependencies enables it.
    f.publish(
        &cat,
        "f5984d44-92bd-51ac-8e8f-d911dd824ef3",
        "1.0.0",
        &[("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^1")],
    );
    m.enable("757fd5ca-2b6a-5b2f-8a5d-60980b028665").unwrap();
    m.download_from_catalog("f5984d44-92bd-51ac-8e8f-d911dd824ef3", None)
        .unwrap();
    let enabled = m
        .install_from_catalog("f5984d44-92bd-51ac-8e8f-d911dd824ef3", None)
        .unwrap();
    assert!(enabled.enabled && !enabled.already_installed);
    assert!(
        f.path("workspace/dist/f5984d44-92bd-51ac-8e8f-d911dd824ef3/index.js")
            .is_file()
    );

    // install --force of a new package next to a disabled dependency.
    f.publish(
        &cat,
        "1a620c0a-aa3c-533c-9960-3d49016fa231",
        "1.0.0",
        &[("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^1")],
    );
    m.set_enabled_with("757fd5ca-2b6a-5b2f-8a5d-60980b028665", false, true)
        .unwrap();
    f.refuses("dependency_conflict", || {
        m.install_from_catalog("1a620c0a-aa3c-533c-9960-3d49016fa231", None)
    });
    let forced = m
        .install_from_catalog_with("1a620c0a-aa3c-533c-9960-3d49016fa231", None, true)
        .unwrap();
    assert!(
        forced.enabled
            && f.path("workspace/dist/1a620c0a-aa3c-533c-9960-3d49016fa231/index.js")
                .is_file()
    );
    let app_repo = f.path("authors/1a620c0a-aa3c-533c-9960-3d49016fa231");
    assert!(
        m.install_package_with(
            app_repo.to_str().unwrap(),
            None,
            gitpm::InstallTarget::Release,
            true
        )
        .unwrap()
        .already_installed
    );
}

/// rollback of an active package and restore of an enabled record refuse a disabled
/// dependency like install and update do.
#[test]
fn rollback_and_restore_refuse_disabled_dependencies() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "757fd5ca-2b6a-5b2f-8a5d-60980b028665", "1.2.0", &[]);
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.0.0",
        &[("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^1")],
    );
    let m = f.open(&cat);
    m.install_from_catalog("9692fc07-23c7-5f20-8779-f09374f1a571", None)
        .unwrap();
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.1.0",
        &[("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^1")],
    );
    m.update("9692fc07-23c7-5f20-8779-f09374f1a571").unwrap();
    m.set_enabled_with("757fd5ca-2b6a-5b2f-8a5d-60980b028665", false, true)
        .unwrap();
    let error = f.refuses("dependency_conflict", || {
        m.rollback("9692fc07-23c7-5f20-8779-f09374f1a571")
    });
    assert!(
        error
            .to_string()
            .contains("757fd5ca-2b6a-5b2f-8a5d-60980b028665 is disabled; enable it"),
        "{error}"
    );
    m.forget("9692fc07-23c7-5f20-8779-f09374f1a571").unwrap();
    let error = f.refuses("dependency_conflict", || {
        m.restore("9692fc07-23c7-5f20-8779-f09374f1a571")
    });
    assert!(
        error
            .to_string()
            .contains("757fd5ca-2b6a-5b2f-8a5d-60980b028665 is disabled; enable it"),
        "{error}"
    );
    m.enable("757fd5ca-2b6a-5b2f-8a5d-60980b028665").unwrap();
    m.restore("9692fc07-23c7-5f20-8779-f09374f1a571").unwrap();
    m.rollback("9692fc07-23c7-5f20-8779-f09374f1a571").unwrap();
    assert_eq!(
        plugin(&m, "9692fc07-23c7-5f20-8779-f09374f1a571")
            .current
            .unwrap()
            .version,
        "1.0.0"
    );
}

/// Only direct dependencies must be active: app → lib (enabled) → base (disabled by
/// force) is no refusal for app in any operation; lib is reported broken.
#[test]
fn only_direct_dependencies_must_be_active() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "d688e809-d3cf-568a-95e7-8c404dc59d03", "1.0.0", &[]);
    f.publish(
        &cat,
        "c4304791-586c-54cb-a7c6-27bd09f28dbd",
        "1.0.0",
        &[("d688e809-d3cf-568a-95e7-8c404dc59d03", "^1")],
    );
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.0.0",
        &[("c4304791-586c-54cb-a7c6-27bd09f28dbd", "^1")],
    );
    let m = f.open(&cat);
    m.install_from_catalog("9692fc07-23c7-5f20-8779-f09374f1a571", None)
        .unwrap();
    m.set_enabled_with("d688e809-d3cf-568a-95e7-8c404dc59d03", false, true)
        .unwrap();
    assert_eq!(
        provider(
            &m,
            "c4304791-586c-54cb-a7c6-27bd09f28dbd",
            "d688e809-d3cf-568a-95e7-8c404dc59d03"
        ),
        (Some("d688e809-d3cf-568a-95e7-8c404dc59d03".into()), false)
    );
    m.disable("9692fc07-23c7-5f20-8779-f09374f1a571").unwrap();
    m.enable("9692fc07-23c7-5f20-8779-f09374f1a571").unwrap();
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.1.0",
        &[("c4304791-586c-54cb-a7c6-27bd09f28dbd", "^1")],
    );
    m.update("9692fc07-23c7-5f20-8779-f09374f1a571").unwrap();
    m.rollback("9692fc07-23c7-5f20-8779-f09374f1a571").unwrap();
    m.forget("9692fc07-23c7-5f20-8779-f09374f1a571").unwrap();
    m.restore("9692fc07-23c7-5f20-8779-f09374f1a571").unwrap();
    m.uninstall("9692fc07-23c7-5f20-8779-f09374f1a571", &[], false)
        .unwrap();
    let installed = m
        .install_from_catalog("9692fc07-23c7-5f20-8779-f09374f1a571", None)
        .unwrap();
    assert!(installed.enabled);
    assert!(
        f.path("workspace/dist/9692fc07-23c7-5f20-8779-f09374f1a571/index.js")
            .is_file()
    );
}

/// An update of a disabled package records the dependencies it pulls disabled.
#[test]
fn update_of_a_disabled_package_pulls_disabled_dependencies() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "9692fc07-23c7-5f20-8779-f09374f1a571", "1.0.0", &[]);
    let m = f.open(&cat);
    m.install_from_catalog("9692fc07-23c7-5f20-8779-f09374f1a571", None)
        .unwrap();
    m.disable("9692fc07-23c7-5f20-8779-f09374f1a571").unwrap();
    f.publish(&cat, "385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70", "1.0.0", &[]);
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.1.0",
        &[("385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70", "^1")],
    );
    let change = m.update("9692fc07-23c7-5f20-8779-f09374f1a571").unwrap();
    assert!(
        change.changed && change.dist.deployed.is_empty(),
        "{change:?}"
    );
    assert!(!plugin(&m, "385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70").enabled);
    assert!(
        !f.path("workspace/dist/385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70")
            .exists()
    );
    // Enabled again: an update pulls enabled dependencies as before.
    m.enable("385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70").unwrap();
    m.enable("9692fc07-23c7-5f20-8779-f09374f1a571").unwrap();
    f.publish(&cat, "fb22f7c6-1c68-59f8-a3f5-b3054c64e945", "1.0.0", &[]);
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.2.0",
        &[
            ("385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70", "^1"),
            ("fb22f7c6-1c68-59f8-a3f5-b3054c64e945", "^1"),
        ],
    );
    m.update("9692fc07-23c7-5f20-8779-f09374f1a571").unwrap();
    assert!(plugin(&m, "fb22f7c6-1c68-59f8-a3f5-b3054c64e945").enabled);
    assert!(
        f.path("workspace/dist/fb22f7c6-1c68-59f8-a3f5-b3054c64e945/index.js")
            .is_file()
    );
}

/// 9692fc07-23c7-5f20-8779-f09374f1a571 1.0.0 installed; its catalog release 1.1.0 pulls 385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70, whose catalog
/// release points at a commit with a broken plugin.json.
fn broken_pulled_dependency() -> (Fixture, Manager) {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "9692fc07-23c7-5f20-8779-f09374f1a571", "1.0.0", &[]);
    let m = f.open(&cat);
    m.install_from_catalog("9692fc07-23c7-5f20-8779-f09374f1a571", None)
        .unwrap();
    f.publish(&cat, "385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70", "1.0.0", &[]);
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.1.0",
        &[("385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70", "^1")],
    );
    // The catalog points 385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70 at a commit whose plugin.json is broken.
    let author = f.path("authors/385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70");
    fs::write(author.join("plugin.json"), "{ broken").unwrap();
    git(&author, &["commit", "-qam", "break"]);
    let broken = git(&author, &["rev-parse", "HEAD"]);
    let release = Path::new(&cat)
        .join("packages/385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70/versions/1.0.0/release.json");
    let mut value: Value = serde_json::from_slice(&fs::read(&release).unwrap()).unwrap();
    value["commit"] = json!(broken.trim());
    fs::write(&release, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    (f, m)
}

/// A dependency an update pulls that cannot be installed is named in the refusal;
/// the updated package's ID is not put on it.
#[test]
fn update_refusal_from_a_pulled_dependency_names_the_dependency() {
    let (_f, m) = broken_pulled_dependency();
    let error = m
        .update("9692fc07-23c7-5f20-8779-f09374f1a571")
        .unwrap_err();
    assert!(
        matches!(&error, Error::InvalidManifest { id: Some(id), .. } if id == "385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70"),
        "{error:?}"
    );
    assert!(
        error
            .to_string()
            .starts_with("385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70: invalid plugin.json"),
        "{error}"
    );
}

/// A dependency an update pulls whose catalog release has an invalid version
/// requirement is named in the refusal, not the updated package.
#[test]
fn update_refusal_from_a_pulled_dependency_requirement_names_the_dependency() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "9692fc07-23c7-5f20-8779-f09374f1a571", "1.0.0", &[]);
    let m = f.open(&cat);
    m.install_from_catalog("9692fc07-23c7-5f20-8779-f09374f1a571", None)
        .unwrap();
    f.publish(&cat, "757fd5ca-2b6a-5b2f-8a5d-60980b028665", "1.0.0", &[]);
    f.publish(
        &cat,
        "385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70",
        "1.0.0",
        &[("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^1")],
    );
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.1.0",
        &[("385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70", "^1")],
    );
    // register refuses such a manifest; a hand-edited catalog can still carry one.
    let release = Path::new(&cat)
        .join("packages/385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70/versions/1.0.0/release.json");
    let mut value: Value = serde_json::from_slice(&fs::read(&release).unwrap()).unwrap();
    value["manifest"]["dependencies"] =
        json!({ "757fd5ca-2b6a-5b2f-8a5d-60980b028665": "^1 || ^2" });
    fs::write(&release, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    for error in [
        m.update("9692fc07-23c7-5f20-8779-f09374f1a571")
            .unwrap_err(),
        m.update_all_plan().unwrap_err(),
        m.update_all().unwrap_err(),
    ] {
        assert!(
            matches!(&error, Error::InvalidManifest { id: Some(id), .. } if id == "385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70"),
            "{error:?}"
        );
        assert!(
            error
                .to_string()
                .starts_with("385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70: invalid version requirement"),
            "{error}"
        );
    }
}

/// `update all`: a dependency it pulls that cannot be installed is named in the
/// refusal, as with `update ID`.
#[test]
fn update_all_refusal_from_a_pulled_dependency_names_the_dependency() {
    let (_f, m) = broken_pulled_dependency();
    for error in [
        m.update_all_plan().unwrap_err(),
        m.update_all().unwrap_err(),
    ] {
        assert!(
            matches!(&error, Error::InvalidManifest { id: Some(id), .. } if id == "385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70"),
            "{error:?}"
        );
        assert!(
            error
                .to_string()
                .starts_with("385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70: invalid plugin.json"),
            "{error}"
        );
    }
}

/// `update all`: a newer release of an installed package with an invalid version
/// requirement is refused with that package's ID.
#[test]
fn update_all_invalid_requirement_names_the_package() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "757fd5ca-2b6a-5b2f-8a5d-60980b028665", "1.0.0", &[]);
    f.publish(&cat, "3a53cdea-2cb1-5653-bf22-898b679440ee", "1.0.0", &[]);
    let m = f.open(&cat);
    m.install_from_catalog("3a53cdea-2cb1-5653-bf22-898b679440ee", None)
        .unwrap();
    f.release(
        &f.path("authors/3a53cdea-2cb1-5653-bf22-898b679440ee"),
        "3a53cdea-2cb1-5653-bf22-898b679440ee",
        "1.1.0",
        &[("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^1 || ^2")],
    );
    for error in [
        m.update_all_plan().unwrap_err(),
        m.update_all().unwrap_err(),
    ] {
        assert!(
            matches!(&error, Error::InvalidManifest { id: Some(id), .. } if id == "3a53cdea-2cb1-5653-bf22-898b679440ee"),
            "{error:?}"
        );
        assert!(
            error
                .to_string()
                .starts_with("3a53cdea-2cb1-5653-bf22-898b679440ee: invalid version requirement"),
            "{error}"
        );
    }
}

/// `update all`: a newer release of a protected package whose catalog manifest has an
/// invalid version requirement is refused with that package's ID.
#[test]
fn update_all_invalid_requirement_in_the_catalog_names_the_package() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "3a53cdea-2cb1-5653-bf22-898b679440ee", "1.0.0", &[]);
    f.verify(&cat);
    let m = f.open(&cat);
    m.install_from_catalog("3a53cdea-2cb1-5653-bf22-898b679440ee", None)
        .unwrap();
    f.publish(&cat, "3a53cdea-2cb1-5653-bf22-898b679440ee", "1.1.0", &[]);
    // register refuses such a manifest; a hand-edited catalog can still carry one.
    let release = Path::new(&cat)
        .join("packages/3a53cdea-2cb1-5653-bf22-898b679440ee/versions/1.1.0/release.json");
    let mut value: Value = serde_json::from_slice(&fs::read(&release).unwrap()).unwrap();
    value["manifest"]["dependencies"] =
        json!({ "757fd5ca-2b6a-5b2f-8a5d-60980b028665": "^1 || ^2" });
    fs::write(&release, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    for error in [
        m.update_all_plan().unwrap_err(),
        m.update_all().unwrap_err(),
    ] {
        assert!(
            matches!(&error, Error::InvalidManifest { id: Some(id), .. } if id == "3a53cdea-2cb1-5653-bf22-898b679440ee"),
            "{error:?}"
        );
        assert!(
            error
                .to_string()
                .starts_with("3a53cdea-2cb1-5653-bf22-898b679440ee: invalid version requirement"),
            "{error}"
        );
    }
}

/// Install over a download fills `available`/`requested_mode` and warns that the
/// recorded mode is kept.
#[test]
fn install_over_a_download_reports_the_requested_mode() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "9692fc07-23c7-5f20-8779-f09374f1a571", "1.0.0", &[]);
    let repo = f.path("authors/9692fc07-23c7-5f20-8779-f09374f1a571");
    let m = f.open(&cat);
    m.download(repo.to_str().unwrap(), None, gitpm::InstallTarget::Release)
        .unwrap();
    f.release(&repo, "9692fc07-23c7-5f20-8779-f09374f1a571", "1.1.0", &[]);
    let result = m
        .install_package(
            repo.to_str().unwrap(),
            None,
            gitpm::InstallTarget::Ref("v1.1.0".into()),
        )
        .unwrap();
    assert!(result.enabled && !result.already_installed);
    assert_eq!(result.current.version, "1.0.0");
    assert!(result.mode.is_release());
    assert_eq!(
        result.requested_mode,
        Some(gitpm::Mode::Pinned("v1.1.0".into()))
    );
    assert_eq!(result.available.as_ref().unwrap().version, "1.1.0");
    assert!(
        result.warnings[0].contains("recorded mode release, not the requested pinned at v1.1.0"),
        "{:?}",
        result.warnings
    );
    assert!(
        f.path("workspace/dist/9692fc07-23c7-5f20-8779-f09374f1a571/index.js")
            .is_file()
    );
}

#[test]
fn cycles_resolve_and_updates_keep_dependents_working() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(
        &cat,
        "d35b3151-28fd-557a-a363-b5f18ae491c8",
        "1.0.0",
        &[("3a53cdea-2cb1-5653-bf22-898b679440ee", "^1")],
    );
    f.publish(
        &cat,
        "3a53cdea-2cb1-5653-bf22-898b679440ee",
        "1.0.0",
        &[("d35b3151-28fd-557a-a363-b5f18ae491c8", "^1")],
    );
    f.publish(&cat, "757fd5ca-2b6a-5b2f-8a5d-60980b028665", "1.1.0", &[]);
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.0.0",
        &[("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^1.0")],
    );
    let m = f.open(&cat);
    m.install_from_catalog("d35b3151-28fd-557a-a363-b5f18ae491c8", None)
        .unwrap();
    assert_eq!(
        ids(&m),
        [
            "3a53cdea-2cb1-5653-bf22-898b679440ee",
            "d35b3151-28fd-557a-a363-b5f18ae491c8"
        ]
    );

    m.install_from_catalog("9692fc07-23c7-5f20-8779-f09374f1a571", None)
        .unwrap();
    // 757fd5ca-2b6a-5b2f-8a5d-60980b028665 2.0.0 released upstream: 9692fc07-23c7-5f20-8779-f09374f1a571 needs ^1.0, so the update is refused.
    let core = f.path("authors/757fd5ca-2b6a-5b2f-8a5d-60980b028665");
    f.release(&core, "757fd5ca-2b6a-5b2f-8a5d-60980b028665", "2.0.0", &[]);
    let error = f.refuses("dependency_conflict", || {
        m.update("757fd5ca-2b6a-5b2f-8a5d-60980b028665")
    });
    assert!(
        error
            .to_string()
            .contains("9692fc07-23c7-5f20-8779-f09374f1a571"),
        "{error}"
    );
    assert_eq!(
        plugin(&m, "757fd5ca-2b6a-5b2f-8a5d-60980b028665")
            .current
            .unwrap()
            .version,
        "1.1.0"
    );
    // A compatible release is fine.
    f.release(&core, "757fd5ca-2b6a-5b2f-8a5d-60980b028665", "1.2.0", &[]);
    m.update_to(
        "757fd5ca-2b6a-5b2f-8a5d-60980b028665",
        gitpm::Target::Ref("v1.2.0".into()),
    )
    .unwrap();
    // An update that adds a dependency pulls it from the catalog.
    f.publish(&cat, "385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70", "1.0.0", &[]);
    let app = f.path("authors/9692fc07-23c7-5f20-8779-f09374f1a571");
    f.release(
        &app,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.1.0",
        &[
            ("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^1.0"),
            ("385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70", "^1"),
        ],
    );
    m.update("9692fc07-23c7-5f20-8779-f09374f1a571").unwrap();
    assert!(ids(&m).contains(&"385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70".to_string()));
    // Rollback is offline and checked the same way.
    m.rollback("9692fc07-23c7-5f20-8779-f09374f1a571").unwrap();
    assert_eq!(
        plugin(&m, "9692fc07-23c7-5f20-8779-f09374f1a571")
            .current
            .unwrap()
            .version,
        "1.0.0"
    );
}

#[test]
fn direct_git_install_uses_installed_plugins_or_the_catalog() {
    let f = Fixture::new();
    let core = f.author("757fd5ca-2b6a-5b2f-8a5d-60980b028665");
    f.release(&core, "757fd5ca-2b6a-5b2f-8a5d-60980b028665", "1.0.0", &[]);
    let app = f.author("9692fc07-23c7-5f20-8779-f09374f1a571");
    f.release(
        &app,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.0.0",
        &[("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^1")],
    );
    let m = Manager::open(f.path("workspace")).unwrap();
    let error = f.refuses("dependency_conflict", || m.install(app.to_str().unwrap()));
    assert!(
        error.to_string().contains("no catalog configured"),
        "{error}"
    );
    m.install(core.to_str().unwrap()).unwrap();
    m.install(app.to_str().unwrap()).unwrap();
    assert_eq!(
        provider(
            &m,
            "9692fc07-23c7-5f20-8779-f09374f1a571",
            "757fd5ca-2b6a-5b2f-8a5d-60980b028665"
        ),
        (Some("757fd5ca-2b6a-5b2f-8a5d-60980b028665".into()), true)
    );
    assert_eq!(
        m.state().unwrap().packages["9692fc07-23c7-5f20-8779-f09374f1a571"].catalog,
        None
    );
}

#[test]
fn dependents_guard_disable_forget_and_uninstall() {
    let f = Fixture::new();
    let cat = core_catalog(&f);
    f.publish(&cat, "a768d335-0cf7-597a-b1ba-0eb542495aec", "1.0.0", &[]);
    let m = f.open(&cat);
    m.install_from_catalog("9692fc07-23c7-5f20-8779-f09374f1a571", None)
        .unwrap();
    m.install_from_catalog("1a620c0a-aa3c-533c-9960-3d49016fa231", None)
        .unwrap();
    m.install_from_catalog("a768d335-0cf7-597a-b1ba-0eb542495aec", None)
        .unwrap();

    for error in [
        f.refuses("has_dependents", || {
            m.uninstall("757fd5ca-2b6a-5b2f-8a5d-60980b028665", &[], false)
        }),
        f.refuses("has_dependents", || {
            m.forget("757fd5ca-2b6a-5b2f-8a5d-60980b028665")
        }),
        f.refuses("has_dependents", || {
            m.disable("757fd5ca-2b6a-5b2f-8a5d-60980b028665")
        }),
    ] {
        let Error::HasDependents { dependents, .. } = &error else {
            panic!("{error:?}")
        };
        assert_eq!(
            dependents,
            &[
                "1a620c0a-aa3c-533c-9960-3d49016fa231",
                "9692fc07-23c7-5f20-8779-f09374f1a571"
            ]
        );
        assert!(
            error.to_string().contains(
                "1a620c0a-aa3c-533c-9960-3d49016fa231, 9692fc07-23c7-5f20-8779-f09374f1a571"
            ),
            "{error}"
        );
    }
    let plan = m
        .uninstall_plan("757fd5ca-2b6a-5b2f-8a5d-60980b028665")
        .unwrap();
    assert_eq!(
        plan.dependents,
        [
            "1a620c0a-aa3c-533c-9960-3d49016fa231",
            "9692fc07-23c7-5f20-8779-f09374f1a571"
        ]
    );
    m.disable("a768d335-0cf7-597a-b1ba-0eb542495aec").unwrap();

    // One refusal tells everything: the dependents and the work uninstall would lose.
    fs::write(
        f.path("workspace")
            .join("worktrees/757fd5ca-2b6a-5b2f-8a5d-60980b028665")
            .join("notes.txt"),
        "mine",
    )
    .unwrap();
    let error = f.refuses("has_dependents", || {
        m.uninstall("757fd5ca-2b6a-5b2f-8a5d-60980b028665", &[], false)
    });
    let Error::HasDependents { lost_work, .. } = &error else {
        panic!("{error:?}")
    };
    assert_eq!(
        lost_work,
        &[gitpm::LostWork::Untracked {
            path: "notes.txt".into()
        }]
    );
    assert!(error.to_string().contains("local work"), "{error}");
    let plan = m
        .uninstall_plan("757fd5ca-2b6a-5b2f-8a5d-60980b028665")
        .unwrap();
    assert_eq!(plan.refusal().unwrap().to_string(), error.to_string());

    // Forced: removed; consumers report the unsatisfied dependency.
    m.uninstall(
        "757fd5ca-2b6a-5b2f-8a5d-60980b028665",
        &plan.lost_work,
        true,
    )
    .unwrap();
    let (provider_id, satisfied) = provider(
        &m,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "757fd5ca-2b6a-5b2f-8a5d-60980b028665",
    );
    assert_eq!((provider_id, satisfied), (None, false));
    let status = plugin(&m, "9692fc07-23c7-5f20-8779-f09374f1a571").dependencies[0].clone();
    assert_eq!(status.problem.as_deref(), Some("not installed"));
    // A broken consumer does not block unrelated operations.
    m.enable("a768d335-0cf7-597a-b1ba-0eb542495aec").unwrap();
    // Reinstall it; once its consumers are gone it is removable without a guard.
    m.install_from_catalog("757fd5ca-2b6a-5b2f-8a5d-60980b028665", Some("1.1.0"))
        .unwrap();
    m.uninstall("9692fc07-23c7-5f20-8779-f09374f1a571", &[], false)
        .unwrap();
    m.uninstall("1a620c0a-aa3c-533c-9960-3d49016fa231", &[], false)
        .unwrap();
    m.uninstall("757fd5ca-2b6a-5b2f-8a5d-60980b028665", &[], false)
        .unwrap();
}

/// c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 1.0.0; 9692fc07-23c7-5f20-8779-f09374f1a571 and f5984d44-92bd-51ac-8e8f-d911dd824ef3 need ^1.0 (UC-066 main scenario).
#[test]
fn fork_replacement_main_scenario() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "1.0.0", &[]);
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.0.0",
        &[("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "^1.0")],
    );
    f.publish(
        &cat,
        "f5984d44-92bd-51ac-8e8f-d911dd824ef3",
        "1.0.0",
        &[("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "^1.0")],
    );
    f.publish(
        &cat,
        "1a620c0a-aa3c-533c-9960-3d49016fa231",
        "1.0.0",
        &[("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "^1.0")],
    );
    f.publish(&cat, "a768d335-0cf7-597a-b1ba-0eb542495aec", "1.0.0", &[]);
    let m = f.open(&cat);
    m.install_from_catalog("9692fc07-23c7-5f20-8779-f09374f1a571", None)
        .unwrap();
    m.install_from_catalog("f5984d44-92bd-51ac-8e8f-d911dd824ef3", None)
        .unwrap();
    m.install_from_catalog("a768d335-0cf7-597a-b1ba-0eb542495aec", None)
        .unwrap();
    assert_eq!(
        ids(&m),
        [
            "9692fc07-23c7-5f20-8779-f09374f1a571",
            "a768d335-0cf7-597a-b1ba-0eb542495aec",
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            "f5984d44-92bd-51ac-8e8f-d911dd824ef3"
        ]
    );

    // Fork, edit, release 1.0.1; the fork declares what it replaces.
    let fork = m
        .fork_into(
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            &f.path("5c0dbec5-2209-5ad7-b092-7ad16acbd4a5"),
        )
        .unwrap();
    let generated_0: String =
        serde_json::from_slice::<serde_json::Value>(&fs::read(fork.join("plugin.json")).unwrap())
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
    let manifest: Value =
        serde_json::from_slice(&fs::read(fork.join("plugin.json")).unwrap()).unwrap();
    assert_eq!(
        manifest["replaces"],
        json!({"id": "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "version": "1.0.0"})
    );
    fs::write(fork.join("dist/index.js"), "fork payload").unwrap();
    scaffold::commit(&fork, "1.0.1").unwrap();
    m.install(fork.to_str().unwrap()).unwrap();
    // Installing the fork (provenance) alone redirects nothing.
    assert_eq!(
        provider(
            &m,
            "9692fc07-23c7-5f20-8779-f09374f1a571",
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"
        )
        .0
        .as_deref(),
        Some("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9")
    );

    // Returning to an unreplaced original is an idempotent checked replace.
    m.replace(
        "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
    )
    .unwrap();
    f.refuses("not_installed", || {
        m.replace(
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            "75a7a47e-8189-5cd3-8ca7-e27427331e5a",
        )
    });
    f.refuses("invalid_argument", || {
        m.replace("a768d335-0cf7-597a-b1ba-0eb542495aec", generated_0.as_str())
    });

    let plan = m
        .replace_plan("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", generated_0.as_str())
        .unwrap();
    assert_eq!(
        plan.consumers,
        [
            "9692fc07-23c7-5f20-8779-f09374f1a571",
            "f5984d44-92bd-51ac-8e8f-d911dd824ef3"
        ]
    );
    assert!(plan.incompatible.is_empty());
    m.replace("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", generated_0.as_str())
        .unwrap();
    for consumer in [
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "f5984d44-92bd-51ac-8e8f-d911dd824ef3",
    ] {
        assert_eq!(
            provider(&m, consumer, "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"),
            (Some(generated_0.as_str().into()), true)
        );
    }
    assert_eq!(
        plugin(&m, generated_0.as_str()).replacing,
        ["c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"]
    );
    assert_eq!(
        plugin(&m, generated_0.as_str()).dependents,
        [
            "9692fc07-23c7-5f20-8779-f09374f1a571",
            "f5984d44-92bd-51ac-8e8f-d911dd824ef3"
        ]
    );
    assert_eq!(
        fs::read_to_string(f.path(format!("workspace/dist/{generated_0}/index.js").as_str()))
            .unwrap(),
        "fork payload"
    );
    // The original stays installed and addressable; the unrelated package is untouched.
    assert!(ids(&m).contains(&"c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9".to_string()));
    assert!(
        plugin(&m, "a768d335-0cf7-597a-b1ba-0eb542495aec")
            .dependencies
            .is_empty()
    );
    // A new consumer resolves to the fork too, without fetching anything new.
    m.install_from_catalog("1a620c0a-aa3c-533c-9960-3d49016fa231", None)
        .unwrap();
    assert_eq!(
        provider(
            &m,
            "1a620c0a-aa3c-533c-9960-3d49016fa231",
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"
        )
        .0
        .as_deref(),
        Some(generated_0.as_str())
    );
    // Cyclic replacement is refused.
    f.refuses("invalid_argument", || {
        m.replace(generated_0.as_str(), "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9")
    });

    // Reopen: the binding persists.
    drop(m);
    let m = f.open(&cat);
    assert_eq!(
        m.state().unwrap().replacements["c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"],
        generated_0.as_str()
    );

    // Fork release 1.0.2: consumers follow it.
    fs::write(fork.join("dist/index.js"), "fork payload 2").unwrap();
    scaffold::commit(&fork, "1.0.2").unwrap();
    m.update(generated_0.as_str()).unwrap();
    assert_eq!(
        provider(
            &m,
            "9692fc07-23c7-5f20-8779-f09374f1a571",
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"
        )
        .0
        .as_deref(),
        Some(generated_0.as_str())
    );
    assert_eq!(
        fs::read_to_string(f.path(format!("workspace/dist/{generated_0}/index.js").as_str()))
            .unwrap(),
        "fork payload 2"
    );
    // A new original release does not switch consumers back.
    let original = f.path("authors/c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9");
    f.release(
        &original,
        "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        "1.1.0",
        &[],
    );
    m.update("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9").unwrap();
    assert_eq!(
        provider(
            &m,
            "f5984d44-92bd-51ac-8e8f-d911dd824ef3",
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"
        )
        .0
        .as_deref(),
        Some(generated_0.as_str())
    );

    // The installed original would take every consumer over: nothing guards the fork.
    assert!(
        m.uninstall_plan(generated_0.as_str())
            .unwrap()
            .dependents
            .is_empty()
    );

    // Switch back: everyone uses the original; the fork keeps its work.
    m.replace(
        "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
    )
    .unwrap();
    for consumer in [
        "1a620c0a-aa3c-533c-9960-3d49016fa231",
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "f5984d44-92bd-51ac-8e8f-d911dd824ef3",
    ] {
        assert_eq!(
            provider(&m, consumer, "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"),
            (Some("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9".into()), true)
        );
    }
    assert!(m.state().unwrap().replacements.is_empty());
    assert!(plugin(&m, generated_0.as_str()).dependents.is_empty());
    m.uninstall(generated_0.as_str(), &[], false).unwrap();
}

/// Compatibility by declared versions, strict refusals, two forks and update rules.
#[test]
fn replacement_compatibility_is_strict() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "1.4.0", &[]);
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.0.0",
        &[("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "^1.0")],
    );
    f.publish(
        &cat,
        "f5984d44-92bd-51ac-8e8f-d911dd824ef3",
        "1.0.0",
        &[("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", ">=1.3")],
    );
    f.publish(
        &cat,
        "c4304791-586c-54cb-a7c6-27bd09f28dbd",
        "1.0.0",
        &[("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "^1")],
    );
    f.publish(
        &cat,
        "1a620c0a-aa3c-533c-9960-3d49016fa231",
        "1.0.0",
        &[("c4304791-586c-54cb-a7c6-27bd09f28dbd", "^1")],
    );
    let m = f.open(&cat);
    for id in [
        "1a620c0a-aa3c-533c-9960-3d49016fa231",
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "f5984d44-92bd-51ac-8e8f-d911dd824ef3",
    ] {
        m.install_from_catalog(id, None).unwrap();
    }
    let fork = |id: &str, declares: &str| {
        let repo = f.author(id);
        f.release_manifest(
            &repo,
            json!({"id": id, "name": "fixture", "version": "0.1.0", "replaces": {"id": "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "version": declares}}),
        );
        m.install(repo.to_str().unwrap()).unwrap();
        repo
    };
    fork("db33e43e-e652-5767-8e21-27fb3cef9723", "1.4.0");
    let old = fork("4e02af40-ea75-510c-b524-5f21e6de2d55", "1.2.0");

    // Old fork declares 1.2.0: f5984d44-92bd-51ac-8e8f-d911dd824ef3 (>=1.3) is not satisfied, so it is refused.
    let plan = m
        .replace_plan(
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            "4e02af40-ea75-510c-b524-5f21e6de2d55",
        )
        .unwrap();
    assert_eq!(plan.incompatible.len(), 1);
    assert_eq!(
        (
            plan.incompatible[0].id.as_str(),
            plan.incompatible[0].requirement.as_str()
        ),
        ("f5984d44-92bd-51ac-8e8f-d911dd824ef3", ">=1.3")
    );
    let error = f.refuses("incompatible_replacement", || {
        m.replace(
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            "4e02af40-ea75-510c-b524-5f21e6de2d55",
        )
    });
    assert!(
        error.to_string().contains(
            "f5984d44-92bd-51ac-8e8f-d911dd824ef3 needs c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 >=1.3"
        ),
        "{error}"
    );
    assert!(
        !error
            .to_string()
            .contains("9692fc07-23c7-5f20-8779-f09374f1a571"),
        "{error}"
    );
    // Refused for every caller: the original still serves both consumers.
    assert_eq!(
        provider(
            &m,
            "f5984d44-92bd-51ac-8e8f-d911dd824ef3",
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"
        ),
        (Some("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9".into()), true)
    );
    // Second fork: one global binding is switched, never two providers.
    m.replace(
        "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        "db33e43e-e652-5767-8e21-27fb3cef9723",
    )
    .unwrap();
    let state = m.state().unwrap();
    assert_eq!(state.replacements.len(), 1);
    assert_eq!(
        state.replacements["c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"],
        "db33e43e-e652-5767-8e21-27fb3cef9723"
    );
    assert!(
        plugin(&m, "4e02af40-ea75-510c-b524-5f21e6de2d55")
            .replacing
            .is_empty()
    );

    // An unused fork is removable; the active one cannot drop its declaration.
    m.uninstall("4e02af40-ea75-510c-b524-5f21e6de2d55", &[], false)
        .unwrap();
    let _ = old;
    let mine = f.path("authors/db33e43e-e652-5767-8e21-27fb3cef9723");
    f.release_manifest(&mine, json!({"id": "db33e43e-e652-5767-8e21-27fb3cef9723", "name": "fixture", "version": "0.2.0"}));
    let error = f.refuses("version_conflict", || {
        m.update("db33e43e-e652-5767-8e21-27fb3cef9723")
    });
    assert!(error.to_string().contains("no longer declares"), "{error}");
    // A fork release declaring an unacceptable version is refused too.
    f.release_manifest(
        &mine,
        json!({"id": "db33e43e-e652-5767-8e21-27fb3cef9723", "name": "fixture", "version": "0.3.0", "replaces": {"id": "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "version": "2.0.0"}}),
    );
    f.refuses("dependency_conflict", || {
        m.update("db33e43e-e652-5767-8e21-27fb3cef9723")
    });

    // The installed original would satisfy every consumer; once it is gone, the
    // consumers of the fork break and guard it.
    assert!(
        m.uninstall_plan("db33e43e-e652-5767-8e21-27fb3cef9723")
            .unwrap()
            .dependents
            .is_empty()
    );
    m.uninstall("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", &[], false)
        .unwrap();
    let error = f.refuses("has_dependents", || {
        m.uninstall("db33e43e-e652-5767-8e21-27fb3cef9723", &[], false)
    });
    let Error::HasDependents { dependents, .. } = &error else {
        panic!("{error:?}")
    };
    assert_eq!(
        dependents,
        &[
            "9692fc07-23c7-5f20-8779-f09374f1a571",
            "c4304791-586c-54cb-a7c6-27bd09f28dbd",
            "f5984d44-92bd-51ac-8e8f-d911dd824ef3"
        ]
    );
    assert_eq!(
        m.state().unwrap().replacements["c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"],
        "db33e43e-e652-5767-8e21-27fb3cef9723"
    );
}

#[test]
fn one_catalog_per_workspace() {
    let f = Fixture::new();
    let a = f.catalog("catalog-a");
    let b = f.catalog("catalog-b");
    f.publish(&a, "c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5", "1.0.0", &[]);
    let other = f.author("627a9f5c-10d4-5960-a0ab-4df072d8b52d");
    let tag = f.release(&other, "627a9f5c-10d4-5960-a0ab-4df072d8b52d", "1.0.0", &[]);
    catalog::register(&b, other.to_str().unwrap(), &tag).unwrap();
    f.verify(&a);
    f.verify(&b);
    f.open(&a)
        .install_from_catalog("c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5", None)
        .unwrap();
    let m = f.open(&b);
    let error = f.refuses("catalog", || {
        m.install_from_catalog("627a9f5c-10d4-5960-a0ab-4df072d8b52d", None)
    });
    assert!(
        error.to_string().contains("mixing verified catalogs"),
        "{error}"
    );
    // An unverified catalog is exempt: it records nothing.
    let c = f.catalog("catalog-c");
    let third = f.author("055241a5-ec83-58c8-9b54-93e74bee2e74");
    let tag = f.release(&third, "055241a5-ec83-58c8-9b54-93e74bee2e74", "1.0.0", &[]);
    catalog::register(&c, third.to_str().unwrap(), &tag).unwrap();
    drop(m);
    let m = f.open(&c);
    m.install_from_catalog("055241a5-ec83-58c8-9b54-93e74bee2e74", None)
        .unwrap();
    let state = m.state().unwrap();
    assert!(
        state.packages["055241a5-ec83-58c8-9b54-93e74bee2e74"]
            .catalog
            .is_none()
    );
    assert_eq!(
        state.packages["c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5"]
            .catalog
            .as_deref(),
        Some(catalog::canonical_location(&a).unwrap().as_str())
    );
    // Same catalog through another spelling is the same catalog.
    drop(m);
    let m = f.open(&format!("{a}/catalog.json"));
    assert!(
        m.install_from_catalog("c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5", None)
            .unwrap()
            .already_installed
    );
}

#[test]
fn check_shows_fork_upstream_and_other_catalogs() {
    use gitpm::check::{CheckSource, CheckStatus};
    let f = Fixture::new();
    let a = f.catalog("catalog-a");
    f.publish(&a, "dbe02e84-996e-560c-91fc-cce0f29e320e", "1.0.0", &[]);
    f.verify(&a);
    let m = f.open(&a);
    m.install_from_catalog("dbe02e84-996e-560c-91fc-cce0f29e320e", None)
        .unwrap();
    let fork = m
        .fork_into(
            "dbe02e84-996e-560c-91fc-cce0f29e320e",
            &f.path("bc81542b-e3ef-5d55-8250-7713457b269a"),
        )
        .unwrap();
    let generated_0: String =
        serde_json::from_slice::<serde_json::Value>(&fs::read(fork.join("plugin.json")).unwrap())
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
    scaffold::commit(&fork, "1.0.1").unwrap();
    m.install(fork.to_str().unwrap()).unwrap();
    let row = |m: &Manager, id: &str| m.check().unwrap().into_iter().find(|r| r.id == id).unwrap();
    let upstream = row(&m, generated_0.as_str()).upstream.unwrap();
    assert_eq!(upstream.source, Some(CheckSource::Catalog));
    assert_eq!(
        (
            upstream.id.as_str(),
            upstream.base.as_str(),
            upstream.latest.as_deref(),
            upstream.newer
        ),
        (
            "dbe02e84-996e-560c-91fc-cce0f29e320e",
            "1.0.0",
            Some("1.0.0"),
            false
        )
    );
    assert!(
        row(&m, "dbe02e84-996e-560c-91fc-cce0f29e320e")
            .upstream
            .is_none()
    );
    // The original releases 1.1.0: the fork shows it, without changing anything.
    f.publish(&a, "dbe02e84-996e-560c-91fc-cce0f29e320e", "1.1.0", &[]);
    let before = f.snapshot();
    let upstream = row(&m, generated_0.as_str()).upstream.unwrap();
    assert_eq!(
        (upstream.latest.as_deref(), upstream.newer),
        (Some("1.1.0"), true)
    );
    assert_eq!(
        row(&m, "dbe02e84-996e-560c-91fc-cce0f29e320e").status,
        CheckStatus::UpdateAvailable
    );
    assert!(f.snapshot() == before);

    // Another catalog selected: packages from catalog A are not compared against it.
    let b = f.catalog("catalog-b");
    let other = f.author("627a9f5c-10d4-5960-a0ab-4df072d8b52d");
    let tag = f.release(&other, "627a9f5c-10d4-5960-a0ab-4df072d8b52d", "1.0.0", &[]);
    catalog::register(&b, other.to_str().unwrap(), &tag).unwrap();
    drop(m);
    let m = f.open(&b);
    assert_eq!(
        row(&m, "dbe02e84-996e-560c-91fc-cce0f29e320e").status,
        CheckStatus::OtherCatalog
    );
    // Catalog B does not list the original: its upstream comes from forkOf.source.
    let upstream = row(&m, generated_0.as_str()).upstream.unwrap();
    assert_eq!(
        (upstream.latest.as_deref(), upstream.source),
        (Some("1.1.0"), Some(CheckSource::Origin))
    );

    // Without any catalog the protected original is compared with its verified catalog;
    // the fork's row and its upstream come from the origins.
    drop(m);
    let m = Manager::open_with(f.path("workspace"), Options::default()).unwrap();
    let original = row(&m, "dbe02e84-996e-560c-91fc-cce0f29e320e");
    assert_eq!(
        (original.status, original.source, original.latest.as_deref()),
        (
            CheckStatus::UpdateAvailable,
            CheckSource::Catalog,
            Some("1.1.0")
        )
    );
    let upstream = row(&m, generated_0.as_str()).upstream.unwrap();
    assert_eq!(
        (upstream.latest.as_deref(), upstream.newer, upstream.source),
        (Some("1.1.0"), true, Some(CheckSource::Origin))
    );
}

#[test]
fn update_all_coordinates_versions_and_adds_dependencies_atomically() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "757fd5ca-2b6a-5b2f-8a5d-60980b028665", "1.0.0", &[]);
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.0.0",
        &[("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^1")],
    );
    let m = f.open(&cat);
    m.install_from_catalog("9692fc07-23c7-5f20-8779-f09374f1a571", None)
        .unwrap();
    f.publish(&cat, "757fd5ca-2b6a-5b2f-8a5d-60980b028665", "2.0.0", &[]);
    f.publish(&cat, "385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70", "1.0.0", &[]);
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "2.0.0",
        &[
            ("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^2"),
            ("385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70", "^1"),
        ],
    );
    // Neither individual update can cross the version boundary alone.
    f.refuses("dependency_conflict", || {
        m.update("757fd5ca-2b6a-5b2f-8a5d-60980b028665")
    });
    f.refuses("dependency_conflict", || {
        m.update("9692fc07-23c7-5f20-8779-f09374f1a571")
    });
    let before = f.snapshot();
    let plan = m.update_all_plan().unwrap();
    assert!(
        f.snapshot() == before,
        "planning must not change refs, files or state"
    );
    assert_eq!(plan.updates.len(), 2);
    // Both cross a major version, which is marked, not refused.
    assert!(plan.updates.iter().all(|c| c.major), "{plan:?}");
    assert_eq!(plan.installs[0].id, "385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70");
    assert!(plan.kept_back.is_empty());
    let applied = m.update_all().unwrap();
    assert_eq!(
        applied.dist.deployed,
        [
            "385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70",
            "757fd5ca-2b6a-5b2f-8a5d-60980b028665",
            "9692fc07-23c7-5f20-8779-f09374f1a571"
        ]
    );
    // The plan is what was applied; only the applied one carries a dist report.
    assert_eq!(
        serde_json::to_value(plan).unwrap(),
        serde_json::to_value(gitpm::UpdateAllPlan {
            dist: Default::default(),
            ..applied
        })
        .unwrap()
    );
    for id in [
        "757fd5ca-2b6a-5b2f-8a5d-60980b028665",
        "9692fc07-23c7-5f20-8779-f09374f1a571",
    ] {
        assert_eq!(plugin(&m, id).current.unwrap().version, "2.0.0");
        assert_eq!(
            m.state().unwrap().packages[id]
                .previous
                .as_ref()
                .unwrap()
                .version,
            "1.0.0"
        );
        assert_eq!(
            fs::read_to_string(f.path(&format!("workspace/dist/{id}/index.js"))).unwrap(),
            format!("{id} 2.0.0")
        );
    }
    assert!(
        plugin(&m, "9692fc07-23c7-5f20-8779-f09374f1a571")
            .dependencies
            .iter()
            .all(|d| d.satisfied)
    );
    assert!(m.update_all().unwrap().updates.is_empty());
}

#[test]
fn update_all_backtracks_to_compatible_versions_and_keeps_current_fallback() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "757fd5ca-2b6a-5b2f-8a5d-60980b028665", "1.0.0", &[]);
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.0.0",
        &[("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^1")],
    );
    f.publish(
        &cat,
        "f5984d44-92bd-51ac-8e8f-d911dd824ef3",
        "1.0.0",
        &[("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^1")],
    );
    let m = f.open(&cat);
    m.install_from_catalog("9692fc07-23c7-5f20-8779-f09374f1a571", None)
        .unwrap();
    m.install_from_catalog("f5984d44-92bd-51ac-8e8f-d911dd824ef3", None)
        .unwrap();
    f.publish(&cat, "757fd5ca-2b6a-5b2f-8a5d-60980b028665", "1.5.0", &[]);
    f.publish(&cat, "757fd5ca-2b6a-5b2f-8a5d-60980b028665", "2.0.0", &[]);
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.5.0",
        &[("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^1")],
    );
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "2.0.0",
        &[("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^2")],
    );
    let plan = m.update_all().unwrap();
    assert_eq!(
        plugin(&m, "9692fc07-23c7-5f20-8779-f09374f1a571")
            .current
            .unwrap()
            .version,
        "1.5.0"
    );
    assert_eq!(
        plugin(&m, "757fd5ca-2b6a-5b2f-8a5d-60980b028665")
            .current
            .unwrap()
            .version,
        "1.5.0"
    );
    assert_eq!(
        plugin(&m, "f5984d44-92bd-51ac-8e8f-d911dd824ef3")
            .current
            .unwrap()
            .version,
        "1.0.0"
    );
    assert_eq!(
        plan.kept_back
            .iter()
            .map(|p| (p.id.as_str(), &p.reason))
            .collect::<Vec<_>>(),
        [
            (
                "757fd5ca-2b6a-5b2f-8a5d-60980b028665",
                &gitpm::HeldBackReason::Constraints
            ),
            (
                "9692fc07-23c7-5f20-8779-f09374f1a571",
                &gitpm::HeldBackReason::Constraints
            )
        ]
    );
    // An unavailable new dependency makes this release unusable, not the whole upgrade.
    f.publish(
        &cat,
        "f5984d44-92bd-51ac-8e8f-d911dd824ef3",
        "2.0.0",
        &[("0276fe29-bef5-5d0b-aae9-f9920c809729", "^1")],
    );
    let plan = m.update_all().unwrap();
    assert!(plan.updates.is_empty());
    assert_eq!(plan.kept_back.len(), 3);
}

#[test]
fn update_all_handles_cycles_without_a_catalog() {
    let f = Fixture::new();
    let a = f.author("d35b3151-28fd-557a-a363-b5f18ae491c8");
    let b = f.author("3a53cdea-2cb1-5653-bf22-898b679440ee");
    f.release(&a, "d35b3151-28fd-557a-a363-b5f18ae491c8", "1.0.0", &[]);
    f.release(
        &b,
        "3a53cdea-2cb1-5653-bf22-898b679440ee",
        "1.0.0",
        &[("d35b3151-28fd-557a-a363-b5f18ae491c8", "^1")],
    );
    let m = Manager::open(f.path("workspace")).unwrap();
    m.install(a.to_str().unwrap()).unwrap();
    m.install(b.to_str().unwrap()).unwrap();
    f.release(
        &a,
        "d35b3151-28fd-557a-a363-b5f18ae491c8",
        "2.0.0",
        &[("3a53cdea-2cb1-5653-bf22-898b679440ee", "^2")],
    );
    f.release(
        &b,
        "3a53cdea-2cb1-5653-bf22-898b679440ee",
        "2.0.0",
        &[("d35b3151-28fd-557a-a363-b5f18ae491c8", "^2")],
    );
    assert_eq!(m.update_all().unwrap().updates.len(), 2);
    assert!(plugin(&m, "d35b3151-28fd-557a-a363-b5f18ae491c8").dependencies[0].satisfied);
    assert!(plugin(&m, "3a53cdea-2cb1-5653-bf22-898b679440ee").dependencies[0].satisfied);
}

#[test]
fn update_all_preserves_local_work_and_never_partially_deploys() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    for id in [
        "3a53cdea-2cb1-5653-bf22-898b679440ee",
        "d35b3151-28fd-557a-a363-b5f18ae491c8",
    ] {
        f.publish(&cat, id, "1.0.0", &[]);
    }
    let m = f.open(&cat);
    for id in [
        "3a53cdea-2cb1-5653-bf22-898b679440ee",
        "d35b3151-28fd-557a-a363-b5f18ae491c8",
    ] {
        m.install_from_catalog(id, None).unwrap();
        f.publish(&cat, id, "2.0.0", &[]);
    }
    let local = f
        .path("workspace")
        .join("worktrees/3a53cdea-2cb1-5653-bf22-898b679440ee")
        .join("dist/local.js");
    fs::write(&local, "local work").unwrap();
    f.refuses("local_changes", || m.update_all());
    f.refuses("local_changes", || m.update_all_plan());
    fs::remove_file(local).unwrap();
    let repo = f.path("authors/3a53cdea-2cb1-5653-bf22-898b679440ee");
    // A valid manifest with an undeployable payload fails after d35b3151-28fd-557a-a363-b5f18ae491c8 has been staged.
    fs::remove_dir_all(repo.join("dist")).unwrap();
    git(&repo, &["add", "--all"]);
    git(&repo, &["commit", "-qm", "remove dist"]);
    git(&repo, &["tag", "-f", "v2.0.0"]);
    let error = f.refuses("invalid_manifest", || m.update_all());
    assert!(
        error
            .to_string()
            .starts_with("3a53cdea-2cb1-5653-bf22-898b679440ee: "),
        "{error}"
    );
    assert!(
        matches!(error, gitpm::Error::InvalidManifest { id: Some(id), .. } if id == "3a53cdea-2cb1-5653-bf22-898b679440ee")
    );
    assert_eq!(
        plugin(&m, "d35b3151-28fd-557a-a363-b5f18ae491c8")
            .current
            .unwrap()
            .version,
        "1.0.0"
    );
}

#[test]
fn update_all_does_not_downgrade_and_warns_about_moved_and_retracted_releases() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "d35b3151-28fd-557a-a363-b5f18ae491c8", "2.0.0", &[]);
    let m = f.open(&cat);
    m.install_from_catalog("d35b3151-28fd-557a-a363-b5f18ae491c8", None)
        .unwrap();
    let installed = plugin(&m, "d35b3151-28fd-557a-a363-b5f18ae491c8")
        .current
        .unwrap()
        .commit;
    let repo = f.path("authors/d35b3151-28fd-557a-a363-b5f18ae491c8");
    git(&repo, &["tag", "-d", "v2.0.0"]);
    f.release(&repo, "d35b3151-28fd-557a-a363-b5f18ae491c8", "1.0.0", &[]);
    // Retracted: no downgrade, up to date with a warning.
    let plan = m.update_all().unwrap();
    assert!(plan.updates.is_empty());
    assert_eq!(
        plan.unchanged[0].warnings,
        [
            "installed release d35b3151-28fd-557a-a363-b5f18ae491c8@2.0.0 no longer exists on the origin"
        ]
    );
    assert_eq!(
        plugin(&m, "d35b3151-28fd-557a-a363-b5f18ae491c8")
            .current
            .unwrap()
            .version,
        "2.0.0"
    );
    // Retagged at another commit: taken as it is, the installed commit is kept.
    f.release(&repo, "d35b3151-28fd-557a-a363-b5f18ae491c8", "2.0.0", &[]);
    let plan = m.update_all().unwrap();
    assert!(plan.updates.is_empty());
    assert_eq!(plan.unchanged[0].warnings.len(), 1);
    assert!(
        plan.unchanged[0].warnings[0].starts_with("release tag v2.0.0 now points to "),
        "{:?}",
        plan.unchanged[0].warnings
    );
    assert_eq!(
        plugin(&m, "d35b3151-28fd-557a-a363-b5f18ae491c8")
            .current
            .unwrap()
            .commit,
        installed
    );
}

#[test]
fn update_all_reports_unsatisfiable_state_and_preserves_disabled_flags() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "757fd5ca-2b6a-5b2f-8a5d-60980b028665", "1.0.0", &[]);
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.0.0",
        &[("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^1")],
    );
    let m = f.open(&cat);
    m.install_from_catalog("9692fc07-23c7-5f20-8779-f09374f1a571", None)
        .unwrap();
    m.set_enabled_with("757fd5ca-2b6a-5b2f-8a5d-60980b028665", false, true)
        .unwrap();
    // The consumer of a disabled dependency is broken: skipped, not a failure.
    f.publish(&cat, "757fd5ca-2b6a-5b2f-8a5d-60980b028665", "1.1.0", &[]);
    let plan = m.update_all().unwrap();
    assert_eq!(plan.updates[0].id, "757fd5ca-2b6a-5b2f-8a5d-60980b028665");
    let skipped = plan
        .unchanged
        .iter()
        .find(|u| u.id == "9692fc07-23c7-5f20-8779-f09374f1a571")
        .unwrap();
    assert_eq!(
        skipped.reason,
        Some(gitpm::UnchangedReason::BrokenDependency {
            dependency: "757fd5ca-2b6a-5b2f-8a5d-60980b028665".into(),
            problem: "757fd5ca-2b6a-5b2f-8a5d-60980b028665 is disabled".into(),
        })
    );
    assert!(!plugin(&m, "757fd5ca-2b6a-5b2f-8a5d-60980b028665").enabled);
    m.enable("757fd5ca-2b6a-5b2f-8a5d-60980b028665").unwrap();
    m.disable("9692fc07-23c7-5f20-8779-f09374f1a571").unwrap();
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "2.0.0",
        &[("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^1")],
    );
    m.update_all().unwrap();
    assert!(!plugin(&m, "9692fc07-23c7-5f20-8779-f09374f1a571").enabled);
    // An inactive consumer takes a disabled dependency as given (as `update ID` does).
    m.set_enabled_with("757fd5ca-2b6a-5b2f-8a5d-60980b028665", false, true)
        .unwrap();
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "3.0.0",
        &[("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^1")],
    );
    let plan = m.update_all().unwrap();
    assert_eq!(plan.updates[0].to.version, "3.0.0");
    assert!(plan.unchanged.iter().all(|u| u.reason.is_none()));
}

#[test]
fn update_all_resolves_replacement_versions_together_with_consumers() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "1.0.0", &[]);
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.0.0",
        &[("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "^1")],
    );
    let m = f.open(&cat);
    m.install_from_catalog("9692fc07-23c7-5f20-8779-f09374f1a571", None)
        .unwrap();
    let fork = f.author("db33e43e-e652-5767-8e21-27fb3cef9723");
    f.release_manifest(&fork, json!({"id":"db33e43e-e652-5767-8e21-27fb3cef9723", "name": "fixture", "version":"1.0.0", "replaces":{"id":"c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "version":"1.0.0"}}));
    m.install(fork.to_str().unwrap()).unwrap();
    m.replace(
        "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        "db33e43e-e652-5767-8e21-27fb3cef9723",
    )
    .unwrap();
    f.release_manifest(&fork, json!({"id":"db33e43e-e652-5767-8e21-27fb3cef9723", "name": "fixture", "version":"2.0.0", "replaces":{"id":"c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "version":"2.0.0"}}));
    // A newer fork that drops its declaration is not an eligible replacement.
    f.release_manifest(
        &fork,
        json!({"id":"db33e43e-e652-5767-8e21-27fb3cef9723", "name": "fixture", "version":"3.0.0"}),
    );
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "2.0.0",
        &[("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "^2")],
    );
    let plan = m.update_all().unwrap();
    let held = plan
        .kept_back
        .iter()
        .find(|h| h.id == "db33e43e-e652-5767-8e21-27fb3cef9723")
        .unwrap();
    assert_eq!(
        (held.selected.as_str(), held.latest.as_str(), &held.reason),
        (
            "2.0.0",
            "3.0.0",
            &gitpm::HeldBackReason::NoLongerReplaces {
                original: "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9".into()
            }
        )
    );
    assert_eq!(
        held.reason.to_string(),
        "no longer replaces c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"
    );
    assert_eq!(
        plugin(&m, "db33e43e-e652-5767-8e21-27fb3cef9723")
            .current
            .unwrap()
            .version,
        "2.0.0"
    );
    assert_eq!(
        plugin(&m, "9692fc07-23c7-5f20-8779-f09374f1a571")
            .current
            .unwrap()
            .version,
        "2.0.0"
    );
    assert_eq!(
        plugin(&m, "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9")
            .current
            .unwrap()
            .version,
        "1.0.0"
    );
    assert_eq!(
        provider(
            &m,
            "9692fc07-23c7-5f20-8779-f09374f1a571",
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"
        ),
        (Some("db33e43e-e652-5767-8e21-27fb3cef9723".into()), true)
    );
}

/// The replace and switch-back lines are built from the plan alone.
#[test]
fn replacement_plan_carries_installed_version() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "1.4.0", &[]);
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.0.0",
        &[("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "^1.0")],
    );
    let m = f.open(&cat);
    m.install_from_catalog("9692fc07-23c7-5f20-8779-f09374f1a571", None)
        .unwrap();
    let repo = f.author("db33e43e-e652-5767-8e21-27fb3cef9723");
    f.release_manifest(
        &repo,
        json!({"id": "db33e43e-e652-5767-8e21-27fb3cef9723", "name": "fixture", "version": "0.3.0", "replaces": {"id": "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "version": "1.4.0"}}),
    );
    m.install(repo.to_str().unwrap()).unwrap();
    let plan = m
        .replace_plan(
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            "db33e43e-e652-5767-8e21-27fb3cef9723",
        )
        .unwrap();
    assert_eq!(
        (plan.version.as_str(), plan.installed_version.as_deref()),
        ("1.4.0", Some("0.3.0"))
    );
    assert!(plan.dist.deployed.is_empty() && plan.dist.removed.is_empty());
    let applied = m
        .replace(
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            "db33e43e-e652-5767-8e21-27fb3cef9723",
        )
        .unwrap();
    assert_eq!(applied.installed_version.as_deref(), Some("0.3.0"));
    assert_eq!(
        applied.dist.removed,
        ["c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"]
    );
    // Without consumers left, disabling the fork drops the binding and deploys the
    // original again; a later enable does not bring the binding back.
    m.uninstall("9692fc07-23c7-5f20-8779-f09374f1a571", &[], true)
        .unwrap();
    let dist = m.disable("db33e43e-e652-5767-8e21-27fb3cef9723").unwrap();
    assert_eq!(
        (dist.deployed.as_slice(), dist.removed.as_slice()),
        (
            ["c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9".to_owned()].as_slice(),
            ["db33e43e-e652-5767-8e21-27fb3cef9723".to_owned()].as_slice()
        )
    );
    assert!(m.state().unwrap().replacements.is_empty());
    m.enable("db33e43e-e652-5767-8e21-27fb3cef9723").unwrap();
    assert!(m.state().unwrap().replacements.is_empty());
    // Switching back: the plan names the fork and the original's installed version.
    m.replace(
        "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        "db33e43e-e652-5767-8e21-27fb3cef9723",
    )
    .unwrap();
    let plan = m
        .replace_plan(
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        )
        .unwrap();
    assert_eq!(
        plan.current.as_deref(),
        Some("db33e43e-e652-5767-8e21-27fb3cef9723")
    );
    assert_eq!(plan.installed_version.as_deref(), Some("1.4.0"));
    let back = m
        .replace(
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        )
        .unwrap();
    assert_eq!(back.dist.deployed, ["c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"]);
}

/// A replacing package is always enabled (disabling it drops the binding): a state that
/// binds an original to a disabled package was edited by hand and is refused.
#[test]
fn a_binding_to_a_disabled_fork_is_invalid_state() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "1.0.0", &[]);
    let m = f.open(&cat);
    m.install_from_catalog("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", None)
        .unwrap();
    let repo = f.author("db33e43e-e652-5767-8e21-27fb3cef9723");
    f.release_manifest(
        &repo,
        json!({"id": "db33e43e-e652-5767-8e21-27fb3cef9723", "name": "fixture", "version": "1.0.0", "replaces": {"id": "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "version": "1.0.0"}}),
    );
    m.install(repo.to_str().unwrap()).unwrap();
    m.disable("db33e43e-e652-5767-8e21-27fb3cef9723").unwrap();
    let path = f.path("workspace/state.json");
    let mut state: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    state["replacements"] =
        json!({"c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9": "db33e43e-e652-5767-8e21-27fb3cef9723"});
    fs::write(&path, serde_json::to_vec_pretty(&state).unwrap()).unwrap();
    let error = m.state().unwrap_err();
    assert_eq!(error.kind(), "invalid_data");
    assert!(
        error
            .to_string()
            .contains("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 by the disabled package db33e43e-e652-5767-8e21-27fb3cef9723"),
        "{error}"
    );
    assert_eq!(
        m.replace(
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"
        )
        .unwrap_err()
        .kind(),
        "invalid_data"
    );
    let error = gitpm::list_with(&f.path("workspace"), &Default::default()).unwrap_err();
    assert_eq!(error.kind(), "invalid_data");
}

/// Every installed package is in exactly one list of the plan or in `updates` with a
/// `kept_back` note; current ones are `unchanged`.
#[test]
fn update_all_plan_lists_unchanged_packages() {
    let f = Fixture::new();
    let a = f.author("9692fc07-23c7-5f20-8779-f09374f1a571");
    f.release(&a, "9692fc07-23c7-5f20-8779-f09374f1a571", "1.0.0", &[]);
    let b = f.author("f5984d44-92bd-51ac-8e8f-d911dd824ef3");
    f.release(&b, "f5984d44-92bd-51ac-8e8f-d911dd824ef3", "2.0.0", &[]);
    let m = Manager::open(f.path("workspace")).unwrap();
    m.install(a.to_str().unwrap()).unwrap();
    m.install(b.to_str().unwrap()).unwrap();
    f.release(&a, "9692fc07-23c7-5f20-8779-f09374f1a571", "1.1.0", &[]);
    let plan = m.update_all_plan().unwrap();
    assert_eq!(plan.updates.len(), 1);
    assert_eq!(plan.updates[0].id, "9692fc07-23c7-5f20-8779-f09374f1a571");
    assert!(plan.kept_back.is_empty() && plan.installs.is_empty());
    assert_eq!(
        plan.unchanged,
        [gitpm::Unchanged {
            id: "f5984d44-92bd-51ac-8e8f-d911dd824ef3".into(),
            version: "2.0.0".into(),
            reason: None,
            warnings: Vec::new(),
        }]
    );
    let applied = m.update_all().unwrap();
    assert_eq!(applied.unchanged, plan.unchanged);
    assert_eq!(
        applied.dist.deployed,
        ["9692fc07-23c7-5f20-8779-f09374f1a571"]
    );
    let again = m.update_all().unwrap();
    assert_eq!(again.unchanged.len(), 2);
    assert!(again.updates.is_empty());
}

/// dnf model: a consumer whose dependency was removed with force is skipped with its
/// reason, the others are updated, and the removed dependency is never installed again,
/// with or without a catalog. `list` and `check` keep reporting it unsatisfied.
#[test]
fn update_all_skips_broken_consumers_and_never_reinstalls() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "757fd5ca-2b6a-5b2f-8a5d-60980b028665", "1.0.0", &[]);
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.0.0",
        &[("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^1")],
    );
    f.publish(&cat, "a768d335-0cf7-597a-b1ba-0eb542495aec", "1.0.0", &[]);
    let m = f.open(&cat);
    m.install_from_catalog("9692fc07-23c7-5f20-8779-f09374f1a571", None)
        .unwrap();
    m.install_from_catalog("a768d335-0cf7-597a-b1ba-0eb542495aec", None)
        .unwrap();
    m.uninstall("757fd5ca-2b6a-5b2f-8a5d-60980b028665", &[], true)
        .unwrap();
    f.publish(&cat, "a768d335-0cf7-597a-b1ba-0eb542495aec", "1.1.0", &[]);
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.1.0",
        &[("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^1")],
    );
    let broken = gitpm::Unchanged {
        id: "9692fc07-23c7-5f20-8779-f09374f1a571".into(),
        version: "1.0.0".into(),
        reason: Some(gitpm::UnchangedReason::BrokenDependency {
            dependency: "757fd5ca-2b6a-5b2f-8a5d-60980b028665".into(),
            problem: "not installed".into(),
        }),
        warnings: Vec::new(),
    };
    let assert_skipped = |plan: &gitpm::UpdateAllPlan, other: &str| {
        assert_eq!(
            plan.updates
                .iter()
                .map(|c| (c.id.as_str(), c.to.version.as_str()))
                .collect::<Vec<_>>(),
            [("a768d335-0cf7-597a-b1ba-0eb542495aec", other)]
        );
        assert!(plan.installs.is_empty() && plan.kept_back.is_empty());
        assert_eq!(plan.unchanged, std::slice::from_ref(&broken));
        assert_eq!(
            broken.reason.as_ref().unwrap().to_string(),
            "dependency 757fd5ca-2b6a-5b2f-8a5d-60980b028665 is not installed"
        );
    };
    // With a catalog that has 757fd5ca-2b6a-5b2f-8a5d-60980b028665.
    assert_skipped(&m.update_all_plan().unwrap(), "1.1.0");
    assert_skipped(&m.update_all().unwrap(), "1.1.0");
    assert!(!ids(&m).contains(&"757fd5ca-2b6a-5b2f-8a5d-60980b028665".to_string()));
    assert!(
        !f.path("workspace/dist/757fd5ca-2b6a-5b2f-8a5d-60980b028665")
            .exists()
    );
    assert_eq!(
        provider(
            &m,
            "9692fc07-23c7-5f20-8779-f09374f1a571",
            "757fd5ca-2b6a-5b2f-8a5d-60980b028665"
        ),
        (None, false)
    );
    let report = gitpm::check::check(&f.path("workspace"), Some(&cat)).unwrap();
    assert!(
        report
            .unsatisfied
            .iter()
            .any(|u| u.package == "9692fc07-23c7-5f20-8779-f09374f1a571"
                && u.dependency == "757fd5ca-2b6a-5b2f-8a5d-60980b028665"),
        "{:?}",
        report.unsatisfied
    );
    // Without a catalog the run does not fail either.
    drop(m);
    let m = Manager::open_with(f.path("workspace"), Options::default()).unwrap();
    f.publish(&cat, "a768d335-0cf7-597a-b1ba-0eb542495aec", "1.2.0", &[]);
    assert_skipped(&m.update_all().unwrap(), "1.2.0");
    assert!(!ids(&m).contains(&"757fd5ca-2b6a-5b2f-8a5d-60980b028665".to_string()));
    // A single update of the broken consumer still fails.
    f.refuses("dependency_conflict", || {
        m.update("9692fc07-23c7-5f20-8779-f09374f1a571")
    });
}

/// Without a catalog, a newer release that needs a package that is not installed is kept
/// back with that reason, not a failure.
#[test]
fn update_all_keeps_back_a_release_needing_a_missing_dependency_without_a_catalog() {
    let f = Fixture::new();
    let a = f.author("9692fc07-23c7-5f20-8779-f09374f1a571");
    f.release(&a, "9692fc07-23c7-5f20-8779-f09374f1a571", "1.0.0", &[]);
    let m = Manager::open(f.path("workspace")).unwrap();
    m.install(a.to_str().unwrap()).unwrap();
    f.release(&a, "9692fc07-23c7-5f20-8779-f09374f1a571", "1.1.0", &[]);
    f.release(
        &a,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "2.0.0",
        &[("61cfe0c5-cc76-56a1-8a43-f9ca4faad534", "^1")],
    );
    let plan = m.update_all().unwrap();
    assert_eq!(plan.updates[0].to.version, "1.1.0");
    let held = &plan.kept_back[0];
    assert_eq!(
        (
            held.id.as_str(),
            held.selected.as_str(),
            held.latest.as_str()
        ),
        ("9692fc07-23c7-5f20-8779-f09374f1a571", "1.1.0", "2.0.0")
    );
    assert_eq!(
        held.reason,
        gitpm::HeldBackReason::MissingDependency {
            dependency: "61cfe0c5-cc76-56a1-8a43-f9ca4faad534".into(),
            requirement: "^1".into(),
        }
    );
    assert_eq!(
        held.reason.to_string(),
        "needs 61cfe0c5-cc76-56a1-8a43-f9ca4faad534 ^1, which is not installed and no catalog is configured"
    );
}

/// Dependencies `update all` pulls for a disabled consumer are recorded disabled, and a
/// new dependency only a disabled consumer pulls takes disabled providers as given (as
/// `update ID` of a disabled package does).
#[test]
fn update_all_pulls_disabled_dependencies_for_a_disabled_consumer() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "757fd5ca-2b6a-5b2f-8a5d-60980b028665", "1.0.0", &[]);
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.0.0",
        &[("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^1")],
    );
    let m = f.open(&cat);
    m.install_from_catalog("9692fc07-23c7-5f20-8779-f09374f1a571", None)
        .unwrap();
    m.disable("9692fc07-23c7-5f20-8779-f09374f1a571").unwrap();
    f.publish(&cat, "385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70", "1.0.0", &[]);
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.1.0",
        &[
            ("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^1"),
            ("385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70", "^1"),
        ],
    );
    let plan = m.update_all().unwrap();
    assert_eq!(plan.updates[0].to.version, "1.1.0");
    assert_eq!(plan.installs[0].id, "385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70");
    assert!(
        !plan.installs[0].enabled && !plugin(&m, "385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70").enabled
    );
    assert!(
        !f.path("workspace/dist/385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70")
            .exists()
    );
    // A new c4304791-586c-54cb-a7c6-27bd09f28dbd needs the disabled 757fd5ca-2b6a-5b2f-8a5d-60980b028665: 9692fc07-23c7-5f20-8779-f09374f1a571 still moves, c4304791-586c-54cb-a7c6-27bd09f28dbd disabled.
    m.set_enabled_with("757fd5ca-2b6a-5b2f-8a5d-60980b028665", false, true)
        .unwrap();
    f.publish(
        &cat,
        "c4304791-586c-54cb-a7c6-27bd09f28dbd",
        "1.0.0",
        &[("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^1")],
    );
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.2.0",
        &[
            ("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^1"),
            ("385bd2ba-9e86-5dbd-bd4e-1dc5e7eb5b70", "^1"),
            ("c4304791-586c-54cb-a7c6-27bd09f28dbd", "^1"),
        ],
    );
    let plan = m.update_all().unwrap();
    assert_eq!(plan.updates[0].to.version, "1.2.0");
    assert!(plan.kept_back.is_empty(), "{:?}", plan.kept_back);
    assert_eq!(plan.installs[0].id, "c4304791-586c-54cb-a7c6-27bd09f28dbd");
    assert!(!plugin(&m, "c4304791-586c-54cb-a7c6-27bd09f28dbd").enabled);
    assert!(!plugin(&m, "9692fc07-23c7-5f20-8779-f09374f1a571").enabled);
}

/// A new dependency an active consumer pulls is enabled, and is not satisfied by a
/// disabled provider.
#[test]
fn update_all_enables_dependencies_of_active_consumers_only() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "757fd5ca-2b6a-5b2f-8a5d-60980b028665", "1.0.0", &[]);
    f.publish(&cat, "9692fc07-23c7-5f20-8779-f09374f1a571", "1.0.0", &[]);
    f.publish(&cat, "f5984d44-92bd-51ac-8e8f-d911dd824ef3", "1.0.0", &[]);
    let m = f.open(&cat);
    m.install_from_catalog("9692fc07-23c7-5f20-8779-f09374f1a571", None)
        .unwrap();
    m.install_from_catalog("f5984d44-92bd-51ac-8e8f-d911dd824ef3", None)
        .unwrap();
    m.install_from_catalog("757fd5ca-2b6a-5b2f-8a5d-60980b028665", None)
        .unwrap();
    m.disable("757fd5ca-2b6a-5b2f-8a5d-60980b028665").unwrap();
    f.publish(
        &cat,
        "c4304791-586c-54cb-a7c6-27bd09f28dbd",
        "1.0.0",
        &[("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^1")],
    );
    f.publish(&cat, "61cfe0c5-cc76-56a1-8a43-f9ca4faad534", "1.0.0", &[]);
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.1.0",
        &[("61cfe0c5-cc76-56a1-8a43-f9ca4faad534", "^1")],
    );
    f.publish(
        &cat,
        "f5984d44-92bd-51ac-8e8f-d911dd824ef3",
        "1.1.0",
        &[("c4304791-586c-54cb-a7c6-27bd09f28dbd", "^1")],
    );
    let plan = m.update_all().unwrap();
    assert!(plugin(&m, "61cfe0c5-cc76-56a1-8a43-f9ca4faad534").enabled);
    assert_eq!(
        plugin(&m, "9692fc07-23c7-5f20-8779-f09374f1a571")
            .current
            .unwrap()
            .version,
        "1.1.0"
    );
    // f5984d44-92bd-51ac-8e8f-d911dd824ef3 is active: c4304791-586c-54cb-a7c6-27bd09f28dbd would be active next to the disabled 757fd5ca-2b6a-5b2f-8a5d-60980b028665.
    assert_eq!(
        plugin(&m, "f5984d44-92bd-51ac-8e8f-d911dd824ef3")
            .current
            .unwrap()
            .version,
        "1.0.0"
    );
    assert!(
        plan.kept_back
            .iter()
            .any(|h| h.id == "f5984d44-92bd-51ac-8e8f-d911dd824ef3")
    );
    assert!(!ids(&m).contains(&"c4304791-586c-54cb-a7c6-27bd09f28dbd".to_string()));
}

/// db33e43e-e652-5767-8e21-27fb3cef9723 declares c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 2.0.0 and replaces the installed c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 1.4.0;
/// 9692fc07-23c7-5f20-8779-f09374f1a571 (`>=1`) is satisfied by both, f5984d44-92bd-51ac-8e8f-d911dd824ef3 (`^2`) only by the fork.
fn fork_over_installed_original(f: &Fixture) -> Manager {
    let cat = f.catalog("catalog");
    f.publish(&cat, "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "1.4.0", &[]);
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.0.0",
        &[("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", ">=1")],
    );
    f.publish(
        &cat,
        "f5984d44-92bd-51ac-8e8f-d911dd824ef3",
        "1.0.0",
        &[("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "^2")],
    );
    let m = f.open(&cat);
    m.install_from_catalog("9692fc07-23c7-5f20-8779-f09374f1a571", None)
        .unwrap();
    let fork = f.author("db33e43e-e652-5767-8e21-27fb3cef9723");
    f.release_manifest(
        &fork,
        json!({"id": "db33e43e-e652-5767-8e21-27fb3cef9723", "name": "fixture", "version": "0.1.0", "replaces": {"id": "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "version": "2.0.0"}}),
    );
    m.install(fork.to_str().unwrap()).unwrap();
    m.replace(
        "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        "db33e43e-e652-5767-8e21-27fb3cef9723",
    )
    .unwrap();
    m.install_from_catalog("f5984d44-92bd-51ac-8e8f-d911dd824ef3", None)
        .unwrap();
    m
}

#[test]
fn removing_an_active_fork_guards_only_consumers_the_original_breaks() {
    let f = Fixture::new();
    let m = fork_over_installed_original(&f);
    // 9692fc07-23c7-5f20-8779-f09374f1a571 is taken over by c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 1.4.0; only f5984d44-92bd-51ac-8e8f-d911dd824ef3 breaks.
    for error in [
        f.refuses("has_dependents", || {
            m.disable("db33e43e-e652-5767-8e21-27fb3cef9723")
        }),
        f.refuses("has_dependents", || {
            m.forget("db33e43e-e652-5767-8e21-27fb3cef9723")
        }),
        f.refuses("has_dependents", || {
            m.uninstall("db33e43e-e652-5767-8e21-27fb3cef9723", &[], false)
        }),
    ] {
        let Error::HasDependents {
            dependents,
            lost_work,
            ..
        } = &error
        else {
            panic!("{error:?}")
        };
        assert_eq!(dependents, &["f5984d44-92bd-51ac-8e8f-d911dd824ef3"]);
        assert!(lost_work.is_empty());
        assert!(
            !error
                .to_string()
                .contains("9692fc07-23c7-5f20-8779-f09374f1a571"),
            "{error}"
        );
    }
    let plan = m
        .uninstall_plan("db33e43e-e652-5767-8e21-27fb3cef9723")
        .unwrap();
    assert_eq!(plan.dependents, ["f5984d44-92bd-51ac-8e8f-d911dd824ef3"]);
    let refusal = plan.refusal().unwrap();
    assert_eq!(refusal.kind(), "has_dependents");
    assert_eq!(
        refusal.to_string(),
        "cannot uninstall db33e43e-e652-5767-8e21-27fb3cef9723: required by f5984d44-92bd-51ac-8e8f-d911dd824ef3; doing it anyway requires force"
    );

    // Without f5984d44-92bd-51ac-8e8f-d911dd824ef3 nothing breaks: the binding is dropped and the original deployed.
    m.uninstall("f5984d44-92bd-51ac-8e8f-d911dd824ef3", &[], false)
        .unwrap();
    assert!(
        m.uninstall_plan("db33e43e-e652-5767-8e21-27fb3cef9723")
            .unwrap()
            .refusal()
            .is_none()
    );
    let dist = m.disable("db33e43e-e652-5767-8e21-27fb3cef9723").unwrap();
    assert_eq!(dist.deployed, ["c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"]);
    assert!(m.state().unwrap().replacements.is_empty());
    assert_eq!(
        provider(
            &m,
            "9692fc07-23c7-5f20-8779-f09374f1a571",
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"
        ),
        (Some("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9".into()), true)
    );
    assert!(
        f.path("workspace/dist/c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9")
            .is_dir()
    );

    // A disabled original takes nothing over: the fork is guarded again ...
    m.enable("db33e43e-e652-5767-8e21-27fb3cef9723").unwrap();
    m.replace(
        "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        "db33e43e-e652-5767-8e21-27fb3cef9723",
    )
    .unwrap();
    m.disable("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9").unwrap();
    let error = f.refuses("has_dependents", || {
        m.forget("db33e43e-e652-5767-8e21-27fb3cef9723")
    });
    let Error::HasDependents { dependents, .. } = &error else {
        panic!("{error:?}")
    };
    assert_eq!(dependents, &["9692fc07-23c7-5f20-8779-f09374f1a571"]);
    // ... unless forced: the binding is dropped and 9692fc07-23c7-5f20-8779-f09374f1a571 is reported broken.
    m.forget_with("db33e43e-e652-5767-8e21-27fb3cef9723", true)
        .unwrap();
    assert!(m.state().unwrap().replacements.is_empty());
    assert_eq!(
        provider(
            &m,
            "9692fc07-23c7-5f20-8779-f09374f1a571",
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"
        ),
        (Some("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9".into()), false)
    );
}

#[test]
fn dropping_a_binding_checks_the_original_s_dependencies() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "7bb3523b-dc85-5ff2-a33f-55b071c001cb", "1.0.0", &[]);
    f.publish(
        &cat,
        "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        "1.0.0",
        &[("7bb3523b-dc85-5ff2-a33f-55b071c001cb", "^1")],
    );
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.0.0",
        &[("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "^1")],
    );
    let m = f.open(&cat);
    m.install_from_catalog("9692fc07-23c7-5f20-8779-f09374f1a571", None)
        .unwrap();
    let fork = f.author("db33e43e-e652-5767-8e21-27fb3cef9723");
    f.release_manifest(
        &fork,
        json!({"id": "db33e43e-e652-5767-8e21-27fb3cef9723", "name": "fixture", "version": "1.0.0", "replaces": {"id": "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "version": "1.0.0"}}),
    );
    m.install(fork.to_str().unwrap()).unwrap();
    m.replace(
        "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        "db33e43e-e652-5767-8e21-27fb3cef9723",
    )
    .unwrap();
    // c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 (replaced, but installed) still depends on 7bb3523b-dc85-5ff2-a33f-55b071c001cb.
    f.refuses("has_dependents", || {
        m.disable("7bb3523b-dc85-5ff2-a33f-55b071c001cb")
    });
    m.set_enabled_with("7bb3523b-dc85-5ff2-a33f-55b071c001cb", false, true)
        .unwrap();

    // Each way of reactivating c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 refuses its disabled dependency.
    for error in [
        f.refuses("dependency_conflict", || {
            m.disable("db33e43e-e652-5767-8e21-27fb3cef9723")
        }),
        f.refuses("dependency_conflict", || {
            m.forget("db33e43e-e652-5767-8e21-27fb3cef9723")
        }),
        f.refuses("dependency_conflict", || {
            m.uninstall("db33e43e-e652-5767-8e21-27fb3cef9723", &[], false)
        }),
        f.refuses("dependency_conflict", || {
            m.replace(
                "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
                "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            )
        }),
    ] {
        assert!(
            error
                .to_string()
                .contains("7bb3523b-dc85-5ff2-a33f-55b071c001cb is disabled"),
            "{error}"
        );
    }
    // The uninstall plan carries that refusal, so nothing asks about lost work first.
    let plan = m
        .uninstall_plan("db33e43e-e652-5767-8e21-27fb3cef9723")
        .unwrap();
    assert!(plan.dependents.is_empty());
    let refusal = plan.refusal().unwrap();
    assert_eq!(refusal.kind(), "dependency_conflict");
    assert_eq!(
        refusal.to_string(),
        "cannot uninstall db33e43e-e652-5767-8e21-27fb3cef9723, which would reactivate c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9: 7bb3523b-dc85-5ff2-a33f-55b071c001cb is disabled; enable it; force does it anyway"
    );
    let error = m
        .disable("db33e43e-e652-5767-8e21-27fb3cef9723")
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "cannot disable db33e43e-e652-5767-8e21-27fb3cef9723, which would reactivate c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9: 7bb3523b-dc85-5ff2-a33f-55b071c001cb is disabled; enable it; force does it anyway"
    );
    // Forced: the original is active again and reported broken.
    m.set_enabled_with("db33e43e-e652-5767-8e21-27fb3cef9723", false, true)
        .unwrap();
    assert!(m.state().unwrap().replacements.is_empty());
    assert!(
        f.path("workspace/dist/c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9")
            .is_dir()
    );
    let status = plugin(&m, "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9").dependencies[0].clone();
    assert_eq!(
        (status.satisfied, status.problem.as_deref()),
        (
            false,
            Some("7bb3523b-dc85-5ff2-a33f-55b071c001cb is disabled")
        )
    );
}

#[test]
fn replace_and_return_require_an_installed_original() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "1.0.0", &[]);
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.0.0",
        &[("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "^1")],
    );
    let m = f.open(&cat);
    m.install_from_catalog("9692fc07-23c7-5f20-8779-f09374f1a571", None)
        .unwrap();
    let fork = f.author("db33e43e-e652-5767-8e21-27fb3cef9723");
    f.release_manifest(
        &fork,
        json!({"id": "db33e43e-e652-5767-8e21-27fb3cef9723", "name": "fixture", "version": "1.0.0", "replaces": {"id": "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "version": "1.0.0"}}),
    );
    m.install(fork.to_str().unwrap()).unwrap();

    // replace --with needs the original installed.
    m.uninstall("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", &[], true)
        .unwrap();
    let error = f.refuses("not_installed", || {
        m.replace(
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            "db33e43e-e652-5767-8e21-27fb3cef9723",
        )
    });
    assert_eq!(
        error.to_string(),
        "package c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 is not installed"
    );
    f.refuses("not_installed", || {
        m.replace_plan(
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            "db33e43e-e652-5767-8e21-27fb3cef9723",
        )
    });

    // Replaced, then the original is uninstalled: the binding stays.
    m.install_from_catalog("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", None)
        .unwrap();
    m.replace(
        "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        "db33e43e-e652-5767-8e21-27fb3cef9723",
    )
    .unwrap();
    m.uninstall("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", &[], false)
        .unwrap();
    assert_eq!(
        m.state().unwrap().replacements["c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"],
        "db33e43e-e652-5767-8e21-27fb3cef9723"
    );

    // Returning requires an installed original and never drops an unusable binding.
    f.refuses("not_installed", || {
        m.replace_plan(
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        )
    });
    f.refuses("not_installed", || {
        m.replace(
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        )
    });
    assert_eq!(
        m.state().unwrap().replacements["c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"],
        "db33e43e-e652-5767-8e21-27fb3cef9723"
    );
    m.install_from_catalog("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", None)
        .unwrap();
    m.replace(
        "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
    )
    .unwrap();
    assert!(m.state().unwrap().replacements.is_empty());
    assert_eq!(
        provider(
            &m,
            "9692fc07-23c7-5f20-8779-f09374f1a571",
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"
        ),
        (Some("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9".into()), true)
    );
}

#[test]
fn returning_to_a_disabled_original_is_refused() {
    let f = Fixture::new();
    let m = fork_over_installed_original(&f);
    m.uninstall("f5984d44-92bd-51ac-8e8f-d911dd824ef3", &[], false)
        .unwrap();
    m.disable("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9").unwrap();
    let error = f.refuses("invalid_argument", || {
        m.replace(
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        )
    });
    assert!(error.to_string().contains("enable it first"));
    assert_eq!(
        m.state().unwrap().replacements["c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"],
        "db33e43e-e652-5767-8e21-27fb3cef9723"
    );
    m.enable("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9").unwrap();
    m.replace(
        "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
    )
    .unwrap();
    assert!(m.state().unwrap().replacements.is_empty());
}

#[test]
fn dependency_ranges_pick_pre_releases_only_when_named() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "757fd5ca-2b6a-5b2f-8a5d-60980b028665", "1.2.0", &[]);
    f.publish(
        &cat,
        "757fd5ca-2b6a-5b2f-8a5d-60980b028665",
        "1.3.0-beta.1",
        &[],
    );
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.0.0",
        &[("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^1.2.0")],
    );
    f.publish(
        &cat,
        "f5984d44-92bd-51ac-8e8f-d911dd824ef3",
        "1.0.0",
        &[("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "^1.3.0-beta.1")],
    );
    f.publish(
        &cat,
        "1a620c0a-aa3c-533c-9960-3d49016fa231",
        "1.0.0",
        &[("757fd5ca-2b6a-5b2f-8a5d-60980b028665", "1.2.0")],
    );
    let m = f.open(&cat);
    // A plain range never takes the newer pre-release.
    m.install_from_catalog("9692fc07-23c7-5f20-8779-f09374f1a571", None)
        .unwrap();
    assert_eq!(
        plugin(&m, "757fd5ca-2b6a-5b2f-8a5d-60980b028665")
            .current
            .unwrap()
            .version,
        "1.2.0"
    );
    // The bare version is exact and satisfied.
    m.install_from_catalog("1a620c0a-aa3c-533c-9960-3d49016fa231", None)
        .unwrap();
    // A range naming the pre-release needs it; 1.2.0 is installed and fixed.
    let error = f.refuses("dependency_conflict", || {
        m.install_from_catalog("f5984d44-92bd-51ac-8e8f-d911dd824ef3", None)
    });
    assert!(
        error
            .to_string()
            .contains("757fd5ca-2b6a-5b2f-8a5d-60980b028665"),
        "{error}"
    );
    // update all keeps 757fd5ca-2b6a-5b2f-8a5d-60980b028665 at its release: no consumer opts into the beta.
    let plan = m.update_all_plan().unwrap();
    assert!(plan.updates.is_empty(), "{plan:?}");
    // In another workspace the named pre-release is pulled for f5984d44-92bd-51ac-8e8f-d911dd824ef3.
    let other = Manager::open_with(
        f.path("other"),
        Options {
            catalog: Some(cat.clone()),
            custom_ssh: false,
        },
    )
    .unwrap();
    other
        .install_from_catalog("f5984d44-92bd-51ac-8e8f-d911dd824ef3", None)
        .unwrap();
    assert_eq!(
        other.state().unwrap().packages["757fd5ca-2b6a-5b2f-8a5d-60980b028665"]
            .current
            .version,
        "1.3.0-beta.1"
    );
}

/// Check and resolve are explicit, and replacement never installs dependencies.
#[test]
fn replacement_check_resolve_and_atomic_apply() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "1.0.0", &[]);
    f.publish(&cat, "9c429448-e9d1-5a4d-93af-f240b2ace3bd", "1.0.0", &[]);
    f.publish(
        &cat,
        "61cfe0c5-cc76-56a1-8a43-f9ca4faad534",
        "1.0.0",
        &[("9c429448-e9d1-5a4d-93af-f240b2ace3bd", "^1")],
    );
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.0.0",
        &[("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "^1")],
    );
    let m = f.open(&cat);
    m.install_from_catalog("9692fc07-23c7-5f20-8779-f09374f1a571", None)
        .unwrap();
    let fork = m
        .fork_into("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", &f.path("fork"))
        .unwrap();
    let generated_0: String =
        serde_json::from_slice::<serde_json::Value>(&fs::read(fork.join("plugin.json")).unwrap())
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
    let path = fork.join("plugin.json");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    manifest["dependencies"] = json!({"61cfe0c5-cc76-56a1-8a43-f9ca4faad534":"^1"});
    fs::write(&path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    scaffold::commit(&fork, "1.0.1").unwrap();
    // Install resolves dependencies by its own existing contract; remove them explicitly
    // to exercise the independently installed fork in an incomplete workspace.
    m.install(fork.to_str().unwrap()).unwrap();
    m.uninstall("61cfe0c5-cc76-56a1-8a43-f9ca4faad534", &[], true)
        .unwrap();
    m.uninstall("9c429448-e9d1-5a4d-93af-f240b2ace3bd", &[], true)
        .unwrap();
    let before = f.snapshot();
    let check = m
        .replace_check("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", generated_0.as_str())
        .unwrap();
    assert_eq!(check.issues.len(), 1);
    assert_eq!(check.issues[0].consumer, generated_0.as_str());
    assert_eq!(
        check.issues[0].dependency.id,
        "61cfe0c5-cc76-56a1-8a43-f9ca4faad534"
    );
    assert_eq!(
        check.missing,
        BTreeMap::from([
            (
                "61cfe0c5-cc76-56a1-8a43-f9ca4faad534".into(),
                "1.0.0".into()
            ),
            (
                "9c429448-e9d1-5a4d-93af-f240b2ace3bd".into(),
                "1.0.0".into()
            )
        ])
    );
    assert!(check.resolution_error.is_none());
    assert_eq!(before, f.snapshot());
    let error = f
        .refuses("dependency_conflict", || {
            m.replace("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", generated_0.as_str())
        })
        .to_string();
    assert!(!error.contains("no catalog configured"), "{error}");
    assert!(
        error.contains(
            format!("{generated_0} needs 61cfe0c5-cc76-56a1-8a43-f9ca4faad534 ^1").as_str()
        ),
        "{error}"
    );
    assert!(
        error.contains(
            "61cfe0c5-cc76-56a1-8a43-f9ca4faad534 1.0.0, 9c429448-e9d1-5a4d-93af-f240b2ace3bd 1.0.0"
        ),
        "{error}"
    );
    assert!(!error.contains("{\""), "{error}");
    assert_eq!(before, f.snapshot());
    let resolved = m
        .replace_resolve("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", generated_0.as_str())
        .unwrap();
    assert_eq!(resolved.installed.len(), 2);
    assert!(resolved.check.issues.is_empty());
    assert!(m.state().unwrap().replacements.is_empty());
    assert!(
        m.replace_check("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", generated_0.as_str())
            .unwrap()
            .issues
            .is_empty()
    );
    m.replace("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", generated_0.as_str())
        .unwrap();
    assert_eq!(
        provider(
            &m,
            "9692fc07-23c7-5f20-8779-f09374f1a571",
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"
        ),
        (Some(generated_0.as_str().into()), true)
    );
    m.replace(
        "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
    )
    .unwrap();
    assert!(m.state().unwrap().replacements.is_empty());

    // A stale successful check does not authorize later replacement.
    m.disable("61cfe0c5-cc76-56a1-8a43-f9ca4faad534")
        .unwrap_err();
    m.set_enabled_with("61cfe0c5-cc76-56a1-8a43-f9ca4faad534", false, true)
        .unwrap();
    let before = f.snapshot();
    let check = m
        .replace_check("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", generated_0.as_str())
        .unwrap();
    assert!(check.issues.iter().any(|i| i.dependency.problem.as_deref()
        == Some("61cfe0c5-cc76-56a1-8a43-f9ca4faad534 is disabled")));
    f.refuses("dependency_conflict", || {
        m.replace_resolve("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", generated_0.as_str())
    });
    f.refuses("dependency_conflict", || {
        m.replace("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", generated_0.as_str())
    });
    assert_eq!(before, f.snapshot());
}

#[test]
fn replacement_check_reports_version_conflicts_and_whole_workspace() {
    let f = Fixture::new();
    let m = fork_over_installed_original(&f);
    // Returning would break f5984d44-92bd-51ac-8e8f-d911dd824ef3's ^2 requirement. Both APIs use the same check.
    let check = m
        .replace_check(
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        )
        .unwrap();
    assert_eq!(
        check.plan.incompatible[0].id,
        "f5984d44-92bd-51ac-8e8f-d911dd824ef3"
    );
    assert!(
        check
            .issues
            .iter()
            .any(|i| i.consumer == "f5984d44-92bd-51ac-8e8f-d911dd824ef3")
    );
    f.refuses("incompatible_replacement", || {
        m.replace(
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        )
    });
    f.refuses("incompatible_replacement", || {
        m.replace_resolve(
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        )
    });
    // Unrelated forced breakage remains diagnostic and must not block replacement.
    m.uninstall("f5984d44-92bd-51ac-8e8f-d911dd824ef3", &[], false)
        .unwrap();
    let lib = f.author("7bb3523b-dc85-5ff2-a33f-55b071c001cb");
    f.release(&lib, "7bb3523b-dc85-5ff2-a33f-55b071c001cb", "1.0.0", &[]);
    m.install(lib.to_str().unwrap()).unwrap();
    let app = f.author("fb3d0c94-3a3f-56ed-8fc9-4a77028e5a66");
    f.release(
        &app,
        "fb3d0c94-3a3f-56ed-8fc9-4a77028e5a66",
        "1.0.0",
        &[("7bb3523b-dc85-5ff2-a33f-55b071c001cb", "^1")],
    );
    m.install(app.to_str().unwrap()).unwrap();
    m.uninstall("7bb3523b-dc85-5ff2-a33f-55b071c001cb", &[], true)
        .unwrap();
    let check = m
        .replace_check(
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        )
        .unwrap();
    assert!(
        check
            .issues
            .iter()
            .any(|i| i.consumer == "fb3d0c94-3a3f-56ed-8fc9-4a77028e5a66"
                && i.dependency.id == "7bb3523b-dc85-5ff2-a33f-55b071c001cb")
    );
    assert!(check.missing.is_empty());
    assert!(check.resolution_error.is_none());
    let resolved = m
        .replace_resolve(
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        )
        .unwrap();
    assert!(resolved.installed.is_empty());
    assert!(
        resolved
            .check
            .issues
            .iter()
            .any(|i| i.consumer == "fb3d0c94-3a3f-56ed-8fc9-4a77028e5a66")
    );
    m.replace(
        "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
    )
    .unwrap();
    assert!(
        !m.state()
            .unwrap()
            .packages
            .contains_key("7bb3523b-dc85-5ff2-a33f-55b071c001cb")
    );
    // Reapplying a compatible fork also leaves the unrelated consumer broken.
    m.replace_resolve(
        "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        "db33e43e-e652-5767-8e21-27fb3cef9723",
    )
    .unwrap();
    m.replace(
        "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        "db33e43e-e652-5767-8e21-27fb3cef9723",
    )
    .unwrap();
    assert!(
        !m.state()
            .unwrap()
            .packages
            .contains_key("7bb3523b-dc85-5ff2-a33f-55b071c001cb")
    );
}

#[test]
fn replacement_check_reports_fork_dependency_version_mismatch() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "1.0.0", &[]);
    f.publish(&cat, "61cfe0c5-cc76-56a1-8a43-f9ca4faad534", "1.0.0", &[]);
    f.publish(&cat, "61cfe0c5-cc76-56a1-8a43-f9ca4faad534", "2.0.0", &[]);
    let m = f.open(&cat);
    m.install_from_catalog("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", None)
        .unwrap();
    let fork = f.author("db33e43e-e652-5767-8e21-27fb3cef9723");
    f.release_manifest(
        &fork,
        json!({"id":"db33e43e-e652-5767-8e21-27fb3cef9723", "name": "fixture", "version":"1.0.0",
        "replaces":{"id":"c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9","version":"1.0.0"},
        "dependencies":{"61cfe0c5-cc76-56a1-8a43-f9ca4faad534":"^2"}}),
    );
    m.install(fork.to_str().unwrap()).unwrap();
    m.uninstall("61cfe0c5-cc76-56a1-8a43-f9ca4faad534", &[], true)
        .unwrap();
    m.install_from_catalog("61cfe0c5-cc76-56a1-8a43-f9ca4faad534", Some("1.0.0"))
        .unwrap();
    let before = f.snapshot();
    let check = m
        .replace_check(
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            "db33e43e-e652-5767-8e21-27fb3cef9723",
        )
        .unwrap();
    assert_eq!(check.issues.len(), 1);
    let issue = &check.issues[0];
    assert_eq!(issue.consumer, "db33e43e-e652-5767-8e21-27fb3cef9723");
    assert_eq!(issue.dependency.requirement, "^2");
    assert_eq!(issue.dependency.version.as_deref(), Some("1.0.0"));
    assert!(check.resolution_error.is_some());
    f.refuses("dependency_conflict", || {
        m.replace(
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            "db33e43e-e652-5767-8e21-27fb3cef9723",
        )
    });
    f.refuses("dependency_conflict", || {
        m.replace_resolve(
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            "db33e43e-e652-5767-8e21-27fb3cef9723",
        )
    });
    assert_eq!(before, f.snapshot());
}

#[test]
fn replacement_resolve_refuses_disabled_transitive_dependencies_without_partial_installs() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "1.0.0", &[]);
    f.publish(&cat, "9c429448-e9d1-5a4d-93af-f240b2ace3bd", "1.0.0", &[]);
    f.publish(
        &cat,
        "61cfe0c5-cc76-56a1-8a43-f9ca4faad534",
        "1.0.0",
        &[("9c429448-e9d1-5a4d-93af-f240b2ace3bd", "^1")],
    );
    let m = f.open(&cat);
    m.install_from_catalog("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", None)
        .unwrap();
    let fork = f.author("db33e43e-e652-5767-8e21-27fb3cef9723");
    f.release_manifest(
        &fork,
        json!({"id":"db33e43e-e652-5767-8e21-27fb3cef9723", "name": "fixture", "version":"1.0.0",
        "replaces":{"id":"c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9","version":"1.0.0"},
        "dependencies":{"61cfe0c5-cc76-56a1-8a43-f9ca4faad534":"^1"}}),
    );
    m.install(fork.to_str().unwrap()).unwrap();
    m.uninstall("61cfe0c5-cc76-56a1-8a43-f9ca4faad534", &[], true)
        .unwrap();
    m.disable("9c429448-e9d1-5a4d-93af-f240b2ace3bd").unwrap();
    f.refuses("dependency_conflict", || {
        m.replace_resolve(
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            "db33e43e-e652-5767-8e21-27fb3cef9723",
        )
    });
    assert!(
        !m.state()
            .unwrap()
            .packages
            .contains_key("61cfe0c5-cc76-56a1-8a43-f9ca4faad534")
    );
    assert!(!m.state().unwrap().packages["9c429448-e9d1-5a4d-93af-f240b2ace3bd"].enabled);
    assert!(m.state().unwrap().replacements.is_empty());
}

#[test]
fn replacement_scope_keeps_unrelated_forced_breakage_and_checks_transitives() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "1.0.0", &[]);
    f.publish(&cat, "9c429448-e9d1-5a4d-93af-f240b2ace3bd", "1.0.0", &[]);
    f.publish(
        &cat,
        "c4304791-586c-54cb-a7c6-27bd09f28dbd",
        "1.0.0",
        &[("9c429448-e9d1-5a4d-93af-f240b2ace3bd", "^1")],
    );
    f.publish(&cat, "ae639f0b-9311-54e6-a84c-c1f53dd6be2e", "1.0.0", &[]);
    f.publish(
        &cat,
        "9692fc07-23c7-5f20-8779-f09374f1a571",
        "1.0.0",
        &[
            ("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "^1"),
            ("ae639f0b-9311-54e6-a84c-c1f53dd6be2e", "^1"),
        ],
    );
    let m = f.open(&cat);
    m.install_from_catalog("9692fc07-23c7-5f20-8779-f09374f1a571", None)
        .unwrap();
    let fork = f.author("db33e43e-e652-5767-8e21-27fb3cef9723");
    f.release_manifest(
        &fork,
        json!({"id":"db33e43e-e652-5767-8e21-27fb3cef9723", "name": "fixture", "version":"1.0.0",
        "replaces":{"id":"c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9","version":"1.0.0"},
        "dependencies":{"c4304791-586c-54cb-a7c6-27bd09f28dbd":"^1"}}),
    );
    m.install(fork.to_str().unwrap()).unwrap();
    m.uninstall("ae639f0b-9311-54e6-a84c-c1f53dd6be2e", &[], true)
        .unwrap();
    m.uninstall("9c429448-e9d1-5a4d-93af-f240b2ace3bd", &[], true)
        .unwrap();
    let check = m
        .replace_check(
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            "db33e43e-e652-5767-8e21-27fb3cef9723",
        )
        .unwrap();
    assert!(
        check
            .issues
            .iter()
            .any(|i| i.consumer == "9692fc07-23c7-5f20-8779-f09374f1a571"
                && i.dependency.id == "ae639f0b-9311-54e6-a84c-c1f53dd6be2e")
    );
    assert_eq!(
        check.missing,
        BTreeMap::from([(
            "9c429448-e9d1-5a4d-93af-f240b2ace3bd".into(),
            "1.0.0".into()
        )])
    );
    let error = f
        .refuses("dependency_conflict", || {
            m.replace(
                "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
                "db33e43e-e652-5767-8e21-27fb3cef9723",
            )
        })
        .to_string();
    assert!(
        error.contains(
            "c4304791-586c-54cb-a7c6-27bd09f28dbd needs 9c429448-e9d1-5a4d-93af-f240b2ace3bd ^1"
        ),
        "{error}"
    );
    assert!(
        error.contains("missing: 9c429448-e9d1-5a4d-93af-f240b2ace3bd 1.0.0"),
        "{error}"
    );
    assert!(
        !error.contains("ae639f0b-9311-54e6-a84c-c1f53dd6be2e"),
        "{error}"
    );
    let resolved = m
        .replace_resolve(
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            "db33e43e-e652-5767-8e21-27fb3cef9723",
        )
        .unwrap();
    assert_eq!(resolved.installed.len(), 1);
    assert!(
        !m.state()
            .unwrap()
            .packages
            .contains_key("ae639f0b-9311-54e6-a84c-c1f53dd6be2e")
    );
    m.replace(
        "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        "db33e43e-e652-5767-8e21-27fb3cef9723",
    )
    .unwrap();
    m.replace(
        "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
    )
    .unwrap();
    m.set_enabled_with("9c429448-e9d1-5a4d-93af-f240b2ace3bd", false, true)
        .unwrap();
    let check = m
        .replace_check(
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            "db33e43e-e652-5767-8e21-27fb3cef9723",
        )
        .unwrap();
    assert!(
        check
            .resolution_error
            .as_deref()
            .unwrap()
            .contains("9c429448-e9d1-5a4d-93af-f240b2ace3bd is disabled")
    );
    assert!(check.missing.is_empty());
    f.refuses("dependency_conflict", || {
        m.replace_resolve(
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            "db33e43e-e652-5767-8e21-27fb3cef9723",
        )
    });
    f.refuses("dependency_conflict", || {
        m.replace(
            "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
            "db33e43e-e652-5767-8e21-27fb3cef9723",
        )
    });
}
