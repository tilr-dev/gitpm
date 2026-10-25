"""Single-package author repositories, a direct install and a local catalog."""

from gitpmtest import T, fixture


@fixture()
def hello(t: T):
    """3dce1568-569c-5b53-9321-37185e751947: one release 0.1.0 on main."""
    t.new_package("3dce1568-569c-5b53-9321-37185e751947", "0.1.0")


@hello.check
def _(t: T):
    repo = t.repo("3dce1568-569c-5b53-9321-37185e751947")
    t.eq(t.git(repo, "tag", "--list"), "v0.1.0", "tags")
    t.eq(t.git(repo, "branch", "--show-current"), "main", "branch")
    t.eq(t.git(repo, "status", "--porcelain"), "", "clean worktree")
    t.has(t.git(repo, "show", "v0.1.0:plugin.json"), '"version": "0.1.0"')


@fixture(requires=["hello"])
def hello_releases(t: T):
    """3dce1568-569c-5b53-9321-37185e751947: releases 0.1.0, 0.1.1, 0.2.0 and an untagged commit on top of main."""
    t.release("3dce1568-569c-5b53-9321-37185e751947", "0.1.1")
    t.release("3dce1568-569c-5b53-9321-37185e751947", "0.2.0")
    t.commit("3dce1568-569c-5b53-9321-37185e751947", "unreleased work", {"dist/index.js": "export const greeting = 'dev';\n"})


@hello_releases.check
def _(t: T):
    repo = t.repo("3dce1568-569c-5b53-9321-37185e751947")
    t.eq(t.git(repo, "tag", "--list").split(), ["v0.1.0", "v0.1.1", "v0.2.0"], "tags")
    t.ok(t.rev(repo) != t.rev(repo, "v0.2.0"), "HEAD is past the last release")
    t.has(t.git(repo, "show", "HEAD:plugin.json"), '"version": "0.2.0"')


@fixture()
def numeric(t: T):
    """2190eee8-4be5-5a91-81d2-6ca95c1e61d0: releases 1.9.0 and 1.10.0 (numeric, not lexical, ordering)."""
    t.new_package("2190eee8-4be5-5a91-81d2-6ca95c1e61d0", "1.9.0")
    t.release("2190eee8-4be5-5a91-81d2-6ca95c1e61d0", "1.10.0")


@numeric.check
def _(t: T):
    t.eq(sorted(t.git("2190eee8-4be5-5a91-81d2-6ca95c1e61d0", "tag", "--list").split()), ["v1.10.0", "v1.9.0"], "tags")


@fixture(requires=["hello"], gitpm=True)
def hello_installed(t: T):
    """3dce1568-569c-5b53-9321-37185e751947 0.1.0 installed directly from Git into /fx/ws; no catalog."""
    t.gitpm_ok("install", t.repo("3dce1568-569c-5b53-9321-37185e751947"))


@hello_installed.check
def _(t: T):
    p = t.package("3dce1568-569c-5b53-9321-37185e751947")
    t.eq(p["current"]["version"], "0.1.0", "current version")
    t.eq(p["current"]["commit"], t.rev("3dce1568-569c-5b53-9321-37185e751947", "v0.1.0"), "current commit")
    t.eq(p["previous"], None, "previous")
    t.eq(p["enabled"], True, "enabled")
    t.eq(t.rev(t.clone("3dce1568-569c-5b53-9321-37185e751947")), p["current"]["commit"], "clone HEAD")
    t.has((t.dist("3dce1568-569c-5b53-9321-37185e751947") / "index.js").read_text(), "3dce1568-569c-5b53-9321-37185e751947 0.1.0")
    t.ok(t.catalog_empty(), "no catalog was created")


@fixture(requires=["hello_releases", "numeric"], gitpm=True)
def catalog(t: T):
    """Local catalog /fx/catalog: 3dce1568-569c-5b53-9321-37185e751947 0.1.0, 0.1.1, 0.2.0 and 2190eee8-4be5-5a91-81d2-6ca95c1e61d0 1.9.0, 1.10.0; empty workspace."""
    for id, versions in [("3dce1568-569c-5b53-9321-37185e751947", ["0.1.0", "0.1.1", "0.2.0"]), ("2190eee8-4be5-5a91-81d2-6ca95c1e61d0", ["1.9.0", "1.10.0"])]:
        for v in versions:
            t.gitpm_ok("register", t.repo(id), "--ref", f"v{v}")


@catalog.check
def _(t: T):
    t.ok((t.catalog / "catalog.json").is_file(), "catalog.json exists")
    info = t.gitpm_ok("--catalog", t.catalog, "info").out
    t.has(info, "3dce1568-569c-5b53-9321-37185e751947", "2190eee8-4be5-5a91-81d2-6ca95c1e61d0")
    for id, versions in [("3dce1568-569c-5b53-9321-37185e751947", ["0.1.0", "0.1.1", "0.2.0"]), ("2190eee8-4be5-5a91-81d2-6ca95c1e61d0", ["1.9.0", "1.10.0"])]:
        index = next(t.catalog.rglob(f"{id}/plugin.json"))
        t.has(index.read_text(), *[f'"{v}"' for v in versions])
    t.ok(not t.ws.exists(), "workspace untouched")
