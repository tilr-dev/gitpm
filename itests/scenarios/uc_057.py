"""UC-057: Single selected catalog policy (P0).

Fixture g4_two_catalogs: A=/fx/catA (c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5 1.0.0 from /fx/repos/c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5, 055241a5-ec83-58c8-9b54-93e74bee2e74 1.0.0) and
B=/fx/catB (c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5 1.0.0 + 1.1.0 from /fx/mirror/c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5 - a different source, 627a9f5c-10d4-5960-a0ab-4df072d8b52d 1.0.0,
055241a5-ec83-58c8-9b54-93e74bee2e74 1.0.0 identical to A). Both are verified; B is also served over loopback HTTP where noted.
"""

import json
import os

from gitpmtest import T, case

XP, YQ, ZR = "c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5", "627a9f5c-10d4-5960-a0ab-4df072d8b52d", "055241a5-ec83-58c8-9b54-93e74bee2e74"


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
    """1. install-catalog c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5 from A stores the originating catalog in state."""
    install_from_a(t)
    p = t.package(XP)
    t.eq(p["catalog"], os.path.realpath(cat(t, "catA") / "catalog.json"), "state.catalog")
    t.eq(p["source"], str(t.repo(XP)), "source from A")
    t.eq(p["current"]["version"], "1.0.0", "version")


@case(fixture="g4_two_catalogs", spec=["catalog/check-compares-installed-packages-with-their-catalog-or-origin#identity-mismatch", "catalog/check-compares-installed-packages-with-their-catalog-or-origin#other-catalog"])
def check_against_b_reports_different_source(t: T):
    """2. GITPM_CATALOG switched to B (local and HTTP): c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5 is "different source", not an update to 1.1.0."""
    install_from_a(t)
    srv = t.serve(cat(t, "catB"))
    for location in [str(cat(t, "catB")), srv.url]:
        out = readonly(t, lambda: t.gitpm_ok("check", env={"GITPM_CATALOG": location}).out)
        t.has(row(out, XP), "different source in catalog")
        t.lacks(row(out, XP), "update available", "1.1.0")
        t.has(out, "Updates: 0.")


@case(fixture="g4_two_catalogs", spec=["catalog/one-catalog-per-workspace", "updates/protected-packages-update-through-their-catalog#newer-tag-not-in-the-catalog"])
def update_keeps_original_source(t: T):
    """3. update c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5 (no --ref) with GITPM_CATALOG=B reads c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5's own verified catalog A, never B:
    its origin's 1.1.0 is not in A, so it stays; once A lists 1.1.0 it moves there from its own
    origin; source and recorded catalog stay A's."""
    install_from_a(t)
    r = t.gitpm_ok("update", XP, env={"GITPM_CATALOG": str(cat(t, "catB"))})
    t.has(r.out, "c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5 kept at 1.0.0 (upstream 1.1.0 not in catalog)")
    t.gitpm_ok("--catalog", cat(t, "catA"), "register", t.repo(XP), "--ref", "v1.1.0")
    t.gitpm_ok("update", XP, env={"GITPM_CATALOG": str(cat(t, "catB"))})
    p = t.package(XP)
    t.eq(p["source"], str(t.repo(XP)), "source unchanged (A)")
    t.eq(p["catalog"], os.path.realpath(cat(t, "catA") / "catalog.json"), "recorded catalog unchanged")
    t.eq(p["current"], {"version": "1.1.0", "commit": t.rev(XP, "v1.1.0")}, "updated from its own origin")
    t.eq(t.git(t.worktree(XP), "remote", "get-url", "origin"), str(t.repo(XP)), "clone origin unchanged")


@case(fixture="g4_two_catalogs", spec=["catalog/one-catalog-per-workspace#second-catalog"])
def mixing_catalogs_refused(t: T):
    """4. install-catalog 627a9f5c-10d4-5960-a0ab-4df072d8b52d from B while c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5 from A is installed is refused (no mixing)."""
    install_from_a(t)
    before = t.snap(t.ws)
    for location in [str(cat(t, "catB")), t.serve(cat(t, "catB")).url]:
        t.gitpm_err("install-catalog", YQ, env={"GITPM_CATALOG": location}, kind="catalog",
                 contains="mixing verified catalogs is not supported")
        t.same(before, "workspace")
        t.not_installed(YQ)


@case(fixture="g4_two_catalogs", spec=["catalog/check-compares-installed-packages-with-their-catalog-or-origin#other-catalog"])
def same_commit_same_source_is_up_to_date(t: T):
    """5. 055241a5-ec83-58c8-9b54-93e74bee2e74 installed from A, identical release (source + commit) in B: check against B says "up to date"."""
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
    t.has(out, f"Catalog: {b}\n", "627a9f5c-10d4-5960-a0ab-4df072d8b52d — latest: 1.0.0", "c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5 — latest: 1.1.0")
    out = readonly(t, lambda: t.gitpm_ok("--catalog", a, "info", env={"GITPM_CATALOG": b}).out)
    t.has(out, f"Catalog: {a}\n", "c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5 — latest: 1.0.0")
    t.lacks(out, "627a9f5c-10d4-5960-a0ab-4df072d8b52d", b)


@case(fixture="g4_two_catalogs", spec=["catalog/one-catalog-per-workspace#second-catalog", "install-uninstall/install-from-the-catalog#install-from-an-unverified-catalog", "catalog/check-compares-installed-packages-with-their-catalog-or-origin#verified-or-not"])
def unverified_catalog_is_exempt(t: T):
    """7. B without the flag serves discovery only: 627a9f5c-10d4-5960-a0ab-4df072d8b52d installs beside c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5 from A, records no
    catalog and updates from its origin; check says the catalog is unverified."""
    install_from_a(t)
    t.edit_json(cat(t, "catB") / "catalog.json", lambda c: c.pop("verified"))
    b = str(cat(t, "catB"))
    t.gitpm_ok("install-catalog", YQ, env={"GITPM_CATALOG": b})
    t.ok("catalog" not in t.package(YQ), "no catalog recorded from an unverified catalog")
    t.has(t.gitpm_ok("check", env={"GITPM_CATALOG": b}).out, f"Catalog: {b} (unverified: discovery only)")
    t.release(YQ, "1.1.0")
    t.gitpm_ok("update", YQ)
    t.eq(t.package(YQ)["current"]["version"], "1.1.0", "unprotected: follows the origin")


@case(fixture="g4_two_catalogs", spec=["catalog/verified-catalogs#bulk-generation-refused", "catalog/verified-catalogs#register-into-a-verified-catalog", "catalog/verified-catalogs#overview-shows-the-flag"])
def catalog_update_refused_for_a_verified_catalog(t: T):
    """8. catalog-update into verified A is refused (versions enter only through register) and
    changes no file; register keeps the flag."""
    a = cat(t, "catA")
    (t.root / "sources.txt").write_text(f"{t.repo(XP)}\n")
    before = t.snap(a)
    t.gitpm_err("--catalog", a, "catalog-update", kind="invalid_argument",
                contains="a verified catalog takes versions only one by one through register")
    t.same(before, "catalog A")
    t.gitpm_ok("--catalog", a, "register", t.repo(XP), "--ref", "v1.1.0")
    t.eq(json.loads((a / "catalog.json").read_text()).get("verified"), True, "flag kept by register")
    t.has(t.gitpm_ok("--catalog", a, "info").out, "Verified: yes")


@case(fixture="g4_two_catalogs", spec=["updates/protected-packages-update-through-their-catalog#detach", "updates/protected-packages-update-through-their-catalog#newer-tag-not-in-the-catalog", "catalog/check-compares-installed-packages-with-their-catalog-or-origin#protected-package-with-a-newer-upstream", "catalog/check-compares-installed-packages-with-their-catalog-or-origin#verified-or-not"])
def protected_update_check_and_detach(t: T):
    """9. Protected c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5: update all keeps it; check uses the listed catalog release
    without querying origin; --detach takes the origin's 1.1.0."""
    install_from_a(t)
    a = str(cat(t, "catA"))
    r = t.gitpm_ok("update", "all")
    t.has(r.out, "c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5 kept at 1.0.0 (upstream 1.1.0 not in catalog)")
    out = readonly(t, lambda: t.gitpm_ok("check", env={"GITPM_CATALOG": a}).out)
    t.has(out, f"Catalog: {a} (verified)")
    t.has(row(out, XP), "up to date", "catalog")
    t.lacks(row(out, XP), "1.1.0")
    t.gitpm_err("update", XP, "--ref", "v1.1.0", kind="invalid_argument", contains="detach it")
    r = t.gitpm_ok("update", XP, "--detach")
    t.has(r.out, "c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5 detached from catalog", "c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5 updated 1.0.0 → 1.1.0")
    p = t.package(XP)
    t.ok("catalog" not in p, "catalog dropped")
    t.eq(p["current"], {"version": "1.1.0", "commit": t.rev(XP, "v1.1.0")}, "origin release")


@case(fixture="g4_two_catalogs", spec=["updates/protected-packages-update-through-their-catalog#hash-mismatch-on-update"])
def tampered_release_refused_on_update(t: T):
    """10. A lists c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5 1.1.0 with file hashes that do not match the tagged dist: update and
    update all fail with `catalog` and the workspace is unchanged."""
    install_from_a(t)
    a = cat(t, "catA")
    t.gitpm_ok("--catalog", a, "register", t.repo(XP), "--ref", "v1.1.0")
    def tamper(release):
        for digest in release["files"].values():
            digest["sha256"] = "0" * 64
    t.edit_json(t.catalog_release(XP, "1.1.0", a), tamper)
    before = t.snap(t.ws)
    t.gitpm_err("update", XP, kind="catalog", contains="catalog file hashes do not match git dist of c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5@1.1.0")
    t.gitpm_err("update", "all", kind="catalog", contains="catalog file hashes do not match git dist")
    t.same(before, "workspace")
    t.eq(t.package(XP)["current"]["version"], "1.0.0", "still 1.0.0")
