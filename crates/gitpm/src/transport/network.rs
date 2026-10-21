use super::{Fetch, SystemTransport, Transport};
use crate::test_support::{TempDir, git as fixture_git};
use std::{fs, path::Path};
fn git(repo: &Path, args: &[&str]) -> String {
    fixture_git(repo, args).trim().to_owned()
}
#[test]
fn typed_network_operations_preserve_refs_and_pruning() {
    let root = TempDir::new("transport-network");
    let origin = root.join("origin");
    fs::create_dir(&origin).unwrap();
    git(&origin, &["init", "-q", "-b", "main"]);
    fs::write(origin.join("data"), "one").unwrap();
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-qm", "first"]);
    git(&origin, &["tag", "v1"]);
    git(&origin, &["branch", "other"]);
    let transport: &dyn Transport = &SystemTransport;
    let source = origin.to_str().unwrap();
    transport
        .clone_repo(root.path(), source, "clone", false)
        .unwrap();
    let cloned = root.join("clone");
    git(&origin, &["symbolic-ref", "HEAD", "refs/heads/other"]);
    transport.set_remote_head(&cloned, "origin", false).unwrap();
    assert_eq!(
        git(&cloned, &["symbolic-ref", "refs/remotes/origin/HEAD"]),
        "refs/remotes/origin/other"
    );
    git(&origin, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    let first = git(&cloned, &["rev-parse", "refs/tags/v1"]);
    assert_eq!(
        transport.ls_remote_refs(source, false).unwrap().tags["v1"],
        first
    );
    assert!(transport.ls_remote(source, false, "start").unwrap().success);
    assert!(
        transport
            .remote_tags(&cloned, "origin", false)
            .unwrap()
            .contains("refs/tags/v1")
    );
    fs::write(origin.join("data"), "two").unwrap();
    git(&origin, &["commit", "-qam", "second"]);
    git(&origin, &["tag", "v2"]);
    git(&origin, &["tag", "-d", "v1"]);
    transport
        .fetch(
            &cloned,
            Fetch::Tags {
                remote: "origin",
                prune: false,
            },
            false,
        )
        .unwrap();
    assert_eq!(git(&cloned, &["rev-parse", "refs/tags/v1"]), first);
    let second = git(&origin, &["rev-parse", "HEAD"]);
    assert_eq!(git(&cloned, &["rev-parse", "refs/tags/v2"]), second);
    transport
        .fetch(
            &cloned,
            Fetch::RecordTags {
                remote: "origin",
                namespace: "refs/origin-tags/",
            },
            false,
        )
        .unwrap();
    assert_eq!(git(&cloned, &["rev-parse", "refs/origin-tags/v2"]), second);
    git(&origin, &["tag", "-d", "v2"]);
    transport
        .fetch(
            &cloned,
            Fetch::RecordTags {
                remote: "origin",
                namespace: "refs/origin-tags/",
            },
            false,
        )
        .unwrap();
    assert!(git(&cloned, &["for-each-ref", "refs/origin-tags/"]).is_empty());
    transport
        .fetch(
            &cloned,
            Fetch::Tags {
                remote: "origin",
                prune: true,
            },
            false,
        )
        .unwrap();
    assert!(git(&cloned, &["tag", "--list"]).is_empty());
    git(
        &cloned,
        &[
            "remote",
            "set-url",
            "origin",
            root.join("missing").to_str().unwrap(),
        ],
    );
    assert_eq!(
        transport
            .set_remote_head(&cloned, "origin", false)
            .unwrap_err()
            .kind(),
        "git"
    );
    let error = transport
        .clone_repo(root.path(), "--upload-pack=invalid", "bad", false)
        .unwrap_err();
    assert_eq!(error.kind(), "git");
}
