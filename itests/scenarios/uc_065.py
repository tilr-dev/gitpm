"""UC-065: Review regressions: refs, symlinks, transport and managed names (P0).

Subcases 1–8 of the 2026-10-15 review, each split into independent cases. Network
peers are loopback servers started by the case (redirect, oversized and silent TCP
servers). SSH: the container has no ssh client, so `ssh` on PATH is a stub that
behaves like a passphrase-protected key without an agent: with BatchMode=yes it fails
like OpenSSH ("Permission denied (publickey)"), otherwise it prompts on /dev/tty and
blocks — run under script(1), a prompt would hang the case into its timeout.

"""

import json
import os
import shlex
import socket
import subprocess
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

from gitpmtest import R, T, case

ID = "3dce1568-569c-5b53-9321-37185e751947"


def symlinked_plugin(t: T, id: str, links: dict[str, str], version: str = "0.1.0") -> str:
    repo = t.repo(id)
    if not repo.exists():
        (repo / "dist").mkdir(parents=True)
        t.git(repo, "init", "-q", "-b", "main")
    t.write(repo, {"plugin.json": json.dumps({"id": id, "name": "fixture", "version": version}) + "\n",
                   "dist/index.js": f"export const v = '{id} {version}';\n", "README.md": "readme\n"})
    for path, target in links.items():
        if os.path.lexists(repo / path):
            os.unlink(repo / path)
        (repo / path).parent.mkdir(parents=True, exist_ok=True)
        os.symlink(target, repo / path)
    t.git(repo, "add", "--all")
    t.git(repo, "commit", "-q", "-m", f"{id} {version}")
    t.git(repo, "tag", f"v{version}")
    return t.rev(repo)


def timed(t: T, *args, limit: float, env=None) -> R:
    start = time.monotonic()
    r = t.gitpm(*args, env=env, timeout=120)
    took = time.monotonic() - start
    print(f"  took {took:.1f}s")
    t.ok(took < limit, f"finished within {limit:.0f}s")
    return r


# ------------------------------------------------------------------- 1. symlinks

@case(fixture="hello_installed", spec=["transactions-locking/managed-paths-are-never-followed-through-symlinks"])
def outside_dist_symlink_does_not_break_any_operation(t: T):
    """1. `link -> README.md` at the repo root: install keeps it; disable, enable,
    update and uninstall --force work for this package and for an unrelated one."""
    symlinked_plugin(t, "fc8bc03a-1524-54e6-9111-f57fd8ce96c2", {"link": "README.md"})
    t.gitpm_ok("install", t.repo("fc8bc03a-1524-54e6-9111-f57fd8ce96c2"))
    link = t.clone("fc8bc03a-1524-54e6-9111-f57fd8ce96c2") / "link"
    t.ok(link.is_symlink() and os.readlink(link) == "README.md", "symlink kept in the clone")
    t.eq(sorted(p.name for p in t.dist("fc8bc03a-1524-54e6-9111-f57fd8ce96c2").iterdir()), sorted(["index.js"]), "dist without the link")
    symlinked_plugin(t, "fc8bc03a-1524-54e6-9111-f57fd8ce96c2", {"link": "README.md"}, version="0.2.0")
    t.release(ID, "0.1.1")
    for id in ["fc8bc03a-1524-54e6-9111-f57fd8ce96c2", ID]:
        t.gitpm_ok("disable", id)
        t.gitpm_ok("enable", id)
        t.gitpm_ok("update", id)
    t.eq((t.package("fc8bc03a-1524-54e6-9111-f57fd8ce96c2")["current"]["version"], t.package(ID)["current"]["version"]), ("0.2.0", "0.1.1"), "both updated")
    t.ok(link.is_symlink() and os.readlink(link) == "README.md", "symlink kept through update")
    for id in ["fc8bc03a-1524-54e6-9111-f57fd8ce96c2", ID]:
        t.gitpm_ok("uninstall", id, "--force")
        t.not_installed(id)


@case(fixture="hello_installed", spec=["package-manifest/package-folder-contents"])
def symlink_inside_dist_is_refused_at_install(t: T):
    symlinked_plugin(t, "5f85b937-f3a5-549c-82d3-736e8d3eff88", {"dist/alias.js": "index.js"})
    before = t.snap(t.ws)
    t.gitpm_err("install", t.repo("5f85b937-f3a5-549c-82d3-736e8d3eff88"), kind="unsupported", contains="symlinks are not supported")
    t.not_installed("5f85b937-f3a5-549c-82d3-736e8d3eff88")
    t.same(before, "workspace")


# --------------------------------------------------------------- 2. update --ref

@case(fixture="hello_installed", spec=["updates/explicit-ref-resolves-against-the-origin"])
def update_to_a_branch_follows_the_moved_upstream(t: T):
    """2. `update --ref feature` resolves origin/feature after fetch, not a stale local
    branch of the installed clone."""
    t.git(ID, "checkout", "-q", "-b", "feature")
    first = t.release(ID, "0.1.1", tag=False)
    t.gitpm_ok("update", ID, "--ref", "feature")
    t.eq(t.package(ID)["current"], {"version": "0.1.1", "commit": first}, "on feature")
    t.git(t.clone(ID), "branch", "feature", first)  # a local branch that will go stale
    second = t.release(ID, "0.1.2", tag=False)
    t.gitpm_ok("update", ID, "--ref", "feature")
    t.eq(t.package(ID)["current"], {"version": "0.1.2", "commit": second}, "followed the moved branch")
    t.eq(t.rev(t.clone(ID)), second, "clone HEAD")
    t.eq(t.rev(t.clone(ID), "refs/remotes/origin/feature"), second, "HEAD == origin/feature")
    t.has((t.dist(ID) / "index.js").read_text(), "0.1.2")


@case(fixture="hello_installed", spec=["updates/explicit-ref-resolves-against-the-origin"])
def update_to_head_is_origin_head(t: T):
    head = t.release(ID, "0.2.0", tag=False)
    t.gitpm_ok("update", ID, "--ref", "HEAD")
    t.eq(t.package(ID)["current"], {"version": "0.2.0", "commit": head}, "origin HEAD")
    t.eq(t.rev(t.clone(ID)), t.rev(ID), "clone HEAD == origin HEAD")
    moved = t.release(ID, "0.2.1", tag=False)
    t.gitpm_ok("update", ID, "--ref", "HEAD")
    t.eq(t.rev(t.clone(ID)), moved, "HEAD follows the moved origin HEAD")


@case(fixture="hello_installed", spec=["git-safety/no-option-injection", "updates/explicit-ref-resolves-against-the-origin"])
def refs_heads_are_refused(t: T):
    t.git(ID, "checkout", "-q", "-b", "feature")
    t.release(ID, "0.1.1", tag=False)
    before = t.snap(t.ws)
    for ref in ["refs/heads/main", "refs/heads/feature"]:
        t.gitpm_err("update", ID, "--ref", ref, kind="invalid_argument",
                 contains="local branches of an installed clone are stale; use the branch name")
    t.same(before, "workspace")


# ----------------------------------------------------------- 3. HTTP catalog rules

class Redirect:
    def __init__(self, target: str):
        self.requests = []
        log = self.requests

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_GET(self):
                log.append(self.path)
                self.send_response(301)
                self.send_header("Location", target + self.path)
                self.send_header("Content-Length", "0")
                self.end_headers()

        self.httpd = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.url = f"http://127.0.0.1:{self.httpd.server_address[1]}"
        threading.Thread(target=self.httpd.serve_forever, daemon=True).start()


class Huge:
    """Serves the real catalog, but one path answers with ~50 MB: announced
    (Content-Length) or streamed until close (HTTP/1.0, no length)."""

    SIZE = 50 * 1024 * 1024

    def __init__(self, root: Path, path: str, announce: bool):
        outer = self
        self.sent = 0

        class Handler(BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.0"

            def log_message(self, *_):
                pass

            def do_GET(self):
                p = self.path.split("?")[0]
                f = root / p.lstrip("/")
                if p != path:
                    data = f.read_bytes() if f.is_file() else None
                    self.send_response(200 if data is not None else 404)
                    self.send_header("Content-Length", str(len(data or b"")))
                    self.end_headers()
                    self.wfile.write(data or b"")
                    return
                body = f.read_bytes()  # valid JSON, padded with whitespace to 50 MB
                self.send_response(200)
                if announce:
                    self.send_header("Content-Length", str(outer.SIZE))
                self.end_headers()
                try:
                    self.wfile.write(body)
                    outer.sent = len(body)
                    chunk = b" " * (1024 * 1024)
                    while outer.sent + len(chunk) <= outer.SIZE:
                        self.wfile.write(chunk)
                        outer.sent += len(chunk)
                    self.wfile.write(b" " * (outer.SIZE - outer.sent))
                    outer.sent = outer.SIZE
                except (BrokenPipeError, ConnectionResetError):
                    pass

        self.httpd = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.url = f"http://127.0.0.1:{self.httpd.server_address[1]}"
        threading.Thread(target=self.httpd.serve_forever, daemon=True).start()


@case(fixture="catalog", spec=["catalog/catalog-locations-and-transport#plain-http-to-a-remote-host"])
def non_loopback_http_catalog_is_refused(t: T):
    t.gitpm_ok("install", t.repo("2190eee8-4be5-5a91-81d2-6ca95c1e61d0"))  # makes check read the catalog
    before = t.snap(t.ws)
    for url in ["http://192.0.2.10", "http://pkg.example.invalid/catalog.json", "http://127.0.0.1.example.invalid"]:
        for args in [("install-catalog", ID), ("check",), ("info",), ("catalog",)]:
            r = timed(t, "--catalog", url, *args, limit=10)
            t.ok(r.code != 0, "refused")
            t.has(r.err, "Error kind: invalid_argument", "catalog URL must use HTTPS (http:// only for localhost)")
    t.same(before, "workspace")
    t.not_installed(ID)


@case(fixture="catalog", spec=["catalog/catalog-locations-and-transport"])
def loopback_http_catalog_is_allowed(t: T):
    srv = t.serve(t.catalog)
    port = srv.url.rsplit(":", 1)[1]
    t.gitpm_ok("--root", t.root / "a", "--catalog", srv.url, "install-catalog", ID)
    t.gitpm_ok("--root", t.root / "b", "--catalog", f"http://localhost:{port}/catalog.json", "install-catalog", ID)
    # Any 127.0.0.0/8 address counts as loopback (nothing listens there: a transport
    # error, not the HTTPS refusal).
    r = t.gitpm("--catalog", f"http://127.0.0.2:{port}", "info")
    t.has(r.err, "Error kind: http")
    t.lacks(r.err, "must use HTTPS")
    t.ok(len(srv.requests) > 0, "served over loopback HTTP")


@case(fixture="catalog", spec=["catalog/catalog-locations-and-transport"])
def redirect_is_not_followed(t: T):
    srv = t.serve(t.catalog)
    redirect = Redirect(srv.url)
    srv.take()
    for args in [("install-catalog", ID), ("info",)]:
        r = timed(t, "--catalog", redirect.url, *args, limit=30)
        t.ok(r.code != 0, "refused")
        t.has(r.err, "Error kind: http", "catalog HTTP 301")
    t.eq(srv.take(), [], "redirect target never contacted")
    t.ok(redirect.requests, "the redirecting server was asked")
    t.not_installed(ID)


@case(fixture="catalog", spec=["catalog/catalog-locations-and-transport"])
def oversized_catalog_response_is_refused(t: T):
    """50 MB announced by Content-Length for the package index: refused before download."""
    huge = Huge(t.catalog, "/packages/3dce1568-569c-5b53-9321-37185e751947/plugin.json", announce=True)
    r = timed(t, "--catalog", huge.url, "install-catalog", ID, limit=30)
    t.ok(r.code != 0, "refused")
    t.has(r.err, "Error kind: http", "Maximum file size exceeded")
    t.not_installed(ID)


@case(fixture="catalog", spec=["catalog/catalog-locations-and-transport"])
def oversized_streamed_catalog_response_is_refused(t: T):
    """50 MB without Content-Length (streamed until close): the 16 MiB limit must still
    refuse it."""
    huge = Huge(t.catalog, "/packages/3dce1568-569c-5b53-9321-37185e751947/plugin.json", announce=False)
    r = timed(t, "--catalog", huge.url, "install-catalog", ID, limit=30)
    print(f"  server sent {huge.sent} bytes")
    t.ok(r.code != 0, f"a {Huge.SIZE // 2**20} MiB response is refused (16 MiB limit)")
    t.has(r.err, "Error kind: http")
    t.not_installed(ID)


# ---------------------------------------------------------- 4. release without hashes

def no_hashes(t: T, change):
    t.edit_json(t.catalog_release(ID, "0.2.0"), change)
    t.gitpm_err("install-catalog", ID, "--version", "0.2.0", kind="catalog", contains="has no file hashes")
    t.gitpm_err("install-catalog", ID, kind="catalog", contains="has no file hashes")
    t.not_installed(ID)


@case(fixture="catalog", spec=["catalog/release-record#uuid-release-without-hashes"])
def namespaced_release_with_empty_files_is_refused(t: T):
    no_hashes(t, lambda r: r.update(files={}))


@case(fixture="catalog", spec=["catalog/release-record#uuid-release-without-hashes"])
def namespaced_release_without_files_is_refused(t: T):
    no_hashes(t, lambda r: r.pop("files"))


# ----------------------------------------------------------- 5. hanging transports

class Silent:
    """Accepts TCP connections and never answers."""

    def __init__(self):
        self.sock = socket.socket()
        self.sock.bind(("127.0.0.1", 0))
        self.sock.listen(16)
        self.port = self.sock.getsockname()[1]
        self.conns = []
        threading.Thread(target=self.accept, daemon=True).start()

    def accept(self):
        while True:
            conn, _ = self.sock.accept()
            self.conns.append(conn)


@case(fixture="hello_installed", spec=["git-safety/git-never-waits-for-input"])
def silent_git_server_fails_in_bounded_time(t: T):
    silent = Silent()
    url = f"http://127.0.0.1:{silent.port}/dan/silent.git"
    before = t.snap(t.ws)
    r = timed(t, "install", url, limit=120)
    t.ok(r.code != 0, "install failed")
    t.has(r.err, "Error kind: git")
    r = timed(t, "auth", "check", url, limit=120)
    t.ok(r.code != 0, "auth check failed")
    t.has(r.err, "Error kind: git", "timed out after 10s")
    t.ok(len(silent.conns) >= 2, "the server really accepted the connections")
    t.same(before, "workspace")
    t.gitpm_ok("disable", ID)  # lock released right after
    t.eq(t.package(ID)["enabled"], False, "next command works")


SSH_STUB = """#!/bin/sh
# Stub for a passphrase-protected key without ssh-agent (no ssh client in the image).
for a in "$@"; do
  if [ "$a" = "BatchMode=yes" ]; then
    echo "git@git.example.invalid: Permission denied (publickey)." >&2
    exit 255
  fi
done
printf "Enter passphrase for key '/root/.ssh/id_ed25519': " > /dev/tty
read -r pass < /dev/tty
exit 255
"""


@case(fixture="hello_installed", spec=["git-safety/git-never-waits-for-input#batch-ssh-command"])
def ssh_passphrase_without_agent_never_prompts(t: T):
    bin = t.root / "sshbin"
    bin.mkdir()
    (bin / "ssh").write_text(SSH_STUB)
    (bin / "ssh").chmod(0o755)
    env = {"PATH": f"{bin}:{os.environ['PATH']}"}
    os.environ.pop("GIT_TERMINAL_PROMPT", None)
    os.environ.pop("SSH_AUTH_SOCK", None)
    before = t.snap(t.ws)
    for args, kind in [(["install", "ssh://git@git.example.invalid/dan/private.git"], "git"),
                       (["auth", "check", "git@git.example.invalid:dan/private.git"], "auth")]:
        start = time.monotonic()
        r = t.run("script", "-qec", shlex.join(["gitpm", *args]), "/dev/null", input="", env=env, timeout=120)
        took = time.monotonic() - start
        print(f"  took {took:.1f}s on a TTY")
        t.ok(took < 60, "no hang")
        t.ok(r.code != 0, "failed")
        t.has(r.out, f"Error kind: {kind}")
        t.lacks(r.out, "Enter passphrase")
    t.same(before, "workspace")
    t.gitpm_ok("disable", ID)


# ------------------------------------------------------- 6. .<root>.stage/.backup

def neighbours(t: T) -> tuple[Path, Path]:
    stage, backup = t.root / ".ws.stage", t.root / ".ws.backup"
    t.write(stage, {"user.txt": "my notes\n", "sub/data.bin": bytes(range(256))})
    t.write(backup, {"notes.txt": "keep\n", "repos/fake-0123456789abcdef/fake.txt": "not a package\n", "state.json": "{}\n"})
    return stage, backup


@case(fixture="hello_installed", spec=["transactions-locking/neighbouring-folders-are-never-touched"])
def stage_and_backup_neighbours_are_never_touched(t: T):
    stage, backup = neighbours(t)
    before = t.snap(stage, backup, git=True)
    t.release(ID, "0.1.1")
    t.new_package("95d14dc8-6dfc-5022-9fd1-7146ee6aee58", "0.1.0")
    for args in [("install", t.repo("95d14dc8-6dfc-5022-9fd1-7146ee6aee58")), ("update", ID), ("rollback", ID), ("disable", ID), ("enable", ID),
                 ("forget", ID), ("restore", ID), ("uninstall", "95d14dc8-6dfc-5022-9fd1-7146ee6aee58", "--force"), ("list",)]:
        t.gitpm_ok(*args)
    t.same(before, ".ws.stage and .ws.backup")


@case(fixture="hello_installed", spec=["transactions-locking/neighbouring-folders-are-never-touched"])
def fork_into_stage_or_backup_folder_works(t: T):
    stage, backup = neighbours(t)
    kept = t.snap(backup, git=True)
    t.gitpm_ok("fork", ID, "--into", backup / "myfork")
    import shutil
    shutil.rmtree(stage)
    t.gitpm_ok("fork", ID, "--into", stage)
    fork_ids = []
    for path in [backup / "myfork", stage]:
        id = t.manifest_id(path)
        t.ok(id != ID and id not in fork_ids, f"fresh fork UUID at {path.name}")
        fork_ids.append(id)
        t.gitpm_ok("install", path, "--dev")
        t.eq(t.package(id)["source"], str(path), f"{id} installed from {path}")
    after = t.snap(backup, git=True)
    t.eq({k: v for k, v in after.items() if "/myfork" not in k and not k.endswith("/myfork")}, dict(kept),
         "user data in .ws.backup untouched")


# ---------------------------------------------- 7. symlinked root, two processes

SLOW_GIT = """#!/bin/sh
for a in "$@"; do [ "$a" = clone ] && sleep 4; done
exec /usr/bin/git "$@"
"""


def slow_env(t: T) -> dict:
    bin = t.root / "slowbin"
    bin.mkdir(exist_ok=True)
    (bin / "git").write_text(SLOW_GIT)
    (bin / "git").chmod(0o755)
    return dict(os.environ, PATH=f"{bin}:{os.environ['PATH']}")


def through_two_spellings(t: T, first: Path, second: Path):
    t.new_package("e831df69-dae1-5d20-a2d4-b5a63a849bdb", "0.1.0")
    a = subprocess.Popen(["gitpm", "--root", str(first), "install", str(t.repo("e831df69-dae1-5d20-a2d4-b5a63a849bdb"))], cwd=t.root,
                         env=slow_env(t), stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    deadline = time.monotonic() + 10
    while not (t.ws / ".pm-txn").exists() and time.monotonic() < deadline:
        time.sleep(0.05)
    t.ok((t.ws / ".pm-txn").exists(), f"first process (--root {first}) is inside its transaction")
    t.gitpm_err("--root", second, "disable", ID, kind="locked", contains="workspace is already in use")
    out, err = a.communicate(timeout=60)
    print(f"  first process: exit {a.returncode}\n{out}{err}")
    t.eq(a.returncode, 0, "first process finished")
    t.gitpm_ok("--root", second, "disable", ID)
    s = t.state()["packages"]
    t.ok("e831df69-dae1-5d20-a2d4-b5a63a849bdb" in s and s[ID]["enabled"] is False, "both mutations kept (no lost update)")


@case(fixture="hello_installed", spec=["transactions-locking/workspace-lock"])
def symlinked_root_shares_the_lock(t: T):
    link = t.root / "wslink"
    os.symlink(t.ws, link)
    through_two_spellings(t, t.ws, link)


@case(fixture="hello_installed", spec=["transactions-locking/workspace-lock"])
def real_root_is_locked_by_a_symlinked_spelling(t: T):
    link = t.root / "wslink"
    os.symlink(t.ws, link)
    through_two_spellings(t, link, t.ws)


# ---------------------------------------------- 8. readers during many mutations

@case(fixture="catalog", spec=["catalog/writers-are-serialized-and-crash-safe", "transactions-locking/one-manager-serializes-its-operations"])
def check_and_list_never_see_partial_state(t: T):
    t.gitpm_ok("install-catalog", ID, "--version", "0.1.0")
    t.gitpm_ok("update", ID)
    stop = threading.Event()
    writes = {"done": 0, "locked": 0}

    def writer():
        ops = [["rollback", ID], ["disable", ID], ["update", ID], ["enable", ID]] * 8
        for op in ops:
            while True:
                p = subprocess.run(["gitpm", *op], cwd=t.root, capture_output=True, text=True, timeout=60)
                if p.returncode == 0:
                    writes["done"] += 1
                    break
                if "Error kind: locked" not in p.stderr:
                    writes["error"] = f"gitpm {' '.join(op)}: {p.stderr}"
                    stop.set()
                    return
                writes["locked"] += 1
                time.sleep(0.01)
        stop.set()

    thread = threading.Thread(target=writer)
    thread.start()
    reads = {"check": 0, "list": 0, "list_locked": 0, "raw": 0}
    bad = []
    while not stop.is_set():
        p = subprocess.run(["gitpm", "check"], cwd=t.root, capture_output=True, text=True, timeout=60)
        reads["check"] += 1
        row = [l for l in p.stdout.splitlines() if l.split()[:1] == [ID]]
        if p.returncode != 0 or len(row) != 1 or "нет установленных" in p.stdout:
            bad.append(f"check: exit {p.returncode}\n{p.stdout}{p.stderr}")
        p = subprocess.run(["gitpm", "list"], cwd=t.root, capture_output=True, text=True, timeout=60)
        if p.returncode == 0:
            reads["list"] += 1
            try:
                if ID not in json.loads(p.stdout)["packages"]:
                    bad.append(f"list without {ID}: {p.stdout}")
            except ValueError as e:
                bad.append(f"list: invalid JSON ({e}): {p.stdout[:200]}")
        elif "Error kind: locked" in p.stderr:
            reads["list_locked"] += 1
        else:
            bad.append(f"list: {p.stderr}")
        try:
            state = json.loads((t.ws / "state.json").read_text())
            reads["raw"] += 1
            if state["packages"][ID]["current"]["version"] not in ("0.1.0", "0.2.0"):
                bad.append(f"raw state: {state}")
        except (ValueError, KeyError) as e:
            bad.append(f"raw state.json unreadable: {e}")
    thread.join()
    print(f"  writes: {writes}; reads: {reads}")
    t.ok("error" not in writes, f"writer ok: {writes.get('error', '')}")
    t.eq(writes["done"], 32, "all mutations applied")
    t.ok(reads["check"] >= 5, "readers really ran during the mutations")
    t.eq(bad, [], "no empty or partial state seen")
