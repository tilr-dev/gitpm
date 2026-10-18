"""UC-007: Remote configuration (P1).

`gitpm remote REPO URL` adds or replaces origin (fetch and push URLs), keeps other remotes,
does no network operation and never rewrites a catalog or an installed source.
"""

from gitpmtest import T, case


ID = "dan/hello"


def bare(t: T, name: str):
    path = t.root / f"{name}.git"
    t.run("git", "init", "-q", "--bare", path, check=True)
    return path


def config(t: T, repo, key: str) -> list[str]:
    return t.git(repo, "config", "--get-all", key, check=False).splitlines()


@case(fixture="hello_installed", spec=["authoring/remote-sets-origin-without-network#set-origin", "authoring/remote-sets-origin-without-network#other-remotes-are-kept"])
def set_then_replace_origin(t: T):
    """Set origin, add a push URL and another remote, replace origin: both URLs replaced, others kept."""
    repo = t.repo(ID)
    one, two, push, backup = bare(t, "one"), bare(t, "two"), bare(t, "push"), bare(t, "backup")
    ws_before = t.snap(t.ws, git=True)

    r = t.gitpm_ok("remote", repo, one)
    t.has(r.out, str(one))
    t.eq(config(t, repo, "remote.origin.url"), [str(one)], "origin url")
    t.eq(config(t, repo, "remote.origin.pushurl"), [], "no push url yet")

    t.git(repo, "config", "remote.origin.pushurl", str(push))
    t.git(repo, "remote", "add", "backup", str(backup))
    t.gitpm_ok("remote", repo, two)
    t.eq(config(t, repo, "remote.origin.url"), [str(two)], "origin url replaced")
    t.eq(config(t, repo, "remote.origin.pushurl"), [str(two)], "push url replaced")
    t.eq(t.git(repo, "remote", "get-url", "--push", "origin"), str(two), "effective push url")
    t.eq(config(t, repo, "remote.backup.url"), [str(backup)], "other remote kept")

    for b in (one, two, push, backup):
        t.eq(t.git(b, "for-each-ref"), "", f"nothing pushed to {b.name}")
    t.same(ws_before, "workspace (installed source, clone, dist)")
    t.eq(t.package(ID)["source"], str(repo), "installed source unchanged")
    t.ok(t.catalog_empty(), "no catalog written")


@case(fixture="hello", spec=["authoring/remote-sets-origin-without-network#set-origin"])
def secondary_origin_urls_collapse(t: T):
    """An origin with two fetch URLs ends with exactly the new one."""
    repo = t.repo(ID)
    t.git(repo, "remote", "add", "origin", "https://example.invalid/a.git")
    t.git(repo, "config", "--add", "remote.origin.url", "https://example.invalid/b.git")
    t.gitpm_ok("remote", repo, "git@example.invalid:dan/hello.git")
    t.eq(config(t, repo, "remote.origin.url"), ["git@example.invalid:dan/hello.git"], "origin url")


def refused_case(name: str, doc: str, setup, kind: str, contains: str = ""):
    def fn(t: T):
        target = setup(t)
        before = t.snap(t.root, git=True)
        t.gitpm_err("remote", target, "https://example.invalid/x.git", kind=kind, contains=contains)
        t.same(before, "fixture")
    fn.__name__, fn.__doc__, fn.__module__ = name, doc, __name__
    case(fixture="hello", spec=["authoring/remote-sets-origin-without-network#not-a-package-repository-root" if name == "subdirectory_refused" else "authoring/remote-sets-origin-without-network"])(fn)


def _plain_dir(t: T):
    d = t.root / "plain"
    d.mkdir()
    (d / "plugin.json").write_text('{"id": "dan/plain", "version": "0.1.0"}\n')
    return d


def _repo_without_manifest(t: T):
    d = t.root / "nomanifest"
    d.mkdir()
    t.git(d, "init", "-q", "-b", "main")
    return d


refused_case("non_repository_refused", "A directory that is not a Git repository fails clearly.",
             _plain_dir, "git")
refused_case("missing_path_refused", "A path that does not exist fails clearly.",
             lambda t: t.root / "missing", "io")
refused_case("repository_without_manifest_refused", "A repository without plugin.json fails clearly.",
             _repo_without_manifest, "invalid_manifest", "plugin.json not found")
refused_case("subdirectory_refused", "A subdirectory of a package repository is not its root.",
             lambda t: t.repo(ID) / "dist", "invalid_argument", "repository root")


@case(fixture="hello", spec=["authoring/remote-sets-origin-without-network", "git-safety/no-option-injection"])
def invalid_url_refused(t: T):
    """An empty or option-like URL is refused without touching the repository."""
    repo = t.repo(ID)
    before = t.snap(t.root, git=True)
    t.gitpm_err("remote", repo, "", kind="invalid_argument", contains="invalid git remote URL")
    t.gitpm_err("remote", repo, "   ", kind="invalid_argument")
    t.same(before, "fixture")
