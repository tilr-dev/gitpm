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
                "version": version,
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
        let tag = format!("{id}@{version}");
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

/// lib/core 1.0.0, 1.1.0, 2.0.0; app/a needs ^1.0; app/b needs ^2.0; app/c needs ^1.
fn core_catalog(f: &Fixture) -> String {
    let cat = f.catalog("catalog");
    for v in ["1.0.0", "1.1.0", "2.0.0"] {
        f.publish(&cat, "lib/core", v, &[]);
    }
    f.publish(&cat, "app/a", "1.0.0", &[("lib/core", "^1.0")]);
    f.publish(&cat, "app/b", "1.0.0", &[("lib/core", "^2.0")]);
    f.publish(&cat, "app/c", "1.0.0", &[("lib/core", "^1")]);
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
    let repo = fs::canonicalize(f.author("app/local")).unwrap();
    let tag = f.release(&repo, "app/local", "1.0.0", &[]);
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
    assert_eq!(release.manifest.id, "app/local");
    let sources = f.path("sources.txt");
    fs::write(&sources, source).unwrap();
    with_transient_server_info(&repo, || {
        catalog::update_sources(&cat, &sources, &mut |_| {})
    })
    .unwrap();
    let manager = f.open(&cat);
    let installed =
        with_transient_server_info(&repo, || manager.install_from_catalog("app/local", None))
            .unwrap();
    assert_eq!(installed.id, "app/local");
    let clone = f.path("workspace/packages/app/local");
    assert_eq!(git(&clone, &["remote", "get-url", "origin"]), source);
    assert!(!clone.join(".git/objects/info/packs_controlled").exists());
    assert!(!clone.join(".git/objects/info/alternates").exists());
    fs::remove_dir_all(repo).unwrap();
    assert_eq!(
        git(&clone, &["show", "HEAD:dist/index.js"]),
        "app/local 1.0.0"
    );
}

#[test]
fn catalog_install_resolves_dependencies_and_records_edges() {
    // An unverified catalog serves discovery only: nothing records it.
    let f = Fixture::new();
    let cat = core_catalog(&f);
    let m = f.open(&cat);
    m.install_from_catalog("app/a", None).unwrap();
    let state = m.state().unwrap();
    assert!(state.packages["app/a"].catalog.is_none());
    assert!(state.packages["lib/core"].catalog.is_none());
    drop((m, f));

    let f = Fixture::new();
    let cat = core_catalog(&f);
    f.verify(&cat);
    let m = f.open(&cat);
    let installed = m.install_from_catalog("app/a", None).unwrap();
    assert_eq!(ids(&m), ["app/a", "lib/core"]);
    // The pulled dependency is enabled and deployed in the same transaction.
    assert!(installed.enabled);
    let pulled: Vec<_> = installed
        .dependencies
        .iter()
        .map(|d| (d.id.as_str(), d.current.version.as_str(), d.enabled))
        .collect();
    assert_eq!(pulled, [("lib/core", "1.1.0", true)]);
    assert_eq!(installed.dist.deployed, ["app/a", "lib/core"]);
    let core = plugin(&m, "lib/core");
    assert_eq!(
        core.current.as_ref().unwrap().version,
        "1.1.0",
        "highest version matching ^1.0"
    );
    assert_eq!(core.dependents, ["app/a"]);
    let state = m.state().unwrap();
    assert_eq!(state.packages["app/a"].dependencies["lib/core"], "^1.0");
    let origin = catalog::canonical_location(&cat).unwrap();
    assert_eq!(
        state.packages["lib/core"].catalog.as_deref(),
        Some(origin.as_str())
    );
    assert_eq!(
        provider(&m, "app/a", "lib/core"),
        (Some("lib/core".into()), true)
    );
    assert!(f.path("workspace/dist/lib/core/index.js").is_file());
    assert!(core.enabled);

    // Missing dependency: refused with its name, nothing changes.
    f.publish(&cat, "app/lonely", "1.0.0", &[("lib/missing", "^1")]);
    let error = f.refuses("dependency_conflict", || {
        m.install_from_catalog("app/lonely", None)
    });
    assert!(error.to_string().contains("lib/missing"), "{error}");

    // Conflict with an installed consumer: both are named, nothing changes.
    let error = f.refuses("dependency_conflict", || {
        m.install_from_catalog("app/b", None)
    });
    let text = error.to_string();
    assert!(text.contains("app/a") && text.contains("app/b"), "{text}");

    // Shared dependency: one copy; uninstalling one consumer keeps it.
    m.install_from_catalog("app/c", None).unwrap();
    assert_eq!(ids(&m), ["app/a", "app/c", "lib/core"]);
    assert_eq!(plugin(&m, "lib/core").dependents, ["app/a", "app/c"]);
    m.uninstall("app/a", &[], false).unwrap();
    assert_eq!(ids(&m), ["app/c", "lib/core"]);
}

/// A disabled package satisfies no dependency: install, enable and single update of an
/// enabled consumer refuse it, `update all`, `list` and `check` report it unsatisfied;
/// only a download takes it as given.
#[test]
fn disabled_packages_satisfy_no_dependency() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "lib/core", "1.2.0", &[]);
    f.publish(&cat, "app/a", "1.0.0", &[("lib/core", "^1")]);
    let m = f.open(&cat);
    m.install_from_catalog("lib/core", None).unwrap();
    m.disable("lib/core").unwrap();
    // Install next to a disabled dependency: refused, naming it; nothing changes.
    let error = f.refuses("dependency_conflict", || {
        m.install_from_catalog("app/a", None)
    });
    assert!(
        error
            .to_string()
            .contains("lib/core is disabled; enable it"),
        "{error}"
    );
    let app_repo = f.path("authors/app/a");
    let error = f.refuses("dependency_conflict", || {
        m.install(app_repo.to_str().unwrap())
    });
    assert!(
        error
            .to_string()
            .contains("lib/core is disabled; enable it"),
        "{error}"
    );
    // A download takes the disabled version as given and enables nothing.
    let downloaded = m.download_from_catalog("app/a", None).unwrap();
    assert!(!downloaded.enabled && downloaded.dependencies.is_empty());
    assert!(!f.path("workspace/dist/app/a").exists());
    // list and check report the dependency unsatisfied.
    assert_eq!(
        provider(&m, "app/a", "lib/core"),
        (Some("lib/core".into()), false)
    );
    let row = plugin(&m, "app/a").dependencies[0].clone();
    assert_eq!(row.problem.as_deref(), Some("lib/core is disabled"));
    let report = gitpm::check::check(&f.path("workspace"), Some(&cat)).unwrap();
    assert_eq!(report.unsatisfied.len(), 1);
    assert_eq!(
        (
            report.unsatisfied[0].package.as_str(),
            report.unsatisfied[0].dependency.as_str(),
            report.unsatisfied[0].problem.as_deref()
        ),
        ("app/a", "lib/core", Some("lib/core is disabled"))
    );
    // enable refuses; enabling the dependency first works.
    let error = f.refuses("dependency_conflict", || m.enable("app/a"));
    assert!(
        error.to_string().contains("lib/core is disabled"),
        "{error}"
    );
    m.enable("lib/core").unwrap();
    m.enable("app/a").unwrap();
    assert!(f.path("workspace/dist/app/a/index.js").is_file());

    // Single update of an enabled consumer whose dependency was disabled by force.
    m.set_enabled_with("lib/core", false, true).unwrap();
    f.publish(&cat, "app/a", "1.1.0", &[("lib/core", "^1")]);
    let error = f.refuses("dependency_conflict", || m.update("app/a"));
    assert!(
        error
            .to_string()
            .contains("lib/core is disabled; enable it"),
        "{error}"
    );
    // update all counts it unsatisfied as well: the consumer is skipped, not updated.
    let plan = m.update_all().unwrap();
    assert!(plan.updates.is_empty());
    assert_eq!(
        plan.unchanged[0].reason,
        Some(gitpm::UnchangedReason::BrokenDependency {
            dependency: "lib/core".into(),
            problem: "lib/core is disabled".into(),
        })
    );
    // A disabled consumer may move: it does not become active.
    m.disable("app/a").unwrap();
    assert!(m.update("app/a").unwrap().changed);
    assert!(!plugin(&m, "app/a").enabled);
}

/// `install --force` enables the named package over a disabled dependency; an install
/// over a download from the same source enables it (with the same check).
#[test]
fn forced_install_and_install_over_a_download() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "lib/core", "1.2.0", &[]);
    f.publish(&cat, "app/a", "1.0.0", &[("lib/core", "^1")]);
    let m = f.open(&cat);
    m.download_from_catalog("app/a", None).unwrap();
    // Install over the download: refused while lib/core is disabled, nothing changes.
    let error = f.refuses("dependency_conflict", || {
        m.install_from_catalog("app/a", None)
    });
    assert!(
        error
            .to_string()
            .contains("lib/core is disabled; enable it"),
        "{error}"
    );
    // Forced: only app/a is enabled; it is no "already installed".
    let forced = m.install_from_catalog_with("app/a", None, true).unwrap();
    assert!(forced.enabled && !forced.already_installed, "{forced:?}");
    assert_eq!(forced.dist.deployed, ["app/a"]);
    assert!(!plugin(&m, "lib/core").enabled);
    assert_eq!(
        provider(&m, "app/a", "lib/core"),
        (Some("lib/core".into()), false)
    );
    // Enabled now: installing again is "already installed".
    assert!(
        m.install_from_catalog("app/a", None)
            .unwrap()
            .already_installed
    );
    // An install over a download with active dependencies enables it.
    f.publish(&cat, "app/b", "1.0.0", &[("lib/core", "^1")]);
    m.enable("lib/core").unwrap();
    m.download_from_catalog("app/b", None).unwrap();
    let enabled = m.install_from_catalog("app/b", None).unwrap();
    assert!(enabled.enabled && !enabled.already_installed);
    assert!(f.path("workspace/dist/app/b/index.js").is_file());

    // install --force of a new package next to a disabled dependency.
    f.publish(&cat, "app/c", "1.0.0", &[("lib/core", "^1")]);
    m.set_enabled_with("lib/core", false, true).unwrap();
    f.refuses("dependency_conflict", || {
        m.install_from_catalog("app/c", None)
    });
    let forced = m.install_from_catalog_with("app/c", None, true).unwrap();
    assert!(forced.enabled && f.path("workspace/dist/app/c/index.js").is_file());
    let app_repo = f.path("authors/app/c");
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
    f.publish(&cat, "lib/core", "1.2.0", &[]);
    f.publish(&cat, "app/a", "1.0.0", &[("lib/core", "^1")]);
    let m = f.open(&cat);
    m.install_from_catalog("app/a", None).unwrap();
    f.publish(&cat, "app/a", "1.1.0", &[("lib/core", "^1")]);
    m.update("app/a").unwrap();
    m.set_enabled_with("lib/core", false, true).unwrap();
    let error = f.refuses("dependency_conflict", || m.rollback("app/a"));
    assert!(
        error
            .to_string()
            .contains("lib/core is disabled; enable it"),
        "{error}"
    );
    m.forget("app/a").unwrap();
    let error = f.refuses("dependency_conflict", || m.restore("app/a"));
    assert!(
        error
            .to_string()
            .contains("lib/core is disabled; enable it"),
        "{error}"
    );
    m.enable("lib/core").unwrap();
    m.restore("app/a").unwrap();
    m.rollback("app/a").unwrap();
    assert_eq!(plugin(&m, "app/a").current.unwrap().version, "1.0.0");
}

/// Only direct dependencies must be active: app → lib (enabled) → base (disabled by
/// force) is no refusal for app in any operation; lib is reported broken.
#[test]
fn only_direct_dependencies_must_be_active() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "lib/base", "1.0.0", &[]);
    f.publish(&cat, "lib/mid", "1.0.0", &[("lib/base", "^1")]);
    f.publish(&cat, "app/a", "1.0.0", &[("lib/mid", "^1")]);
    let m = f.open(&cat);
    m.install_from_catalog("app/a", None).unwrap();
    m.set_enabled_with("lib/base", false, true).unwrap();
    assert_eq!(
        provider(&m, "lib/mid", "lib/base"),
        (Some("lib/base".into()), false)
    );
    m.disable("app/a").unwrap();
    m.enable("app/a").unwrap();
    f.publish(&cat, "app/a", "1.1.0", &[("lib/mid", "^1")]);
    m.update("app/a").unwrap();
    m.rollback("app/a").unwrap();
    m.forget("app/a").unwrap();
    m.restore("app/a").unwrap();
    m.uninstall("app/a", &[], false).unwrap();
    let installed = m.install_from_catalog("app/a", None).unwrap();
    assert!(installed.enabled);
    assert!(f.path("workspace/dist/app/a/index.js").is_file());
}

/// An update of a disabled package records the dependencies it pulls disabled.
#[test]
fn update_of_a_disabled_package_pulls_disabled_dependencies() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "app/a", "1.0.0", &[]);
    let m = f.open(&cat);
    m.install_from_catalog("app/a", None).unwrap();
    m.disable("app/a").unwrap();
    f.publish(&cat, "lib/extra", "1.0.0", &[]);
    f.publish(&cat, "app/a", "1.1.0", &[("lib/extra", "^1")]);
    let change = m.update("app/a").unwrap();
    assert!(
        change.changed && change.dist.deployed.is_empty(),
        "{change:?}"
    );
    assert!(!plugin(&m, "lib/extra").enabled);
    assert!(!f.path("workspace/dist/lib/extra").exists());
    // Enabled again: an update pulls enabled dependencies as before.
    m.enable("lib/extra").unwrap();
    m.enable("app/a").unwrap();
    f.publish(&cat, "lib/more", "1.0.0", &[]);
    f.publish(
        &cat,
        "app/a",
        "1.2.0",
        &[("lib/extra", "^1"), ("lib/more", "^1")],
    );
    m.update("app/a").unwrap();
    assert!(plugin(&m, "lib/more").enabled);
    assert!(f.path("workspace/dist/lib/more/index.js").is_file());
}

/// app/a 1.0.0 installed; its catalog release 1.1.0 pulls lib/extra, whose catalog
/// release points at a commit with a broken plugin.json.
fn broken_pulled_dependency() -> (Fixture, Manager) {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "app/a", "1.0.0", &[]);
    let m = f.open(&cat);
    m.install_from_catalog("app/a", None).unwrap();
    f.publish(&cat, "lib/extra", "1.0.0", &[]);
    f.publish(&cat, "app/a", "1.1.0", &[("lib/extra", "^1")]);
    // The catalog points lib/extra at a commit whose plugin.json is broken.
    let author = f.path("authors/lib/extra");
    fs::write(author.join("plugin.json"), "{ broken").unwrap();
    git(&author, &["commit", "-qam", "break"]);
    let broken = git(&author, &["rev-parse", "HEAD"]);
    let release = Path::new(&cat).join("namespaces/lib/extra/versions/1.0.0/release.json");
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
    let error = m.update("app/a").unwrap_err();
    assert!(
        matches!(&error, Error::InvalidManifest { id: Some(id), .. } if id == "lib/extra"),
        "{error:?}"
    );
    assert!(
        error
            .to_string()
            .starts_with("lib/extra: invalid plugin.json"),
        "{error}"
    );
}

/// A dependency an update pulls whose catalog release has an invalid version
/// requirement is named in the refusal, not the updated package.
#[test]
fn update_refusal_from_a_pulled_dependency_requirement_names_the_dependency() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "app/a", "1.0.0", &[]);
    let m = f.open(&cat);
    m.install_from_catalog("app/a", None).unwrap();
    f.publish(&cat, "lib/core", "1.0.0", &[]);
    f.publish(&cat, "lib/extra", "1.0.0", &[("lib/core", "^1")]);
    f.publish(&cat, "app/a", "1.1.0", &[("lib/extra", "^1")]);
    // register refuses such a manifest; a hand-edited catalog can still carry one.
    let release = Path::new(&cat).join("namespaces/lib/extra/versions/1.0.0/release.json");
    let mut value: Value = serde_json::from_slice(&fs::read(&release).unwrap()).unwrap();
    value["manifest"]["dependencies"] = json!({ "lib/core": "^1 || ^2" });
    fs::write(&release, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    for error in [
        m.update("app/a").unwrap_err(),
        m.update_all_plan().unwrap_err(),
        m.update_all().unwrap_err(),
    ] {
        assert!(
            matches!(&error, Error::InvalidManifest { id: Some(id), .. } if id == "lib/extra"),
            "{error:?}"
        );
        assert!(
            error
                .to_string()
                .starts_with("lib/extra: invalid version requirement"),
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
            matches!(&error, Error::InvalidManifest { id: Some(id), .. } if id == "lib/extra"),
            "{error:?}"
        );
        assert!(
            error
                .to_string()
                .starts_with("lib/extra: invalid plugin.json"),
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
    f.publish(&cat, "lib/core", "1.0.0", &[]);
    f.publish(&cat, "x/b", "1.0.0", &[]);
    let m = f.open(&cat);
    m.install_from_catalog("x/b", None).unwrap();
    f.release(
        &f.path("authors/x/b"),
        "x/b",
        "1.1.0",
        &[("lib/core", "^1 || ^2")],
    );
    for error in [
        m.update_all_plan().unwrap_err(),
        m.update_all().unwrap_err(),
    ] {
        assert!(
            matches!(&error, Error::InvalidManifest { id: Some(id), .. } if id == "x/b"),
            "{error:?}"
        );
        assert!(
            error
                .to_string()
                .starts_with("x/b: invalid version requirement"),
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
    f.publish(&cat, "x/b", "1.0.0", &[]);
    f.verify(&cat);
    let m = f.open(&cat);
    m.install_from_catalog("x/b", None).unwrap();
    f.publish(&cat, "x/b", "1.1.0", &[]);
    // register refuses such a manifest; a hand-edited catalog can still carry one.
    let release = Path::new(&cat).join("namespaces/x/b/versions/1.1.0/release.json");
    let mut value: Value = serde_json::from_slice(&fs::read(&release).unwrap()).unwrap();
    value["manifest"]["dependencies"] = json!({ "lib/core": "^1 || ^2" });
    fs::write(&release, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    for error in [
        m.update_all_plan().unwrap_err(),
        m.update_all().unwrap_err(),
    ] {
        assert!(
            matches!(&error, Error::InvalidManifest { id: Some(id), .. } if id == "x/b"),
            "{error:?}"
        );
        assert!(
            error
                .to_string()
                .starts_with("x/b: invalid version requirement"),
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
    f.publish(&cat, "app/a", "1.0.0", &[]);
    let repo = f.path("authors/app/a");
    let m = f.open(&cat);
    m.download(repo.to_str().unwrap(), None, gitpm::InstallTarget::Release)
        .unwrap();
    f.release(&repo, "app/a", "1.1.0", &[]);
    let result = m
        .install_package(
            repo.to_str().unwrap(),
            None,
            gitpm::InstallTarget::Ref("app/a@1.1.0".into()),
        )
        .unwrap();
    assert!(result.enabled && !result.already_installed);
    assert_eq!(result.current.version, "1.0.0");
    assert!(result.mode.is_release());
    assert_eq!(
        result.requested_mode,
        Some(gitpm::Mode::Pinned("app/a@1.1.0".into()))
    );
    assert_eq!(result.available.as_ref().unwrap().version, "1.1.0");
    assert!(
        result.warnings[0]
            .contains("recorded mode release, not the requested pinned at app/a@1.1.0"),
        "{:?}",
        result.warnings
    );
    assert!(f.path("workspace/dist/app/a/index.js").is_file());
}

#[test]
fn cycles_resolve_and_updates_keep_dependents_working() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "x/a", "1.0.0", &[("x/b", "^1")]);
    f.publish(&cat, "x/b", "1.0.0", &[("x/a", "^1")]);
    f.publish(&cat, "lib/core", "1.1.0", &[]);
    f.publish(&cat, "app/a", "1.0.0", &[("lib/core", "^1.0")]);
    let m = f.open(&cat);
    m.install_from_catalog("x/a", None).unwrap();
    assert_eq!(ids(&m), ["x/a", "x/b"]);

    m.install_from_catalog("app/a", None).unwrap();
    // lib/core 2.0.0 released upstream: app/a needs ^1.0, so the update is refused.
    let core = f.path("authors/lib/core");
    f.release(&core, "lib/core", "2.0.0", &[]);
    let error = f.refuses("dependency_conflict", || m.update("lib/core"));
    assert!(error.to_string().contains("app/a"), "{error}");
    assert_eq!(plugin(&m, "lib/core").current.unwrap().version, "1.1.0");
    // A compatible release is fine.
    f.release(&core, "lib/core", "1.2.0", &[]);
    m.update_to("lib/core", gitpm::Target::Ref("lib/core@1.2.0".into()))
        .unwrap();
    // An update that adds a dependency pulls it from the catalog.
    f.publish(&cat, "lib/extra", "1.0.0", &[]);
    let app = f.path("authors/app/a");
    f.release(
        &app,
        "app/a",
        "1.1.0",
        &[("lib/core", "^1.0"), ("lib/extra", "^1")],
    );
    m.update("app/a").unwrap();
    assert!(ids(&m).contains(&"lib/extra".to_string()));
    // Rollback is offline and checked the same way.
    m.rollback("app/a").unwrap();
    assert_eq!(plugin(&m, "app/a").current.unwrap().version, "1.0.0");
}

#[test]
fn direct_git_install_uses_installed_plugins_or_the_catalog() {
    let f = Fixture::new();
    let core = f.author("lib/core");
    f.release(&core, "lib/core", "1.0.0", &[]);
    let app = f.author("app/a");
    f.release(&app, "app/a", "1.0.0", &[("lib/core", "^1")]);
    let m = Manager::open(f.path("workspace")).unwrap();
    let error = f.refuses("dependency_conflict", || m.install(app.to_str().unwrap()));
    assert!(
        error.to_string().contains("no catalog configured"),
        "{error}"
    );
    m.install(core.to_str().unwrap()).unwrap();
    m.install(app.to_str().unwrap()).unwrap();
    assert_eq!(
        provider(&m, "app/a", "lib/core"),
        (Some("lib/core".into()), true)
    );
    assert_eq!(m.state().unwrap().packages["app/a"].catalog, None);
}

#[test]
fn dependents_guard_disable_forget_and_uninstall() {
    let f = Fixture::new();
    let cat = core_catalog(&f);
    f.publish(&cat, "other/x", "1.0.0", &[]);
    let m = f.open(&cat);
    m.install_from_catalog("app/a", None).unwrap();
    m.install_from_catalog("app/c", None).unwrap();
    m.install_from_catalog("other/x", None).unwrap();

    for error in [
        f.refuses("has_dependents", || m.uninstall("lib/core", &[], false)),
        f.refuses("has_dependents", || m.forget("lib/core")),
        f.refuses("has_dependents", || m.disable("lib/core")),
    ] {
        let Error::HasDependents { dependents, .. } = &error else {
            panic!("{error:?}")
        };
        assert_eq!(dependents, &["app/a", "app/c"]);
        assert!(error.to_string().contains("app/a, app/c"), "{error}");
    }
    let plan = m.uninstall_plan("lib/core").unwrap();
    assert_eq!(plan.dependents, ["app/a", "app/c"]);
    m.disable("other/x").unwrap();

    // One refusal tells everything: the dependents and the work uninstall would lose.
    fs::write(f.path("workspace/packages/lib/core/notes.txt"), "mine").unwrap();
    let error = f.refuses("has_dependents", || m.uninstall("lib/core", &[], false));
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
    let plan = m.uninstall_plan("lib/core").unwrap();
    assert_eq!(plan.refusal().unwrap().to_string(), error.to_string());

    // Forced: removed; consumers report the unsatisfied dependency.
    m.uninstall("lib/core", &plan.lost_work, true).unwrap();
    let (provider_id, satisfied) = provider(&m, "app/a", "lib/core");
    assert_eq!((provider_id, satisfied), (None, false));
    let status = plugin(&m, "app/a").dependencies[0].clone();
    assert_eq!(status.problem.as_deref(), Some("not installed"));
    // A broken consumer does not block unrelated operations.
    m.enable("other/x").unwrap();
    // Reinstall it; once its consumers are gone it is removable without a guard.
    m.install_from_catalog("lib/core", Some("1.1.0")).unwrap();
    m.uninstall("app/a", &[], false).unwrap();
    m.uninstall("app/c", &[], false).unwrap();
    m.uninstall("lib/core", &[], false).unwrap();
}

/// vendor/base 1.0.0; app/a and app/b need ^1.0 (UC-066 main scenario).
#[test]
fn fork_replacement_main_scenario() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "vendor/base", "1.0.0", &[]);
    f.publish(&cat, "app/a", "1.0.0", &[("vendor/base", "^1.0")]);
    f.publish(&cat, "app/b", "1.0.0", &[("vendor/base", "^1.0")]);
    f.publish(&cat, "app/c", "1.0.0", &[("vendor/base", "^1.0")]);
    f.publish(&cat, "other/x", "1.0.0", &[]);
    let m = f.open(&cat);
    m.install_from_catalog("app/a", None).unwrap();
    m.install_from_catalog("app/b", None).unwrap();
    m.install_from_catalog("other/x", None).unwrap();
    assert_eq!(ids(&m), ["app/a", "app/b", "other/x", "vendor/base"]);

    // Fork, edit, release 1.0.1; the fork declares what it replaces.
    let fork = m
        .fork_in_namespace("vendor/base", &f.path("forks/base"), "mine")
        .unwrap();
    let manifest: Value =
        serde_json::from_slice(&fs::read(fork.join("plugin.json")).unwrap()).unwrap();
    assert_eq!(
        manifest["replaces"],
        json!({"id": "vendor/base", "version": "1.0.0"})
    );
    fs::write(fork.join("dist/index.js"), "fork payload").unwrap();
    scaffold::commit(&fork, "1.0.1").unwrap();
    m.install(fork.to_str().unwrap()).unwrap();
    // Installing the fork (provenance) alone redirects nothing.
    assert_eq!(
        provider(&m, "app/a", "vendor/base").0.as_deref(),
        Some("vendor/base")
    );

    // Errors first: self and unknown, no change.
    f.refuses("invalid_argument", || {
        m.replace("vendor/base", "vendor/base", true)
    });
    f.refuses("not_installed", || {
        m.replace("vendor/base", "mine/none", true)
    });
    f.refuses("invalid_argument", || {
        m.replace("other/x", "mine/base", true)
    });

    let plan = m.replace_plan("vendor/base", "mine/base").unwrap();
    assert_eq!(plan.consumers, ["app/a", "app/b"]);
    assert!(plan.incompatible.is_empty());
    m.replace("vendor/base", "mine/base", false).unwrap();
    for consumer in ["app/a", "app/b"] {
        assert_eq!(
            provider(&m, consumer, "vendor/base"),
            (Some("mine/base".into()), true)
        );
    }
    assert_eq!(plugin(&m, "mine/base").replacing, ["vendor/base"]);
    assert_eq!(plugin(&m, "mine/base").dependents, ["app/a", "app/b"]);
    assert_eq!(
        fs::read_to_string(f.path("workspace/dist/mine/base/index.js")).unwrap(),
        "fork payload"
    );
    // The original stays installed and addressable; the unrelated package is untouched.
    assert!(ids(&m).contains(&"vendor/base".to_string()));
    assert!(plugin(&m, "other/x").dependencies.is_empty());
    // A new consumer resolves to the fork too, without fetching anything new.
    m.install_from_catalog("app/c", None).unwrap();
    assert_eq!(
        provider(&m, "app/c", "vendor/base").0.as_deref(),
        Some("mine/base")
    );
    // Cyclic replacement is refused.
    f.refuses("invalid_argument", || {
        m.replace("mine/base", "vendor/base", true)
    });

    // Reopen: the binding persists.
    drop(m);
    let m = f.open(&cat);
    assert_eq!(m.state().unwrap().replacements["vendor/base"], "mine/base");

    // Fork release 1.0.2: consumers follow it.
    fs::write(fork.join("dist/index.js"), "fork payload 2").unwrap();
    scaffold::commit(&fork, "1.0.2").unwrap();
    m.update("mine/base").unwrap();
    assert_eq!(
        provider(&m, "app/a", "vendor/base").0.as_deref(),
        Some("mine/base")
    );
    assert_eq!(
        fs::read_to_string(f.path("workspace/dist/mine/base/index.js")).unwrap(),
        "fork payload 2"
    );
    // A new original release does not switch consumers back.
    let original = f.path("authors/vendor/base");
    f.release(&original, "vendor/base", "1.1.0", &[]);
    m.update("vendor/base").unwrap();
    assert_eq!(
        provider(&m, "app/b", "vendor/base").0.as_deref(),
        Some("mine/base")
    );

    // The installed original would take every consumer over: nothing guards the fork.
    assert!(m.uninstall_plan("mine/base").unwrap().dependents.is_empty());

    // Switch back: everyone uses the original; the fork keeps its work.
    m.switch_back("vendor/base", false, false).unwrap();
    for consumer in ["app/a", "app/b", "app/c"] {
        assert_eq!(
            provider(&m, consumer, "vendor/base"),
            (Some("vendor/base".into()), true)
        );
    }
    assert!(m.state().unwrap().replacements.is_empty());
    assert!(plugin(&m, "mine/base").dependents.is_empty());
    m.uninstall("mine/base", &[], false).unwrap();
}

/// UC-054: compatibility by declared versions, confirmation, two forks, update rules.
#[test]
fn replacement_compatibility_needs_confirmation() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "vendor/base", "1.4.0", &[]);
    f.publish(&cat, "app/a", "1.0.0", &[("vendor/base", "^1.0")]);
    f.publish(&cat, "app/b", "1.0.0", &[("vendor/base", ">=1.3")]);
    f.publish(&cat, "lib/mid", "1.0.0", &[("vendor/base", "^1")]);
    f.publish(&cat, "app/c", "1.0.0", &[("lib/mid", "^1")]);
    let m = f.open(&cat);
    for id in ["app/a", "app/b", "app/c"] {
        m.install_from_catalog(id, None).unwrap();
    }
    let fork = |id: &str, declares: &str| {
        let repo = f.author(id);
        f.release_manifest(
            &repo,
            json!({"id": id, "version": "0.1.0", "replaces": {"id": "vendor/base", "version": declares}}),
        );
        m.install(repo.to_str().unwrap()).unwrap();
        repo
    };
    fork("mine/base", "1.4.0");
    let old = fork("old/base", "1.2.0");

    // Old fork declares 1.2.0: app/b (>=1.3) is not satisfied → confirmation needed.
    let plan = m.replace_plan("vendor/base", "old/base").unwrap();
    assert_eq!(plan.incompatible.len(), 1);
    assert_eq!(
        (
            plan.incompatible[0].id.as_str(),
            plan.incompatible[0].requirement.as_str()
        ),
        ("app/b", ">=1.3")
    );
    let error = f.refuses("incompatible_replacement", || {
        m.replace("vendor/base", "old/base", false)
    });
    assert!(
        error.to_string().contains("app/b needs vendor/base >=1.3"),
        "{error}"
    );
    assert!(!error.to_string().contains("app/a"), "{error}");
    // Confirmed: applied for every consumer, app/b now reports the mismatch.
    m.replace("vendor/base", "old/base", true).unwrap();
    assert_eq!(
        provider(&m, "app/b", "vendor/base"),
        (Some("old/base".into()), false)
    );
    assert_eq!(
        provider(&m, "lib/mid", "vendor/base"),
        (Some("old/base".into()), true)
    );

    // Second fork: one global binding is switched, never two providers.
    m.replace("vendor/base", "mine/base", false).unwrap();
    let state = m.state().unwrap();
    assert_eq!(state.replacements.len(), 1);
    assert_eq!(state.replacements["vendor/base"], "mine/base");
    assert!(plugin(&m, "old/base").replacing.is_empty());

    // An unused fork is removable; the active one cannot drop its declaration.
    m.uninstall("old/base", &[], false).unwrap();
    let _ = old;
    let mine = f.path("authors/mine/base");
    f.release_manifest(&mine, json!({"id": "mine/base", "version": "0.2.0"}));
    let error = f.refuses("version_conflict", || m.update("mine/base"));
    assert!(error.to_string().contains("no longer declares"), "{error}");
    // A fork release declaring an unacceptable version is refused too.
    f.release_manifest(
        &mine,
        json!({"id": "mine/base", "version": "0.3.0", "replaces": {"id": "vendor/base", "version": "2.0.0"}}),
    );
    f.refuses("dependency_conflict", || m.update("mine/base"));

    // The installed original would satisfy every consumer; once it is gone, the
    // consumers of the fork break and guard it.
    assert!(m.uninstall_plan("mine/base").unwrap().dependents.is_empty());
    m.uninstall("vendor/base", &[], false).unwrap();
    let error = f.refuses("has_dependents", || m.uninstall("mine/base", &[], false));
    let Error::HasDependents { dependents, .. } = &error else {
        panic!("{error:?}")
    };
    assert_eq!(dependents, &["app/a", "app/b", "lib/mid"]);
    assert_eq!(m.state().unwrap().replacements["vendor/base"], "mine/base");
}

#[test]
fn one_catalog_per_workspace() {
    let f = Fixture::new();
    let a = f.catalog("catalog-a");
    let b = f.catalog("catalog-b");
    f.publish(&a, "x/p", "1.0.0", &[]);
    let other = f.author("y/q");
    let tag = f.release(&other, "y/q", "1.0.0", &[]);
    catalog::register(&b, other.to_str().unwrap(), &tag).unwrap();
    f.verify(&a);
    f.verify(&b);
    f.open(&a).install_from_catalog("x/p", None).unwrap();
    let m = f.open(&b);
    let error = f.refuses("catalog", || m.install_from_catalog("y/q", None));
    assert!(
        error.to_string().contains("mixing verified catalogs"),
        "{error}"
    );
    // An unverified catalog is exempt: it records nothing.
    let c = f.catalog("catalog-c");
    let third = f.author("z/r");
    let tag = f.release(&third, "z/r", "1.0.0", &[]);
    catalog::register(&c, third.to_str().unwrap(), &tag).unwrap();
    drop(m);
    let m = f.open(&c);
    m.install_from_catalog("z/r", None).unwrap();
    let state = m.state().unwrap();
    assert!(state.packages["z/r"].catalog.is_none());
    assert_eq!(
        state.packages["x/p"].catalog.as_deref(),
        Some(catalog::canonical_location(&a).unwrap().as_str())
    );
    // Same catalog through another spelling is the same catalog.
    drop(m);
    let m = f.open(&format!("{a}/catalog.json"));
    assert!(
        m.install_from_catalog("x/p", None)
            .unwrap()
            .already_installed
    );
}

#[test]
fn check_shows_fork_upstream_and_other_catalogs() {
    use gitpm::check::{CheckSource, CheckStatus};
    let f = Fixture::new();
    let a = f.catalog("catalog-a");
    f.publish(&a, "vendor/p", "1.0.0", &[]);
    f.verify(&a);
    let m = f.open(&a);
    m.install_from_catalog("vendor/p", None).unwrap();
    let fork = m
        .fork_in_namespace("vendor/p", &f.path("forks/p"), "mine")
        .unwrap();
    scaffold::commit(&fork, "1.0.1").unwrap();
    m.install(fork.to_str().unwrap()).unwrap();
    let row = |m: &Manager, id: &str| m.check().unwrap().into_iter().find(|r| r.id == id).unwrap();
    let upstream = row(&m, "mine/p").upstream.unwrap();
    assert_eq!(upstream.source, Some(CheckSource::Catalog));
    assert_eq!(
        (
            upstream.id.as_str(),
            upstream.base.as_str(),
            upstream.latest.as_deref(),
            upstream.newer
        ),
        ("vendor/p", "1.0.0", Some("1.0.0"), false)
    );
    assert!(row(&m, "vendor/p").upstream.is_none());
    // The original releases 1.1.0: the fork shows it, without changing anything.
    f.publish(&a, "vendor/p", "1.1.0", &[]);
    let before = f.snapshot();
    let upstream = row(&m, "mine/p").upstream.unwrap();
    assert_eq!(
        (upstream.latest.as_deref(), upstream.newer),
        (Some("1.1.0"), true)
    );
    assert_eq!(row(&m, "vendor/p").status, CheckStatus::UpdateAvailable);
    assert!(f.snapshot() == before);

    // Another catalog selected: packages from catalog A are not compared against it.
    let b = f.catalog("catalog-b");
    let other = f.author("y/q");
    let tag = f.release(&other, "y/q", "1.0.0", &[]);
    catalog::register(&b, other.to_str().unwrap(), &tag).unwrap();
    drop(m);
    let m = f.open(&b);
    assert_eq!(row(&m, "vendor/p").status, CheckStatus::OtherCatalog);
    // Catalog B does not list the original: its upstream comes from forkOf.source.
    let upstream = row(&m, "mine/p").upstream.unwrap();
    assert_eq!(
        (upstream.latest.as_deref(), upstream.source),
        (Some("1.1.0"), Some(CheckSource::Origin))
    );

    // Without any catalog the protected original is compared with its verified catalog;
    // the fork's row and its upstream come from the origins.
    drop(m);
    let m = Manager::open_with(f.path("workspace"), Options::default()).unwrap();
    let original = row(&m, "vendor/p");
    assert_eq!(
        (original.status, original.source, original.latest.as_deref()),
        (
            CheckStatus::UpdateAvailable,
            CheckSource::Catalog,
            Some("1.1.0")
        )
    );
    let upstream = row(&m, "mine/p").upstream.unwrap();
    assert_eq!(
        (upstream.latest.as_deref(), upstream.newer, upstream.source),
        (Some("1.1.0"), true, Some(CheckSource::Origin))
    );
}

#[test]
fn update_all_coordinates_versions_and_adds_dependencies_atomically() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "lib/core", "1.0.0", &[]);
    f.publish(&cat, "app/a", "1.0.0", &[("lib/core", "^1")]);
    let m = f.open(&cat);
    m.install_from_catalog("app/a", None).unwrap();
    f.publish(&cat, "lib/core", "2.0.0", &[]);
    f.publish(&cat, "lib/extra", "1.0.0", &[]);
    f.publish(
        &cat,
        "app/a",
        "2.0.0",
        &[("lib/core", "^2"), ("lib/extra", "^1")],
    );
    // Neither individual update can cross the version boundary alone.
    f.refuses("dependency_conflict", || m.update("lib/core"));
    f.refuses("dependency_conflict", || m.update("app/a"));
    let before = f.snapshot();
    let plan = m.update_all_plan().unwrap();
    assert!(
        f.snapshot() == before,
        "planning must not change refs, files or state"
    );
    assert_eq!(plan.updates.len(), 2);
    // Both cross a major version, which is marked, not refused.
    assert!(plan.updates.iter().all(|c| c.major), "{plan:?}");
    assert_eq!(plan.installs[0].id, "lib/extra");
    assert!(plan.kept_back.is_empty());
    let applied = m.update_all().unwrap();
    assert_eq!(applied.dist.deployed, ["app/a", "lib/core", "lib/extra"]);
    // The plan is what was applied; only the applied one carries a dist report.
    assert_eq!(
        serde_json::to_value(plan).unwrap(),
        serde_json::to_value(gitpm::UpdateAllPlan {
            dist: Default::default(),
            ..applied
        })
        .unwrap()
    );
    for id in ["app/a", "lib/core"] {
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
    assert!(plugin(&m, "app/a").dependencies.iter().all(|d| d.satisfied));
    assert!(m.update_all().unwrap().updates.is_empty());
}

#[test]
fn update_all_backtracks_to_compatible_versions_and_keeps_current_fallback() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "lib/core", "1.0.0", &[]);
    f.publish(&cat, "app/a", "1.0.0", &[("lib/core", "^1")]);
    f.publish(&cat, "app/b", "1.0.0", &[("lib/core", "^1")]);
    let m = f.open(&cat);
    m.install_from_catalog("app/a", None).unwrap();
    m.install_from_catalog("app/b", None).unwrap();
    f.publish(&cat, "lib/core", "1.5.0", &[]);
    f.publish(&cat, "lib/core", "2.0.0", &[]);
    f.publish(&cat, "app/a", "1.5.0", &[("lib/core", "^1")]);
    f.publish(&cat, "app/a", "2.0.0", &[("lib/core", "^2")]);
    let plan = m.update_all().unwrap();
    assert_eq!(plugin(&m, "app/a").current.unwrap().version, "1.5.0");
    assert_eq!(plugin(&m, "lib/core").current.unwrap().version, "1.5.0");
    assert_eq!(plugin(&m, "app/b").current.unwrap().version, "1.0.0");
    assert_eq!(
        plan.kept_back
            .iter()
            .map(|p| (p.id.as_str(), &p.reason))
            .collect::<Vec<_>>(),
        [
            ("app/a", &gitpm::HeldBackReason::Constraints),
            ("lib/core", &gitpm::HeldBackReason::Constraints)
        ]
    );
    // An unavailable new dependency makes this release unusable, not the whole upgrade.
    f.publish(&cat, "app/b", "2.0.0", &[("lib/missing", "^1")]);
    let plan = m.update_all().unwrap();
    assert!(plan.updates.is_empty());
    assert_eq!(plan.kept_back.len(), 3);
}

#[test]
fn update_all_handles_cycles_without_a_catalog() {
    let f = Fixture::new();
    let a = f.author("x/a");
    let b = f.author("x/b");
    f.release(&a, "x/a", "1.0.0", &[]);
    f.release(&b, "x/b", "1.0.0", &[("x/a", "^1")]);
    let m = Manager::open(f.path("workspace")).unwrap();
    m.install(a.to_str().unwrap()).unwrap();
    m.install(b.to_str().unwrap()).unwrap();
    f.release(&a, "x/a", "2.0.0", &[("x/b", "^2")]);
    f.release(&b, "x/b", "2.0.0", &[("x/a", "^2")]);
    assert_eq!(m.update_all().unwrap().updates.len(), 2);
    assert!(plugin(&m, "x/a").dependencies[0].satisfied);
    assert!(plugin(&m, "x/b").dependencies[0].satisfied);
}

#[test]
fn update_all_preserves_local_work_and_never_partially_deploys() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    for id in ["x/a", "x/b"] {
        f.publish(&cat, id, "1.0.0", &[]);
    }
    let m = f.open(&cat);
    for id in ["x/a", "x/b"] {
        m.install_from_catalog(id, None).unwrap();
        f.publish(&cat, id, "2.0.0", &[]);
    }
    let local = f.path("workspace/packages/x/b/dist/local.js");
    fs::write(&local, "local work").unwrap();
    f.refuses("local_changes", || m.update_all());
    f.refuses("local_changes", || m.update_all_plan());
    fs::remove_file(local).unwrap();
    let repo = f.path("authors/x/b");
    // A valid manifest with an undeployable payload fails after x/a has been staged.
    fs::remove_dir_all(repo.join("dist")).unwrap();
    git(&repo, &["add", "--all"]);
    git(&repo, &["commit", "-qm", "remove dist"]);
    git(&repo, &["tag", "-f", "x/b@2.0.0"]);
    let error = f.refuses("invalid_manifest", || m.update_all());
    assert!(error.to_string().starts_with("x/b: "), "{error}");
    assert!(matches!(error, gitpm::Error::InvalidManifest { id: Some(id), .. } if id == "x/b"));
    assert_eq!(plugin(&m, "x/a").current.unwrap().version, "1.0.0");
}

#[test]
fn update_all_does_not_downgrade_and_warns_about_moved_and_retracted_releases() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "x/a", "2.0.0", &[]);
    let m = f.open(&cat);
    m.install_from_catalog("x/a", None).unwrap();
    let installed = plugin(&m, "x/a").current.unwrap().commit;
    let repo = f.path("authors/x/a");
    git(&repo, &["tag", "-d", "x/a@2.0.0"]);
    f.release(&repo, "x/a", "1.0.0", &[]);
    // Retracted: no downgrade, up to date with a warning.
    let plan = m.update_all().unwrap();
    assert!(plan.updates.is_empty());
    assert_eq!(
        plan.unchanged[0].warnings,
        ["installed release x/a@2.0.0 no longer exists on the origin"]
    );
    assert_eq!(plugin(&m, "x/a").current.unwrap().version, "2.0.0");
    // Retagged at another commit: taken as it is, the installed commit is kept.
    f.release(&repo, "x/a", "2.0.0", &[]);
    let plan = m.update_all().unwrap();
    assert!(plan.updates.is_empty());
    assert_eq!(plan.unchanged[0].warnings.len(), 1);
    assert!(
        plan.unchanged[0].warnings[0].starts_with("release tag x/a@2.0.0 now points to "),
        "{:?}",
        plan.unchanged[0].warnings
    );
    assert_eq!(plugin(&m, "x/a").current.unwrap().commit, installed);
}

#[test]
fn update_all_reports_unsatisfiable_state_and_preserves_disabled_flags() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "lib/core", "1.0.0", &[]);
    f.publish(&cat, "app/a", "1.0.0", &[("lib/core", "^1")]);
    let m = f.open(&cat);
    m.install_from_catalog("app/a", None).unwrap();
    m.set_enabled_with("lib/core", false, true).unwrap();
    // The consumer of a disabled dependency is broken: skipped, not a failure.
    f.publish(&cat, "lib/core", "1.1.0", &[]);
    let plan = m.update_all().unwrap();
    assert_eq!(plan.updates[0].id, "lib/core");
    let skipped = plan.unchanged.iter().find(|u| u.id == "app/a").unwrap();
    assert_eq!(
        skipped.reason,
        Some(gitpm::UnchangedReason::BrokenDependency {
            dependency: "lib/core".into(),
            problem: "lib/core is disabled".into(),
        })
    );
    assert!(!plugin(&m, "lib/core").enabled);
    m.enable("lib/core").unwrap();
    m.disable("app/a").unwrap();
    f.publish(&cat, "app/a", "2.0.0", &[("lib/core", "^1")]);
    m.update_all().unwrap();
    assert!(!plugin(&m, "app/a").enabled);
    // An inactive consumer takes a disabled dependency as given (as `update ID` does).
    m.set_enabled_with("lib/core", false, true).unwrap();
    f.publish(&cat, "app/a", "3.0.0", &[("lib/core", "^1")]);
    let plan = m.update_all().unwrap();
    assert_eq!(plan.updates[0].to.version, "3.0.0");
    assert!(plan.unchanged.iter().all(|u| u.reason.is_none()));
}

#[test]
fn update_all_resolves_replacement_versions_together_with_consumers() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "vendor/base", "1.0.0", &[]);
    f.publish(&cat, "app/a", "1.0.0", &[("vendor/base", "^1")]);
    let m = f.open(&cat);
    m.install_from_catalog("app/a", None).unwrap();
    let fork = f.author("mine/base");
    f.release_manifest(&fork, json!({"id":"mine/base", "version":"1.0.0", "replaces":{"id":"vendor/base", "version":"1.0.0"}}));
    m.install(fork.to_str().unwrap()).unwrap();
    m.replace("vendor/base", "mine/base", false).unwrap();
    f.release_manifest(&fork, json!({"id":"mine/base", "version":"2.0.0", "replaces":{"id":"vendor/base", "version":"2.0.0"}}));
    // A newer fork that drops its declaration is not an eligible replacement.
    f.release_manifest(&fork, json!({"id":"mine/base", "version":"3.0.0"}));
    f.publish(&cat, "app/a", "2.0.0", &[("vendor/base", "^2")]);
    let plan = m.update_all().unwrap();
    let held = plan.kept_back.iter().find(|h| h.id == "mine/base").unwrap();
    assert_eq!(
        (held.selected.as_str(), held.latest.as_str(), &held.reason),
        (
            "2.0.0",
            "3.0.0",
            &gitpm::HeldBackReason::NoLongerReplaces {
                original: "vendor/base".into()
            }
        )
    );
    assert_eq!(held.reason.to_string(), "no longer replaces vendor/base");
    assert_eq!(plugin(&m, "mine/base").current.unwrap().version, "2.0.0");
    assert_eq!(plugin(&m, "app/a").current.unwrap().version, "2.0.0");
    assert_eq!(plugin(&m, "vendor/base").current.unwrap().version, "1.0.0");
    assert_eq!(
        provider(&m, "app/a", "vendor/base"),
        (Some("mine/base".into()), true)
    );
}

/// The replace and switch-back lines are built from the plan alone.
#[test]
fn replacement_plan_carries_installed_version() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "vendor/base", "1.4.0", &[]);
    f.publish(&cat, "app/a", "1.0.0", &[("vendor/base", "^1.0")]);
    let m = f.open(&cat);
    m.install_from_catalog("app/a", None).unwrap();
    let repo = f.author("mine/base");
    f.release_manifest(
        &repo,
        json!({"id": "mine/base", "version": "0.3.0", "replaces": {"id": "vendor/base", "version": "1.4.0"}}),
    );
    m.install(repo.to_str().unwrap()).unwrap();
    let plan = m.replace_plan("vendor/base", "mine/base").unwrap();
    assert_eq!(
        (plan.version.as_str(), plan.installed_version.as_deref()),
        ("1.4.0", Some("0.3.0"))
    );
    assert!(plan.dist.deployed.is_empty() && plan.dist.removed.is_empty());
    let applied = m.replace("vendor/base", "mine/base", false).unwrap();
    assert_eq!(applied.installed_version.as_deref(), Some("0.3.0"));
    assert_eq!(applied.dist.removed, ["vendor/base"]);
    // Without consumers left, disabling the fork drops the binding and deploys the
    // original again; a later enable does not bring the binding back.
    m.uninstall("app/a", &[], true).unwrap();
    let dist = m.disable("mine/base").unwrap();
    assert_eq!(
        (dist.deployed.as_slice(), dist.removed.as_slice()),
        (
            ["vendor/base".to_owned()].as_slice(),
            ["mine/base".to_owned()].as_slice()
        )
    );
    assert!(m.state().unwrap().replacements.is_empty());
    m.enable("mine/base").unwrap();
    assert!(m.state().unwrap().replacements.is_empty());
    // Switching back: the plan names the fork and the original's installed version.
    m.replace("vendor/base", "mine/base", false).unwrap();
    let plan = m.switch_back_plan("vendor/base").unwrap();
    assert_eq!(plan.current.as_deref(), Some("mine/base"));
    assert_eq!(plan.installed_version.as_deref(), Some("1.4.0"));
    let back = m.switch_back("vendor/base", false, false).unwrap();
    assert_eq!(back.dist.deployed, ["vendor/base"]);
}

/// A replacing package is always enabled (disabling it drops the binding): a state that
/// binds an original to a disabled package was edited by hand and is refused.
#[test]
fn a_binding_to_a_disabled_fork_is_invalid_state() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "vendor/base", "1.0.0", &[]);
    let m = f.open(&cat);
    m.install_from_catalog("vendor/base", None).unwrap();
    let repo = f.author("mine/base");
    f.release_manifest(
        &repo,
        json!({"id": "mine/base", "version": "1.0.0", "replaces": {"id": "vendor/base", "version": "1.0.0"}}),
    );
    m.install(repo.to_str().unwrap()).unwrap();
    m.disable("mine/base").unwrap();
    let path = f.path("workspace/state.json");
    let mut state: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    state["replacements"] = json!({"vendor/base": "mine/base"});
    fs::write(&path, serde_json::to_vec_pretty(&state).unwrap()).unwrap();
    let error = m.state().unwrap_err();
    assert_eq!(error.kind(), "invalid_data");
    assert!(
        error
            .to_string()
            .contains("vendor/base by the disabled package mine/base"),
        "{error}"
    );
    assert_eq!(
        m.switch_back("vendor/base", true, true).unwrap_err().kind(),
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
    let a = f.author("app/a");
    f.release(&a, "app/a", "1.0.0", &[]);
    let b = f.author("app/b");
    f.release(&b, "app/b", "2.0.0", &[]);
    let m = Manager::open(f.path("workspace")).unwrap();
    m.install(a.to_str().unwrap()).unwrap();
    m.install(b.to_str().unwrap()).unwrap();
    f.release(&a, "app/a", "1.1.0", &[]);
    let plan = m.update_all_plan().unwrap();
    assert_eq!(plan.updates.len(), 1);
    assert_eq!(plan.updates[0].id, "app/a");
    assert!(plan.kept_back.is_empty() && plan.installs.is_empty());
    assert_eq!(
        plan.unchanged,
        [gitpm::Unchanged {
            id: "app/b".into(),
            version: "2.0.0".into(),
            reason: None,
            warnings: Vec::new(),
        }]
    );
    let applied = m.update_all().unwrap();
    assert_eq!(applied.unchanged, plan.unchanged);
    assert_eq!(applied.dist.deployed, ["app/a"]);
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
    f.publish(&cat, "lib/core", "1.0.0", &[]);
    f.publish(&cat, "app/a", "1.0.0", &[("lib/core", "^1")]);
    f.publish(&cat, "other/x", "1.0.0", &[]);
    let m = f.open(&cat);
    m.install_from_catalog("app/a", None).unwrap();
    m.install_from_catalog("other/x", None).unwrap();
    m.uninstall("lib/core", &[], true).unwrap();
    f.publish(&cat, "other/x", "1.1.0", &[]);
    f.publish(&cat, "app/a", "1.1.0", &[("lib/core", "^1")]);
    let broken = gitpm::Unchanged {
        id: "app/a".into(),
        version: "1.0.0".into(),
        reason: Some(gitpm::UnchangedReason::BrokenDependency {
            dependency: "lib/core".into(),
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
            [("other/x", other)]
        );
        assert!(plan.installs.is_empty() && plan.kept_back.is_empty());
        assert_eq!(plan.unchanged, std::slice::from_ref(&broken));
        assert_eq!(
            broken.reason.as_ref().unwrap().to_string(),
            "dependency lib/core is not installed"
        );
    };
    // With a catalog that has lib/core.
    assert_skipped(&m.update_all_plan().unwrap(), "1.1.0");
    assert_skipped(&m.update_all().unwrap(), "1.1.0");
    assert!(!ids(&m).contains(&"lib/core".to_string()));
    assert!(!f.path("workspace/dist/lib/core").exists());
    assert_eq!(provider(&m, "app/a", "lib/core"), (None, false));
    let report = gitpm::check::check(&f.path("workspace"), Some(&cat)).unwrap();
    assert!(
        report
            .unsatisfied
            .iter()
            .any(|u| u.package == "app/a" && u.dependency == "lib/core"),
        "{:?}",
        report.unsatisfied
    );
    // Without a catalog the run does not fail either.
    drop(m);
    let m = Manager::open_with(f.path("workspace"), Options::default()).unwrap();
    f.publish(&cat, "other/x", "1.2.0", &[]);
    assert_skipped(&m.update_all().unwrap(), "1.2.0");
    assert!(!ids(&m).contains(&"lib/core".to_string()));
    // A single update of the broken consumer still fails.
    f.refuses("dependency_conflict", || m.update("app/a"));
}

/// Without a catalog, a newer release that needs a package that is not installed is kept
/// back with that reason, not a failure.
#[test]
fn update_all_keeps_back_a_release_needing_a_missing_dependency_without_a_catalog() {
    let f = Fixture::new();
    let a = f.author("app/a");
    f.release(&a, "app/a", "1.0.0", &[]);
    let m = Manager::open(f.path("workspace")).unwrap();
    m.install(a.to_str().unwrap()).unwrap();
    f.release(&a, "app/a", "1.1.0", &[]);
    f.release(&a, "app/a", "2.0.0", &[("lib/new", "^1")]);
    let plan = m.update_all().unwrap();
    assert_eq!(plan.updates[0].to.version, "1.1.0");
    let held = &plan.kept_back[0];
    assert_eq!(
        (
            held.id.as_str(),
            held.selected.as_str(),
            held.latest.as_str()
        ),
        ("app/a", "1.1.0", "2.0.0")
    );
    assert_eq!(
        held.reason,
        gitpm::HeldBackReason::MissingDependency {
            dependency: "lib/new".into(),
            requirement: "^1".into(),
        }
    );
    assert_eq!(
        held.reason.to_string(),
        "needs lib/new ^1, which is not installed and no catalog is configured"
    );
}

/// Dependencies `update all` pulls for a disabled consumer are recorded disabled, and a
/// new dependency only a disabled consumer pulls takes disabled providers as given (as
/// `update ID` of a disabled package does).
#[test]
fn update_all_pulls_disabled_dependencies_for_a_disabled_consumer() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "lib/core", "1.0.0", &[]);
    f.publish(&cat, "app/a", "1.0.0", &[("lib/core", "^1")]);
    let m = f.open(&cat);
    m.install_from_catalog("app/a", None).unwrap();
    m.disable("app/a").unwrap();
    f.publish(&cat, "lib/extra", "1.0.0", &[]);
    f.publish(
        &cat,
        "app/a",
        "1.1.0",
        &[("lib/core", "^1"), ("lib/extra", "^1")],
    );
    let plan = m.update_all().unwrap();
    assert_eq!(plan.updates[0].to.version, "1.1.0");
    assert_eq!(plan.installs[0].id, "lib/extra");
    assert!(!plan.installs[0].enabled && !plugin(&m, "lib/extra").enabled);
    assert!(!f.path("workspace/dist/lib/extra").exists());
    // A new lib/mid needs the disabled lib/core: app/a still moves, lib/mid disabled.
    m.set_enabled_with("lib/core", false, true).unwrap();
    f.publish(&cat, "lib/mid", "1.0.0", &[("lib/core", "^1")]);
    f.publish(
        &cat,
        "app/a",
        "1.2.0",
        &[("lib/core", "^1"), ("lib/extra", "^1"), ("lib/mid", "^1")],
    );
    let plan = m.update_all().unwrap();
    assert_eq!(plan.updates[0].to.version, "1.2.0");
    assert!(plan.kept_back.is_empty(), "{:?}", plan.kept_back);
    assert_eq!(plan.installs[0].id, "lib/mid");
    assert!(!plugin(&m, "lib/mid").enabled);
    assert!(!plugin(&m, "app/a").enabled);
}

/// A new dependency an active consumer pulls is enabled, and is not satisfied by a
/// disabled provider.
#[test]
fn update_all_enables_dependencies_of_active_consumers_only() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "lib/core", "1.0.0", &[]);
    f.publish(&cat, "app/a", "1.0.0", &[]);
    f.publish(&cat, "app/b", "1.0.0", &[]);
    let m = f.open(&cat);
    m.install_from_catalog("app/a", None).unwrap();
    m.install_from_catalog("app/b", None).unwrap();
    m.install_from_catalog("lib/core", None).unwrap();
    m.disable("lib/core").unwrap();
    f.publish(&cat, "lib/mid", "1.0.0", &[("lib/core", "^1")]);
    f.publish(&cat, "lib/new", "1.0.0", &[]);
    f.publish(&cat, "app/a", "1.1.0", &[("lib/new", "^1")]);
    f.publish(&cat, "app/b", "1.1.0", &[("lib/mid", "^1")]);
    let plan = m.update_all().unwrap();
    assert!(plugin(&m, "lib/new").enabled);
    assert_eq!(plugin(&m, "app/a").current.unwrap().version, "1.1.0");
    // app/b is active: lib/mid would be active next to the disabled lib/core.
    assert_eq!(plugin(&m, "app/b").current.unwrap().version, "1.0.0");
    assert!(plan.kept_back.iter().any(|h| h.id == "app/b"));
    assert!(!ids(&m).contains(&"lib/mid".to_string()));
}

/// mine/base declares vendor/base 2.0.0 and replaces the installed vendor/base 1.4.0;
/// app/a (`>=1`) is satisfied by both, app/b (`^2`) only by the fork.
fn fork_over_installed_original(f: &Fixture) -> Manager {
    let cat = f.catalog("catalog");
    f.publish(&cat, "vendor/base", "1.4.0", &[]);
    f.publish(&cat, "app/a", "1.0.0", &[("vendor/base", ">=1")]);
    f.publish(&cat, "app/b", "1.0.0", &[("vendor/base", "^2")]);
    let m = f.open(&cat);
    m.install_from_catalog("app/a", None).unwrap();
    let fork = f.author("mine/base");
    f.release_manifest(
        &fork,
        json!({"id": "mine/base", "version": "0.1.0", "replaces": {"id": "vendor/base", "version": "2.0.0"}}),
    );
    m.install(fork.to_str().unwrap()).unwrap();
    m.replace("vendor/base", "mine/base", false).unwrap();
    m.install_from_catalog("app/b", None).unwrap();
    m
}

#[test]
fn removing_an_active_fork_guards_only_consumers_the_original_breaks() {
    let f = Fixture::new();
    let m = fork_over_installed_original(&f);
    // app/a is taken over by vendor/base 1.4.0; only app/b breaks.
    for error in [
        f.refuses("has_dependents", || m.disable("mine/base")),
        f.refuses("has_dependents", || m.forget("mine/base")),
        f.refuses("has_dependents", || m.uninstall("mine/base", &[], false)),
    ] {
        let Error::HasDependents {
            dependents,
            lost_work,
            ..
        } = &error
        else {
            panic!("{error:?}")
        };
        assert_eq!(dependents, &["app/b"]);
        assert!(lost_work.is_empty());
        assert!(!error.to_string().contains("app/a"), "{error}");
    }
    let plan = m.uninstall_plan("mine/base").unwrap();
    assert_eq!(plan.dependents, ["app/b"]);
    let refusal = plan.refusal().unwrap();
    assert_eq!(refusal.kind(), "has_dependents");
    assert_eq!(
        refusal.to_string(),
        "cannot uninstall mine/base: required by app/b; doing it anyway requires force"
    );

    // Without app/b nothing breaks: the binding is dropped and the original deployed.
    m.uninstall("app/b", &[], false).unwrap();
    assert!(m.uninstall_plan("mine/base").unwrap().refusal().is_none());
    let dist = m.disable("mine/base").unwrap();
    assert_eq!(dist.deployed, ["vendor/base"]);
    assert!(m.state().unwrap().replacements.is_empty());
    assert_eq!(
        provider(&m, "app/a", "vendor/base"),
        (Some("vendor/base".into()), true)
    );
    assert!(f.path("workspace/dist/vendor/base").is_dir());

    // A disabled original takes nothing over: the fork is guarded again ...
    m.enable("mine/base").unwrap();
    m.replace("vendor/base", "mine/base", false).unwrap();
    m.disable("vendor/base").unwrap();
    let error = f.refuses("has_dependents", || m.forget("mine/base"));
    let Error::HasDependents { dependents, .. } = &error else {
        panic!("{error:?}")
    };
    assert_eq!(dependents, &["app/a"]);
    // ... unless forced: the binding is dropped and app/a is reported broken.
    m.forget_with("mine/base", true).unwrap();
    assert!(m.state().unwrap().replacements.is_empty());
    assert_eq!(
        provider(&m, "app/a", "vendor/base"),
        (Some("vendor/base".into()), false)
    );
}

#[test]
fn dropping_a_binding_checks_the_original_s_dependencies() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "lib/x", "1.0.0", &[]);
    f.publish(&cat, "vendor/base", "1.0.0", &[("lib/x", "^1")]);
    f.publish(&cat, "app/a", "1.0.0", &[("vendor/base", "^1")]);
    let m = f.open(&cat);
    m.install_from_catalog("app/a", None).unwrap();
    let fork = f.author("mine/base");
    f.release_manifest(
        &fork,
        json!({"id": "mine/base", "version": "1.0.0", "replaces": {"id": "vendor/base", "version": "1.0.0"}}),
    );
    m.install(fork.to_str().unwrap()).unwrap();
    m.replace("vendor/base", "mine/base", false).unwrap();
    // vendor/base (replaced, but installed) still depends on lib/x.
    f.refuses("has_dependents", || m.disable("lib/x"));
    m.set_enabled_with("lib/x", false, true).unwrap();

    // Each way of reactivating vendor/base refuses its disabled dependency.
    for error in [
        f.refuses("dependency_conflict", || m.disable("mine/base")),
        f.refuses("dependency_conflict", || m.forget("mine/base")),
        f.refuses("dependency_conflict", || {
            m.uninstall("mine/base", &[], false)
        }),
        f.refuses("dependency_conflict", || {
            m.switch_back("vendor/base", true, true)
        }),
    ] {
        assert!(
            error.to_string().contains("lib/x is disabled; enable it"),
            "{error}"
        );
    }
    // The uninstall plan carries that refusal, so nothing asks about lost work first.
    let plan = m.uninstall_plan("mine/base").unwrap();
    assert!(plan.dependents.is_empty());
    let refusal = plan.refusal().unwrap();
    assert_eq!(refusal.kind(), "dependency_conflict");
    assert_eq!(
        refusal.to_string(),
        "cannot uninstall mine/base, which would reactivate vendor/base: lib/x is disabled; enable it; force does it anyway"
    );
    let error = m.disable("mine/base").unwrap_err();
    assert_eq!(
        error.to_string(),
        "cannot disable mine/base, which would reactivate vendor/base: lib/x is disabled; enable it; force does it anyway"
    );
    // Forced: the original is active again and reported broken.
    m.set_enabled_with("mine/base", false, true).unwrap();
    assert!(m.state().unwrap().replacements.is_empty());
    assert!(f.path("workspace/dist/vendor/base").is_dir());
    let status = plugin(&m, "vendor/base").dependencies[0].clone();
    assert_eq!(
        (status.satisfied, status.problem.as_deref()),
        (false, Some("lib/x is disabled"))
    );
}

#[test]
fn replace_requires_the_original_and_back_drops_an_absent_one() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "vendor/base", "1.0.0", &[]);
    f.publish(&cat, "app/a", "1.0.0", &[("vendor/base", "^1")]);
    let m = f.open(&cat);
    m.install_from_catalog("app/a", None).unwrap();
    let fork = f.author("mine/base");
    f.release_manifest(
        &fork,
        json!({"id": "mine/base", "version": "1.0.0", "replaces": {"id": "vendor/base", "version": "1.0.0"}}),
    );
    m.install(fork.to_str().unwrap()).unwrap();

    // replace --with needs the original installed.
    m.uninstall("vendor/base", &[], true).unwrap();
    let error = f.refuses("not_installed", || {
        m.replace("vendor/base", "mine/base", true)
    });
    assert_eq!(error.to_string(), "package vendor/base is not installed");
    f.refuses("not_installed", || {
        m.replace_plan("vendor/base", "mine/base")
    });

    // Replaced, then the original is uninstalled: the binding stays.
    m.install_from_catalog("vendor/base", None).unwrap();
    m.replace("vendor/base", "mine/base", false).unwrap();
    m.uninstall("vendor/base", &[], false).unwrap();
    assert_eq!(m.state().unwrap().replacements["vendor/base"], "mine/base");

    // --back with the original absent: the consumers would break.
    let plan = m.switch_back_plan("vendor/base").unwrap();
    assert_eq!(
        (
            plan.provider.as_str(),
            plan.version.as_str(),
            plan.installed_version.as_deref(),
            plan.current.as_deref()
        ),
        ("vendor/base", "", None, Some("mine/base"))
    );
    assert_eq!(plan.consumers, ["app/a"]);
    assert!(plan.incompatible.is_empty());
    let error = f.refuses("has_dependents", || {
        m.switch_back("vendor/base", false, false)
    });
    let Error::HasDependents { dependents, .. } = &error else {
        panic!("{error:?}")
    };
    assert_eq!(dependents, &["app/a"]);
    // Confirmed: only the binding is dropped; app/a is reported broken.
    let back = m.switch_back("vendor/base", true, true).unwrap();
    assert_eq!(back.installed_version, None);
    assert!(m.state().unwrap().replacements.is_empty());
    assert_eq!(provider(&m, "app/a", "vendor/base"), (None, false));
    assert!(ids(&m).contains(&"mine/base".to_owned()));
}

/// `replace --back` follows the one rule of removals: an installed but disabled
/// original serves no consumer, so they guard the binding unless forced.
#[test]
fn switch_back_to_a_disabled_original_is_guarded() {
    let f = Fixture::new();
    let m = fork_over_installed_original(&f);
    m.uninstall("app/b", &[], false).unwrap();
    m.disable("vendor/base").unwrap();
    // A confirmation covers incompatible versions, not a disabled original.
    let error = f.refuses("has_dependents", || {
        m.switch_back("vendor/base", true, false)
    });
    let Error::HasDependents { dependents, .. } = &error else {
        panic!("{error:?}")
    };
    assert_eq!(dependents, &["app/a"]);
    assert_eq!(
        error.to_string(),
        "cannot switch vendor/base back: it would leave app/a without a working vendor/base; doing it anyway requires force"
    );
    m.switch_back("vendor/base", false, true).unwrap();
    assert!(m.state().unwrap().replacements.is_empty());
    assert_eq!(
        provider(&m, "app/a", "vendor/base"),
        (Some("vendor/base".into()), false)
    );
}

#[test]
fn dependency_ranges_pick_pre_releases_only_when_named() {
    let f = Fixture::new();
    let cat = f.catalog("catalog");
    f.publish(&cat, "lib/core", "1.2.0", &[]);
    f.publish(&cat, "lib/core", "1.3.0-beta.1", &[]);
    f.publish(&cat, "app/a", "1.0.0", &[("lib/core", "^1.2.0")]);
    f.publish(&cat, "app/b", "1.0.0", &[("lib/core", "^1.3.0-beta.1")]);
    f.publish(&cat, "app/c", "1.0.0", &[("lib/core", "1.2.0")]);
    let m = f.open(&cat);
    // A plain range never takes the newer pre-release.
    m.install_from_catalog("app/a", None).unwrap();
    assert_eq!(plugin(&m, "lib/core").current.unwrap().version, "1.2.0");
    // The bare version is exact and satisfied.
    m.install_from_catalog("app/c", None).unwrap();
    // A range naming the pre-release needs it; 1.2.0 is installed and fixed.
    let error = f.refuses("dependency_conflict", || {
        m.install_from_catalog("app/b", None)
    });
    assert!(error.to_string().contains("lib/core"), "{error}");
    // update all keeps lib/core at its release: no consumer opts into the beta.
    let plan = m.update_all_plan().unwrap();
    assert!(plan.updates.is_empty(), "{plan:?}");
    // In another workspace the named pre-release is pulled for app/b.
    let other = Manager::open_with(
        f.path("other"),
        Options {
            catalog: Some(cat.clone()),
            custom_ssh: false,
        },
    )
    .unwrap();
    other.install_from_catalog("app/b", None).unwrap();
    assert_eq!(
        other.state().unwrap().packages["lib/core"].current.version,
        "1.3.0-beta.1"
    );
}
