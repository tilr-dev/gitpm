//! Dependencies between plugins, dependent guards, global fork replacement and the
//! single-catalog rule, through the public API (ATQ UC-053..059, UC-066).
use gitpm::{Error, InstalledPlugin, Manager, Options, catalog, scaffold};
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
    fn open(&self, catalog: &str) -> Manager {
        Manager::open_with(
            self.path("workspace"),
            Options {
                catalog: Some(catalog.into()),
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

fn plugin(m: &Manager, id: &str) -> InstalledPlugin {
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
    f.publish(&cat, "app/c", "1.0.0", &[("lib/core", "1")]);
    cat
}

#[test]
fn catalog_install_resolves_dependencies_and_records_edges() {
    let f = Fixture::new();
    let cat = core_catalog(&f);
    let m = f.open(&cat);
    m.install_from_catalog("app/a", None).unwrap();
    assert_eq!(ids(&m), ["app/a", "lib/core"]);
    let core = plugin(&m, "lib/core");
    assert_eq!(
        core.current.version, "1.1.0",
        "highest version matching ^1.0"
    );
    assert_eq!(core.dependents, ["app/a"]);
    let state = m.state().unwrap();
    assert_eq!(state.plugins["app/a"].dependencies["lib/core"], "^1.0");
    let origin = catalog::canonical_location(&cat).unwrap();
    assert_eq!(
        state.plugins["lib/core"].catalog.as_deref(),
        Some(origin.as_str())
    );
    assert_eq!(
        provider(&m, "app/a", "lib/core"),
        (Some("lib/core".into()), true)
    );
    assert!(f.path("workspace/dist/lib/core/index.js").is_file());

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
    m.uninstall("app/a", false).unwrap();
    assert_eq!(ids(&m), ["app/c", "lib/core"]);
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
    assert_eq!(plugin(&m, "lib/core").current.version, "1.1.0");
    // A compatible release is fine.
    f.release(&core, "lib/core", "1.2.0", &[]);
    m.update_ref("lib/core", Some("lib/core@1.2.0")).unwrap();
    // An update that adds a dependency pulls it from the catalog.
    f.publish(&cat, "lib/extra", "1.0.0", &[]);
    let app = f.path("authors/app/a");
    f.release(
        &app,
        "app/a",
        "1.1.0",
        &[("lib/core", "^1.0"), ("lib/extra", "1")],
    );
    m.update("app/a").unwrap();
    assert!(ids(&m).contains(&"lib/extra".to_string()));
    // Rollback is offline and checked the same way.
    m.rollback("app/a").unwrap();
    assert_eq!(plugin(&m, "app/a").current.version, "1.0.0");
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
    assert_eq!(m.state().unwrap().plugins["app/a"].catalog, None);
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
        f.refuses("has_dependents", || m.uninstall("lib/core", true)),
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

    // Forced: removed; consumers report the unsatisfied dependency.
    m.uninstall_with("lib/core", false, true).unwrap();
    let (provider_id, satisfied) = provider(&m, "app/a", "lib/core");
    assert_eq!((provider_id, satisfied), (None, false));
    let status = plugin(&m, "app/a").dependencies[0].clone();
    assert_eq!(status.problem.as_deref(), Some("not installed"));
    // A broken consumer does not block unrelated operations.
    m.enable("other/x").unwrap();
    // Reinstall it; once its consumers are gone it is removable without a guard.
    m.install_from_catalog("lib/core", Some("1.1.0")).unwrap();
    m.uninstall("app/a", false).unwrap();
    m.uninstall("app/c", false).unwrap();
    m.uninstall("lib/core", false).unwrap();
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
    // The original stays installed and addressable; the unrelated plugin is untouched.
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

    // The active provider is guarded.
    f.refuses("has_dependents", || m.disable("mine/base"));
    f.refuses("has_dependents", || m.uninstall("mine/base", true));

    // Switch back: everyone uses the original; the fork keeps its work.
    m.switch_back("vendor/base", false).unwrap();
    for consumer in ["app/a", "app/b", "app/c"] {
        assert_eq!(
            provider(&m, consumer, "vendor/base"),
            (Some("vendor/base".into()), true)
        );
    }
    assert!(m.state().unwrap().replacements.is_empty());
    assert!(plugin(&m, "mine/base").dependents.is_empty());
    m.uninstall("mine/base", false).unwrap();
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
    m.uninstall("old/base", false).unwrap();
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

    // Switching back to an original that is not installed fails and keeps the fork.
    f.refuses("has_dependents", || m.uninstall("mine/base", false));
    m.uninstall("vendor/base", false).unwrap();
    f.refuses("not_installed", || m.switch_back("vendor/base", true));
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
    f.open(&a).install_from_catalog("x/p", None).unwrap();
    let m = f.open(&b);
    let error = f.refuses("catalog", || m.install_from_catalog("y/q", None));
    assert!(error.to_string().contains("mixing catalogs"), "{error}");
    // Same catalog through another spelling is the same catalog.
    drop(m);
    let m = f.open(&format!("{a}/catalog.json"));
    m.install_from_catalog("x/p", None)
        .expect_err("already installed");
}

#[test]
fn check_shows_fork_upstream_and_other_catalogs() {
    use gitpm::check::CheckStatus;
    let f = Fixture::new();
    let a = f.catalog("catalog-a");
    f.publish(&a, "vendor/p", "1.0.0", &[]);
    let m = f.open(&a);
    m.install_from_catalog("vendor/p", None).unwrap();
    let fork = m
        .fork_in_namespace("vendor/p", &f.path("forks/p"), "mine")
        .unwrap();
    scaffold::commit(&fork, "1.0.1").unwrap();
    m.install(fork.to_str().unwrap()).unwrap();
    let row = |m: &Manager, id: &str| m.check().unwrap().into_iter().find(|r| r.id == id).unwrap();
    let upstream = row(&m, "mine/p").upstream.unwrap();
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

    // Another catalog selected: plugins from catalog A are not compared against it.
    let b = f.catalog("catalog-b");
    let other = f.author("y/q");
    let tag = f.release(&other, "y/q", "1.0.0", &[]);
    catalog::register(&b, other.to_str().unwrap(), &tag).unwrap();
    drop(m);
    let m = f.open(&b);
    assert_eq!(row(&m, "vendor/p").status, CheckStatus::OtherCatalog);
}
