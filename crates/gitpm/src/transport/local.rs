//! Exercise the typed local interface against real, isolated repositories.
use super::{CommitScope, HeadHistory, SystemTransport, Transport};
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
    fs::write(root.join("package/data"), "stashed").unwrap();
    git(repo, &["stash", "push", "-qm", "saved"]);
    assert!(!transport.stashes(repo, false).unwrap().is_empty());
}

#[test]
fn typed_commit_paths_preserves_unrelated_changes() {
    assert_selected_commit_preserves_unrelated_changes(CommitScope::Paths(&["package/data"]));
}

#[test]
fn typed_commit_only_preserves_unrelated_changes() {
    assert_selected_commit_preserves_unrelated_changes(CommitScope::Only(&["package/data"]));
}

fn assert_selected_commit_preserves_unrelated_changes(scope: CommitScope<'_>) {
    let root = repository();
    let repo = root.path();
    fs::write(root.join("unrelated"), "original\n").unwrap();
    git(repo, &["add", "unrelated"]);
    git(repo, &["commit", "-qm", "unrelated baseline"]);
    fs::write(root.join("unrelated"), "staged\n").unwrap();
    git(repo, &["add", "unrelated"]);
    fs::write(root.join("unrelated"), "unstaged\n").unwrap();
    fs::write(root.join("package/data"), "selected staged\n").unwrap();
    git(repo, &["add", "package/data"]);
    // Both path scopes commit the selected working tree, even if its index differs.
    fs::write(root.join("package/data"), "selected working tree\n").unwrap();

    let transport: &dyn Transport = &SystemTransport;
    transport
        .commit(repo, "selected change", scope, false)
        .unwrap();

    assert_eq!(fixture(repo, &["show", "HEAD:unrelated"]), "original");
    assert_eq!(fixture(repo, &["show", ":unrelated"]), "staged");
    assert_eq!(
        fs::read_to_string(root.join("unrelated")).unwrap(),
        "unstaged\n"
    );
    assert_eq!(
        fixture(repo, &["diff", "--cached", "--name-only"]),
        "unrelated"
    );
    assert_eq!(fixture(repo, &["diff", "--name-only"]), "unrelated");
    assert_eq!(
        fixture(repo, &["show", "HEAD:package/data"]),
        "selected working tree"
    );
    assert_eq!(
        fixture(repo, &["show", ":package/data"]),
        "selected working tree"
    );
    assert_eq!(
        fixture(
            repo,
            &["diff-tree", "--no-commit-id", "--name-only", "-r", "HEAD"]
        ),
        "package/data"
    );
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

#[test]
fn bare_sources_have_independent_worktrees_and_report_stash_refs() {
    let source = repository();
    let storage = TempDir::new("bare-worktrees");
    let transport: &dyn Transport = &SystemTransport;
    transport
        .clone_bare(
            storage.path(),
            source.path().to_str().unwrap(),
            "source",
            false,
        )
        .unwrap();
    let bare = storage.join("source");
    assert_eq!(
        fixture(&bare, &["rev-parse", "--is-bare-repository"]),
        "true"
    );
    assert!(
        transport
            .refs(&bare, "refs/heads/", false)
            .unwrap()
            .is_empty()
    );
    assert!(transport.stashes(&bare, false).unwrap().is_empty());
    let commit = fixture(source.path(), &["rev-parse", "HEAD"]);
    transport
        .add_worktree(&bare, &storage.join("checkout"), &commit, false)
        .unwrap();
    fs::write(storage.join("checkout/package/data"), "saved work").unwrap();
    git(
        &storage.join("checkout"),
        &["stash", "push", "-qm", "saved"],
    );
    assert!(transport.stashes(&bare, false).unwrap().contains("saved"));
}
