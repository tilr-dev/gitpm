"""Group 2 fixtures: namespaces, tag forms, update history and guarded clones (UC-010…021)."""

from gitpmtest import T, fixture

HELLO = "3dce1568-569c-5b53-9321-37185e751947"


@fixture()
def g2_demos(t: T):
    """dd3387ed-a5a3-5059-81ad-b1f6987a28f2 and be517c52-cf84-5615-9a18-9fae7d82c819: same basename, different namespaces and payloads; 0.1.0 each."""
    t.new_package("dd3387ed-a5a3-5059-81ad-b1f6987a28f2", "0.1.0")
    t.new_package("be517c52-cf84-5615-9a18-9fae7d82c819", "0.1.0")


@g2_demos.check
def _(t: T):
    for id in ["dd3387ed-a5a3-5059-81ad-b1f6987a28f2", "be517c52-cf84-5615-9a18-9fae7d82c819"]:
        t.eq(t.git(id, "tag", "--list"), f"v0.1.0", f"{id} tags")
        t.has(t.git(id, "show", "HEAD:dist/index.js"), f"{id} 0.1.0")


@fixture(requires=["g2_demos"], gitpm=True)
def g2_demos_installed(t: T):
    """dd3387ed-a5a3-5059-81ad-b1f6987a28f2 and be517c52-cf84-5615-9a18-9fae7d82c819 0.1.0 both installed directly from Git into /fx/ws."""
    t.gitpm_ok("install", t.repo("dd3387ed-a5a3-5059-81ad-b1f6987a28f2"))
    t.gitpm_ok("install", t.repo("be517c52-cf84-5615-9a18-9fae7d82c819"))


@g2_demos_installed.check
def _(t: T):
    for id in ["dd3387ed-a5a3-5059-81ad-b1f6987a28f2", "be517c52-cf84-5615-9a18-9fae7d82c819"]:
        p = t.package(id)
        t.eq(p["current"], {"version": "0.1.0", "commit": t.rev(id)}, f"{id} current")
        t.eq(p["enabled"], True, f"{id} enabled")
        t.eq(t.rev(t.worktree(id)), p["current"]["commit"], f"{id} clone HEAD")
        t.has((t.dist(id) / "index.js").read_text(), f"{id} 0.1.0")
    t.eq(sorted(t.state()["packages"]), ["be517c52-cf84-5615-9a18-9fae7d82c819", "dd3387ed-a5a3-5059-81ad-b1f6987a28f2"], "installed")


def _reverse(t: T, id: str, annotated_first: bool):
    """1.10.0 committed and tagged first, 1.9.0 on top, then an untagged 1.11.0 HEAD."""
    t.new_package(id, "1.10.0", tag=False)
    if annotated_first:
        t.git(id, "tag", "-a", f"v1.10.0", "-m", "release 1.10.0")
    else:
        t.tag(id, f"v1.10.0")
    t.release(id, "1.9.0", tag=False)
    if annotated_first:
        t.tag(id, f"v1.9.0")
    else:
        t.git(id, "tag", "-a", f"v1.9.0", "-m", "release 1.9.0")
    t.release(id, "1.11.0", tag=False)


@fixture()
def g2_reverse_tags(t: T):
    """d0a24c12-6c98-54aa-9226-f2a923716f63: 1.10.0 (annotated tag) created before 1.9.0 (lightweight) and an untagged
    1.11.0 HEAD; ac004289-237b-5412-9383-e963b2c10d1d: the same with lightweight 1.10.0 and annotated 1.9.0."""
    _reverse(t, "d0a24c12-6c98-54aa-9226-f2a923716f63", annotated_first=True)
    _reverse(t, "ac004289-237b-5412-9383-e963b2c10d1d", annotated_first=False)


@g2_reverse_tags.check
def _(t: T):
    for id, kinds in [("d0a24c12-6c98-54aa-9226-f2a923716f63", ("tag", "commit")), ("ac004289-237b-5412-9383-e963b2c10d1d", ("commit", "tag"))]:
        t.eq(sorted(t.git(id, "tag", "--list").split()), [f"v1.10.0", f"v1.9.0"], f"{id} tags")
        t.eq((t.git(id, "cat-file", "-t", f"refs/tags/v1.10.0"),
              t.git(id, "cat-file", "-t", f"refs/tags/v1.9.0")), kinds, f"{id} tag object types")
        old, new = t.rev(id, f"v1.10.0"), t.rev(id, f"v1.9.0")
        t.eq(t.git(id, "rev-list", "--count", f"{old}..{new}"), "1", f"{id}: 1.9.0 is committed after 1.10.0")
        t.has(t.git(id, "show", "HEAD:plugin.json"), '"version": "1.11.0"')
        t.eq(t.git(id, "tag", "--points-at", "HEAD"), "", f"{id} HEAD untagged")


@fixture(requires=["hello_installed"], gitpm=True)
def g2_hello_history(t: T):
    """3dce1568-569c-5b53-9321-37185e751947 installed at 0.1.0, updated to 0.1.1 and then 0.1.2: current 0.1.2, previous 0.1.1."""
    t.release(HELLO, "0.1.1")
    t.gitpm_ok("update", HELLO)
    t.release(HELLO, "0.1.2")
    t.gitpm_ok("update", HELLO)


@g2_hello_history.check
def _(t: T):
    p = t.package(HELLO)
    t.eq(p["current"], {"version": "0.1.2", "commit": t.rev(HELLO, f"v0.1.2")}, "current")
    t.eq(p["previous"], {"version": "0.1.1", "commit": t.rev(HELLO, f"v0.1.1")}, "previous")
    t.eq(p["enabled"], True, "enabled")
    t.eq(t.rev(t.worktree(HELLO)), p["current"]["commit"], "clone HEAD")
    t.has((t.dist(HELLO) / "index.js").read_text(), "3dce1568-569c-5b53-9321-37185e751947 0.1.2")
    t.eq(t.git(t.worktree(HELLO), "status", "--porcelain", "--ignored"), "", "clean clone")


@fixture(requires=["g2_hello_history"], gitpm=True)
def g2_guarded(t: T):
    """g2_hello_history plus an unrelated 95d14dc8-6dfc-5022-9fd1-7146ee6aee58 0.1.0 installed, and an unapplied
    upstream release 3dce1568-569c-5b53-9321-37185e751947 0.1.3 (so update and rollback would both change something)."""
    t.new_package("95d14dc8-6dfc-5022-9fd1-7146ee6aee58", "0.1.0")
    t.gitpm_ok("install", t.repo("95d14dc8-6dfc-5022-9fd1-7146ee6aee58"))
    t.release(HELLO, "0.1.3")


@g2_guarded.check
def _(t: T):
    p = t.package(HELLO)
    t.eq((p["current"]["version"], p["previous"]["version"]), ("0.1.2", "0.1.1"), "3dce1568-569c-5b53-9321-37185e751947 history")
    t.eq(t.package("95d14dc8-6dfc-5022-9fd1-7146ee6aee58")["current"]["version"], "0.1.0", "95d14dc8-6dfc-5022-9fd1-7146ee6aee58 installed")
    t.has(t.git(HELLO, "tag", "--list"), f"v0.1.3")
    for id in [HELLO, "95d14dc8-6dfc-5022-9fd1-7146ee6aee58"]:
        t.eq(t.git(t.worktree(id), "status", "--porcelain", "--ignored"), "", f"{id} clean clone")
        t.eq(t.git(t.worktree(id), "stash", "list"), "", f"{id} no stash")
