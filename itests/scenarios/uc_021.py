"""UC-021: Forget and restore (P0).

forget keeps the whole record under `forgotten`; restore records it again as it was
(revisions, catalog, enabled flag, mode).

Fixture g2_hello_history: 3dce1568-569c-5b53-9321-37185e751947 installed at 0.1.0 and updated to 0.1.1 and 0.1.2
(current 0.1.2, previous 0.1.1). g1_ws_catalog: 3dce1568-569c-5b53-9321-37185e751947 0.1.1 installed from the catalog.
"""

import shutil

from gitpmtest import T, case

ID = "3dce1568-569c-5b53-9321-37185e751947"


def tagged(t: T, version: str) -> str:
    return t.rev(ID, f"v{version}")


def retained(t: T):
    return t.snap(t.clone(ID), t.dist(ID), git=True)


def no_entry(t: T):
    t.ok(ID not in t.state().get("packages", {}), f"{ID} not in state")


@case(fixture="g2_hello_history", spec=["dist/forget-restore-and-uninstall-settle-dist", "install-uninstall/three-removal-levels"])
def forget_removes_state_only(t: T):
    """forget: the state entry is gone; the clone (with .git) stays byte-identical, dist/ID is removed."""
    files = t.snap(t.clone(ID), git=True)
    record = t.package(ID)
    t.gitpm_ok("forget", ID)
    no_entry(t)
    t.eq(t.state()["forgotten"][ID], record, "forgotten record (the whole record)")
    t.eq(t.gitpm_ok("list").json()["packages"], {}, "list")
    t.same(files, "retained clone")
    t.ok(not t.dist(ID).exists(), "dist/ID removed (forgotten packages are inactive)")
    t.eq([str(p) for p in t.ws.rglob("*.pm-txn*")], [], "no leftover transaction")


@case(fixture="g2_hello_history", spec=["install-uninstall/three-removal-levels#forget-then-restore"])
def restore_keeps_the_forgotten_record(t: T):
    """restore after forget: the record as it was (current 0.1.2, previous 0.1.1, enabled); files unchanged."""
    record = t.package(ID)
    files = retained(t)
    t.gitpm_ok("forget", ID)
    t.eq(t.gitpm_ok("restore", ID).out, f"{ID} restored 0.1.2\n", "output")
    p = t.package(ID)
    t.eq(p, record, "record")
    t.eq(p["current"], {"version": "0.1.2", "commit": tagged(t, "0.1.2")}, "current")
    t.eq(p["previous"], {"version": "0.1.1", "commit": tagged(t, "0.1.1")}, "rollback history kept")
    t.ok("forgotten" not in t.state() or ID not in t.state()["forgotten"], "forgotten record moved back")
    t.same(files, "clone and dist (dist redeployed with the same bytes)")
    t.gitpm_ok("rollback", ID)
    t.eq(t.package(ID)["current"]["version"], "0.1.1", "rollback after restore")


@case(fixture="g2_hello_history", spec=["install-uninstall/three-removal-levels#forget-then-restore"])
def restore_of_a_disabled_package_keeps_it_disabled(t: T):
    """A forgotten disabled package is restored disabled: nothing is deployed."""
    t.gitpm_ok("disable", ID)
    t.gitpm_ok("forget", ID)
    t.gitpm_ok("restore", ID)
    t.eq(t.package(ID)["enabled"], False, "still disabled after restore")
    t.ok(not t.dist(ID).exists(), "dist/ID not deployed")


@case(fixture="g1_ws_catalog", spec=["install-uninstall/three-removal-levels#forget-then-restore"])
def restore_keeps_the_catalog(t: T):
    """A package installed from a verified catalog keeps its catalog binding through forget and
    restore (the fixture's catalog is verified and the package installed from it again)."""
    t.edit_json(t.catalog / "catalog.json", lambda c: c.update(verified=True))
    t.gitpm_ok("uninstall", ID)
    t.gitpm_ok("install-catalog", ID, "--version", "0.1.1")
    record = t.package(ID)
    t.ok(record.get("catalog"), "installed from the catalog")
    t.gitpm_ok("forget", ID)
    t.gitpm_ok("restore", ID)
    t.eq(t.package(ID), record, "record with catalog")
    t.has((t.dist(ID) / "index.js").read_text(), "3dce1568-569c-5b53-9321-37185e751947 0.1.1")


@case(fixture="g2_hello_history", spec=["install-uninstall/restore-preconditions"])
def restore_refuses_a_moved_head(t: T):
    """The retained clone checked out at another (published) release: restore refuses, HEAD moved."""
    t.gitpm_ok("forget", ID)
    t.git(t.clone(ID), "checkout", "-q", "--detach", f"v0.1.1")
    before = t.snap(t.ws, git=True)
    t.gitpm_err("restore", ID, kind="local_changes", contains="HEAD moved")
    t.same(before, "workspace")
    no_entry(t)
    t.git(t.clone(ID), "checkout", "-q", "--detach", f"v0.1.2")
    t.gitpm_ok("restore", ID)
    t.eq(t.package(ID)["current"]["version"], "0.1.2", "restored at the recorded commit")


@case(fixture="g2_hello_history", spec=["errors/stable-error-kinds#removal-of-an-unknown-package"])
def unknown_id(t: T):
    """forget/restore of an ID that has neither entry nor files: error naming the ID, nothing changes."""
    before = t.snap(t.ws, git=True)
    t.gitpm_err("forget", "a68376c0-a262-545a-a4f8-afe9998880a6", kind="not_installed", contains="package a68376c0-a262-545a-a4f8-afe9998880a6 is not installed")
    t.same(before, "workspace")
    t.gitpm_err("restore", "a68376c0-a262-545a-a4f8-afe9998880a6", kind="repository_not_found", contains="a68376c0-a262-545a-a4f8-afe9998880a6")
    t.same(before, "workspace")
    t.gitpm_err("restore", "dan", kind="invalid_id", contains="UUID")
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
    """Retained clone whose manifest is edited (uncommitted) or committed broken (unpublished): restore refuses."""
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
    t.gitpm_err("restore", ID, kind="local_changes", contains="unpublished commit")
    t.same(before, "workspace (committed broken manifest)")
    no_entry(t)


@case(fixture="g2_hello_history", spec=["install-uninstall/restore-preconditions"])
def retained_manifest_with_other_id(t: T):
    """Retained clone with a commit renaming the package: unpublished work, restore refuses, nothing fabricated."""
    t.gitpm_ok("forget", ID)
    clone = t.clone(ID)
    (clone / "plugin.json").write_text('{"id": "d92f82f0-1472-59b9-b4c5-eb76b0706d47", "name": "fixture", "version": "0.1.2"}\n')
    t.git(clone, "commit", "-q", "-am", "rename")
    before = t.snap(t.ws, git=True)
    t.gitpm_err("restore", ID, kind="local_changes", contains="unpublished commit")
    t.same(before, "workspace")
    no_entry(t)
    t.ok("d92f82f0-1472-59b9-b4c5-eb76b0706d47" not in t.state()["packages"], "no entry for the other ID")
