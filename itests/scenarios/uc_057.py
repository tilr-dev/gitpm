"""UC-057: Single selected catalog policy (P0).

Fixture g4_two_catalogs: A=/fx/catA (x/p 1.0.0 from /fx/repos/x/p, z/r 1.0.0) and
B=/fx/catB (x/p 1.0.0 + 1.1.0 from /fx/mirror/x/p - a different source, y/q 1.0.0,
z/r 1.0.0 identical to A). B is also served over loopback HTTP where noted.
"""

import os

from gitpmtest import T, case

XP, YQ, ZR = "x/p", "y/q", "z/r"


def cat(t: T, name: str):
    return t.root / name


def install_from_a(t: T, *ids):
    for id in ids or (XP,):
        t.gitpm_ok("install-catalog", id, env={"GITPM_CATALOG": str(cat(t, "catA"))})


def row(out: str, id: str) -> str:
    return next(l for l in out.splitlines() if l.split() and l.split()[0] == id)


def readonly(t: T, fn):
    before = t.snap(t.ws)
    result = fn()
    t.same(before, "installed state/dist (read-only step)")
    return result


@case(fixture="g4_two_catalogs", spec=["workspace-layout/installed-package-record", "install-uninstall/install-from-the-catalog"])
def origin_catalog_recorded(t: T):
    """1. install-catalog x/p from A stores the originating catalog in state."""
    install_from_a(t)
    p = t.package(XP)
    t.eq(p["catalog"], os.path.realpath(cat(t, "catA") / "catalog.json"), "state.catalog")
    t.eq(p["source"], str(t.repo(XP)), "source from A")
    t.eq(p["current"]["version"], "1.0.0", "version")


@case(fixture="g4_two_catalogs", spec=["catalog/check-compares-installed-packages-with-their-catalog-or-origin#identity-mismatch", "catalog/check-compares-installed-packages-with-their-catalog-or-origin#other-catalog"])
def check_against_b_reports_different_source(t: T):
    """2. GITPM_CATALOG switched to B (local and HTTP): x/p is "different source", not an update to 1.1.0."""
    install_from_a(t)
    srv = t.serve(cat(t, "catB"))
    for location in [str(cat(t, "catB")), srv.url]:
        out = readonly(t, lambda: t.gitpm_ok("check", env={"GITPM_CATALOG": location}).out)
        t.has(row(out, XP), "different source in catalog")
        t.lacks(row(out, XP), "update available", "1.1.0")
        t.has(out, "Updates: 0.")


@case(fixture="g4_two_catalogs", spec=["catalog/one-catalog-per-workspace", "updates/stable-update-moves-to-the-newest-release"])
def update_keeps_original_source(t: T):
    """3. update x/p (no --ref) with GITPM_CATALOG=B follows x/p's own Git origin (A's source,
    which has 1.1.0 too); source and recorded catalog stay A's - never switched to B."""
    install_from_a(t)
    t.gitpm_ok("update", XP, env={"GITPM_CATALOG": str(cat(t, "catB"))})
    p = t.package(XP)
    t.eq(p["source"], str(t.repo(XP)), "source unchanged (A)")
    t.eq(p["catalog"], os.path.realpath(cat(t, "catA") / "catalog.json"), "recorded catalog unchanged")
    t.eq(p["current"], {"version": "1.1.0", "commit": t.rev(XP, "x/p@1.1.0")}, "updated from its own origin")
    t.eq(t.git(t.clone(XP), "remote", "get-url", "origin"), str(t.repo(XP)), "clone origin unchanged")


@case(fixture="g4_two_catalogs", spec=["catalog/one-catalog-per-workspace#second-catalog"])
def mixing_catalogs_refused(t: T):
    """4. install-catalog y/q from B while x/p from A is installed is refused (no mixing)."""
    install_from_a(t)
    before = t.snap(t.ws)
    for location in [str(cat(t, "catB")), t.serve(cat(t, "catB")).url]:
        t.gitpm_err("install-catalog", YQ, env={"GITPM_CATALOG": location}, kind="catalog",
                 contains="mixing catalogs is not supported")
        t.same(before, "workspace")
        t.not_installed(YQ)


@case(fixture="g4_two_catalogs", spec=["catalog/check-compares-installed-packages-with-their-catalog-or-origin#other-catalog"])
def same_commit_same_source_is_up_to_date(t: T):
    """5. z/r installed from A, identical release (source + commit) in B: check against B says "up to date"."""
    install_from_a(t, ZR)
    srv = t.serve(cat(t, "catB"))
    for location in [str(cat(t, "catB")), srv.url]:
        out = readonly(t, lambda: t.gitpm_ok("check", env={"GITPM_CATALOG": location}).out)
        t.has(row(out, ZR), "up to date")


@case(fixture="g4_two_catalogs", spec=["cli/settings-come-from-flags-environment-defaults"])
def info_shows_the_selected_catalog_only(t: T):
    """6. info shows only the catalog given by GITPM_CATALOG, and --catalog wins over it;
    there is no second (overview or public) address."""
    install_from_a(t)
    a, b = str(cat(t, "catA")), str(cat(t, "catB"))
    out = readonly(t, lambda: t.gitpm_ok("info", env={"GITPM_CATALOG": b}).out)
    t.has(out, f"Catalog: {b}\n", "y/q — latest: 1.0.0", "x/p — latest: 1.1.0")
    out = readonly(t, lambda: t.gitpm_ok("--catalog", a, "info", env={"GITPM_CATALOG": b}).out)
    t.has(out, f"Catalog: {a}\n", "x/p — latest: 1.0.0")
    t.lacks(out, "y/q", b)
