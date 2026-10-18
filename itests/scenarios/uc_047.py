"""UC-047: Offline local lifecycle (P1).

g5_offline: dan/hello installed from the bare origin /fx/origin/hello.git and updated
0.1.0 → 0.1.1; the fork mine/hello 0.1.2 installed from its author folder. "Offline":
the origins are renamed away and GITPM_CATALOG points at a loopback endpoint that answers
503 to everything and logs each request (so any catalog access would be visible).
"""

import json
import os
import subprocess
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

from gitpmtest import T, case

ID, FORK = "dan/hello", "mine/hello"
ORIGIN = Path("/fx/origin/hello.git")
FORK_ORIGIN = Path("/fx/authors/mine-hello")


class Unavailable:
    def __init__(self):
        self.requests: list[str] = []
        log = self.requests

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_GET(self):
                log.append(self.path)
                self.send_response(503)
                self.end_headers()

        self.httpd = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.url = f"http://127.0.0.1:{self.httpd.server_address[1]}"
        threading.Thread(target=self.httpd.serve_forever, daemon=True).start()


def offline(t: T, fork_too: bool = True) -> Unavailable:
    ORIGIN.rename(ORIGIN.with_name("hello.git.away"))
    if fork_too:
        FORK_ORIGIN.rename(FORK_ORIGIN.with_name("mine-hello.away"))
    catalog = Unavailable()
    os.environ["GITPM_CATALOG"] = catalog.url
    print(f"  offline: origins moved away, GITPM_CATALOG={catalog.url} (503)")
    return catalog


def no_catalog_access(t: T, catalog: Unavailable):
    t.eq(catalog.requests, [], "no catalog request")


def entry(t: T, id: str) -> dict:
    return t.plugin(id)


def paths(t: T, id: str):
    return t.snap(t.clone(id), t.dist(id))


def blob(repo: Path, rev: str, path: str) -> bytes:
    return subprocess.run(["git", "-C", str(repo), "cat-file", "blob", f"{rev}:{path}"],
                          capture_output=True, check=True).stdout


@case(fixture="g5_offline", spec=["dist/enable-and-disable-deploy-and-remove#disable-and-enable"])
def list_disable_enable_offline(t: T):
    """Offline: list, disable and enable work from the clones alone; disable removes only the
    dist folder, enable redeploys the same bytes and restores the exact state."""
    catalog = offline(t)
    files = t.snap(t.ws / "plugins", t.ws / "dist")
    clones = t.snap(t.ws / "plugins")
    state = t.state()
    listed = t.gitpm_ok("list").json()
    t.eq(sorted(listed["plugins"]), [ID, FORK], "list works offline")
    t.gitpm_ok("disable", ID)
    changed = t.state()
    t.eq(changed["plugins"][ID]["enabled"], False, "disabled")
    changed["plugins"][ID]["enabled"] = True
    t.eq(changed, state, "disable changed only the flag")
    t.same(clones, "clones")
    t.ok(not t.dist(ID).exists(), "dist of the disabled plugin removed")
    t.gitpm_ok("disable", FORK)
    t.eq((entry(t, ID)["enabled"], entry(t, FORK)["enabled"]), (False, False), "flags are independent")
    t.gitpm_ok("enable", ID)
    t.gitpm_ok("enable", FORK)
    t.eq(t.state(), state, "enable restores the exact state")
    t.same(files, "plugin files")
    no_catalog_access(t, catalog)


@case(fixture="g5_offline", spec=["updates/rollback-swaps-current-and-previous#rollback-after-update"])
def rollback_offline(t: T):
    """Offline rollback swaps current/previous from the clone; the fork has no history."""
    catalog = offline(t)
    fork = paths(t, FORK)
    fork_entry = entry(t, FORK)
    v010, v011 = entry(t, ID)["previous"], entry(t, ID)["current"]
    t.gitpm_ok("rollback", ID)
    p = entry(t, ID)
    t.eq((p["current"], p["previous"]), (v010, v011), "current/previous swapped")
    t.eq(t.rev(t.clone(ID)), v010["commit"], "clone HEAD at 0.1.0")
    t.eq((t.dist(ID) / "index.js").read_bytes(), blob(t.repo(ID), "dan/hello@0.1.0", "dist/index.js"), "dist bytes of 0.1.0")
    t.gitpm_ok("rollback", ID)
    t.eq(entry(t, ID)["current"], v011, "rolled back again to 0.1.1")
    t.eq((t.dist(ID) / "index.js").read_bytes(), blob(t.repo(ID), "dan/hello@0.1.1", "dist/index.js"), "dist bytes of 0.1.1")
    t.gitpm_err("rollback", FORK, kind="no_previous_version", contains=f"no previous version of {FORK}")
    t.same(fork, "fork files")
    t.eq(entry(t, FORK), fork_entry, "fork state entry")
    no_catalog_access(t, catalog)


@case(fixture="g5_offline", spec=["install-uninstall/three-removal-levels#forget-then-restore"])
def forget_restore_offline(t: T):
    """Offline forget keeps the clone (dist removed); restore re-registers it from the clone's origin URL."""
    catalog = offline(t)
    files = paths(t, ID)
    clone = t.snap(t.clone(ID))
    old = entry(t, ID)
    t.gitpm_ok("forget", ID)
    t.ok(ID not in t.state()["plugins"], "forgotten")
    t.same(clone, "clone retained by forget")
    t.ok(not t.dist(ID).exists(), "dist removed by forget")
    t.gitpm_ok("restore", ID)
    p = entry(t, ID)
    t.eq(p["current"], old["current"], "restored at the clone's revision")
    t.eq(p["previous"], None, "no rollback history after restore")
    t.eq(p["enabled"], True, "enabled")
    t.eq(p["source"], str(ORIGIN), "source from the clone's origin URL (no network)")
    t.same(files, "files after restore")
    no_catalog_access(t, catalog)


@case(fixture="g5_offline", spec=["install-uninstall/only-manager-owned-files-are-deleted"])
def uninstall_offline(t: T):
    """Offline uninstall of the original leaves the fork untouched."""
    catalog = offline(t)
    fork = paths(t, FORK)
    fork_entry = entry(t, FORK)
    t.gitpm_ok("uninstall", ID)
    t.not_installed(ID)
    t.same(fork, "fork files")
    t.eq(entry(t, FORK), fork_entry, "fork state entry")
    no_catalog_access(t, catalog)


@case(fixture="g5_offline", spec=["errors/failing-operations-leave-the-workspace-unchanged#network-failure-mid-update"])
def update_fails_cleanly_offline_then_succeeds(t: T):
    """update with the origin gone fails with git and changes nothing; succeeds once it is back."""
    catalog = offline(t)
    before = t.snap(t.ws)
    t.gitpm_err("update", ID, kind="git", contains="git fetch origin")
    t.same(before, "workspace after the failed update")
    t.ok(not (t.ws / ".pm-txn").exists(), "no transaction left")
    ORIGIN.with_name("hello.git.away").rename(ORIGIN)
    commit = t.release(ID, "0.1.2")
    t.git(ID, "push", "-q", str(ORIGIN), "main", "--tags")
    t.gitpm_ok("update", ID)
    t.eq(entry(t, ID)["current"], {"version": "0.1.2", "commit": commit}, "updated after the origin came back")
    no_catalog_access(t, catalog)


@case(fixture="g5_offline", spec=["package-manifest/forkof-is-provenance-replaces-is-an-offer#installing-a-fork-changes-no-consumer"])
def fork_and_original_stay_independent(t: T):
    """Original's origin gone, fork's author folder present: the fork updates and rolls
    back on its own; the original refuses to update and is not touched."""
    catalog = offline(t, fork_too=False)
    original = paths(t, ID)
    t.write(FORK_ORIGIN, {"dist/index.js": "export const greeting = 'mine 0.1.3';\n"})
    t.gitpm_ok("commit", FORK_ORIGIN, "0.1.3")
    t.gitpm_ok("update", FORK)
    t.eq(entry(t, FORK)["current"]["version"], "0.1.3", "fork updated")
    t.has((t.dist(FORK) / "index.js").read_text(), "mine 0.1.3")
    t.gitpm_err("update", ID, kind="git")
    t.gitpm_ok("rollback", FORK)
    t.eq(entry(t, FORK)["current"]["version"], "0.1.2", "fork rolled back")
    t.same(original, "original files")
    t.eq(entry(t, ID)["current"]["version"], "0.1.1", "original version")
    no_catalog_access(t, catalog)
