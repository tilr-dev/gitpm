"""UC-034: Info and catalog read-only browsing (P1).

Fixtures: g4_tree_catalog (v2 with packages/), g4_v1 (monolithic v1, refused).
`info` shows the one catalog given by --catalog or GITPM_CATALOG (there is no remote or
public fallback); `catalog` prints the directory links of one level as JSON.
"""

import json
import uuid
from pathlib import Path

from gitpmtest import T, case, gitpm_argv


def gitpm_env(t: T, *args, env: dict | None = None, unset=(), ok=True, **kw):
    """gitpm with variables removed (env -u) and added."""
    argv = ["env", *[a for u in unset if u not in (env or {}) for a in ("-u", u)], "gitpm", *gitpm_argv(args)]
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
    t.has(out, "3dce1568-569c-5b53-9321-37185e751947 — latest: 0.1.1, versions: 2", f"Git: {t.repo('3dce1568-569c-5b53-9321-37185e751947')}",
          "41b88ba0-f9f1-5e65-acb6-3dc95231e0bc — latest: 1.1.0, versions: 2", f"Git: {t.repo('41b88ba0-f9f1-5e65-acb6-3dc95231e0bc')}",
          "3cfe3093-9e5f-5f64-b4d1-0a169cc3bdf6 — latest: 0.2.0, versions: 2", f"Git: {t.repo('3cfe3093-9e5f-5f64-b4d1-0a169cc3bdf6')}")


@case(fixture="g4_tree_catalog", spec=["catalog/catalog-is-a-tree-of-static-files",
                                      "cli/settings-come-from-flags-environment-defaults", "catalog/bounded-overview#overview-lines"])
def info_local_root(t: T):
    """`info` on the local v2 root (from GITPM_CATALOG): catalog address, local type,
    directories, each package's ID, latest, version count and Git source."""
    out = readonly(t, lambda: gitpm_env(t, "info").out)
    t.eq(line_after(out, "Catalog:"), str(t.catalog), "catalog address")
    t.eq(line_after(out, "Type:"), "local folder/JSON", "type")
    t.has(out, "Directories and packages:", "packages/", '3dce1568-569c-5b53-9321-37185e751947/', '41b88ba0-f9f1-5e65-acb6-3dc95231e0bc/', '3cfe3093-9e5f-5f64-b4d1-0a169cc3bdf6/')
    t.lacks(out, "Format:", "legacy/")
    assert_packages(t, out)
    t.lacks(out, "Overview limit")


@case(fixture="g4_tree_catalog", spec=["catalog/catalog-is-a-tree-of-static-files#direct-lookup"])
def info_branch_path(t: T):
    """`info` on <catalog>/namespaces shows only that branch; --catalog wins over GITPM_CATALOG."""
    out = readonly(t, lambda: gitpm_env(t, "--catalog", t.catalog / "packages", "info").out)
    t.eq(line_after(out, "Catalog:"), str(t.catalog / "packages"), "catalog address")
    t.has(out, "Directories and packages:", "3dce1568-569c-5b53-9321-37185e751947 — latest: 0.1.1", "41b88ba0-f9f1-5e65-acb6-3dc95231e0bc — latest: 1.1.0")
    t.lacks(out, "legacy/")


@case(fixture="g4_tree_catalog", spec=["cli/json-output-for-state-and-catalog",
                                      "catalog/catalog-is-a-tree-of-static-files"])
def catalog_prints_current_level_only(t: T):
    """`catalog` prints only the directory links of the given level (JSON)."""
    root = readonly(t, lambda: gitpm_env(t, "--catalog", t.catalog, "catalog").json())
    t.eq(root, {"schema": 1, "directories": {"packages": "packages/"}}, "root level")
    branch = readonly(t, lambda: gitpm_env(t, "--catalog", t.catalog / "packages", "catalog").json())
    t.eq(branch, {"schema": 1, "directories": {'3dce1568-569c-5b53-9321-37185e751947': '3dce1568-569c-5b53-9321-37185e751947/', '41b88ba0-f9f1-5e65-acb6-3dc95231e0bc': '41b88ba0-f9f1-5e65-acb6-3dc95231e0bc/', '3cfe3093-9e5f-5f64-b4d1-0a169cc3bdf6': '3cfe3093-9e5f-5f64-b4d1-0a169cc3bdf6/'}}, "namespaces level")



@case(fixture="g4_tree_catalog", spec=["catalog/catalog-locations-and-transport"])
def info_and_catalog_over_loopback_http(t: T):
    """The same catalog over http://127.0.0.1: root and branch, info and catalog."""
    srv = t.serve(t.catalog)
    out = readonly(t, lambda: gitpm_env(t, "--catalog", srv.url, "info").out)
    t.eq(line_after(out, "Catalog:"), srv.url, "catalog address")
    t.has(out, "remote (HTTP)", "Directories and packages:")
    assert_packages(t, out)
    branch = readonly(t, lambda: gitpm_env(t, "--catalog", srv.url + "/packages/", "info").out)
    assert_packages(t, branch)
    t.eq(readonly(t, lambda: gitpm_env(t, "--catalog", srv.url, "catalog").json()),
         {"schema": 1, "directories": {"packages": "packages/"}}, "catalog over HTTP")
    t.eq(readonly(t, lambda: gitpm_env(t, "--catalog", srv.url + "/packages/", "catalog").json())["directories"],
         {'3dce1568-569c-5b53-9321-37185e751947': '3dce1568-569c-5b53-9321-37185e751947/', '41b88ba0-f9f1-5e65-acb6-3dc95231e0bc': '41b88ba0-f9f1-5e65-acb6-3dc95231e0bc/', '3cfe3093-9e5f-5f64-b4d1-0a169cc3bdf6': '3cfe3093-9e5f-5f64-b4d1-0a169cc3bdf6/'}, "branch over HTTP")


@case(fixture="g4_v1", spec=["catalog/catalog-is-a-tree-of-static-files"])
def info_v1_is_refused(t: T):
    """`info` and `catalog` on a v1 catalog fail with `catalog` and read it as data only (the library's
    typed v2 links are all `catalog` prints)."""
    v1 = t.root / "v1"
    for command in ("info", "catalog"):
        r = readonly(t, lambda: gitpm_env(t, "--catalog", v1, command, ok=False), v1)
        t.ok(r.code != 0, f"{command} on v1 fails")
        t.has(r.err, "Error kind: catalog", "v1 catalogs")


@case(fixture="g4_tree_catalog", spec=["cli/commands-that-need-a-catalog-say-so"])
def info_without_a_catalog_names_it(t: T):
    """Without --catalog and GITPM_CATALOG, `info` and `catalog` fail naming the missing
    catalog; nothing is created (there is no public fallback)."""
    for command in ["info", "catalog"]:
        r = readonly(t, lambda: gitpm_env(t, command, unset=("GITPM_CATALOG",), ok=False))
        t.ok(r.code != 0, f"{command} without a catalog fails")
        t.has(r.err, f"{' '.join(gitpm_argv([command]))} needs a catalog: pass --catalog DIR_OR_JSON_OR_URL or set GITPM_CATALOG")


def crafted_v2(root: Path, count: int):
    root.mkdir(parents=True)
    (root / "catalog.json").write_text(json.dumps({"schema": 1, "directories": {"packages": "packages/"}}))
    dirs = {f"n{i:02}": f"n{i:02}/" for i in range(count)}
    (root / "packages").mkdir()
    (root / "packages/catalog.json").write_text(json.dumps({"schema": 1, "directories": dirs}))
    for name in dirs:
        (root / "packages" / name).mkdir()
        (root / "packages" / name / "catalog.json").write_text(
            json.dumps({"schema": 1, "directories": {"p": "p/"}}))
        (root / "packages" / name / "p").mkdir()
        (root / "packages" / name / "p/plugin.json").write_text(json.dumps(
            {"schema": 1, "id": str(uuid.UUID(int=int(name[1:]) + 1)), "source": f"https://git.example.invalid/{name}/p.git",
             "versions": {"1.0.0": "versions/1.0.0/release.json"}}))


@case(fixture="g4_tree_catalog", spec=["catalog/bounded-overview#large-catalog"])
def overview_limit_v2(t: T):
    """More than 40 entries in a catalog tree: output stops at 40 and prints the limit hint."""
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
