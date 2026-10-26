"""UC-035: Check statuses and selective HTTP reads (P0).

The fixture catalog is served over loopback HTTP with request logging; every status of
check.rs (CheckStatus) that a workspace can reach is covered by its own case.
"""

import shutil

from gitpmtest import T, case

HELLO, NUM = "3dce1568-569c-5b53-9321-37185e751947", "2190eee8-4be5-5a91-81d2-6ca95c1e61d0"


def status(t: T, out: str, id: str) -> str:
    for line in out.splitlines():
        cells = line.split()
        if cells and cells[0] == id:
            return line
    raise AssertionError(f"no check row for {id} in:\n{out}")


def check(t: T, url: str, id: str, expected: str) -> str:
    """`check` with the catalog at `url`: the row of `id` says `expected`, nothing changes."""
    before = t.snap(t.ws)
    out = t.gitpm_ok("--catalog", url, "check").out
    t.has(status(t, out, id), expected)
    t.same(before, "workspace (check never updates)")
    return out


@case(fixture="catalog", spec=["catalog/check-compares-installed-packages-with-their-catalog-or-origin#cli-table"])
def up_to_date(t: T):
    srv = t.serve(t.catalog)
    t.gitpm_ok("--catalog", srv.url, "install-catalog", HELLO)
    check(t, srv.url, HELLO, "up to date")


@case(fixture="catalog", spec=["catalog/check-compares-installed-packages-with-their-catalog-or-origin", "catalog/catalog-is-a-tree-of-static-files#direct-lookup"])
def update_available_reads_only_installed_plugins(t: T):
    """Two installed packages; requests go to their index and current/latest releases only, plus
    the top-level catalog.json for the verified flag (a broken one reads as unverified)."""
    srv = t.serve(t.catalog)
    t.gitpm_ok("--catalog", srv.url, "install-catalog", NUM, "--version", "1.9.0")
    t.gitpm_ok("--catalog", srv.url, "install-catalog", HELLO, "--version", "0.1.1")
    srv.take()
    out = check(t, srv.url, NUM, "update available")
    t.has(status(t, out, HELLO), "update available")
    requests = srv.take()
    print(f"  {len(requests)} requests: {requests}")
    allowed = {"/packages/2190eee8-4be5-5a91-81d2-6ca95c1e61d0/plugin.json", "/packages/2190eee8-4be5-5a91-81d2-6ca95c1e61d0/versions/1.9.0/release.json",
               "/packages/2190eee8-4be5-5a91-81d2-6ca95c1e61d0/versions/1.10.0/release.json",
               "/packages/3dce1568-569c-5b53-9321-37185e751947/plugin.json", "/packages/3dce1568-569c-5b53-9321-37185e751947/versions/0.1.1/release.json",
               "/packages/3dce1568-569c-5b53-9321-37185e751947/versions/0.2.0/release.json", "/catalog.json"}
    t.eq(sorted(set(requests) - allowed), sorted([]), "requests outside the installed packages")
    t.ok("/packages/3dce1568-569c-5b53-9321-37185e751947/versions/0.1.0/release.json" not in requests, "unrelated release not read")

    # Unrelated directory indexes broken: same statuses, same requests.
    for index in [t.catalog / "catalog.json", t.catalog / "packages/catalog.json"]:
        index.write_text("{ broken")
    again = check(t, srv.url, NUM, "update available")
    t.has(status(t, again, HELLO), "update available")
    t.eq(sorted(set(srv.take())), sorted(set(requests)), "requests after breaking directory indexes")


@case(fixture="catalog", spec=["catalog/check-compares-installed-packages-with-their-catalog-or-origin#statuses"])
def not_in_catalog_reads_the_origin(t: T):
    """A package the catalog does not list gets an origin row from its recorded source:
    up to date, then update available once the origin has a newer release tag."""
    srv = t.serve(t.catalog)
    t.new_package("b1b5f31e-7a3f-5312-baf6-10254d3f2a29", "0.1.0")
    t.gitpm_ok("install", t.repo("b1b5f31e-7a3f-5312-baf6-10254d3f2a29"))
    t.has(status(t, check(t, srv.url, "b1b5f31e-7a3f-5312-baf6-10254d3f2a29", "up to date"), "b1b5f31e-7a3f-5312-baf6-10254d3f2a29"), "origin")
    t.release("b1b5f31e-7a3f-5312-baf6-10254d3f2a29", "0.2.0")
    row = status(t, check(t, srv.url, "b1b5f31e-7a3f-5312-baf6-10254d3f2a29", "update available"), "b1b5f31e-7a3f-5312-baf6-10254d3f2a29")
    t.has(row, "origin", "0.2.0")


@case(fixture="catalog", spec=["catalog/check-compares-installed-packages-with-their-catalog-or-origin#statuses"])
def origin_statuses(t: T):
    """Origin rows: newer than origin (installed past the last tag), no releases on origin
    (tags gone), origin unreachable (repository moved) — each only its own row, check exits 0."""
    srv = t.serve(t.catalog)
    t.new_package("0005e714-a4fd-54f9-9660-55a30d568b9c", "0.1.0")
    t.release("0005e714-a4fd-54f9-9660-55a30d568b9c", "0.2.0")
    t.gitpm_ok("install", t.repo("0005e714-a4fd-54f9-9660-55a30d568b9c"))
    t.git("0005e714-a4fd-54f9-9660-55a30d568b9c", "tag", "-d", "v0.2.0")
    check(t, srv.url, "0005e714-a4fd-54f9-9660-55a30d568b9c", "newer than origin")
    t.git("0005e714-a4fd-54f9-9660-55a30d568b9c", "tag", "-d", "v0.1.0")
    check(t, srv.url, "0005e714-a4fd-54f9-9660-55a30d568b9c", "no releases on origin")
    t.gitpm_ok("--catalog", srv.url, "install-catalog", HELLO)
    shutil.move(t.repo("0005e714-a4fd-54f9-9660-55a30d568b9c"), t.root / "moved")
    out = check(t, srv.url, "0005e714-a4fd-54f9-9660-55a30d568b9c", "origin unreachable")
    t.has(status(t, out, HELLO), "catalog", "up to date")


@case(fixture="catalog", spec=["catalog/check-compares-installed-packages-with-their-catalog-or-origin"])
def newer_than_catalog(t: T):
    """Installed from the catalog, then updated from Git past the last registered release."""
    srv = t.serve(t.catalog)
    t.gitpm_ok("--catalog", srv.url, "install-catalog", HELLO)
    t.release(HELLO, "0.3.0")
    t.gitpm_ok("update", HELLO)
    check(t, srv.url, HELLO, "newer than catalog")


@case(fixture="catalog", spec=["catalog/check-compares-installed-packages-with-their-catalog-or-origin#identity-mismatch"])
def same_version_different_commit(t: T):
    """A direct install (release mode) of another commit that still says 0.1.1: the Git
    tag 0.1.1 moved after the catalog registered it, and 0.2.0 is gone from Git."""
    srv = t.serve(t.catalog)
    t.git(HELLO, "checkout", "-q", "-b", "side", f"v0.1.1")
    t.commit(HELLO, "same version, other code", {"dist/index.js": "side\n"})
    t.tag(HELLO, f"v0.1.1", force=True)
    t.git(HELLO, "tag", "-d", f"v0.2.0")
    t.gitpm_ok("install", t.repo(HELLO))
    check(t, srv.url, HELLO, "same version, different commit")


@case(fixture="catalog", spec=["catalog/check-compares-installed-packages-with-their-catalog-or-origin#same-id-in-an-unrelated-source"])
def different_source(t: T):
    """An unrelated catalog source for the same ID falls back to the installed origin."""
    srv = t.serve(t.catalog)
    t.run("git", "clone", "-q", t.repo(HELLO), t.root / "790a24b3-6ad7-5a64-8632-0c229695164c", check=True)
    t.gitpm_ok("install", t.root / "790a24b3-6ad7-5a64-8632-0c229695164c")
    out = check(t, srv.url, HELLO, "up to date")
    t.has(status(t, out, HELLO), "origin")


@case(fixture="catalog", spec=["catalog/check-compares-installed-packages-with-their-catalog-or-origin#other-catalog"])
def installed_from_other_catalog(t: T):
    """Installed from the local (verified) catalog path, checked against its HTTP copy. An older
    install is not compared ("up to date" still wins for the same release, UC-057)."""
    t.edit_json(t.catalog / "catalog.json", lambda c: c.update(verified=True))
    srv = t.serve(t.catalog)
    t.gitpm_ok("install-catalog", HELLO, "--version", "0.1.1")
    check(t, srv.url, HELLO, "installed from another catalog")


@case(fixture="catalog", spec=["catalog/check-compares-installed-packages-with-their-catalog-or-origin"])
def no_versions(t: T):
    served = t.root / "served"
    shutil.copytree(t.catalog, served)
    srv = t.serve(served)
    t.gitpm_ok("--catalog", srv.url, "install-catalog", HELLO)
    t.edit_json(t.catalog_index(HELLO, served), lambda p: p.update(versions={}))
    check(t, srv.url, HELLO, "no versions in catalog")


@case(fixture="catalog", spec=["catalog/check-compares-installed-packages-with-their-catalog-or-origin#broken-payload"])
def invalid_catalog_data(t: T):
    served = t.root / "served"
    shutil.copytree(t.catalog, served)
    srv = t.serve(served)
    t.gitpm_ok("--catalog", srv.url, "install-catalog", HELLO)
    t.edit_json(t.catalog_release(HELLO, "0.2.0", served), lambda r: r["manifest"].update(version="9.9.9"))
    check(t, srv.url, HELLO, "invalid catalog data")


@case(fixture="catalog", spec=["catalog/check-compares-installed-packages-with-their-catalog-or-origin#broken-payload"])
def invalid_catalog_data_local(t: T):
    """The same mismatch in a local catalog folder."""
    t.gitpm_ok("install-catalog", HELLO)
    t.edit_json(t.catalog_release(HELLO, "0.2.0"), lambda r: r["manifest"].update(version="9.9.9"))
    check(t, str(t.catalog), HELLO, "invalid catalog data")


@case(fixture="catalog", spec=["catalog/check-compares-installed-packages-with-their-catalog-or-origin#broken-payload"])
def unreadable_release_is_an_error(t: T):
    """Malformed JSON of an installed package's release fails check (not "no updates")."""
    served = t.root / "served"
    shutil.copytree(t.catalog, served)
    srv = t.serve(served)
    t.gitpm_ok("--catalog", srv.url, "install-catalog", HELLO)
    t.catalog_release(HELLO, "0.2.0", served).write_text("{ not json")
    t.gitpm_err("--catalog", srv.url, "check", kind="catalog")
