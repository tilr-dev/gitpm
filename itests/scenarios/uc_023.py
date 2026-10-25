"""UC-023: Fork isolates installation work and preserves history and provenance (P0).

dd3387ed-a5a3-5059-81ad-b1f6987a28f2 0.1.0 is installed (g3_vendor_installed). Each case adds a tracked edit and an
untracked file to the installed clone and forks the recorded source with `--ns author --into DIR` into a folder
whose basename differs from the package name.
"""

import json

from gitpmtest import T, case

ID, NEW = "dd3387ed-a5a3-5059-81ad-b1f6987a28f2", "be517c52-cf84-5615-9a18-9fae7d82c819"
EDIT = "export const greeting = 'local edit';\n"


def dirty_and_fork(t: T):
    clone = t.clone(ID)
    t.write(clone, {"dist/index.js": EDIT, "notes/untracked.txt": "mine\n"})
    before = t.snap(t.ws, t.root / "repos", git=True)
    dest = t.root / "authors/some-other-name"
    t.gitpm_ok("fork", ID, "--into", dest)
    NEW = t.manifest_id(dest)
    return dest, before


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-metadata"])
def history_and_branch(t: T):
    """History is kept: the identity commit sits on top of the installed commit on branch fork."""
    installed = t.package(ID)["current"]["commit"]
    dest, _ = dirty_and_fork(t)
    NEW = t.manifest_id(dest)
    t.eq(t.git(dest, "branch", "--show-current"), "fork", "branch")
    t.eq(t.rev(dest, "HEAD~1"), installed, "parent of the identity commit")
    t.eq(t.git(dest, "log", "-1", "--format=%s"), f"Fork {ID} as {NEW}", "identity commit message")
    t.eq(t.git(dest, "log", "--format=%H", "--reverse").splitlines()[0],
         t.git("dd3387ed-a5a3-5059-81ad-b1f6987a28f2", "log", "--format=%H", "--reverse").splitlines()[0], "root commit")
    t.has(t.git(dest, "tag", "--list"), "v0.1.0")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-isolates-source-and-destination-state#installed-clone-has-local-work"])
def local_changes_preserved_uncommitted(t: T):
    """Local commits and staged, unstaged and untracked edits stay only in the installation."""
    clone = t.clone(ID)
    installed = t.package(ID)["current"]["commit"]
    original = t.git(clone, "show", f"{installed}:dist/index.js")
    t.commit(clone, "Installation-only work", {"local-commit.txt": "local commit\n"})
    t.write(clone, {"staged.txt": "staged work\n"})
    t.git(clone, "add", "staged.txt")
    dest, before = dirty_and_fork(t)
    t.eq((dest / "dist/index.js").read_text().strip(), original, "recorded source content")
    for path in ("local-commit.txt", "staged.txt", "notes/untracked.txt"):
        t.ok(not (dest / path).exists(), f"installation-only {path} excluded")
    t.eq(t.git(dest, "status", "--porcelain", "--untracked-files=all"), "", "clean fork")
    t.eq(t.rev(dest, "HEAD~1"), installed, "installed revision, excluding local commits")
    t.same(before, "local installation work and recorded source")
    t.eq(t.git(dest, "show", "--name-only", "--format=", "HEAD"), "plugin.json", "identity commit files")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-metadata#fork-manifest"])
def identity_version_and_provenance(t: T):
    """ID be517c52-cf84-5615-9a18-9fae7d82c819 regardless of the destination basename; version kept; forkOf records the
    original ID/version/commit; replaces declares dd3387ed-a5a3-5059-81ad-b1f6987a28f2 0.1.0; no pm.* Git config."""
    installed = t.package(ID)["current"]["commit"]
    dest, _ = dirty_and_fork(t)
    NEW = t.manifest_id(dest)
    m = json.loads((dest / "plugin.json").read_text())
    t.eq(m["id"], NEW, "id")
    t.eq(m["version"], "0.1.0", "version")
    t.eq(m["forkOf"], {"id": ID, "version": "0.1.0", "commit": installed, "source": t.package(ID)["source"]}, "forkOf")
    t.eq(m["replaces"], {"id": ID, "version": "0.1.0"}, "replaces")
    t.eq(json.loads(t.git(dest, "show", "HEAD:plugin.json")), m, "committed manifest")
    # forkOf is the only provenance: no pm.forkBase/pm.forkVersion Git config.
    r = t.run("git", "-C", dest, "config", "--local", "--get-regexp", r"^pm\.")
    t.eq((r.code, r.out), (1, ""), "no pm.* Git config")
    t.eq(t.git(dest, "tag", "--list"), t.git(ID, "tag", "--list"), "fork preserves upstream tags and creates none")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-copies-an-installed-package"])
def remotes_follow_readme(t: T):
    """README: origin removed, upstream = source of the original installation."""
    dest, _ = dirty_and_fork(t)
    NEW = t.manifest_id(dest)
    t.eq(t.git(dest, "remote").split(), ["upstream"], "remotes")
    t.eq(t.git(dest, "remote", "get-url", "upstream"), t.package(ID)["source"], "upstream url")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-copies-an-installed-package"])
def existing_upstream_is_replaced(t: T):
    """An upstream already configured in the installed clone is replaced by the source."""
    t.git(t.clone(ID), "remote", "add", "upstream", "https://example.invalid/other.git")
    dest = t.root / "authors/x"
    t.gitpm_ok("fork", ID, "--into", dest)
    NEW = t.manifest_id(dest)
    t.eq(t.git(dest, "remote", "get-url", "upstream"), t.package(ID)["source"], "upstream url")
    t.eq(t.git(dest, "remote").split(), ["upstream"], "remotes")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-copies-an-installed-package#fork-one-package"])
def original_installation_unchanged(t: T):
    """Original clone (incl. .git), dist, state and author repository are byte-identical."""
    dest, before = dirty_and_fork(t)
    t.same(before, "workspace and author repository")
    t.eq(sorted(t.state()["packages"]), sorted([ID]), "fork is not installed")
    t.ok(dest.is_dir(), "fork created")
