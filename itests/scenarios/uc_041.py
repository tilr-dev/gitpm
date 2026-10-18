"""UC-041: Symlinks, special files and managed-path safety (P0).

Sentinels live outside the workspace (/fx/outside); every case asserts they are
byte-identical afterwards. FIFOs are created with mkfifo and every command runs with a
bounded timeout, so a copy that blocked on a FIFO would fail the case instead of hanging.
"""

import json
import os
from pathlib import Path

from gitpmtest import T, case

ID = "dan/hello"
OUTSIDE = Path("/fx/outside")


def sentinels(t: T):
    """Outside files a broken path check could follow, overwrite or delete; also a Git
    repository that looks like an installed plugin."""
    t.write(OUTSIDE, {"sentinel.txt": "do not touch\n", "evil/plugin.json": '{"id": "evil", "version": "0.1.0"}\n',
                      "evil/dist/index.js": "evil\n", "x/plugin.json": '{"id": "evil/x", "version": "0.1.0"}\n',
                      "x/dist/index.js": "x\n"})
    for repo in ["evil", "x"]:
        t.git(OUTSIDE / repo, "init", "-q", "-b", "main")
        t.git(OUTSIDE / repo, "add", "--all")
        t.git(OUTSIDE / repo, "commit", "-q", "-m", "sentinel")
    return t.snap(OUTSIDE, git=True)


def symlinked_plugin(t: T, id: str, links: dict[str, str], version: str = "0.1.0") -> str:
    """A plugin release whose commit tracks the given symlinks (path -> target)."""
    repo = t.repo(id)
    if not repo.exists():
        (repo / "dist").mkdir(parents=True)
        t.git(repo, "init", "-q", "-b", "main")
    t.write(repo, {"plugin.json": json.dumps({"id": id, "version": version}) + "\n",
                   "dist/index.js": f"export const v = '{id} {version}';\n", "README.md": "readme\n"})
    for path, target in links.items():
        (repo / path).parent.mkdir(parents=True, exist_ok=True)
        if os.path.lexists(repo / path):
            os.unlink(repo / path)
        os.symlink(target, repo / path)
    t.git(repo, "add", "--all")
    t.git(repo, "commit", "-q", "-m", f"{id} {version}")
    t.git(repo, "tag", f"{id}@{version}")
    return t.rev(repo)


@case(fixture="hello_installed", spec=["transactions-locking/managed-paths-are-never-followed-through-symlinks", "package-manifest/package-folder-contents"])
def dist_symlink_refused_at_install(t: T):
    """dist/ with a relative and an absolute symlink: install refused (unsupported), no
    partial installation, the unrelated plugin and the outside target untouched."""
    outside = sentinels(t)
    for id, target in [("dan/rel", "index.js"), ("dan/abs", str(OUTSIDE / "sentinel.txt"))]:
        symlinked_plugin(t, id, {"dist/link.js": target})
        before = t.snap(t.ws)
        t.gitpm_err("install", t.repo(id), kind="unsupported", contains="symlinks and submodules are not supported")
        t.not_installed(id)
        t.same(before, "workspace")
    t.same(outside, "outside sentinels")


@case(fixture="catalog", spec=["catalog/register-one-version", "package-manifest/package-folder-contents"])
def dist_symlink_refused_at_register(t: T):
    outside = sentinels(t)
    symlinked_plugin(t, "dan/abs", {"dist/link.js": str(OUTSIDE / "sentinel.txt")})
    before = t.snap(t.catalog)
    t.gitpm_err("register", t.repo("dan/abs"), "--ref", "dan/abs@0.1.0", kind="unsupported",
             contains="build contains a symlink")
    t.same(before, "catalog")
    t.same(outside, "outside sentinels")


@case(fixture="hello_installed", spec=["transactions-locking/managed-paths-are-never-followed-through-symlinks"])
def repository_symlink_is_kept_not_followed(t: T):
    """A tracked symlink outside dist/ pointing outside the workspace stays a symlink
    through install, update and fork; its target is never read into or written by gitpm."""
    outside = sentinels(t)
    target = str(OUTSIDE / "sentinel.txt")
    symlinked_plugin(t, "dan/linky", {"docs/outside": target})
    t.gitpm_ok("install", t.repo("dan/linky"))
    link = t.clone("dan/linky") / "docs/outside"
    t.ok(link.is_symlink() and os.readlink(link) == target, "clone keeps the symlink")
    t.eq(sorted(p.name for p in t.dist("dan/linky").iterdir()), ["index.js"], "dist has plain files only")
    symlinked_plugin(t, "dan/linky", {"docs/outside": target}, version="0.2.0")
    t.gitpm_ok("update", "dan/linky")
    t.ok(link.is_symlink() and os.readlink(link) == target, "symlink kept through update")
    fork = t.root / "forks/linky"
    t.gitpm_ok("fork", "dan/linky", "--ns", "mine", "--into", fork)
    t.ok((fork / "docs/outside").is_symlink() and os.readlink(fork / "docs/outside") == target, "fork copies the link")
    t.gitpm_ok("uninstall", "dan/linky", "--force")
    t.not_installed("dan/linky")
    t.same(outside, "outside sentinels")


@case(fixture="hello_installed", spec=["errors/failing-operations-leave-the-workspace-unchanged"])
def fifo_in_clone_is_refused_without_hanging(t: T):
    """A FIFO in the installed clone: fork (copies the clone) refuses it as an unsupported
    file and leaves no destination; update (stages a copy of the clone; git does not
    report FIFOs) refuses it the same way; nothing blocks."""
    outside = sentinels(t)
    os.mkfifo(t.clone(ID) / "pipe")
    before = t.snap(t.ws)
    fork = t.root / "forks/hello"
    t.gitpm_err("fork", ID, "--ns", "mine", "--into", fork, kind="unsupported", contains="unsupported file", timeout=30)
    t.ok(not fork.exists(), "no partial fork destination")
    t.release(ID, "0.1.1")
    t.gitpm_err("update", ID, kind="unsupported", contains="unsupported file: /fx/ws/plugins/dan/hello/pipe", timeout=30)
    t.same(before, "workspace")
    t.gitpm_ok("uninstall", ID, "--force", timeout=30)
    t.not_installed(ID)
    t.same(outside, "outside sentinels")


BAD_IDS = ["../../outside/evil", "/fx/outside/evil", "dan/../../../outside/evil", "..", ".", "dan/./hello",
           "dan/hello/", "./dan", "dan//hello", "dan/hello/extra", "", "-rf"]


@case(fixture="hello_installed", spec=["package-ids/package-id-syntax#unsafe-components-are-refused"])
def traversal_and_absolute_ids_are_refused(t: T):
    outside = sentinels(t)
    before = t.snap(t.ws)
    for bad in BAD_IDS:
        for args in [("uninstall", bad, "--force"), ("forget", bad, "--force"), ("disable", bad, "--force"),
                     ("enable", bad), ("restore", bad), ("update", bad), ("rollback", bad),
                     ("install-catalog", bad), ("fork", bad, "--ns", "mine", "--into", str(t.root / "forks/x")),
                     ("replace", ID, "--with", bad)]:
            r = t.gitpm(*args)
            t.ok(r.code != 0, f"gitpm {' '.join(args)} refused")
            if bad not in ("", "-rf"):
                t.has(r.err, "Error kind: invalid_id")
    t.same(before, "workspace")
    t.same(outside, "outside sentinels")
    t.ok(not (t.root / "forks").exists(), "no fork destination")


@case(fixture="hello_installed", spec=["package-ids/package-id-syntax#unsafe-components-are-refused", "install-uninstall/refused-input-creates-nothing"])
def manifest_with_a_path_id_is_refused(t: T):
    outside = sentinels(t)
    before = t.snap(t.ws)
    for bad in ["../../outside/evil", "/fx/outside/evil", "dan/../evil"]:
        repo = t.root / "authors" / str(abs(hash(bad)))
        t.write(repo, {"plugin.json": json.dumps({"id": bad, "version": "0.1.0"}) + "\n", "dist/index.js": "x\n"})
        t.git(repo, "init", "-q", "-b", "main")
        t.git(repo, "add", "--all")
        t.git(repo, "commit", "-q", "-m", "bad id")
        t.gitpm_err("install", repo, "--channel", "dev", kind="invalid_id")
    t.same(before, "workspace")
    t.same(outside, "outside sentinels")
    t.eq(list((t.ws / ".pm-txn").glob("*")) if (t.ws / ".pm-txn").exists() else [], [], "no leftover transaction")


@case(fixture="hello_installed", spec=["install-uninstall/only-manager-owned-files-are-deleted"])
def namespace_folder_cannot_be_removed(t: T):
    t.new_plugin("dan/other", "0.1.0")
    t.gitpm_ok("install", t.repo("dan/other"))
    before = t.snap(t.ws)
    t.gitpm_err("uninstall", "dan", "--force", kind="repository_not_found", contains="a namespace directory is not a plugin")
    t.gitpm_err("uninstall", "dan", kind="repository_not_found")
    t.gitpm_err("restore", "dan", kind="repository_not_found")
    for args in [("forget", "dan", "--force"), ("disable", "dan", "--force"), ("update", "dan")]:
        t.gitpm_err(*args, kind="not_installed", contains="plugin dan is not installed")
    t.gitpm_err("uninstall", "Dan/Hello", "--force")
    t.same(before, "workspace")
    t.eq(sorted(t.state()["plugins"]), ["dan/hello", "dan/other"], "both plugins kept")


@case(fixture="hello_installed", spec=["transactions-locking/managed-paths-are-never-followed-through-symlinks"])
def symlinked_namespace_folder_is_never_followed(t: T):
    """plugins/evil is a symlink to /fx/outside: uninstall/forget of evil/x and an
    install into the evil namespace are refused; the outside repository survives."""
    outside = sentinels(t)
    os.symlink(OUTSIDE, t.ws / "plugins/evil")
    os.symlink(OUTSIDE, t.ws / "dist/evil")
    before = t.snap(t.ws)
    r = t.gitpm("uninstall", "evil/x", "--force")
    t.ok(r.code != 0, "uninstall through a symlinked namespace refused")
    t.has(r.err, "managed path is a symlink")
    t.new_plugin("evil/y", "0.1.0")
    t.gitpm_err("install", t.repo("evil/y"), kind="workspace", contains="managed path is a symlink")
    t.same(before, "workspace")
    t.same(outside, "outside sentinels")
    t.ok(not (OUTSIDE / "y").exists(), "nothing written through the symlink")
