"""UC-029: Catalog install exact/latest and integrity (P0)."""

import json

from gitpmtest import T, case

ID = "dan/num"


def registered(t: T, version: str) -> dict:
    return json.loads(t.catalog_release(ID, version).read_text())


@case(fixture="catalog", spec=["install-uninstall/install-from-the-catalog"])
def install_exact_version(t: T):
    """install-catalog --version installs exactly the registered commit."""
    t.gitpm_ok("install-catalog", ID, "--version", "1.9.0")
    p = t.package(ID)
    t.eq(p["current"], {"version": "1.9.0", "commit": registered(t, "1.9.0")["commit"]}, "current")
    t.eq(t.rev(t.clone(ID)), p["current"]["commit"], "clone HEAD")
    t.has((t.dist(ID) / "index.js").read_text(), "dan/num 1.9.0")


@case(fixture="catalog", spec=["install-uninstall/install-from-the-catalog", "package-ids/version-format#numeric-ordering"])
def install_latest_is_numeric_and_pinned(t: T):
    """Without --version: 1.10.0 (numeric latest) at the registered commit, even after the
    source HEAD and the release tag moved."""
    pinned = registered(t, "1.10.0")["commit"]
    moved = t.commit(ID, "after registration", {"dist/index.js": "moved\n"})
    t.tag(ID, f"{ID}@1.10.0", moved, force=True)
    t.gitpm_ok("install-catalog", ID)
    p = t.package(ID)
    t.eq(p["current"], {"version": "1.10.0", "commit": pinned}, "current")
    t.has((t.dist(ID) / "index.js").read_text(), "dan/num 1.10.0")


@case(fixture="catalog", spec=["install-uninstall/install-from-the-catalog", "cli/settings-come-from-flags-environment-defaults"])
def independent_roots(t: T):
    """Exact and latest installs into two independent workspaces."""
    t.gitpm_ok("--root", t.root / "exact", "install-catalog", ID, "--version", "1.9.0")
    t.gitpm_ok("--root", t.root / "latest", "install-catalog", ID)
    exact = json.loads((t.root / "exact/state.json").read_text())["packages"][ID]["current"]["version"]
    latest = json.loads((t.root / "latest/state.json").read_text())["packages"][ID]["current"]["version"]
    t.eq((exact, latest), ("1.9.0", "1.10.0"), "versions per root")
    t.ok(not t.ws.exists(), "default workspace untouched")


def tampered(t: T, change, what: str):
    t.edit_json(t.catalog_release(ID, "1.10.0"), change)
    r = t.gitpm_err("install-catalog", ID, "--version", "1.10.0")
    print(f"  rejected ({what}): {r.err.strip().splitlines()[-1]}")
    t.not_installed(ID)


@case(fixture="catalog", spec=["install-uninstall/install-from-the-catalog", "errors/failing-operations-leave-the-workspace-unchanged"])
def tampered_manifest(t: T):
    """A release whose manifest differs from the commit is rejected; nothing installed."""
    tampered(t, lambda r: r["manifest"].update(description="injected"), "manifest")


@case(fixture="catalog", spec=["install-uninstall/install-from-the-catalog", "catalog/release-record"])
def tampered_file_size(t: T):
    tampered(t, lambda r: r["files"]["index.js"].update(size=r["files"]["index.js"]["size"] + 1), "size")


@case(fixture="catalog", spec=["install-uninstall/install-from-the-catalog", "catalog/release-record"])
def tampered_file_hash(t: T):
    tampered(t, lambda r: r["files"]["index.js"].update(sha256="0" * 64), "sha256")


@case(fixture="catalog", spec=["install-uninstall/install-from-the-catalog", "catalog/release-record"])
def tampered_commit(t: T):
    """The commit of another release (1.9.0) under 1.10.0 is rejected."""
    other = registered(t, "1.9.0")["commit"]
    tampered(t, lambda r: r.update(commit=other), "commit")


@case(fixture="catalog", spec=["install-uninstall/install-from-the-catalog", "errors/errors-name-the-package"])
def unknown_id(t: T):
    t.gitpm_err("install-catalog", "dan/missing", contains="dan/missing")
    t.not_installed("dan/missing")


@case(fixture="catalog", spec=["install-uninstall/install-from-the-catalog"])
def unknown_version(t: T):
    t.gitpm_err("install-catalog", ID, "--version", "9.9.9", contains="9.9.9")
    t.not_installed(ID)
