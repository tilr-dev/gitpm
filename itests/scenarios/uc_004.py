"""UC-004: Init and create scaffold (P0).

`gitpm init NS/NAME --into DIR` (`create` = alias). Scaffold layout (plugin.json, dist/index.js, README.md, .gitignore), branch main, one
commit with the scoped 0.1.0 tag, installability, identity fallback configured only in
the new repository, and refusal on an existing directory with a sentinel. Init never
adds an origin, a catalog entry or an installation.
"""

import hashlib
import json
from pathlib import Path

from gitpmtest import T, case

def inspect_scaffold(t: T, repo: Path, plugin_id: str):
    name = plugin_id.split("/")[1]
    manifest = json.loads((repo / "plugin.json").read_text())
    t.eq(manifest, {"id": plugin_id, "version": "0.1.0", "dependencies": {}, "title": name}, "plugin.json")
    t.has((repo / "dist/index.js").read_text(), "export const greeting", f"{name} 0.1.0")
    t.has((repo / "README.md").read_text(), f"# {name}", f"--ref {plugin_id}@0.1.0")
    t.ok((repo / ".gitignore").is_file(), ".gitignore exists")
    t.eq(t.git(repo, "branch", "--show-current"), "main", "branch")
    t.eq(t.git(repo, "rev-list", "--count", "HEAD"), "1", "commits")
    t.eq(t.git(repo, "tag", "--list"), f"{plugin_id}@0.1.0", "tags")
    t.eq(t.rev(repo, f"{plugin_id}@0.1.0"), t.rev(repo), "tag points at HEAD")
    t.eq(t.git(repo, "ls-files").split(), [".gitignore", "README.md", "dist/index.js", "plugin.json"], "tracked")
    t.eq(t.git(repo, "status", "--porcelain", "--ignored"), "", "clean tree")
    t.eq(t.git(repo, "remote"), "", "no remote")
    t.ok(not t.ws.exists(), "no installation (workspace absent)")
    t.ok(not t.catalog.exists(), "no catalog entry (catalog absent)")


def install_scaffold(t: T, repo: Path, plugin_id: str):
    t.gitpm_ok("install", repo, "--ref", f"{plugin_id}@0.1.0")
    p = t.package(plugin_id)
    t.eq(p["current"], {"version": "0.1.0", "commit": t.rev(repo)}, "installed release")
    t.eq((t.dist(plugin_id) / "index.js").read_bytes(), (repo / "dist/index.js").read_bytes(), "dist bytes")


SCAFFOLD = "authoring/init-creates-a-package-with-its-first-release"


@case(spec=[f"{SCAFFOLD}#scaffold-files-and-commit", "install-uninstall/install-an-explicit-ref-or-the-dev-channel"])
def init_nested_relative_path(t: T):
    """init into a new nested relative path: parents created, ID local/demo1, installable."""
    t.gitpm_ok("init", "local/demo1", "--into", "deep/nested dir/demo1")
    repo = t.root / "deep/nested dir/demo1"
    inspect_scaffold(t, repo, "local/demo1")
    install_scaffold(t, repo, "local/demo1")


@case(spec=[f"{SCAFFOLD}#new-package-repository"])
def init_absolute_path_with_spaces(t: T):
    """init into a new absolute path with spaces."""
    repo = t.root / "my packages/sub dir/demo2"
    r = t.gitpm_ok("init", "dan/demo2", "--into", repo)
    t.has(r.out, "ID: dan/demo2", "dan/demo2@0.1.0")
    inspect_scaffold(t, repo, "dan/demo2")
    install_scaffold(t, repo, "dan/demo2")


@case(spec=[f"{SCAFFOLD}#new-package-repository"])
def init_into_siblings_gives_separate_repositories(t: T):
    """Two inits into sibling paths give two independent repositories."""
    t.gitpm_ok("init", "local/one", "--into", "x/one")
    t.gitpm_ok("init", "local/two", "--into", "x/two")
    for name in ("one", "two"):
        inspect_scaffold(t, t.root / "x" / name, f"local/{name}")
    t.ok(not (t.root / "x/.git").exists(), "parent is not a repository")


@case(spec=[SCAFFOLD, "authoring/git-identity-is-never-invented-in-existing-repositories"])
def missing_identity_is_repository_local(t: T):
    """Without a Git identity, Package Demo <demo@example.invalid> is set only in the new repository."""
    gitconfig = t.root / "empty-gitconfig"
    gitconfig.write_text("[init]\n\tdefaultBranch = main\n")
    digest = hashlib.sha256(gitconfig.read_bytes()).hexdigest()
    env = {"GIT_CONFIG_GLOBAL": str(gitconfig), "HOME": str(t.root / "home")}
    unset = ["env"] + [x for v in ("EMAIL", "GIT_AUTHOR_NAME", "GIT_AUTHOR_EMAIL", "GIT_COMMITTER_NAME",
                                   "GIT_COMMITTER_EMAIL") for x in ("-u", v)]
    (t.root / "home").mkdir()
    # Sanity: this environment really has no identity.
    t.ok(t.run(*unset, "git", "config", "user.name", env=env).code != 0, "no user.name in the isolated config")
    r = t.run(*unset, "gitpm", "init", "local/demo3", "--into", "noid/demo3", env=env)
    t.eq(r.code, 0, "init exit without identity")
    repo = t.root / "noid/demo3"
    t.eq(t.git(repo, "config", "--local", "user.name"), "Package Demo", "local user.name")
    t.eq(t.git(repo, "config", "--local", "user.email"), "demo@example.invalid", "local user.email")
    t.eq(t.git(repo, "log", "-1", "--format=%an <%ae>"), "Package Demo <demo@example.invalid>", "author")
    t.eq(hashlib.sha256(gitconfig.read_bytes()).hexdigest(), digest, "global config unchanged")
    t.eq(list((t.root / "home").iterdir()), [], "nothing written to HOME")
    t.eq(t.git(repo, "tag", "--list"), "local/demo3@0.1.0", "tag")


@case(spec=[SCAFFOLD])
def existing_identity_is_not_overridden(t: T):
    """With a configured identity no local user.* is written."""
    t.gitpm_ok("init", "local/demo4", "--into", "withid/demo4")
    repo = t.root / "withid/demo4"
    t.ok(t.git(repo, "config", "--local", "user.name", check=False) == "", "no local user.name")
    t.eq(t.git(repo, "log", "-1", "--format=%an"), "gitpm Test", "author from the global identity")


def existing_case(command: str):
    def fn(t: T):
        target = t.root / "existing" / "demo5"
        target.mkdir(parents=True)
        (target / "keep.txt").write_text("sentinel")
        before = t.snap(t.root, git=True)
        t.gitpm_err(command, "dan/demo5", "--into", target, kind="invalid_argument",
                    contains="exists but is not a Git repository")
        t.same(before, "fixture (sentinel directory)")
        t.ok(not (target / ".git").exists(), "no repository inside the existing directory")
        t.ok(not t.ws.exists() and not t.catalog.exists(), "no workspace or catalog")
    fn.__name__ = f"{command}_existing_directory_preserved"
    fn.__doc__ = f"{command} --into an existing non-repository directory refuses and changes nothing."
    fn.__module__ = __name__
    case(spec=["authoring/init-into-a-multi-package-repository#existing-folder-that-is-not-a-repository"])(fn)


existing_case("init")


@case(spec=["authoring/init-creates-a-package-with-its-first-release", "cli/command-set#removed-command"])
def create_is_an_unknown_command(t: T):
    """The former alias `create` fails with `unknown command: create; use --help` and creates nothing."""
    before = t.snap(t.root, git=True)
    r = t.gitpm("create", "acme/hello", "--into", "hello")
    t.ok(r.code != 0, "create refused")
    t.has(r.err, "unknown command: create; use --help")
    t.same(before, "fixture")
    t.ok(not (t.root / "hello").exists(), "nothing created")
