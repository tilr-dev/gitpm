"""UC-021: Forget and restore (P0).

Fixture g2_hello_history: dan/hello installed at 0.1.0 and updated to 0.1.1 and 0.1.2
(current 0.1.2, previous 0.1.1).
"""

import shutil

from gitpmtest import T, case

ID = "dan/hello"


def tagged(t: T, version: str) -> str:
    return t.rev(ID, f"{ID}@{version}")


def retained(t: T):
    return t.snap(t.clone(ID), t.dist(ID), git=True)


def no_entry(t: T):
    t.ok(ID not in t.state().get("packages", {}), f"{ID} not in state")


@case(fixture="g2_hello_history", spec=["dist/forget-restore-and-uninstall-settle-dist", "install-uninstall/three-removal-levels"])
def forget_removes_state_only(t: T):
    """forget: the state entry is gone; the clone (with .git) stays byte-identical, dist/ID is removed."""
    files = t.snap(t.clone(ID), git=True)
    t.gitpm_ok("forget", ID)
    no_entry(t)
    t.eq(t.gitpm_ok("list").json()["packages"], {}, "list")
    t.same(files, "retained clone")
    t.ok(not t.dist(ID).exists(), "dist/ID removed (forgotten packages are inactive)")
    t.eq([str(p) for p in t.ws.rglob("*.pm-txn*")], [], "no leftover transaction")


@case(fixture="g2_hello_history", spec=["install-uninstall/three-removal-levels#forget-then-restore"])
def restore_recreates_state_without_history(t: T):
    """restore after forget: current 0.1.2 at the retained commit, previous null, enabled; files unchanged."""
    source = t.package(ID)["source"]
    files = retained(t)
    t.gitpm_ok("forget", ID)
    t.gitpm_ok("restore", ID)
    p = t.package(ID)
    t.eq(p["current"], {"version": "0.1.2", "commit": tagged(t, "0.1.2")}, "current")
    t.eq(p["previous"], None, "no rollback history")
    t.eq(p["enabled"], True, "enabled")
    t.eq(p["source"], source, "source")
    t.same(files, "clone and dist (dist redeployed with the same bytes)")
    before = t.snap(t.ws, git=True)
    t.gitpm_err("rollback", ID, kind="no_previous_version", contains=ID)
    t.same(before, "workspace after rollback without history")


@case(fixture="g2_hello_history", spec=["install-uninstall/three-removal-levels#forget-then-restore"])
def restore_of_disabled_plugin_is_enabled(t: T):
    """A forgotten disabled package is restored as a fresh registration (enabled)."""
    t.gitpm_ok("disable", ID)
    t.gitpm_ok("forget", ID)
    t.gitpm_ok("restore", ID)
    t.eq(t.package(ID)["enabled"], True, "enabled after restore")


@case(fixture="g2_hello_history", spec=["errors/stable-error-kinds#removal-of-an-unknown-package"])
def unknown_id(t: T):
    """forget/restore of an ID that has neither entry nor files: error naming the ID, nothing changes."""
    before = t.snap(t.ws, git=True)
    t.gitpm_err("forget", "dan/missing", kind="not_installed", contains="package dan/missing is not installed")
    t.same(before, "workspace")
    t.gitpm_err("restore", "dan/missing", kind="repository_not_found", contains="dan/missing")
    t.same(before, "workspace")
    t.gitpm_err("restore", "dan", kind="invalid_id", contains="namespace/name")
    t.same(before, "workspace (a namespace is not a package ID)")


@case(fixture="g2_hello_history", spec=["install-uninstall/restore-preconditions"])
def duplicate_restore_is_rejected(t: T):
    """restore of a registered package (never forgotten, and after a restore) is already_installed."""
    before = t.snap(t.ws, git=True)
    t.gitpm_err("restore", ID, kind="already_installed", contains=ID)
    t.same(before, "workspace (registered package keeps its history)")
    t.gitpm_ok("forget", ID)
    t.gitpm_ok("restore", ID)
    after = t.snap(t.ws, git=True)
    t.gitpm_err("restore", ID, kind="already_installed", contains=ID)
    t.same(after, "workspace after the duplicate restore")


@case(fixture="g2_hello_history", spec=["install-uninstall/three-removal-levels#restore-without-a-clone"])
def missing_retained_clone(t: T):
    """Forgotten and the clone deleted: restore fails, no entry is fabricated, dist left alone."""

    t.gitpm_ok("forget", ID)
    shutil.rmtree(t.clone(ID))
    before = t.snap(t.ws, git=True)
    t.gitpm_err("restore", ID, kind="repository_not_found", contains=ID)
    t.same(before, "workspace")
    no_entry(t)


@case(fixture="g2_hello_history", spec=["install-uninstall/restore-preconditions"])
def malformed_retained_manifest(t: T):
    """Retained clone whose HEAD manifest is malformed (committed) or edited (uncommitted): restore refuses."""
    t.gitpm_ok("forget", ID)
    clone = t.clone(ID)
    (clone / "plugin.json").write_text("{ broken")
    before = t.snap(t.ws, git=True)
    r = t.gitpm_err("restore", ID)
    t.has(r.err, "Error kind:")
    t.same(before, "workspace (uncommitted broken manifest)")
    no_entry(t)
    t.git(clone, "commit", "-q", "-am", "broken manifest")
    before = t.snap(t.ws, git=True)
    t.gitpm_err("restore", ID, kind="invalid_manifest", contains="plugin.json")
    t.same(before, "workspace (committed broken manifest)")
    no_entry(t)


@case(fixture="g2_hello_history", spec=["install-uninstall/restore-preconditions"])
def retained_manifest_with_other_id(t: T):
    """Retained clone whose manifest names another package: restore refuses, nothing fabricated."""
    t.gitpm_ok("forget", ID)
    clone = t.clone(ID)
    (clone / "plugin.json").write_text('{"id": "dan/renamed", "version": "0.1.2"}\n')
    t.git(clone, "commit", "-q", "-am", "rename")
    before = t.snap(t.ws, git=True)
    t.gitpm_err("restore", ID, kind="invalid_manifest", contains="id")
    t.same(before, "workspace")
    no_entry(t)
    t.ok("dan/renamed" not in t.state()["packages"], "no entry for the other ID")
