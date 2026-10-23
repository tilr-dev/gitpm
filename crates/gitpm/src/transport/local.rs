//! Exercise the typed local interface against real, isolated repositories.
use super::{HeadHistory, SystemTransport, Transport};
use crate::test_support::{TempDir, git};
use std::{fs, path::Path};

fn repository() -> TempDir {
    let root = TempDir::new("transport-local");
    git(root.path(), &["init", "-q", "-b", "main"]);
    fs::create_dir(root.join("package")).unwrap();
    fs::write(root.join("package/data"), "first\n").unwrap();
    fs::write(root.join("package/with space"), [0, 255, 1]).unwrap();
    git(root.path(), &["add", "."]);
    git(root.path(), &["commit", "-qm", "first"]);
    root
}

fn fixture(repo: &Path, args: &[&str]) -> String {
    git(repo, args).trim().to_owned()
}

#[test]
fn typed_local_operations_preserve_revisions_trees_and_errors() {
    let root = repository();
    let repo = root.path();
    let transport: &dyn Transport = &SystemTransport;
    let first = fixture(repo, &["rev-parse", "HEAD"]);
    git(repo, &["tag", "light"]);
    git(repo, &["tag", "-am", "annotated", "annotated"]);
    for reference in ["HEAD", "light", "annotated"] {
        assert_eq!(
            transport.resolve_commit(repo, reference, false).unwrap(),
            first
        );
    }
    assert_eq!(transport.object_id(repo, "HEAD", false).unwrap(), first);
    assert_eq!(
        transport.verified_object_id(repo, "HEAD", false).unwrap(),
        first
    );
    let tag_object = fixture(repo, &["rev-parse", "annotated"]);
    assert_ne!(tag_object, first);
    assert_eq!(
        transport.object_id(repo, "annotated", false).unwrap(),
        tag_object
    );
    assert_eq!(
        transport
            .show_file(repo, &first, "package/data", false)
            .unwrap(),
        "first"
    );
    let entries = transport.tree_entries(repo, &first, false).unwrap();
    assert!(entries.contains(&0));
    assert!(
        entries
            .windows(b"package/with space".len())
            .any(|w| w == b"package/with space")
    );
    assert_eq!(transport.tags(repo, false).unwrap(), "annotated\nlight");
    transport
        .verify_ref(repo, "refs/tags/light", false)
        .unwrap();
    assert_eq!(
        transport
            .verify_ref(repo, "refs/tags/missing", false)
            .unwrap_err()
            .kind(),
        "git"
    );
    assert_eq!(
        transport
            .resolve_commit(repo, "--invalid", false)
            .unwrap_err()
            .kind(),
        "git"
    );
    assert_eq!(
        transport
            .show_file(repo, &first, "missing", false)
            .unwrap_err()
            .kind(),
        "git"
    );
    fs::write(root.join("package/data"), "second").unwrap();
    git(repo, &["commit", "-qam", "second"]);
    transport
        .checkout_detached(repo, &first, false, false)
        .unwrap();
    assert_eq!(
        fs::read_to_string(root.join("package/data")).unwrap(),
        "first\n"
    );
    assert_eq!(fixture(repo, &["rev-parse", "HEAD"]), first);
}

#[test]
fn typed_status_preserves_ignored_untracked_and_path_filtering() {
    let root = repository();
    let repo = root.path();
    let transport: &dyn Transport = &SystemTransport;
    fs::write(root.join(".gitignore"), "ignored\n").unwrap();
    git(repo, &["add", ".gitignore"]);
    git(repo, &["commit", "-qm", "ignore"]);
    git(repo, &["config", "status.showUntrackedFiles", "no"]);
    fs::write(root.join("package/data"), "changed").unwrap();
    fs::write(root.join("package/untracked"), "new").unwrap();
    fs::write(root.join("ignored"), "ignored").unwrap();
    let status = transport.status(repo, false, &["package"], false).unwrap();
    assert!(
        status
            .split(|b| *b == 0)
            .any(|row| row == b" M package/data")
    );
    assert!(
        status
            .split(|b| *b == 0)
            .any(|row| row == b"?? package/untracked")
    );
    assert!(!status.windows(b"ignored".len()).any(|w| w == b"ignored"));
    let status = transport.status(repo, true, &[], false).unwrap();
    assert!(status.split(|b| *b == 0).any(|row| row == b"!! ignored"));
}

#[test]
fn typed_refs_and_history_preserve_reachability() {
    let root = repository();
    let repo = root.path();
    let transport: &dyn Transport = &SystemTransport;
    let first = fixture(repo, &["rev-parse", "HEAD"]);
    git(repo, &["update-ref", "refs/remotes/origin/main", &first]);
    git(
        repo,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ],
    );
    assert_eq!(
        transport.origin_default_branch(repo, false).unwrap(),
        "origin/main"
    );
    fs::write(root.join("package/data"), "second").unwrap();
    git(repo, &["commit", "-qam", "second"]);
    let second = fixture(repo, &["rev-parse", "HEAD"]);
    assert_eq!(
        transport.unpublished_count(repo, "main", false).unwrap(),
        "1"
    );
    assert_eq!(
        transport.refs(repo, "refs/heads/", false).unwrap(),
        "refs/heads/main"
    );
    let history = transport
        .head_history(
            repo,
            HeadHistory::Unpublished {
                exclude_tags: false,
            },
            false,
        )
        .unwrap();
    assert_eq!(history, format!("{second}\tsecond"));
    git(repo, &["tag", "published"]);
    assert!(
        transport
            .head_history(repo, HeadHistory::Unpublished { exclude_tags: true }, false)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        transport
            .tree_paths(repo, "HEAD", "package", false)
            .unwrap(),
        "package"
    );
    assert!(transport.stashes(repo, false).unwrap().is_empty());
    transport
        .checkout_detached(repo, &second, false, false)
        .unwrap();
    fs::write(root.join("package/data"), "third").unwrap();
    git(repo, &["commit", "-qam", "third"]);
    let third = fixture(repo, &["rev-parse", "HEAD"]);
    assert_eq!(
        transport
            .head_history(repo, HeadHistory::Unreferenced { keep: &[] }, false)
            .unwrap(),
        format!("{third}\tthird")
    );
    assert!(
        transport
            .head_history(repo, HeadHistory::Unreferenced { keep: &[&third] }, false)
            .unwrap()
            .is_empty()
    );
    fs::write(root.join("package/data"), "stashed").unwrap();
    git(repo, &["stash", "push", "-qm", "saved"]);
    assert!(!transport.stashes(repo, false).unwrap().is_empty());
}

#[test]
fn typed_authoring_tags_are_atomic_on_conflict() {
    let root = repository();
    let repo = root.path();
    let first = fixture(repo, &["rev-parse", "HEAD"]);
    git(repo, &["tag", "existing"]);
    fs::write(root.join("package/data"), "second\n").unwrap();
    git(repo, &["commit", "-qam", "second"]);
    let head = fixture(repo, &["rev-parse", "HEAD"]);
    let tags = vec!["new".to_owned(), "existing".to_owned()];
    let error = super::create_tags_at_head(repo, &tags, false).unwrap_err();
    assert_eq!(error.kind(), "git");
    assert_eq!(fixture(repo, &["tag", "--list"]), "existing");
    assert_eq!(fixture(repo, &["rev-parse", "existing"]), first);
    super::create_tags_at_head(repo, &["new".into(), "other".into()], false).unwrap();
    assert_eq!(fixture(repo, &["rev-parse", "new"]), head);
    assert_eq!(fixture(repo, &["rev-parse", "other"]), head);
}
