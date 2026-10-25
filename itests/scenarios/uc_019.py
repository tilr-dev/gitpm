"""UC-019: Failed update transaction (P0).

Fixture g2_hello_history: dan/hello current 0.1.2, previous 0.1.1 (rollback is possible).
Every failed update must exit non-zero, leave the whole workspace (state, clone incl.
.git, dist) byte-identical with no `.pm-txn` leftovers, keep rollback working, and a later
valid update must succeed.
"""

import shutil

from gitpmtest import T, case

ID = "dan/hello"


def tagged(t: T, version: str) -> str:
    return t.rev(ID, f"v{version}")


def failed_update(t: T, *extra, kind: str | None = None, contains=()):
    before, entry = t.snap(t.ws, git=True), t.package(ID)
    r = t.gitpm_err("update", ID, *extra, kind=kind, contains=contains)
    t.has(r.err, "Error kind:")
    t.same(before, "workspace (state, clone with .git, dist)")
    t.eq([str(p) for p in t.ws.rglob("*.pm-txn*")], [], "no half-installed transaction files")
    t.eq(t.package(ID), entry, "state entry kept")
    t.eq((entry["current"]["version"], entry["previous"]["version"]), ("0.1.2", "0.1.1"), "versions")
    return r


def rollback_still_works(t: T):
    """Rollback to 0.1.1 and back to 0.1.2 after the failure."""
    t.gitpm_ok("rollback", ID)
    t.eq(t.package(ID)["current"]["version"], "0.1.1", "rolled back")
    t.has((t.dist(ID) / "index.js").read_text(), f"{ID} 0.1.1")
    t.gitpm_ok("rollback", ID)
    t.eq(t.package(ID)["current"]["version"], "0.1.2", "rolled forward again")


def valid_update_succeeds(t: T, version: str):
    commit = t.release(ID, version)
    t.gitpm_ok("update", ID)
    p = t.package(ID)
    t.eq(p["current"], {"version": version, "commit": commit}, "current after the valid update")
    t.eq(p["previous"]["version"], "0.1.2", "previous after the valid update")
    t.eq(t.rev(t.worktree(ID)), commit, "clone HEAD")
    t.eq((t.dist(ID) / "index.js").read_text(), f'export const greeting = "{ID} {version}";\n', "dist")


@case(fixture="g2_hello_history", spec=["dist/a-revision-without-dist-is-refused#release-without-dist", "errors/failing-operations-leave-the-workspace-unchanged"])
def release_without_dist(t: T):
    """Release 0.1.3 without dist/: stable and --ref updates fail; rollback and a later update work."""
    t.release(ID, "0.1.3", files={"dist/index.js": None})
    t.eq(t.git(ID, "ls-tree", "--name-only", f"v0.1.3"), "plugin.json", "0.1.3 has no dist")
    failed_update(t, kind="invalid_manifest", contains="dist/")
    failed_update(t, "--ref", f"v0.1.3", kind="invalid_manifest", contains="dist/")
    rollback_still_works(t)
    t.write(ID, {"dist/index.js": "x\n"})
    valid_update_succeeds(t, "0.1.4")


@case(fixture="g2_hello_history", spec=["errors/failing-operations-leave-the-workspace-unchanged"])
def release_with_malformed_manifest(t: T):
    """Release 0.1.3 with broken plugin.json: update fails; rollback and a later update work."""
    t.commit(ID, "broken manifest", {"plugin.json": "{ broken"})
    t.tag(ID, f"v0.1.3")
    failed_update(t, contains="0.1.3")  # stable: the newest tag is broken (kind not specified)
    failed_update(t, "--ref", f"v0.1.3", kind="invalid_manifest")
    rollback_still_works(t)
    valid_update_succeeds(t, "0.1.4")


@case(fixture="g2_hello_history", spec=["errors/failing-operations-leave-the-workspace-unchanged"])
def release_with_mismatched_id(t: T):
    """Revision 0.1.3 whose manifest says dan/renamed: update --ref (tag and SHA) fails."""
    sha = t.release(ID, "0.1.3", extra={"id": "dan/renamed"})
    failed_update(t, "--ref", f"v0.1.3", kind="invalid_manifest", contains="id changed")
    failed_update(t, "--ref", sha, kind="invalid_manifest", contains="id changed")
    rollback_still_works(t)
    valid_update_succeeds(t, "0.1.4")


@case(fixture="g2_hello_history", spec=["errors/failing-operations-leave-the-workspace-unchanged"])
def nonexistent_ref(t: T):
    """update --ref to a tag / SHA that does not exist fails."""
    failed_update(t, "--ref", f"v9.9.9", kind="invalid_argument", contains=f"v9.9.9")
    failed_update(t, "--ref", "0123456789abcdef0123456789abcdef01234567")
    rollback_still_works(t)
    valid_update_succeeds(t, "0.1.3")


@case(fixture="g2_hello_history", spec=["errors/failing-operations-leave-the-workspace-unchanged#network-failure-mid-update"])
def origin_unavailable(t: T):
    """The local origin repository is gone: update fails (git); after it returns, update works."""
    t.release(ID, "0.1.3")
    away = t.root / "repos-away/hello"
    away.parent.mkdir()
    shutil.move(t.repo(ID), away)
    failed_update(t, kind="git")
    failed_update(t, "--dev", kind="git")
    rollback_still_works(t)
    shutil.move(away, t.repo(ID))
    t.gitpm_ok("update", ID)
    t.eq(t.package(ID)["current"], {"version": "0.1.3", "commit": tagged(t, "0.1.3")}, "update after origin is back")
