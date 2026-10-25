"""UC-044: Fork recorded source, isolate local work and fork of fork (P0).

The spec's vendor/module is played by vendor/demo (g3_vendor_installed). In the installed
clone: a local commit that bumps plugin.json to 0.1.5 and edits dist, then a staged
unrelated file, an unstaged tracked edit, an untracked and an ignored file.
"""

import json

from gitpmtest import T, case

ID, MINE, THIRD = "vendor/demo", "mine/demo", "third/demo"


def prepare(t: T):
    """Local work in the installed clone, then fork to /fx/repos/mine/demo."""
    clone = t.worktree(ID)
    t.write(clone, {"plugin.json": json.dumps({"id": ID, "version": "0.1.5"}, indent=2) + "\n",
                    "dist/index.js": "export const greeting = 'local commit';\n"})
    t.commit(clone, "local commit")
    t.write(clone, {"staged.txt": "staged\n"})
    t.git(clone, "add", "staged.txt")
    t.write(clone, {"dist/index.js": "export const greeting = 'unstaged';\n",
                    "untracked.txt": "untracked\n", "node_modules/x.js": "ignored\n"})
    before = t.snap(t.ws, t.repo(ID), git=True)
    dest = t.repo(MINE)
    t.gitpm_ok("fork", ID, "--ns", "mine", "--into", dest)
    return dest, t.package(ID)["current"]["commit"], before


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-metadata"])
def fork_of_recorded_revision(t: T):
    """Fork metadata and identity commit come from the installed source revision."""
    installed = t.package(ID)["current"]
    dest, source_commit, before = prepare(t)
    m = json.loads((dest / "plugin.json").read_text())
    t.eq(m["forkOf"], {"id": ID, "version": "0.1.0", "commit": source_commit, "source": t.package(ID)["source"]}, "forkOf")
    t.eq(m["version"], "0.1.0", "fork version")
    t.eq(m["replaces"], {"id": ID, "version": "0.1.0"}, "replaces")
    t.ok(t.rev(t.worktree(ID)) != installed["commit"], "local HEAD differs from installed revision")
    t.eq(t.rev(dest, "HEAD~1"), source_commit, "identity commit parent")
    t.eq(t.git(dest, "show", "--name-only", "--format=", "HEAD"), "plugin.json", "identity commit files")
    # forkOf is the only provenance: no pm.forkBase/pm.forkVersion Git config.
    r = t.run("git", "-C", dest, "config", "--local", "--get-regexp", r"^pm\.")
    t.eq((r.code, r.out), (1, ""), "no pm.* Git config")
    t.same(before, "installed clone, dist, state and author repository")
    t.eq(t.package(ID)["current"], installed, "original state")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-copies-an-installed-package"])
def local_work_is_excluded_from_fork(t: T):
    dest, commit, before = prepare(t)
    t.eq(t.git(dest, "status", "--porcelain", "--untracked-files=all"), "", "clean fork")
    expected = t.git(t.worktree(ID), "show", f"{commit}:dist/index.js")
    t.eq((dest / "dist/index.js").read_text().strip(), expected, "source revision bytes")
    for path in ["staged.txt", "untracked.txt", "node_modules/x.js"]:
        t.ok(not (dest / path).exists(), f"excluded {path}")
    t.same(before, "installed local work unchanged")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-metadata"])
def fork_of_fork_records_immediate_parent(t: T):
    """Install the fork by explicit HEAD, fork it again: third/demo forkOf is mine/demo."""
    dest, _, _ = prepare(t)
    fork_head = t.rev(dest)
    t.gitpm_ok("install", dest, "--ref", "HEAD")
    p = t.package(MINE)
    t.eq(p["current"], {"version": "0.1.0", "commit": fork_head}, "fork installed at its HEAD")
    third = t.root / "authors/third"
    t.gitpm_ok("fork", MINE, "--ns", "third", "--into", third)
    m = json.loads((third / "plugin.json").read_text())
    t.eq(m["id"], THIRD, "id")
    t.eq(m["forkOf"], {"id": MINE, "version": "0.1.0", "commit": fork_head, "source": t.package(MINE)["source"]}, "immediate parent")
    t.eq(m["replaces"], {"id": MINE, "version": "0.1.0"}, "replaces the immediate parent")
    t.eq(t.git(third, "remote", "get-url", "upstream"), str(dest), "upstream is the mine/demo source")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-metadata", "catalog/register-one-version"])
def catalog_round_trip_keeps_provenance(t: T):
    """Fork release registered in the catalog keeps forkOf; install-catalog restores it."""
    dest, source_commit, _ = prepare(t)
    t.gitpm_ok("commit", dest, "0.2.0")
    t.gitpm_ok("register", dest, "--ref", f"v0.2.0")
    release = json.loads(t.catalog_release(MINE, "0.2.0").read_text())
    forkof = {"id": ID, "version": "0.1.0", "commit": source_commit, "source": t.package(ID)["source"]}
    t.eq(release["manifest"]["forkOf"], forkof, "catalog manifest forkOf")
    t.gitpm_ok("--root", t.root / "ws2", "install-catalog", MINE)
    state = json.loads((t.root / "ws2/state.json").read_text())
    t.eq(state["packages"][MINE]["forkOf"], forkof, "installed forkOf")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/active-fork-rules", "updates/rollback-swaps-current-and-previous"])
def update_and_rollback_keep_provenance(t: T):
    dest, source_commit, _ = prepare(t)
    forkof = {"id": ID, "version": "0.1.0", "commit": source_commit, "source": t.package(ID)["source"]}
    first = (t.gitpm_ok("commit", dest, "0.2.0"), t.rev(dest))[1]
    t.gitpm_ok("install", dest)
    t.eq(t.package(MINE)["current"], {"version": "0.2.0", "commit": first}, "installed 0.2.0")
    t.write(dest, {"dist/index.js": "export const greeting = 'fork 0.3.0';\n"})
    t.gitpm_ok("commit", dest, "0.3.0")
    second = t.rev(dest)
    t.gitpm_ok("update", MINE)
    p = t.package(MINE)
    t.eq((p["current"], p["previous"], p["forkOf"]),
         ({"version": "0.3.0", "commit": second}, {"version": "0.2.0", "commit": first}, forkof), "after update")
    t.gitpm_ok("rollback", MINE)
    p = t.package(MINE)
    t.eq((p["current"], p["previous"], p["forkOf"]),
         ({"version": "0.2.0", "commit": first}, {"version": "0.3.0", "commit": second}, forkof), "after rollback")
    t.eq(t.package(ID)["current"]["version"], "0.1.0", "original untouched")
