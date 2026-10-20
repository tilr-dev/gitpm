use gitpm_transport::{Fetch, SystemTransport, Transport};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "tag.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}
#[test]
fn typed_network_operations_preserve_refs_and_pruning() {
    let root = Fixture(
        std::env::temp_dir().join(format!("gitpm-transport-network-{}", std::process::id())),
    );
    fs::create_dir(&root.0).unwrap();
    let origin = root.0.join("origin");
    fs::create_dir(&origin).unwrap();
    git(&origin, &["init", "-q", "-b", "main"]);
    fs::write(origin.join("data"), "one").unwrap();
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-qm", "first"]);
    git(&origin, &["tag", "v1"]);
    let transport: &dyn Transport = &SystemTransport;
    let source = origin.to_str().unwrap();
    transport
        .clone_repo(&root.0, source, "clone", false)
        .unwrap();
    let cloned = root.0.join("clone");
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
    let error = transport
        .clone_repo(&root.0, "--upload-pack=invalid", "bad", false)
        .unwrap_err();
    assert_eq!(error.kind(), "git");
}
