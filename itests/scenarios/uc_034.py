"""UC-034: Info and catalog read-only browsing (P1).

Fixtures: g4_tree_catalog (v2 with namespaces/), g4_v1 (monolithic v1, refused).
`info` shows the one catalog given by --catalog or GITPM_CATALOG (there is no remote or
public fallback); `catalog` prints the directory links of one level as JSON.
"""

import json
from pathlib import Path

from gitpmtest import T, case


def gitpm_env(t: T, *args, env: dict | None = None, unset=(), ok=True, **kw):
    """gitpm with variables removed (env -u) and added."""
    argv = ["env", *[a for u in unset if u not in (env or {}) for a in ("-u", u)], "gitpm", *args]
    r = t.run(*argv, env=env, **kw)
    if ok and r.code != 0:
        raise AssertionError(f"gitpm {args} exited {r.code}\n{r.err}")
    return r


def readonly(t: T, fn, *paths):
    before = t.snap(t.ws, t.catalog, *paths)
    result = fn()
    t.same(before, "workspace and catalogs (read-only)")
    t.ok(not t.ws.exists(), "no workspace created")
    return result


def line_after(out: str, prefix: str) -> str:
    for line in out.splitlines():
        if line.startswith(prefix):
            return line[len(prefix):].strip()
    raise AssertionError(f"no line starting with {prefix!r} in:\n{out}")


def assert_packages(t: T, out: str):
    t.has(out, "dan/hello — latest: 0.1.1, versions: 2", f"Git: {t.repo('dan/hello')}",
          "acme/widget — latest: 1.1.0, versions: 2", f"Git: {t.repo('acme/widget')}",
          "old/oldie — latest: 0.2.0, versions: 2", f"Git: {t.repo('old/oldie')}")


@case(fixture="g4_tree_catalog", spec=["catalog/catalog-v2-is-a-tree-of-static-files",
                                      "cli/settings-come-from-flags-environment-defaults", "catalog/bounded-overview#overview-lines"])
def info_local_root(t: T):
    """`info` on the local v2 root (from GITPM_CATALOG): catalog address, local type,
    directories, each package's ID, latest, version count and Git source."""
    out = readonly(t, lambda: gitpm_env(t, "info").out)
    t.eq(line_after(out, "Catalog:"), str(t.catalog), "catalog address")
    t.eq(line_after(out, "Type:"), "local folder/JSON", "type")
    t.has(out, "Directories and packages:", "namespaces/", "acme/", "dan/", "old/")
    t.lacks(out, "Format:", "legacy/")
    assert_packages(t, out)
    t.lacks(out, "Overview limit")


@case(fixture="g4_tree_catalog", spec=["catalog/catalog-v2-is-a-tree-of-static-files#direct-lookup"])
def info_branch_path(t: T):
    """`info` on <catalog>/namespaces shows only that branch; --catalog wins over GITPM_CATALOG."""
    out = readonly(t, lambda: gitpm_env(t, "--catalog", t.catalog / "namespaces", "info").out)
    t.eq(line_after(out, "Catalog:"), str(t.catalog / "namespaces"), "catalog address")
    t.has(out, "Directories and packages:", "dan/hello — latest: 0.1.1", "acme/widget — latest: 1.1.0")
    t.lacks(out, "legacy/")


@case(fixture="g4_tree_catalog", spec=["cli/json-output-for-state-and-catalog",
                                      "catalog/catalog-v2-is-a-tree-of-static-files"])
def catalog_prints_current_level_only(t: T):
    """`catalog` prints only the directory links of the given level (JSON)."""
    root = readonly(t, lambda: gitpm_env(t, "--catalog", t.catalog, "catalog").json())
    t.eq(root, {"schema": 2, "directories": {"namespaces": "namespaces/"}}, "root level")
    branch = readonly(t, lambda: gitpm_env(t, "--catalog", t.catalog / "namespaces", "catalog").json())
    t.eq(branch, {"schema": 2, "directories": {"acme": "acme/", "dan": "dan/", "old": "old/"}}, "namespaces level")
    leaf = readonly(t, lambda: gitpm_env(t, "--catalog", t.catalog / "namespaces/dan", "catalog").json())
    t.eq(leaf, {"schema": 2, "directories": {"hello": "hello/"}}, "dan level")


@case(fixture="g4_tree_catalog", spec=["catalog/catalog-locations-and-transport"])
def info_and_catalog_over_loopback_http(t: T):
    """The same catalog over http://127.0.0.1: root and branch, info and catalog."""
    srv = t.serve(t.catalog)
    out = readonly(t, lambda: gitpm_env(t, "--catalog", srv.url, "info").out)
    t.eq(line_after(out, "Catalog:"), srv.url, "catalog address")
    t.has(out, "remote (HTTP)", "Directories and packages:")
    assert_packages(t, out)
    branch = readonly(t, lambda: gitpm_env(t, "--catalog", srv.url + "/namespaces/dan", "info").out)
    t.has(branch, "dan/hello — latest: 0.1.1")
    t.lacks(branch, "oldie")
    t.eq(readonly(t, lambda: gitpm_env(t, "--catalog", srv.url, "catalog").json()),
         {"schema": 2, "directories": {"namespaces": "namespaces/"}}, "catalog over HTTP")
    t.eq(readonly(t, lambda: gitpm_env(t, "--catalog", srv.url + "/namespaces/", "catalog").json())["directories"],
         {"acme": "acme/", "dan": "dan/", "old": "old/"}, "branch over HTTP")


@case(fixture="g4_v1", spec=["catalog/catalog-v2-is-a-tree-of-static-files"])
def info_v1_is_refused(t: T):
    """`info` on a v1 catalog fails with `catalog` and reads it as data only; `catalog`
    still prints the raw root JSON."""
    v1 = t.root / "v1"
    r = readonly(t, lambda: gitpm_env(t, "--catalog", v1, "info", ok=False), v1)
    t.ok(r.code != 0, "info on v1 fails")
    t.has(r.err, "Error kind: catalog", "v1 catalogs")
    t.eq(sorted(readonly(t, lambda: gitpm_env(t, "--catalog", v1, "catalog").json(), v1)["plugins"]),
         ["dan/hello", "old/oldie"], "raw root of v1")


@case(fixture="g4_tree_catalog", spec=["cli/commands-that-need-a-catalog-say-so"])
def info_without_a_catalog_names_it(t: T):
    """Without --catalog and GITPM_CATALOG, `info` and `catalog` fail naming the missing
    catalog; nothing is created (there is no public fallback)."""
    for command in ["info", "catalog"]:
        r = readonly(t, lambda: gitpm_env(t, command, unset=("GITPM_CATALOG",), ok=False))
        t.ok(r.code != 0, f"{command} without a catalog fails")
        t.has(r.err, f"{command} needs a catalog: pass --catalog DIR_OR_JSON_OR_URL or set GITPM_CATALOG")


def crafted_v2(root: Path, count: int):
    root.mkdir(parents=True)
    (root / "catalog.json").write_text(json.dumps({"schema": 2, "directories": {"namespaces": "namespaces/"}}))
    dirs = {f"n{i:02}": f"n{i:02}/" for i in range(count)}
    (root / "namespaces").mkdir()
    (root / "namespaces/catalog.json").write_text(json.dumps({"schema": 2, "directories": dirs}))
    for name in dirs:
        (root / "namespaces" / name).mkdir()
        (root / "namespaces" / name / "catalog.json").write_text(
            json.dumps({"schema": 2, "directories": {"p": "p/"}}))
        (root / "namespaces" / name / "p").mkdir()
        (root / "namespaces" / name / "p/plugin.json").write_text(json.dumps(
            {"schema": 2, "id": f"{name}/p", "source": f"https://git.example.invalid/{name}/p.git",
             "versions": {"1.0.0": "versions/1.0.0/release.json"}}))


@case(fixture="g4_tree_catalog", spec=["catalog/bounded-overview#large-catalog"])
def overview_limit_v2(t: T):
    """More than 40 entries in a v2 tree: output stops at 40 and prints the limit hint."""
    big = t.root / "big"
    crafted_v2(big, 45)
    out = readonly(t, lambda: gitpm_env(t, "--catalog", big, "info").out, big)
    t.has(out, "Overview limit: 40 entries")
    entries = [l for l in out.splitlines() if l.strip().endswith("/")]
    t.eq(len(entries), 40, "directory entries shown")
    t.lacks(out, "n44/p")


@case(fixture="g4_tree_catalog", spec=["catalog/catalog-locations-and-transport#plain-http-to-a-remote-host"])
def non_loopback_http_refused(t: T):
    """http:// to a non-loopback host is refused (HTTPS only) for info and catalog; nothing read."""
    for url in ["http://pkg.example.invalid", "http://localhost.evil.example/", "http://127.0.0.1@evil.example/"]:
        for command in ["info", "catalog"]:
            r = readonly(t, lambda: gitpm_env(t, "--catalog", url, command, ok=False))
            t.ok(r.code != 0, f"{command} {url} refused")
            t.has(r.err, "Error kind: invalid_argument", "HTTPS")
    srv = t.serve(t.catalog)
    t.has(readonly(t, lambda: gitpm_env(t, "--catalog", srv.url.replace("127.0.0.1", "localhost"), "info").out),
          "Directories and packages:")
