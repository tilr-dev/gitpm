"""Single-package author repositories, a direct install and a local catalog."""

from gitpmtest import T, fixture


@fixture()
def hello(t: T):
    """dan/hello: one release 0.1.0 on main."""
    t.new_package("dan/hello", "0.1.0")


@hello.check
def _(t: T):
    repo = t.repo("dan/hello")
    t.eq(t.git(repo, "tag", "--list"), "dan/hello@0.1.0", "tags")
    t.eq(t.git(repo, "branch", "--show-current"), "main", "branch")
    t.eq(t.git(repo, "status", "--porcelain"), "", "clean worktree")
    t.has(t.git(repo, "show", "dan/hello@0.1.0:plugin.json"), '"version": "0.1.0"')


@fixture(requires=["hello"])
def hello_releases(t: T):
    """dan/hello: releases 0.1.0, 0.1.1, 0.2.0 and an untagged commit on top of main."""
    t.release("dan/hello", "0.1.1")
    t.release("dan/hello", "0.2.0")
    t.commit("dan/hello", "unreleased work", {"dist/index.js": "export const greeting = 'dev';\n"})


@hello_releases.check
def _(t: T):
    repo = t.repo("dan/hello")
    t.eq(t.git(repo, "tag", "--list").split(), ["dan/hello@0.1.0", "dan/hello@0.1.1", "dan/hello@0.2.0"], "tags")
    t.ok(t.rev(repo) != t.rev(repo, "dan/hello@0.2.0"), "HEAD is past the last release")
    t.has(t.git(repo, "show", "HEAD:plugin.json"), '"version": "0.2.0"')


@fixture()
def numeric(t: T):
    """dan/num: releases 1.9.0 and 1.10.0 (numeric, not lexical, ordering)."""
    t.new_package("dan/num", "1.9.0")
    t.release("dan/num", "1.10.0")


@numeric.check
def _(t: T):
    t.eq(sorted(t.git("dan/num", "tag", "--list").split()), ["dan/num@1.10.0", "dan/num@1.9.0"], "tags")


@fixture(requires=["hello"], gitpm=True)
def hello_installed(t: T):
    """dan/hello 0.1.0 installed directly from Git into /fx/ws; no catalog."""
    t.gitpm_ok("install", t.repo("dan/hello"))


@hello_installed.check
def _(t: T):
    p = t.package("dan/hello")
    t.eq(p["current"]["version"], "0.1.0", "current version")
    t.eq(p["current"]["commit"], t.rev("dan/hello", "dan/hello@0.1.0"), "current commit")
    t.eq(p["previous"], None, "previous")
    t.eq(p["enabled"], True, "enabled")
    t.eq(t.rev(t.worktree("dan/hello")), p["current"]["commit"], "clone HEAD")
    t.has((t.dist("dan/hello") / "index.js").read_text(), "dan/hello 0.1.0")
    t.ok(t.catalog_empty(), "no catalog was created")


@fixture(requires=["hello_releases", "numeric"], gitpm=True)
def catalog(t: T):
    """Local catalog /fx/catalog: dan/hello 0.1.0, 0.1.1, 0.2.0 and dan/num 1.9.0, 1.10.0; empty workspace."""
    for id, versions in [("dan/hello", ["0.1.0", "0.1.1", "0.2.0"]), ("dan/num", ["1.9.0", "1.10.0"])]:
        for v in versions:
            t.gitpm_ok("register", t.repo(id), "--ref", f"{id}@{v}")


@catalog.check
def _(t: T):
    t.ok((t.catalog / "catalog.json").is_file(), "catalog.json exists")
    info = t.gitpm_ok("--catalog", t.catalog, "info").out
    t.has(info, "dan/hello", "dan/num")
    for id, versions in [("dan/hello", ["0.1.0", "0.1.1", "0.2.0"]), ("dan/num", ["1.9.0", "1.10.0"])]:
        index = next(t.catalog.rglob(f"{id.split('/')[1]}/plugin.json"))
        t.has(index.read_text(), *[f'"{v}"' for v in versions])
    t.ok(not t.ws.exists(), "workspace untouched")
