//! Public FFI parity for live links without parsing command output.
use gitpm_ffi::{
    ManagerOptions, Mode, PackageManager, PmError, Target, UnchangedReason, check, read_state,
};
use std::{fs, path::PathBuf};
struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn link_records_discovery_results_and_removal_cross_ffi() {
    let f = Fixture(std::env::temp_dir().join(format!("gitpm-ffi-link-{}", std::process::id())));
    fs::create_dir(&f.0).unwrap();
    let package = f.0.join("author");
    fs::create_dir_all(package.join("output")).unwrap();
    fs::write(
        package.join("plugin.json"),
        r#"{"id":"acme/ffi","version":"1.0.0","dist":"output","title":"Live title"}"#,
    )
    .unwrap();
    fs::write(package.join("output/live.txt"), "first").unwrap();
    let root = f.0.join("workspace").to_string_lossy().into_owned();
    let manager = PackageManager::open(root.clone(), ManagerOptions::default()).unwrap();
    let result = manager.link(package.to_string_lossy().into()).unwrap();
    let Mode::Link { descriptor } = result.mode else {
        panic!("not a link")
    };
    assert_eq!(
        descriptor.target,
        fs::canonicalize(package.join("output"))
            .unwrap()
            .to_string_lossy()
    );
    let state = read_state(root.clone()).unwrap();
    let record = &state.packages["acme/ffi"];
    assert!(record.current.is_none() && record.previous.is_none() && record.clone.is_none());
    assert_eq!(record.version.as_deref(), Some("1.0.0"));
    assert!(matches!(state.resolved[0].mode, Mode::Link { .. }));
    assert_eq!(state.resolved[0].title.as_deref(), Some("Live title"));
    let row = &check(root, None, false).unwrap().rows[0];
    assert!(matches!(row.mode, Mode::Link { .. }));
    assert!(row.problem.is_some());
    let result = manager
        .update("acme/ffi".into(), Target::Release, false)
        .unwrap();
    assert!(!result.changed);
    let Some(UnchangedReason::DevLinked { availability, .. }) = result.unchanged_reason else {
        panic!("no link disposition")
    };
    assert!(availability.problem.is_some());
    assert!(matches!(
        manager.update_all_plan().unwrap().unchanged[0].reason,
        Some(UnchangedReason::DevLinked { .. })
    ));
    assert!(matches!(
        manager.update_all().unwrap().unchanged[0].reason,
        Some(UnchangedReason::DevLinked { .. })
    ));
    assert!(matches!(
        manager.rollback("acme/ffi".into()),
        Err(PmError::NoPreviousVersion { .. })
    ));
    manager.disable("acme/ffi".into(), false).unwrap();
    manager.enable("acme/ffi".into(), false).unwrap();
    manager.forget("acme/ffi".into(), false).unwrap();
    let restored = manager.restore("acme/ffi".into()).unwrap();
    assert!(matches!(restored.mode, Mode::Link { .. }));
    let plan = manager.uninstall_plan("acme/ffi".into()).unwrap();
    assert!(plan.lost_work.is_empty());
    assert!(
        plan.deletes
            .iter()
            .all(|p| !p.starts_with(package.to_str().unwrap()))
    );
    manager.uninstall("acme/ffi".into(), vec![], true).unwrap();
    assert_eq!(
        fs::read_to_string(package.join("output/live.txt")).unwrap(),
        "first"
    );
}
