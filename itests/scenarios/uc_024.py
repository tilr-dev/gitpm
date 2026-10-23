"""UC-024: Fork release independence and branch collision (P0).

vendor/demo 0.1.0 installed (g3_vendor_installed); the fork author/demo lives in
/fx/repos/author/demo.

updating_fork_leaves_original: the fork repository also holds the upstream tag
vendor/demo@0.1.1 (fetched from upstream); updating the fork must not count that tag as
local work. updating_fork_without_upstream_tags checks the same without that tag.
"""

from gitpmtest import T, case

ID, NEW = "vendor/demo", "author/demo"


def fork(t: T):
    dest = t.repo(NEW)
    t.gitpm_ok("fork", ID, "--ns", "author", "--into", dest)
    return dest


def fork_release(t: T, dest, version: str) -> str:
    t.write(dest, {"dist/index.js": f"export const greeting = 'fork {version}';\n"})
    t.gitpm_ok("commit", dest, version)
    return t.rev(dest, f"{NEW}@{version}")


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
    t.eq(t.git(dest, "branch", "--show-current"), "fork-3", "new branch")
    t.eq((t.rev(dest, "upstream/fork"), t.rev(dest, "upstream/fork-2")), (head, head), "source branches")
    t.same(before, "installation and source branches")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-metadata"])
def single_existing_fork_branch(t: T):
    t.git(t.repo(ID), "branch", "fork")
    t.eq(t.git(fork(t), "branch", "--show-current"), "fork-2", "new branch")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-isolates-source-and-destination-state"])
def installation_only_branch_is_ignored(t: T):
    """A branch absent from the recorded source cannot reserve the fork's branch name."""
    t.git(t.clone(ID), "branch", "fork")
    before = t.snap(t.ws, t.repo(ID), git=True)
    t.eq(t.git(fork(t), "branch", "--show-current"), "fork", "new branch")
    t.same(before, "installation-only branch")


@case(fixture="g3_vendor_installed", spec=["install-uninstall/install-the-latest-release-of-a-git-source#repository-without-releases", "package-ids/release-tag-names"])
def inherited_vendor_tags_are_not_fork_releases(t: T):
    """A fork without its own release tag has no stable release (vendor tags do not count)."""
    dest = fork(t)
    t.has(t.git(dest, "tag", "--list"), "vendor/demo@0.1.0")
    t.gitpm_err("install", dest, kind="no_releases")
    t.not_installed(NEW)


@case(fixture="g3_vendor_installed", spec=["package-ids/release-tag-names", "install-uninstall/install-the-latest-release-of-a-git-source"])
def newer_vendor_tag_is_ignored(t: T):
    """Inherited vendor/demo@0.3.0 is newer than the fork release author/demo@0.2.0: stable
    install of the fork picks 0.2.0."""
    t.release(ID, "0.3.0")
    t.gitpm_ok("update", ID)
    dest = fork(t)
    t.has(t.git(dest, "tag", "--list"), "vendor/demo@0.3.0")
    commit = fork_release(t, dest, "0.2.0")
    t.gitpm_ok("install", dest)
    t.eq(t.package(NEW)["current"], {"version": "0.2.0", "commit": commit}, "fork current")


def both_released(t: T):
    """Fork releases author/demo@0.1.1; vendor releases vendor/demo@0.1.1 too, fetched into the fork."""
    dest = fork(t)
    mine = fork_release(t, dest, "0.1.1")
    theirs = t.release(ID, "0.1.1")
    t.git(dest, "fetch", "-q", "upstream", "--tags")
    return dest, mine, theirs


@case(fixture="g3_vendor_installed", spec=["package-ids/release-tag-names#scoped-tag"])
def scoped_tags_do_not_conflict(t: T):
    dest, mine, theirs = both_released(t)
    t.eq(t.rev(dest, f"{NEW}@0.1.1"), mine, "fork tag")
    t.eq(t.rev(dest, f"{ID}@0.1.1"), theirs, "vendor tag in the fork")
    t.ok(mine != theirs, "different commits")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/replacement-is-explicit-and-global"])
def fork_installs_alongside_without_replacement(t: T):
    dest, mine, _ = both_released(t)
    base = t.package(ID)["current"]
    vendor_before = t.snap(t.clone(ID), t.dist(ID), git=True)
    t.gitpm_ok("install", dest)
    p = t.package(NEW)
    t.eq(p["current"], {"version": "0.1.1", "commit": mine}, "fork current")
    t.eq(p["forkOf"], {"id": ID, "version": "0.1.0", "commit": base["commit"], "source": t.package(ID)["source"]}, "forkOf")
    t.eq(t.package(ID)["current"], base, "original still at 0.1.0")
    t.ok("forkOf" not in t.package(ID), "original has no forkOf")
    t.ok(not t.state().get("replacements"), "no replacement")
    t.same(vendor_before, "original clone and dist")
    t.has((t.dist(NEW) / "index.js").read_text(), "fork 0.1.1")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-copies-an-installed-package", "updates/stable-update-moves-to-the-newest-release"])
def updating_original_leaves_fork(t: T):
    dest, _, theirs = both_released(t)
    t.gitpm_ok("install", dest)
    fork_before = t.snap(t.clone(NEW), t.dist(NEW), git=True)
    fork_state = t.package(NEW)
    t.gitpm_ok("update", ID)
    t.eq(t.package(ID)["current"], {"version": "0.1.1", "commit": theirs}, "original updated")
    t.has((t.dist(ID) / "index.js").read_text(), "vendor/demo 0.1.1")
    t.eq(t.package(NEW), fork_state, "fork state")
    t.same(fork_before, "fork clone and dist")
    t.ok(not t.state().get("replacements"), "no replacement")


@case(fixture="g3_vendor_installed", spec=["updates/stable-update-moves-to-the-newest-release", "updates/local-work-blocks-update-and-rollback"])
def updating_fork_leaves_original(t: T):
    dest, mine, _ = both_released(t)
    t.gitpm_ok("install", dest)
    vendor_before = t.snap(t.clone(ID), t.dist(ID), git=True)
    vendor_state = t.package(ID)
    forkof = t.package(NEW)["forkOf"]
    newer = fork_release(t, dest, "0.1.2")
    t.gitpm_ok("update", NEW)
    p = t.package(NEW)
    t.eq(p["current"], {"version": "0.1.2", "commit": newer}, "fork current")
    t.eq(p["previous"], {"version": "0.1.1", "commit": mine}, "fork previous")
    t.eq(p["forkOf"], forkof, "provenance kept")
    t.eq(t.package(ID), vendor_state, "original state")
    t.same(vendor_before, "original clone and dist")


@case(fixture="g3_vendor_installed", spec=["updates/stable-update-moves-to-the-newest-release"])
def updating_fork_without_upstream_tags(t: T):
    """Independent fork update when the fork repo holds no upstream-only tags."""
    dest = fork(t)
    mine = fork_release(t, dest, "0.1.1")
    t.gitpm_ok("install", dest)
    vendor_before = t.snap(t.clone(ID), t.dist(ID), git=True)
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
