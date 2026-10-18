"""UC-010: Direct install by tag and commit (P0).

Fixture hello_releases: dan/hello releases 0.1.0, 0.1.1, 0.2.0 (distinct dist payloads)
and an untagged commit on top of main.
"""

import json

from gitpmtest import T, case

ID = "dan/hello"


def blob(t: T, repo, ref: str, path: str = "dist/index.js") -> str:
    """Exact committed bytes of a file (t.git strips output)."""
    return t.run("git", "-C", t.path(repo), "show", f"{ref}:{path}", quiet=True, check=True).out


def dist_files(t: T, repo, ref: str) -> list[str]:
    out = t.git(repo, "ls-tree", "-r", "--name-only", ref, "--", "dist")
    return sorted(p.removeprefix("dist/") for p in out.splitlines())


def check_installed(t: T, root, tag: str):
    """state, list, clone HEAD and dist of ID installed at release `tag` under workspace `root`."""
    sha = t.rev(ID, tag)
    version = tag.split("@")[1]
    state = json.loads((root / "state.json").read_text())
    p = state["packages"][ID]
    t.eq(p["current"], {"version": version, "commit": sha}, "current")
    t.eq(p["previous"], None, "previous")
    t.eq(p["enabled"], True, "enabled")
    t.eq(p["source"], str(t.repo(ID).resolve()), "source")
    listed = t.gitpm_ok("--root", root, "list").json()
    t.eq(listed["packages"][ID]["current"], {"version": version, "commit": sha}, "list current")
    clone, dist = root / "packages" / ID, root / "dist" / ID
    t.eq(t.rev(clone), sha, "clone HEAD")
    t.eq((dist / "index.js").read_text(), blob(t, ID, tag), "dist/index.js bytes")
    t.eq(sorted(str(p.relative_to(dist)) for p in dist.rglob("*") if p.is_file()),
         dist_files(t, ID, tag), "dist file set")


@case(fixture="hello_releases", spec=["cli/option-validation#channel-and-ref"])
def install_by_scoped_tag(t: T):
    """Install --ref dan/hello@0.1.1: state, list, clone HEAD and dist match the tag; source repo unchanged."""
    source = t.snap(t.repo(ID), git=True)
    t.gitpm_ok("install", t.repo(ID), "--ref", f"{ID}@0.1.1")
    check_installed(t, t.ws, f"{ID}@0.1.1")
    t.has((t.dist(ID) / "index.js").read_text(), "dan/hello 0.1.1")
    t.same(source, "author repository")


@case(fixture="hello_releases", spec=["install-uninstall/install-an-explicit-ref-or-the-dev-channel#install-by-exact-commit"])
def install_by_commit_into_second_workspace(t: T):
    """Tag 0.2.0 into the default workspace, exact SHA of 0.1.0 into another --root: independent."""
    source = t.snap(t.repo(ID), git=True)
    sha = t.rev(ID, f"{ID}@0.1.0")
    t.gitpm_ok("install", t.repo(ID), "--ref", f"{ID}@0.2.0")
    first = t.snap(t.ws, git=True)
    other = t.root / "ws-sha"
    t.gitpm_ok("--root", other, "install", t.repo(ID), "--ref", sha)
    check_installed(t, other, f"{ID}@0.1.0")
    check_installed(t, t.ws, f"{ID}@0.2.0")
    t.same(first, "default workspace after installing into the other root")
    t.same(source, "author repository")


@case(fixture="hello_releases", spec=["dist/deploys-copy-the-committed-tree"])
def uncommitted_source_edits_are_not_installed(t: T):
    """Modified and untracked files in the author repo never reach the clone or dist."""
    t.write(ID, {"dist/index.js": "export const greeting = 'UNCOMMITTED';\n",
                 "dist/new.js": "export const extra = 'untracked';\n"})
    source = t.snap(t.repo(ID), git=True)
    t.gitpm_ok("install", t.repo(ID), "--ref", f"{ID}@0.1.1")
    check_installed(t, t.ws, f"{ID}@0.1.1")
    t.lacks((t.dist(ID) / "index.js").read_text(), "UNCOMMITTED")
    t.ok(not (t.dist(ID) / "new.js").exists(), "untracked dist/new.js not deployed")
    t.ok(not (t.clone(ID) / "dist/new.js").exists(), "untracked dist/new.js not in clone")
    t.eq(t.git(t.clone(ID), "status", "--porcelain", "--ignored"), "", "clone is clean")
    t.same(source, "author repository (edits kept, nothing committed)")
    t.has(t.git(ID, "status", "--porcelain"), "dist/index.js", "dist/new.js")


@case(fixture="hello_releases", spec=["install-uninstall/install-is-idempotent-for-the-same-source#repeat-install",
                                     "install-uninstall/install-is-idempotent-for-the-same-source#newer-release-available"])
def repeated_install_from_the_same_source_is_a_noop(t: T):
    """A second install of the same ID from the same source (same ref, other ref, no ref) exits 0
    with `ID already installed (0.1.0)` (plus the update hint when another revision is asked for)
    and changes nothing."""
    t.gitpm_ok("install", t.repo(ID), "--ref", f"{ID}@0.1.0")
    before = t.snap(t.ws, git=True)
    for args, hint in [(("--ref", f"{ID}@0.1.0"), ""),
                       (("--ref", f"{ID}@0.2.0"), f"; requested --ref {ID}@0.2.0 — run update {ID} --ref {ID}@0.2.0"),
                       ((), "; run update")]:
        r = t.gitpm_ok("install", t.repo(ID), *args)
        t.eq(r.out, f"{ID} already installed (0.1.0){hint}\n", "output")
        t.same(before, "workspace")
    t.eq(t.package(ID)["current"]["version"], "0.1.0", "installed version kept")


@case(fixture="hello_releases", spec=["install-uninstall/same-id-from-another-source-is-refused#other-source"])
def same_id_from_another_source_is_rejected(t: T):
    """The same ID from another source (a mirror clone) fails with already_installed; state untouched."""
    t.gitpm_ok("install", t.repo(ID), "--ref", f"{ID}@0.1.0")
    before = t.snap(t.ws, git=True)
    mirror = t.root / "mirror/hello"
    t.run("git", "clone", "-q", t.repo(ID), mirror, check=True)
    t.gitpm_err("install", mirror, "--ref", f"{ID}@0.1.1", kind="already_installed", contains=ID)
    t.same(before, "workspace")
    p = t.package(ID)
    t.eq(p["current"]["version"], "0.1.0", "installed version kept")
    t.eq(p["source"], str(t.repo(ID).resolve()), "installed source kept")
