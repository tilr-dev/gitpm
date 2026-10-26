"""Group g1 (UC-001..009, 061, 063): a workspace and a catalog that both exist, so read-only
commands (auth check, usage errors) can be shown to leave both untouched."""

from gitpmtest import T, fixture


@fixture(requires=["catalog"], gitpm=True)
def g1_ws_catalog(t: T):
    """`catalog` plus 3dce1568-569c-5b53-9321-37185e751947 0.1.1 installed from it into /fx/ws (an update to 0.2.0 is available)."""
    t.gitpm_ok("install-catalog", "3dce1568-569c-5b53-9321-37185e751947", "--version", "0.1.1")


@g1_ws_catalog.check
def _(t: T):
    p = t.package("3dce1568-569c-5b53-9321-37185e751947")
    t.eq(p["current"]["version"], "0.1.1", "installed version")
    t.eq(t.rev(t.worktree("3dce1568-569c-5b53-9321-37185e751947")), p["current"]["commit"], "clone HEAD")
    t.has((t.dist("3dce1568-569c-5b53-9321-37185e751947") / "index.js").read_text(), "3dce1568-569c-5b53-9321-37185e751947 0.1.1")
    t.ok(t.catalog_index("3dce1568-569c-5b53-9321-37185e751947").is_file(), "catalog index of 3dce1568-569c-5b53-9321-37185e751947")
    t.has(t.gitpm_ok("check").out, "update available")
