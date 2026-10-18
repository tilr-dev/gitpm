"""UC-015: Stable update and no-op (P0)."""

from gitpmtest import T, case

ID = "dan/hello"


@case(fixture="hello_installed", spec=["updates/stable-update-moves-to-the-newest-release#update-and-repeat"])
def update_to_new_release(t: T):
    """1. update without a catalog picks the new release tag; previous keeps 0.1.0."""
    commit = t.release(ID, "0.1.1")
    t.gitpm_ok("update", ID)
    p = t.package(ID)
    t.eq(p["current"], {"version": "0.1.1", "commit": commit}, "current")
    t.eq(p["previous"]["version"], "0.1.0", "previous version")
    t.eq(t.rev(t.clone(ID)), commit, "clone HEAD")
    t.eq((t.dist(ID) / "index.js").read_text(), t.git(ID, "show", f"{ID}@0.1.1:dist/index.js") + "\n", "dist")
    t.ok(not t.catalog.exists(), "catalog never created or read")


@case(fixture="hello_installed", spec=["updates/stable-update-moves-to-the-newest-release#update-and-repeat"])
def repeated_update_is_noop(t: T):
    """2. a second update at the same commit changes nothing."""
    t.release(ID, "0.1.1")
    t.gitpm_ok("update", ID)
    before = t.snap(t.ws)
    t.gitpm_ok("update", ID)
    t.same(before, "workspace")


@case(fixture="hello_installed", spec=["updates/stable-update-moves-to-the-newest-release"])
def stable_ignores_untagged_head(t: T):
    """3. an untagged newer commit on main is ignored by the stable channel."""
    t.commit(ID, "unreleased", {"dist/index.js": "dev\n"})
    before = t.snap(t.ws)
    t.gitpm_ok("update", ID)
    t.same(before, "workspace")


@case(fixture="hello_releases", spec=["updates/stable-update-never-downgrades-or-accepts-mutated-releases"])
def latest_release_older_than_installed(t: T):
    """4. installed 0.2.0, upstream tags above 0.1.0 deleted: update refuses."""
    t.gitpm_ok("install", t.repo(ID), "--ref", f"{ID}@0.2.0")
    for v in ["0.1.1", "0.2.0"]:
        t.git(ID, "tag", "-d", f"{ID}@{v}")
    before = t.snap(t.ws)
    t.gitpm_err("update", ID, kind="version_conflict", contains="latest release is older")
    t.same(before, "workspace")


@case(fixture="hello_installed", spec=["updates/stable-update-never-downgrades-or-accepts-mutated-releases#release-commit-of-the-installed-version-moved"])
def installed_release_retagged(t: T):
    """5. the installed version re-tagged on another commit upstream: update refuses."""
    t.commit(ID, "rewritten release", {"dist/index.js": "other\n"})
    t.tag(ID, f"{ID}@0.1.0", force=True)
    before = t.snap(t.ws)
    t.gitpm_err("update", ID, kind="version_conflict", contains="release commit changed for installed version")
    t.same(before, "workspace")
