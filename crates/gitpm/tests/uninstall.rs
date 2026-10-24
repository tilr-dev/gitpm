//! Confirmation-by-plan uninstall: lost-work detection, refusal, confirmed delete, exact IDs.
mod common;
use common::clone_path;
use gitpm::{Error, LostWork, Manager, scaffold};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    dir: PathBuf,
    root: PathBuf,
}
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
impl Fixture {
    fn installed() -> (Self, Manager) {
        let dir = std::env::temp_dir().join(format!(
            "pm-uninstall-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        let author = scaffold::init(&dir.join("module"), "vendor", "module")
            .unwrap()
            .repo;
        let root = dir.join("workspace");
        let m = Manager::open(&root).unwrap();
        m.install(author.to_str().unwrap()).unwrap();
        let repo = clone_path(&root, &author);
        git(&repo, &["config", "user.name", "Test"]);
        git(&repo, &["config", "user.email", "test@example.invalid"]);
        (Self { dir, root }, m)
    }
    fn repo(&self) -> PathBuf {
        clone_path(&self.root, &self.dir.join("module"))
    }
    fn snapshot(&self) -> (Vec<u8>, String, String, Vec<u8>) {
        (
            fs::read(self.root.join("state.json")).unwrap(),
            git(&self.repo(), &["rev-parse", "HEAD"]),
            git(
                &self.repo(),
                &[
                    "status",
                    "--porcelain",
                    "--untracked-files=all",
                    "--ignored",
                ],
            ),
            fs::read(self.root.join("dist/vendor/module/index.js")).unwrap(),
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if std::env::var_os("GITPM_KEEP_TEST_ARTIFACTS").is_none() {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
}
fn kinds(items: &[LostWork]) -> BTreeSet<&'static str> {
    items
        .iter()
        .map(|item| match item {
            LostWork::Modified { .. } => "modified",
            LostWork::Untracked { .. } => "untracked",
            LostWork::Ignored { .. } => "ignored",
            LostWork::HeadMoved { .. } => "head_moved",
            LostWork::UnpublishedCommit { .. } => "unpublished_commit",
            LostWork::UnpublishedBranch { .. } => "unpublished_branch",
            LostWork::UnpublishedTag { .. } => "unpublished_tag",
            LostWork::Stash { .. } => "stash",
        })
        .collect()
}
/// Every kind of local work at once.
fn make_all_lost_work(f: &Fixture) {
    let repo = f.repo();
    fs::write(repo.join("stashed.txt"), "stashed").unwrap();
    git(
        &repo,
        &["stash", "push", "--include-untracked", "-m", "wip"],
    );
    fs::write(repo.join("local.txt"), "local commit").unwrap();
    git(&repo, &["add", "local.txt"]);
    git(&repo, &["commit", "-m", "local work"]);
    git(&repo, &["branch", "work"]);
    git(&repo, &["tag", "mine"]);
    fs::write(repo.join("dist/index.js"), "edited").unwrap();
    fs::write(repo.join("notes.txt"), "untracked").unwrap();
    fs::create_dir_all(repo.join("node_modules")).unwrap();
    fs::write(repo.join("node_modules/private.txt"), "ignored").unwrap();
}

#[test]
fn clean_clone_plan_is_empty_and_uninstall_needs_no_confirmation() {
    let (f, m) = Fixture::installed();
    let plan = m.uninstall_plan("vendor/module").unwrap();
    assert!(plan.lost_work.is_empty(), "{:?}", plan.lost_work);
    assert!(plan.registered);
    assert_eq!(plan.deletes.len(), 2);
    assert_eq!(Path::new(&plan.deletes[0]), f.repo());
    assert!(plan.deletes[1].ends_with("dist/vendor/module"));
    m.uninstall("vendor/module", &[], false).unwrap();
    assert!(!f.repo().exists());
    assert!(!f.root.join("dist/vendor/module").exists());
    assert!(m.state().unwrap().packages.is_empty());
}

#[test]
fn plan_lists_each_lost_work_type_including_stash_without_changes() {
    let (f, m) = Fixture::installed();
    make_all_lost_work(&f);
    let before = f.snapshot();
    let plan = m.uninstall_plan("vendor/module").unwrap();
    assert_eq!(
        kinds(&plan.lost_work),
        BTreeSet::from([
            "modified",
            "untracked",
            "ignored",
            "head_moved",
            "unpublished_commit",
            "unpublished_branch",
            "unpublished_tag",
            "stash",
        ])
    );
    let has = |wanted: &LostWork| plan.lost_work.contains(wanted);
    assert!(has(&LostWork::Modified {
        path: "dist/index.js".into()
    }));
    assert!(has(&LostWork::Untracked {
        path: "notes.txt".into()
    }));
    assert!(has(&LostWork::UnpublishedBranch {
        name: "work".into(),
        commits: 1
    }));
    assert!(has(&LostWork::UnpublishedTag {
        name: "mine".into()
    }));
    assert!(plan.lost_work.iter().any(
        |i| matches!(i, LostWork::UnpublishedCommit { summary, .. } if summary == "local work")
    ));
    assert!(
        plan.lost_work
            .iter()
            .any(|i| matches!(i, LostWork::Stash { entry } if entry.contains("wip")))
    );
    assert!(plan.lost_work.iter().all(|i| !i.describe().is_empty()));
    assert_eq!(f.snapshot(), before);
}

#[test]
fn uninstall_without_confirmation_refuses_and_changes_nothing() {
    let (f, m) = Fixture::installed();
    make_all_lost_work(&f);
    let before = f.snapshot();
    let error = m.uninstall("vendor/module", &[], false).unwrap_err();
    assert_eq!(error.kind(), "local_changes");
    let Error::LocalChanges { id, items, message } = &error else {
        panic!("{error:?}");
    };
    assert_eq!(id, "vendor/module");
    assert_eq!(kinds(items).len(), 8);
    assert!(message.contains("local changes"), "{message}");
    assert_eq!(f.snapshot(), before);
    assert!(f.repo().join("node_modules/private.txt").exists());
    assert_eq!(git(&f.repo(), &["stash", "list"]).lines().count(), 1);
}

#[test]
fn stash_alone_blocks_unconfirmed_uninstall() {
    let (f, m) = Fixture::installed();
    fs::write(f.repo().join("dist/index.js"), "stash me").unwrap();
    git(&f.repo(), &["stash", "push", "-m", "only stash"]);
    let plan = m.uninstall_plan("vendor/module").unwrap();
    assert_eq!(kinds(&plan.lost_work), BTreeSet::from(["stash"]));
    let error = m.uninstall("vendor/module", &[], false).unwrap_err();
    assert_eq!(error.kind(), "local_changes");
    assert!(error.to_string().contains("stash"), "{error}");
    assert!(f.repo().exists());
    assert!(m.state().unwrap().packages.contains_key("vendor/module"));
}

#[test]
fn local_commit_message_is_unchanged() {
    let (f, m) = Fixture::installed();
    fs::write(f.repo().join("local.txt"), "local").unwrap();
    git(&f.repo(), &["add", "local.txt"]);
    git(&f.repo(), &["commit", "-m", "local"]);
    let error = m.uninstall("vendor/module", &[], false).unwrap_err();
    assert_eq!(
        error.to_string(),
        "local commits detected; confirm the listed lost work to remove it"
    );
}

#[test]
fn confirmed_uninstall_deletes_lost_work() {
    let (f, m) = Fixture::installed();
    make_all_lost_work(&f);
    let plan = m.uninstall_plan("vendor/module").unwrap();
    m.uninstall("vendor/module", &plan.lost_work, false)
        .unwrap();
    assert!(!f.repo().exists());
    assert!(!f.root.join("dist/vendor/module").exists());
    assert!(m.state().unwrap().packages.is_empty());
}

/// The confirmation covers exactly the items the plan showed: work that appears after
/// the plan (while a prompt waits) refuses and nothing is deleted.
#[test]
fn confirmation_is_bound_to_the_planned_lost_work() {
    let (f, m) = Fixture::installed();
    fs::write(f.repo().join("seen.txt"), "seen").unwrap();
    let plan = m.uninstall_plan("vendor/module").unwrap();
    assert_eq!(
        plan.lost_work,
        [LostWork::Untracked {
            path: "seen.txt".into()
        }]
    );
    fs::write(f.repo().join("later.txt"), "later").unwrap();
    let before = f.snapshot();
    let error = m
        .uninstall("vendor/module", &plan.lost_work, false)
        .unwrap_err();
    let Error::LocalChanges { items, .. } = &error else {
        panic!("{error:?}");
    };
    assert_eq!(
        items,
        &[LostWork::Untracked {
            path: "later.txt".into()
        }]
    );
    assert_eq!(f.snapshot(), before);
    // Force ignores dependents, not unconfirmed work.
    let error = m
        .uninstall("vendor/module", &plan.lost_work, true)
        .unwrap_err();
    assert_eq!(error.kind(), "local_changes");
    assert_eq!(f.snapshot(), before);
    // Confirming what is there now deletes it.
    let plan = m.uninstall_plan("vendor/module").unwrap();
    m.uninstall("vendor/module", &plan.lost_work, false)
        .unwrap();
    assert!(!f.repo().exists());
}

#[test]
fn case_variant_id_never_touches_the_installed_plugin() {
    let (f, m) = Fixture::installed();
    let before = f.snapshot();
    for id in ["Vendor/Module", "vendor/MODULE", "VENDOR/module"] {
        let plan = m.uninstall_plan(id).unwrap_err();
        assert_eq!(plan.kind(), "repository_not_found", "{id}");
        let error = m.uninstall(id, &[], false).unwrap_err();
        assert_eq!(error.kind(), "repository_not_found", "{id}");
        assert_eq!(f.snapshot(), before);
        assert!(f.repo().join(".git").is_dir());
    }
    assert!(m.state().unwrap().packages.contains_key("vendor/module"));
}

#[test]
fn forgotten_repository_can_be_planned_and_removed() {
    let (f, m) = Fixture::installed();
    m.forget("vendor/module").unwrap();
    let plan = m.uninstall_plan("vendor/module").unwrap();
    assert!(!plan.registered);
    assert!(plan.lost_work.is_empty(), "{:?}", plan.lost_work);
    m.uninstall("vendor/module", &[], false).unwrap();
    assert!(!f.repo().exists());
}

#[test]
fn update_refusal_lists_dirty_files() {
    let (f, m) = Fixture::installed();
    fs::write(f.repo().join("notes.txt"), "keep").unwrap();
    let error = m.update("vendor/module").unwrap_err();
    let Error::LocalChanges { items, .. } = &error else {
        panic!("{error:?}");
    };
    assert_eq!(
        items,
        &vec![LostWork::Untracked {
            path: "notes.txt".into()
        }]
    );
}
