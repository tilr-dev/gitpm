"""Group g1 (UC-001..009, 061, 063): a workspace and a catalog that both exist, so read-only
commands (auth check, usage errors) can be shown to leave both untouched."""

from gitpmtest import T, fixture


@fixture(requires=["catalog"], gitpm=True)
def g1_ws_catalog(t: T):
    """`catalog` plus dan/hello 0.1.1 installed from it into /fx/ws (an update to 0.2.0 is available)."""
    t.gitpm_ok("install-catalog", "dan/hello", "--version", "0.1.1")


@g1_ws_catalog.check
def _(t: T):
    p = t.package("dan/hello")
    t.eq(p["current"]["version"], "0.1.1", "installed version")
    t.eq(t.rev(t.worktree("dan/hello")), p["current"]["commit"], "clone HEAD")
    t.has((t.dist("dan/hello") / "index.js").read_text(), "dan/hello 0.1.1")
    t.ok(t.catalog_index("dan/hello").is_file(), "catalog index of dan/hello")
    t.has(t.gitpm_ok("check").out, "update available")
