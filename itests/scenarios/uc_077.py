"""UC-077: Partial publication seen by a consumer (P1).

A catalog is static files uploaded by the author in some order, so a consumer can read it
half-updated: a package index that already lists a version whose release.json is not there
yet, or an index that does not list a release that is already uploaded. Fixture `catalog`
(dan/hello 0.1.0, 0.1.1, 0.2.0; dan/num 1.9.0, 1.10.0) copied to /fx/served and read over
loopback HTTP. Every failing read must leave the workspace byte-identical and name a
catalog/http error; the same command succeeds once the missing file appears.
"""

import json
import shutil

from gitpmtest import T, case

HELLO = "dan/hello"


def served(t: T):
    root = t.root / "served"
    shutil.copytree(t.catalog, root)
    return root, t.serve(root)


def failed_kind(t: T, r, *kinds: str):
    t.ok(r.code != 0, f"exit {r.code} (expected a failure)")
    t.ok(any(f"Error kind: {k}" in r.err for k in kinds), f"error kind one of {kinds}:\n{r.err}")


def hide(path):
    """Move a published file away (not yet uploaded); returns the restore function."""
    hidden = path.with_name(path.name + ".pending")
    path.rename(hidden)
    return lambda: hidden.rename(path)


@case(fixture="catalog", spec=["catalog/catalog-locations-and-transport", "install-uninstall/install-from-the-catalog",
                               "errors/failing-operations-leave-the-workspace-unchanged"])
def index_lists_a_release_not_yet_uploaded(t: T):
    """The index lists 0.2.0 but versions/0.2.0/release.json is missing: install-catalog of
    the latest fails (catalog/http) and installs nothing; after the upload it installs 0.2.0."""
    root, srv = served(t)
    restore = hide(t.catalog_release(HELLO, "0.2.0", root))
    r = t.gitpm("--catalog", srv.url, "install-catalog", HELLO)
    failed_kind(t, r, "http", "catalog")
    t.not_installed(HELLO)
    t.ok(any("versions/0.2.0/release.json" in p for p in srv.take()), "the missing release was requested")
    restore()
    t.gitpm_ok("--catalog", srv.url, "install-catalog", HELLO)
    t.eq(t.package(HELLO)["current"], {"version": "0.2.0", "commit": t.rev(HELLO, f"v0.2.0")}, "installed")


@case(fixture="catalog", spec=["catalog/catalog-locations-and-transport", "install-uninstall/install-from-the-catalog"])
def exact_version_of_a_missing_release_fails_others_install(t: T):
    """With 0.2.0's release.json missing, --version 0.1.1 still installs (the missing file is
    never needed); --version 0.2.0 fails and leaves 0.1.1 installed unchanged."""
    root, srv = served(t)
    restore = hide(t.catalog_release(HELLO, "0.2.0", root))
    t.gitpm_ok("--catalog", srv.url, "install-catalog", HELLO, "--version", "0.1.1")
    before = t.snap(t.ws / "state.json", t.dist(HELLO))
    t.gitpm_ok("uninstall", HELLO, "--force")
    r = t.gitpm("--catalog", srv.url, "install-catalog", HELLO, "--version", "0.2.0")
    failed_kind(t, r, "http", "catalog")
    t.not_installed(HELLO)
    restore()
    t.gitpm_ok("--catalog", srv.url, "install-catalog", HELLO, "--version", "0.1.1")
    t.same(before, "state and dist reinstalled at 0.1.1")


@case(fixture="catalog", spec=["catalog/check-compares-installed-packages-with-their-catalog-or-origin#broken-payload"])
def check_with_a_missing_latest_release_fails_then_recovers(t: T):
    """dan/hello 0.1.1 installed from the served catalog; 0.2.0 listed but not uploaded:
    check fails (not "up to date" or "no updates") and changes nothing; once uploaded,
    check reports the update."""
    root, srv = served(t)
    t.gitpm_ok("--catalog", srv.url, "install-catalog", HELLO, "--version", "0.1.1")
    restore = hide(t.catalog_release(HELLO, "0.2.0", root))
    before = t.snap(t.ws, git=True)
    r = t.gitpm("--catalog", srv.url, "check")
    failed_kind(t, r, "http", "catalog")
    t.lacks(r.out, "Updates: 0.")
    t.same(before, "workspace after the failed check")
    restore()
    out = t.gitpm_ok("--catalog", srv.url, "check").out
    t.has(next(l for l in out.splitlines() if l.startswith(HELLO)), "update available")


@case(fixture="catalog", spec=["catalog/catalog-is-a-tree-of-static-files#direct-lookup",
                               "install-uninstall/install-from-the-catalog"])
def release_uploaded_before_the_index(t: T):
    """dan/hello 0.3.0 registered, then the old index is served (release.json uploaded, index
    not yet): latest is still 0.2.0 and --version 0.3.0 is not in the catalog; after the
    index is uploaded both see 0.3.0."""
    t.release(HELLO, "0.3.0")
    index = t.catalog_index(HELLO)
    old_index = index.read_bytes()
    t.gitpm_ok("register", t.repo(HELLO), "--ref", f"v0.3.0")
    root, srv = served(t)
    new_index = t.catalog_index(HELLO, root).read_bytes()
    t.catalog_index(HELLO, root).write_bytes(old_index)
    t.ok(t.catalog_release(HELLO, "0.3.0", root).is_file(), "release.json already uploaded")
    r = t.gitpm("--catalog", srv.url, "install-catalog", HELLO, "--version", "0.3.0")
    t.ok(r.code != 0, "0.3.0 is not in the served index yet")
    t.has(r.err, "0.3.0")
    t.not_installed(HELLO)
    t.gitpm_ok("--catalog", srv.url, "install-catalog", HELLO)
    t.eq(t.package(HELLO)["current"]["version"], "0.2.0", "latest per the old index")
    t.catalog_index(HELLO, root).write_bytes(new_index)
    out = t.gitpm_ok("--catalog", srv.url, "check").out
    t.has(next(l for l in out.splitlines() if l.startswith(HELLO)), "update available", "0.3.0")
    t.gitpm_ok("update", HELLO)
    t.eq(t.package(HELLO)["current"]["version"], "0.3.0", "update after the index appeared")


@case(fixture="catalog", spec=["catalog/catalog-locations-and-transport", "install-uninstall/install-from-the-catalog"])
def plugin_index_not_yet_uploaded(t: T):
    """A namespace catalog.json links a package whose plugin.json is missing: install-catalog
    of it fails naming the package; after the upload it installs."""
    root, srv = served(t)
    restore = hide(t.catalog_index("dan/num", root))
    r = t.gitpm("--catalog", srv.url, "install-catalog", "dan/num")
    t.ok(r.code != 0, "install of a package without an index fails")
    t.has(r.err, "dan/num")
    t.not_installed("dan/num")
    restore()
    t.gitpm_ok("--catalog", srv.url, "install-catalog", "dan/num")
    t.eq(t.package("dan/num")["current"]["version"], "1.10.0", "installed after the upload")
    t.eq(json.loads(t.catalog_index("dan/num", root).read_text())["id"], "dan/num", "index restored")
