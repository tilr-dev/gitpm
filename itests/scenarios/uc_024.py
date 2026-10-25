"""UC-024: Fork release independence and branch collision (P0).

dd3387ed-a5a3-5059-81ad-b1f6987a28f2 0.1.0 installed (g3_vendor_installed); the fork be517c52-cf84-5615-9a18-9fae7d82c819 lives in
/fx/repos/be517c52-cf84-5615-9a18-9fae7d82c819.

updating_fork_leaves_original: the fork repository also holds the upstream tag
v0.1.1 (fetched from upstream); updating the fork must not count that tag as
local work. updating_fork_without_upstream_tags checks the same without that tag.
"""

from gitpmtest import T, case

ID, NEW = "dd3387ed-a5a3-5059-81ad-b1f6987a28f2", "be517c52-cf84-5615-9a18-9fae7d82c819"


def fork(t: T):
    dest = t.root / "authors/fork"
    t.gitpm_ok("fork", ID, "--into", dest)
    return dest


def fork_release(t: T, dest, version: str) -> str:
    t.write(dest, {"dist/index.js": f"export const greeting = 'fork {version}';\n"})
    t.gitpm_ok("commit", dest, version)
    return t.rev(dest, f"v{version}")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-metadata"])
def branch_name_is_unique(t: T):
    """Existing fork and fork-2 branches in the recorded source: the new branch is fork-3,
    the existing ones are kept untouched."""
    source = t.repo(ID)
    head = t.rev(source)
    t.git(source, "branch", "fork")
    t.git(source, "branch", "fork-2")
    before = t.snap(t.ws, source, git=True)
    dest = fork(t)
    NEW = t.manifest_id(dest)
    t.eq(t.git(dest, "branch", "--show-current"), "fork-3", "new branch")
    t.eq((t.rev(dest, "3a5fc361-2231-5acd-869e-d470a2c132e9"), t.rev(dest, "5aa26591-f2bf-5aaa-a795-434f318404ed")), (head, head), "source branches")
    t.same(before, "installation and source branches")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-metadata"])
def single_existing_fork_branch(t: T):
    t.git(t.repo(ID), "branch", "fork")
    t.eq(t.git(fork(t), "branch", "--show-current"), "fork-2", "new branch")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-isolates-source-and-destination-state"])
def installation_only_branch_is_ignored(t: T):
    """A branch absent from the recorded source cannot reserve the fork's branch name."""
    t.git(t.worktree(ID), "branch", "fork")
    before = t.snap(t.ws, t.repo(ID), git=True)
    t.eq(t.git(fork(t), "branch", "--show-current"), "fork", "new branch")
    t.same(before, "installation-only branch")


@case(fixture="g3_vendor_installed", spec=["install-uninstall/install-the-latest-release-of-a-git-source#repository-without-releases", "package-ids/release-tag-names"])
def inherited_vendor_tags_are_not_fork_releases(t: T):
    """A fork without its own release tag has no stable release (vendor tags do not count)."""
    dest = fork(t)
    NEW = t.manifest_id(dest)
    t.has(t.git(dest, "tag", "--list"), "v0.1.0")
    r = t.gitpm_err("install", dest, kind="no_releases")
    t.lacks(r.err, "Warning:")
    t.not_installed(NEW)


@case(fixture="g3_vendor_installed", spec=["package-ids/release-tag-names", "install-uninstall/install-the-latest-release-of-a-git-source"])
def fork_release_uses_a_new_repository_version(t: T):
    """The fork publishes 0.4.0 after inheriting the upstream repository tag v0.3.0."""
    t.release(ID, "0.3.0")
    t.gitpm_ok("update", ID)
    dest = fork(t)
    NEW = t.manifest_id(dest)
    t.has(t.git(dest, "tag", "--list"), "v0.3.0")
    commit = fork_release(t, dest, "0.4.0")
    t.gitpm_ok("install", dest)
    t.eq(t.package(NEW)["current"], {"version": "0.4.0", "commit": commit}, "fork current")


def both_released(t: T):
    """The fork releases v0.1.2 and retains the upstream v0.1.1 tag after fetching."""
    dest = fork(t)
    NEW = t.manifest_id(dest)
    mine = fork_release(t, dest, "0.1.2")
    theirs = t.release(ID, "0.1.1")
    t.git(dest, "fetch", "-q", "upstream", "--tags")
    return dest, mine, theirs


@case(fixture="g3_vendor_installed", spec=["package-ids/release-tag-names#vversion-tag-of-a-scoped-single-package-repository"])
def distinct_repository_tags_do_not_conflict(t: T):
    dest, mine, theirs = both_released(t)
    NEW = t.manifest_id(dest)
    t.eq(t.rev(dest, "v0.1.2"), mine, "fork tag")
    t.eq(t.rev(dest, f"v0.1.1"), theirs, "vendor tag in the fork")
    t.ok(mine != theirs, "different commits")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/replacement-is-explicit-and-global"])
def fork_installs_alongside_without_replacement(t: T):
    dest, mine, _ = both_released(t)
    NEW = t.manifest_id(dest)
    base = t.package(ID)["current"]
    vendor_before = t.snap(t.worktree(ID), t.dist(ID), git=True)
    t.gitpm_ok("install", dest)
    p = t.package(NEW)
    t.eq(p["current"], {"version": "0.1.2", "commit": mine}, "fork current")
    t.eq(p["forkOf"], {"id": ID, "version": "0.1.0", "commit": base["commit"], "source": t.package(ID)["source"]}, "forkOf")
    t.eq(t.package(ID)["current"], base, "original still at 0.1.0")
    t.ok("forkOf" not in t.package(ID), "original has no forkOf")
    t.ok(not t.state().get("replacements"), "no replacement")
    t.same(vendor_before, "original clone and dist")
    t.has((t.dist(NEW) / "index.js").read_text(), "fork 0.1.2")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-copies-an-installed-package", "updates/stable-update-moves-to-the-newest-release"])
def updating_original_leaves_fork(t: T):
    dest, _, theirs = both_released(t)
    NEW = t.manifest_id(dest)
    t.gitpm_ok("install", dest)
    fork_before = t.snap(t.worktree(NEW), t.dist(NEW), git=True)
    fork_state = t.package(NEW)
    t.gitpm_ok("update", ID)
    t.eq(t.package(ID)["current"], {"version": "0.1.1", "commit": theirs}, "original updated")
    t.has((t.dist(ID) / "index.js").read_text(), "dd3387ed-a5a3-5059-81ad-b1f6987a28f2 0.1.1")
    t.eq(t.package(NEW), fork_state, "fork state")
    t.same(fork_before, "fork clone and dist")
    t.ok(not t.state().get("replacements"), "no replacement")


@case(fixture="g3_vendor_installed", spec=["updates/stable-update-moves-to-the-newest-release", "updates/local-work-blocks-update-and-rollback"])
def updating_fork_leaves_original(t: T):
    dest, mine, _ = both_released(t)
    NEW = t.manifest_id(dest)
    t.gitpm_ok("install", dest)
    vendor_before = t.snap(t.worktree(ID), t.dist(ID), git=True)
    vendor_state = t.package(ID)
    forkof = t.package(NEW)["forkOf"]
    newer = fork_release(t, dest, "0.1.3")
    t.gitpm_ok("update", NEW)
    p = t.package(NEW)
    t.eq(p["current"], {"version": "0.1.3", "commit": newer}, "fork current")
    t.eq(p["previous"], {"version": "0.1.2", "commit": mine}, "fork previous")
    t.eq(p["forkOf"], forkof, "provenance kept")
    t.eq(t.package(ID), vendor_state, "original state")
    t.same(vendor_before, "original clone and dist")


@case(fixture="g3_vendor_installed", spec=["updates/stable-update-moves-to-the-newest-release"])
def updating_fork_without_upstream_tags(t: T):
    """Independent fork update when the fork repo holds no upstream-only tags."""
    dest = fork(t)
    NEW = t.manifest_id(dest)
    mine = fork_release(t, dest, "0.1.1")
    t.gitpm_ok("install", dest)
    vendor_before = t.snap(t.worktree(ID), t.dist(ID), git=True)
    vendor_state = t.package(ID)
    forkof = t.package(NEW)["forkOf"]
    newer = fork_release(t, dest, "0.1.2")
    t.gitpm_ok("update", NEW)
    p = t.package(NEW)
    t.eq(p["current"], {"version": "0.1.2", "commit": newer}, "fork current")
    t.eq(p["previous"], {"version": "0.1.1", "commit": mine}, "fork previous")
    t.eq(p["forkOf"], forkof, "provenance kept")
    t.has((t.dist(NEW) / "index.js").read_text(), "fork 0.1.2")
    t.eq(t.package(ID), vendor_state, "original state")
    t.same(vendor_before, "original clone and dist")
