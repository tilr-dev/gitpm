"""UC-015: Stable update and no-op (P0)."""

from gitpmtest import T, case

ID = "3dce1568-569c-5b53-9321-37185e751947"


@case(fixture="hello_installed", spec=["updates/stable-update-moves-to-the-newest-release#update-and-repeat"])
def update_to_new_release(t: T):
    """1. update without a catalog picks the new release tag; previous keeps 0.1.0."""
    commit = t.release(ID, "0.1.1")
    t.gitpm_ok("update", ID)
    p = t.package(ID)
    t.eq(p["current"], {"version": "0.1.1", "commit": commit}, "current")
    t.eq(p["previous"]["version"], "0.1.0", "previous version")
    t.eq(t.rev(t.clone(ID)), commit, "clone HEAD")
    t.eq((t.dist(ID) / "index.js").read_text(), t.git(ID, "show", f"v0.1.1:dist/index.js") + "\n", "dist")
    t.ok(t.catalog_empty(), "catalog never created or read")


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
    """3. an untagged newer commit on main is ignored in release mode."""
    t.commit(ID, "unreleased", {"dist/index.js": "dev\n"})
    before = t.snap(t.ws)
    t.gitpm_ok("update", ID)
    t.same(before, "workspace")


RETRACTED = "Warning: installed release 3dce1568-569c-5b53-9321-37185e751947@{} no longer exists on the origin"


@case(fixture="hello_releases", spec=["updates/stable-update-never-downgrades#retracted-release-in-update-all", "updates/update-mirrors-the-origin-s-tags#deleted-installed-release-tag"])
def latest_release_older_than_installed(t: T):
    """4. installed 0.2.0, upstream tags above 0.1.0 deleted: update and update all stay up
    to date with a warning (never a downgrade)."""
    t.gitpm_ok("install", t.repo(ID))
    t.eq(t.package(ID)["current"]["version"], "0.2.0", "installed the newest release")
    for v in ["0.1.1", "0.2.0"]:
        t.git(ID, "tag", "-d", f"v{v}")
    before = t.snap(t.ws)
    r = t.gitpm_ok("update", ID)
    t.has(r.out, f"{ID} up to date (0.2.0)")
    t.has(r.err, RETRACTED.format("0.2.0"))
    t.same(before, "workspace")
    r = t.gitpm_ok("update", "all")
    t.has(r.out, f"{ID} up to date (0.2.0)")
    t.has(r.err, RETRACTED.format("0.2.0"))
    t.same(before, "workspace")
    # An explicit return to releases may go down to the newest one.
    t.gitpm_ok("update", ID, "--release")
    t.eq(t.package(ID)["current"]["version"], "0.1.0", "update --release goes down")


@case(fixture="hello_installed", spec=["updates/stable-update-never-downgrades#release-commit-of-the-installed-version-moved"])
def installed_release_retagged(t: T):
    """5. the installed version re-tagged on another commit upstream: update and update all
    stay up to date with a warning; the installed commit is kept."""
    installed = t.package(ID)["current"]
    moved = t.commit(ID, "rewritten release", {"dist/index.js": "other\n"})
    t.tag(ID, f"v0.1.0", force=True)
    before = t.snap(t.ws)
    for args in [("update", ID), ("update", "all")]:
        r = t.gitpm_ok(*args)
        t.has(r.out, f"{ID} up to date (0.1.0)")
        t.has(r.err, f"Warning: release tag v0.1.0 now points to {moved[:12]}; the installed commit is kept")
        t.same(before, "workspace")
    t.eq(t.package(ID)["current"], installed, "installed revision kept by commit")
    t.eq(t.rev(t.clone(ID), f"v0.1.0"), moved, "the clone took the moved tag")


@case(fixture="hello_installed", spec=["updates/major-updates-are-marked#major-line"])
def major_update_is_marked(t: T):
    """7. 0.1.0 → 0.1.1 is a plain line; across a breaking boundary (0.x minor, then 2.0.0) the line ends with (major)."""
    t.release(ID, "0.1.1")
    t.eq(t.gitpm_ok("update", ID).out.strip(), f"{ID} updated 0.1.0 → 0.1.1", "compatible update")
    t.release(ID, "1.0.0")
    t.release(ID, "2.0.0")
    t.has(t.gitpm_ok("check", env={"GITPM_CATALOG": ""}).out, "2.0.0 (major)")
    t.eq(t.gitpm_ok("update", ID).out.strip(), f"{ID} updated 0.1.1 → 2.0.0 (major)", "major update")
    t.eq(t.package(ID)["current"]["version"], "2.0.0", "moved across the major version")


@case(fixture="hello_installed", spec=["updates/pre-releases-are-opt-in#newest-tag-is-a-pre-release", "updates/pre-releases-are-opt-in#a-ref-install-is-pinned"])
def pre_releases_are_opt_in(t: T):
    """8. A newer pre-release is shown by check, skipped by update and update all, installed by --ref,
    and then followed (back in release mode) to newer pre-releases of the same version and the release."""
    t.release(ID, "0.2.0-beta.1")
    before = t.snap(t.ws)
    t.eq(t.gitpm_ok("update", ID).out.strip(), f"{ID} up to date (0.1.0)", "update skips the pre-release")
    t.has(t.gitpm_ok("update", "all").out, f"{ID} up to date (0.1.0)")
    t.same(before, "workspace")
    t.has(t.gitpm_ok("check", env={"GITPM_CATALOG": ""}).out, "pre-release 0.2.0-beta.1")
    t.gitpm_ok("update", ID, "--ref", f"v0.2.0-beta.1")
    t.eq(t.package(ID)["current"]["version"], "0.2.0-beta.1", "installed by name")
    t.release(ID, "0.2.0-beta.2")
    t.release(ID, "0.3.0-beta.1")
    t.gitpm_ok("update", ID, "--release")
    t.eq(t.package(ID)["current"]["version"], "0.2.0-beta.2", "follows its own track")
    t.release(ID, "0.2.0")
    t.eq(t.gitpm_ok("update", ID).out.strip(), f"{ID} updated 0.2.0-beta.2 → 0.2.0", "to the release, not major")
    t.eq(t.gitpm_ok("update", ID).out.strip(), f"{ID} up to date (0.2.0)", "never another version's pre-release")
