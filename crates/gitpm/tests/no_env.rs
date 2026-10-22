//! The library takes all settings explicitly: GITPM_* (and legacy PM_*) variables must
//! have no effect.
//! Single test in its own binary, so changing the process environment is race-free.
use gitpm::{Manager, Options, check, scaffold};
use std::fs;

#[test]
fn garbage_pm_environment_is_ignored() {
    let dir = std::env::temp_dir().join(format!("pm-no-env-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let garbage = dir.join("must-not-be-used");
    // SAFETY: the only test in this binary; no other threads read the environment.
    unsafe {
        for key in [
            "PM_ROOT",
            "PM_CATALOG",
            "PM_REMOTE_CATALOG",
            "PM_SOURCES",
            "PM_CONFIG",
            "PM_PUBLIC_URL",
            "GITPM_ROOT",
            "GITPM_CATALOG",
            "GITPM_SOURCES",
        ] {
            std::env::set_var(key, &garbage);
        }
        std::env::set_var("PM_NAMESPACE", "garbage-namespace");
        std::env::set_var("GITPM_NAMESPACE", "garbage-namespace");
    }
    let author = scaffold::init(&dir.join("demo"), "explicit", "demo")
        .unwrap()
        .repo;
    let catalog_dir = dir.join("catalog");
    let catalog = catalog_dir.to_str().unwrap().to_owned();
    gitpm::catalog::register(&catalog, author.to_str().unwrap(), "HEAD").unwrap();
    let root = dir.join("workspace");
    let m = Manager::open_with(
        &root,
        Options {
            catalog: Some(catalog.clone()),
            custom_ssh: false,
        },
    )
    .unwrap();
    let installed = m.install_from_catalog("explicit/demo", None).unwrap();
    assert_eq!(installed.id, "explicit/demo");
    assert!(root.join("dist/explicit/demo/index.js").exists());
    assert_eq!(m.list().unwrap().len(), 1);
    let rows = m.check().unwrap();
    assert_eq!(rows[0].status, check::CheckStatus::UpToDate);
    assert_eq!(check::check(&root, Some(&catalog)).unwrap().rows.len(), 1);
    assert!(m.browse_catalog().unwrap().items.len() >= 2);
    drop(m);
    assert!(!garbage.exists());
    assert!(!dir.join("garbage-namespace").exists());
    if std::env::var_os("GITPM_KEEP_TEST_ARTIFACTS").is_none() {
        let _ = fs::remove_dir_all(&dir);
    }
}
