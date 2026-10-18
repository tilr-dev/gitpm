"""UC-035: Check statuses and selective HTTP reads (P0).

The fixture catalog is served over loopback HTTP with request logging; every status of
check.rs (CheckStatus) that a workspace can reach is covered by its own case.
"""

import shutil

from gitpmtest import T, case

HELLO, NUM = "dan/hello", "dan/num"


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


@case(fixture="catalog", spec=["catalog/check-compares-installed-with-the-catalog#cli-table"])
def up_to_date(t: T):
    srv = t.serve(t.catalog)
    t.gitpm_ok("--catalog", srv.url, "install-catalog", HELLO)
    check(t, srv.url, HELLO, "up to date")


@case(fixture="catalog", spec=["catalog/check-compares-installed-with-the-catalog", "catalog/catalog-v2-is-a-tree-of-static-files#direct-lookup"])
def update_available_reads_only_installed_plugins(t: T):
    """Two installed packages; requests go to their index and current/latest releases only."""
    srv = t.serve(t.catalog)
    t.gitpm_ok("--catalog", srv.url, "install-catalog", NUM, "--version", "1.9.0")
    t.gitpm_ok("--catalog", srv.url, "install-catalog", HELLO, "--version", "0.1.1")
    srv.take()
    out = check(t, srv.url, NUM, "update available")
    t.has(status(t, out, HELLO), "update available")
    requests = srv.take()
    print(f"  {len(requests)} requests: {requests}")
    allowed = {"/namespaces/dan/num/plugin.json", "/namespaces/dan/num/versions/1.9.0/release.json",
               "/namespaces/dan/num/versions/1.10.0/release.json",
               "/namespaces/dan/hello/plugin.json", "/namespaces/dan/hello/versions/0.1.1/release.json",
               "/namespaces/dan/hello/versions/0.2.0/release.json"}
    t.eq(sorted(set(requests) - allowed), [], "requests outside the installed packages")
    t.ok("/namespaces/dan/hello/versions/0.1.0/release.json" not in requests, "unrelated release not read")

    # Unrelated directory indexes broken: same statuses, same requests.
    for index in [t.catalog / "catalog.json", t.catalog / "namespaces/catalog.json"]:
        index.write_text("{ broken")
    again = check(t, srv.url, NUM, "update available")
    t.has(status(t, again, HELLO), "update available")
    t.eq(sorted(set(srv.take())), sorted(set(requests)), "requests after breaking directory indexes")


@case(fixture="catalog", spec=["catalog/check-compares-installed-with-the-catalog#statuses"])
def not_in_catalog_reads_the_origin(t: T):
    """A package the catalog does not list gets an origin row from its recorded source:
    up to date, then update available once the origin has a newer release tag."""
    srv = t.serve(t.catalog)
    t.new_package("dan/extra", "0.1.0")
    t.gitpm_ok("install", t.repo("dan/extra"))
    t.has(status(t, check(t, srv.url, "dan/extra", "up to date"), "dan/extra"), "origin")
    t.release("dan/extra", "0.2.0")
    row = status(t, check(t, srv.url, "dan/extra", "update available"), "dan/extra")
    t.has(row, "origin", "0.2.0")


@case(fixture="catalog", spec=["catalog/check-compares-installed-with-the-catalog#statuses"])
def origin_statuses(t: T):
    """Origin rows: newer than origin (installed past the last tag), no releases on origin
    (tags gone), origin unreachable (repository moved) — each only its own row, check exits 0."""
    srv = t.serve(t.catalog)
    t.new_package("dan/ahead", "0.1.0")
    t.release("dan/ahead", "0.2.0")
    t.gitpm_ok("install", t.repo("dan/ahead"))
    t.git("dan/ahead", "tag", "-d", "dan/ahead@0.2.0")
    check(t, srv.url, "dan/ahead", "newer than origin")
    t.git("dan/ahead", "tag", "-d", "dan/ahead@0.1.0")
    check(t, srv.url, "dan/ahead", "no releases on origin")
    t.gitpm_ok("--catalog", srv.url, "install-catalog", HELLO)
    shutil.move(t.repo("dan/ahead"), t.root / "moved")
    out = check(t, srv.url, "dan/ahead", "origin unreachable")
    t.has(status(t, out, HELLO), "catalog", "up to date")


@case(fixture="catalog", spec=["catalog/check-compares-installed-with-the-catalog"])
def newer_than_catalog(t: T):
    """Installed from the catalog, then updated from Git past the last registered release."""
    srv = t.serve(t.catalog)
    t.gitpm_ok("--catalog", srv.url, "install-catalog", HELLO)
    t.release(HELLO, "0.3.0")
    t.gitpm_ok("update", HELLO)
    check(t, srv.url, HELLO, "newer than catalog")


@case(fixture="catalog", spec=["catalog/check-compares-installed-with-the-catalog#identity-mismatch"])
def same_version_different_commit(t: T):
    """A direct install of another commit that still says 0.1.1."""
    srv = t.serve(t.catalog)
    t.git(HELLO, "checkout", "-q", "-b", "side", f"{HELLO}@0.1.1")
    sha = t.commit(HELLO, "same version, other code", {"dist/index.js": "side\n"})
    t.gitpm_ok("install", t.repo(HELLO), "--ref", sha)
    check(t, srv.url, HELLO, "same version, different commit")


@case(fixture="catalog", spec=["catalog/check-compares-installed-with-the-catalog#identity-mismatch"])
def different_source(t: T):
    """A direct install from a mirror path: the catalog source differs literally."""
    srv = t.serve(t.catalog)
    t.run("git", "clone", "-q", t.repo(HELLO), t.root / "mirror/hello", check=True)
    t.gitpm_ok("install", t.root / "mirror/hello", "--ref", f"{HELLO}@0.2.0")
    check(t, srv.url, HELLO, "different source in catalog")


@case(fixture="catalog", spec=["catalog/check-compares-installed-with-the-catalog#other-catalog"])
def installed_from_other_catalog(t: T):
    """Installed from the local catalog path, checked against its HTTP copy. An older
    install is not compared ("up to date" still wins for the same release, UC-057)."""
    srv = t.serve(t.catalog)
    t.gitpm_ok("install-catalog", HELLO, "--version", "0.1.1")
    check(t, srv.url, HELLO, "installed from another catalog")


@case(fixture="catalog", spec=["catalog/check-compares-installed-with-the-catalog"])
def no_versions(t: T):
    served = t.root / "served"
    shutil.copytree(t.catalog, served)
    srv = t.serve(served)
    t.gitpm_ok("--catalog", srv.url, "install-catalog", HELLO)
    t.edit_json(t.catalog_index(HELLO, served), lambda p: p.update(versions={}))
    check(t, srv.url, HELLO, "no versions in catalog")


@case(fixture="catalog", spec=["catalog/check-compares-installed-with-the-catalog#broken-payload"])
def invalid_catalog_data(t: T):
    served = t.root / "served"
    shutil.copytree(t.catalog, served)
    srv = t.serve(served)
    t.gitpm_ok("--catalog", srv.url, "install-catalog", HELLO)
    t.edit_json(t.catalog_release(HELLO, "0.2.0", served), lambda r: r["manifest"].update(version="9.9.9"))
    check(t, srv.url, HELLO, "invalid catalog data")


@case(fixture="catalog", spec=["catalog/check-compares-installed-with-the-catalog#broken-payload"])
def invalid_catalog_data_local(t: T):
    """The same mismatch in a local catalog folder."""
    t.gitpm_ok("install-catalog", HELLO)
    t.edit_json(t.catalog_release(HELLO, "0.2.0"), lambda r: r["manifest"].update(version="9.9.9"))
    check(t, str(t.catalog), HELLO, "invalid catalog data")


@case(fixture="catalog", spec=["catalog/check-compares-installed-with-the-catalog#broken-payload"])
def unreadable_release_is_an_error(t: T):
    """Malformed JSON of an installed package's release fails check (not "no updates")."""
    served = t.root / "served"
    shutil.copytree(t.catalog, served)
    srv = t.serve(served)
    t.gitpm_ok("--catalog", srv.url, "install-catalog", HELLO)
    t.catalog_release(HELLO, "0.2.0", served).write_text("{ not json")
    t.gitpm_err("--catalog", srv.url, "check", kind="catalog")
