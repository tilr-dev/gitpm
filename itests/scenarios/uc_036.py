"""UC-036: HTTP catalog errors and unsafe references (P0).

A copy of the `catalog` fixture is served over loopback HTTP by a local server that can
override single paths (status, headers, body) - gitpmtest's serve() cannot return a 301 or
a crafted body. dan/num 1.9.0 is installed from the served catalog first; every failing
read must leave that workspace byte-identical.
"""

import http.server
import json
import shutil
import socket
import threading
import time
from pathlib import Path

from gitpmtest import T, case

NUM, HELLO = "dan/num", "dan/hello"


class Server:
    """Static loopback server with per-path overrides: {path: (status, headers, body)}."""

    def __init__(self, directory: Path):
        self.overrides: dict[str, tuple[int, dict, bytes]] = {}
        self.requests: list[str] = []
        outer = self

        class Handler(http.server.SimpleHTTPRequestHandler):
            def __init__(self, *a, **kw):
                super().__init__(*a, directory=str(directory), **kw)

            def log_message(self, *_):
                pass

            def do_GET(self):
                outer.requests.append(self.path)
                if self.path in outer.overrides:
                    status, headers, body = outer.overrides[self.path]
                    self.send_response(status)
                    for k, v in headers.items():
                        self.send_header(k, v)
                    self.send_header("Content-Length", str(len(body)))
                    self.end_headers()
                    self.wfile.write(body)
                    return
                super().do_GET()

        self.httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.url = f"http://127.0.0.1:{self.httpd.server_address[1]}"
        threading.Thread(target=self.httpd.serve_forever, daemon=True).start()


def setup(t: T) -> tuple[Server, Path]:
    served = t.root / "served"
    shutil.copytree(t.catalog, served)
    srv = Server(served)
    t.gitpm_ok("--catalog", srv.url, "install-catalog", NUM, "--version", "1.9.0")
    return srv, served


def failing(t: T, srv: Server, *args, kinds=("catalog",), contains=(), limit=25.0):
    """A failing read: exit != 0, one of `kinds`, message parts, within `limit` seconds,
    workspace byte-identical."""
    before = t.snap(t.ws)
    start = time.monotonic()
    r = t.gitpm(*args)
    elapsed = time.monotonic() - start
    t.ok(r.code != 0, f"gitpm {' '.join(map(str, args))} failed")
    t.ok(any(f"Error kind: {k}" in r.err for k in kinds), f"error kind in {kinds}")
    t.has(r.err, *([contains] if isinstance(contains, str) else contains))
    t.ok(elapsed < limit, f"failed within {limit}s ({elapsed:.1f}s)")
    t.same(before, "installed workspace")
    return r


@case(fixture="catalog", spec=["catalog/catalog-locations-and-transport"])
def base_and_full_url_read_the_same(t: T):
    """URL and URL/catalog.json give the same `catalog`, `info` packages and installs."""
    srv, _ = setup(t)
    base = t.gitpm_ok("--catalog", srv.url, "catalog").json()
    full = t.gitpm_ok("--catalog", srv.url + "/catalog.json", "catalog").json()
    t.eq(full, base, "catalog via full URL")
    strip = lambda out: [l for l in out.splitlines() if "—" in l or "Git:" in l]
    t.eq(strip(t.gitpm_ok("--catalog", srv.url + "/catalog.json", "info").out),
         strip(t.gitpm_ok("--catalog", srv.url, "info").out), "info packages via full URL")
    t.gitpm_ok("--root", t.root / "ws2", "--catalog", srv.url + "/catalog.json", "install-catalog", HELLO)
    t.eq(json.loads((t.root / "ws2/state.json").read_text())["packages"][HELLO]["current"]["version"], "0.2.0",
         "install via full URL")


@case(fixture="catalog", spec=["catalog/catalog-locations-and-transport", "errors/errors-name-the-package"])
def plugin_404_is_not_in_catalog(t: T):
    """A package whose index answers 404: install-catalog says it is not in the catalog
    (naming the ID); check of an installed package whose index is gone reads its origin."""
    srv, served = setup(t)
    r = failing(t, srv, "--catalog", srv.url, "install-catalog", "dan/missing", contains="not found in catalog")
    t.not_installed("dan/missing")
    shutil.rmtree(served / "namespaces/dan/num")
    before = t.snap(t.ws)
    out = t.gitpm_ok("--catalog", srv.url, "check").out
    t.has(next(l for l in out.splitlines() if l.startswith(NUM)), " origin ")
    t.same(before, "installed workspace")
    t.has(r.err, "dan/missing")  # error messages name the package ID (product decision)


@case(fixture="catalog", spec=["catalog/catalog-locations-and-transport", "errors/stable-error-kinds"])
def malformed_and_truncated_json(t: T):
    """Malformed root, malformed package index and truncated release: kind catalog/invalid_data."""
    srv, served = setup(t)
    srv.overrides["/catalog.json"] = (200, {}, b"{ broken")
    failing(t, srv, "--catalog", srv.url, "catalog", kinds=("catalog", "invalid_data"))
    failing(t, srv, "--catalog", srv.url, "info", kinds=("catalog", "invalid_data"))
    del srv.overrides["/catalog.json"]
    srv.overrides["/namespaces/dan/hello/plugin.json"] = (200, {}, b'{"schema": 1, "id": ')
    failing(t, srv, "--catalog", srv.url, "install-catalog", HELLO, kinds=("catalog", "invalid_data"))
    del srv.overrides["/namespaces/dan/hello/plugin.json"]
    release = (served / "namespaces/dan/num/versions/1.10.0/release.json").read_bytes()
    srv.overrides["/namespaces/dan/num/versions/1.10.0/release.json"] = (200, {}, release[: len(release) // 2])
    failing(t, srv, "--catalog", srv.url, "check", kinds=("catalog", "invalid_data"))
    t.not_installed(HELLO)


@case(fixture="catalog", spec=["catalog/catalog-is-a-tree-of-static-files"])
def unsupported_schema(t: T):
    """schema 2 in a directory catalog and in a package index: refused (catalog/invalid_data)."""
    srv, served = setup(t)
    srv.overrides["/catalog.json"] = (200, {}, json.dumps({"schema": 2, "directories": {"namespaces": "namespaces/"}}).encode())
    failing(t, srv, "--catalog", srv.url, "info", kinds=("catalog", "invalid_data"))
    del srv.overrides["/catalog.json"]
    index = json.loads((served / "namespaces/dan/hello/plugin.json").read_text())
    index["schema"] = 2
    srv.overrides["/namespaces/dan/hello/plugin.json"] = (200, {}, json.dumps(index).encode())
    failing(t, srv, "--catalog", srv.url, "install-catalog", HELLO, kinds=("catalog", "invalid_data"))
    t.not_installed(HELLO)


@case(fixture="catalog", spec=["catalog/catalog-locations-and-transport", "git-safety/git-never-waits-for-input#catalog-download"])
def server_down_is_http_error(t: T):
    """A closed loopback port: kind http within 25 s for info, install-catalog and check."""
    srv, _ = setup(t)
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        dead = f"http://127.0.0.1:{s.getsockname()[1]}"
    for args in [("info",), ("check",)]:
        failing(t, srv, "--catalog", dead, *args, kinds=("http",))
    # install-catalog in a fresh workspace (the main one is bound to the served catalog).
    failing(t, srv, "--root", t.root / "fresh", "--catalog", dead, "install-catalog", HELLO, kinds=("http",))
    t.ok(not (t.worktree(HELLO, t.root / "fresh")).exists(), "nothing installed in the fresh workspace")


@case(fixture="catalog", spec=["catalog/catalog-locations-and-transport"])
def redirect_is_not_followed(t: T):
    """301 to a valid copy of the catalog is an error (kind http), never followed."""
    srv, _ = setup(t)
    target = t.serve(t.catalog)
    srv.overrides["/catalog.json"] = (301, {"Location": target.url + "/catalog.json"}, b"")
    srv.overrides["/namespaces/dan/hello/plugin.json"] = (301, {"Location": target.url + "/namespaces/dan/hello/plugin.json"}, b"")
    failing(t, srv, "--catalog", srv.url, "catalog", kinds=("http",), contains="301")
    failing(t, srv, "--catalog", srv.url, "install-catalog", HELLO, kinds=("http",), contains="301")
    t.eq(target.requests, [], "redirect target never requested")
    t.not_installed(HELLO)


@case(fixture="catalog", spec=["catalog/catalog-locations-and-transport"])
def response_over_16_mib_refused(t: T):
    """A 17 MiB catalog.json / package index is refused."""
    srv, _ = setup(t)
    big = b'{"schema": 1, "directories": {}, "pad": "' + b"x" * (17 * 1024 * 1024) + b'"}'
    srv.overrides["/catalog.json"] = (200, {}, big)
    failing(t, srv, "--catalog", srv.url, "catalog", kinds=("http",))
    del srv.overrides["/catalog.json"]
    srv.overrides["/namespaces/dan/hello/plugin.json"] = (200, {}, big)
    failing(t, srv, "--catalog", srv.url, "install-catalog", HELLO, kinds=("http",))
    t.not_installed(HELLO)


@case(fixture="catalog", spec=["catalog/catalog-is-a-tree-of-static-files"])
def unsafe_directory_links_refused(t: T):
    """Directory links with `..`, an absolute path or another host are refused (kind catalog)
    and never followed."""
    srv, _ = setup(t)
    evil = t.serve(t.catalog)
    for link in ["../", "../../etc/", "/etc/", "/namespaces/", evil.url + "/namespaces/", "//evil.example/x/"]:
        srv.overrides["/catalog.json"] = (200, {}, json.dumps({"schema": 1, "directories": {"namespaces": link}}).encode())
        failing(t, srv, "--catalog", srv.url, "info", contains="invalid directory link")
    t.eq(evil.requests, [], "foreign host never requested")


@case(fixture="catalog", spec=["catalog/catalog-is-a-tree-of-static-files"])
def unsafe_release_links_refused(t: T):
    """Release links in a package index with `..`, an absolute path or another host are
    refused (kind catalog) by install-catalog, check and info."""
    srv, served = setup(t)
    evil = t.serve(t.catalog)
    index = json.loads((served / "namespaces/dan/num/plugin.json").read_text())
    for link in ["versions/1.10.0/../../../hello/versions/0.2.0/release.json", "../num/versions/1.10.0/release.json",
                 "/namespaces/dan/num/versions/1.10.0/release.json",
                 evil.url + "/namespaces/dan/num/versions/1.10.0/release.json"]:
        crafted = dict(index, versions={**index["versions"], "1.10.0": link})
        srv.overrides["/namespaces/dan/num/plugin.json"] = (200, {}, json.dumps(crafted).encode())
        for args in [("install-catalog", NUM, "--version", "1.10.0"), ("check",), ("info",)]:
            root = ("--root", t.root / "other-ws") if args[0] == "install-catalog" else ()
            failing(t, srv, *root, "--catalog", srv.url, *args, contains="invalid release link")
    t.eq(evil.requests, [], "foreign host never requested")
    t.ok(not (t.worktree(NUM, t.root / "other-ws")).exists(), "no clone in the other workspace")
    t.ok(not (t.root / "other-ws/dist/dan/num").exists(), "no dist in the other workspace")
    state = t.root / "other-ws/state.json"
    t.ok(not state.exists() or NUM not in json.loads(state.read_text()).get("packages", {}), "no state entry")


@case(fixture="catalog", spec=["catalog/catalog-locations-and-transport", "catalog/register-one-version"])
def register_to_http_catalog_refused(t: T):
    """register against an HTTP catalog is refused; the served files are unchanged."""
    srv, served = setup(t)
    t.release(HELLO, "0.3.0")
    before = t.snap(served, t.catalog)
    t.gitpm_err("--catalog", srv.url, "register", t.repo(HELLO), "--ref", f"{HELLO}@0.3.0",
             kind="invalid_argument", contains="local catalog")
    t.same(before, "served and local catalogs")


@case(fixture="catalog", spec=["catalog/catalog-locations-and-transport#plain-http-to-a-remote-host"])
def non_loopback_http_refused(t: T):
    """Non-loopback http:// is refused before any request (invalid_argument, "HTTPS")."""
    srv, _ = setup(t)
    for args in [("info",), ("catalog",), ("install-catalog", HELLO), ("check",)]:
        failing(t, srv, "--catalog", "http://pkg.example.invalid", *args, kinds=("invalid_argument",),
                contains="HTTPS")
