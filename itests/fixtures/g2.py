"""Group 2 fixtures: namespaces, tag forms, update history and guarded clones (UC-010…021)."""

from gitpmtest import T, fixture

HELLO = "dan/hello"


@fixture()
def g2_demos(t: T):
    """vendor/demo and author/demo: same basename, different namespaces and payloads; 0.1.0 each."""
    t.new_package("vendor/demo", "0.1.0")
    t.new_package("author/demo", "0.1.0")


@g2_demos.check
def _(t: T):
    for id in ["vendor/demo", "author/demo"]:
        t.eq(t.git(id, "tag", "--list"), f"v0.1.0", f"{id} tags")
        t.has(t.git(id, "show", "HEAD:dist/index.js"), f"{id} 0.1.0")


@fixture(requires=["g2_demos"], gitpm=True)
def g2_demos_installed(t: T):
    """vendor/demo and author/demo 0.1.0 both installed directly from Git into /fx/ws."""
    t.gitpm_ok("install", t.repo("vendor/demo"))
    t.gitpm_ok("install", t.repo("author/demo"))


@g2_demos_installed.check
def _(t: T):
    for id in ["vendor/demo", "author/demo"]:
        p = t.package(id)
        t.eq(p["current"], {"version": "0.1.0", "commit": t.rev(id)}, f"{id} current")
        t.eq(p["enabled"], True, f"{id} enabled")
        t.eq(t.rev(t.worktree(id)), p["current"]["commit"], f"{id} clone HEAD")
        t.has((t.dist(id) / "index.js").read_text(), f"{id} 0.1.0")
    t.eq(sorted(t.state()["packages"]), ["author/demo", "vendor/demo"], "installed")


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
    """dan/rev: 1.10.0 (annotated tag) created before 1.9.0 (lightweight) and an untagged
    1.11.0 HEAD; dan/rev2: the same with lightweight 1.10.0 and annotated 1.9.0."""
    _reverse(t, "dan/rev", annotated_first=True)
    _reverse(t, "dan/rev2", annotated_first=False)


@g2_reverse_tags.check
def _(t: T):
    for id, kinds in [("dan/rev", ("tag", "commit")), ("dan/rev2", ("commit", "tag"))]:
        t.eq(sorted(t.git(id, "tag", "--list").split()), [f"v1.10.0", f"v1.9.0"], f"{id} tags")
        t.eq((t.git(id, "cat-file", "-t", f"refs/tags/v1.10.0"),
              t.git(id, "cat-file", "-t", f"refs/tags/v1.9.0")), kinds, f"{id} tag object types")
        old, new = t.rev(id, f"v1.10.0"), t.rev(id, f"v1.9.0")
        t.eq(t.git(id, "rev-list", "--count", f"{old}..{new}"), "1", f"{id}: 1.9.0 is committed after 1.10.0")
        t.has(t.git(id, "show", "HEAD:plugin.json"), '"version": "1.11.0"')
        t.eq(t.git(id, "tag", "--points-at", "HEAD"), "", f"{id} HEAD untagged")


@fixture(requires=["hello_installed"], gitpm=True)
def g2_hello_history(t: T):
    """dan/hello installed at 0.1.0, updated to 0.1.1 and then 0.1.2: current 0.1.2, previous 0.1.1."""
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
    t.has((t.dist(HELLO) / "index.js").read_text(), "dan/hello 0.1.2")
    t.eq(t.git(t.worktree(HELLO), "status", "--porcelain", "--ignored"), "", "clean clone")


@fixture(requires=["g2_hello_history"], gitpm=True)
def g2_guarded(t: T):
    """g2_hello_history plus an unrelated dan/other 0.1.0 installed, and an unapplied
    upstream release dan/hello 0.1.3 (so update and rollback would both change something)."""
    t.new_package("dan/other", "0.1.0")
    t.gitpm_ok("install", t.repo("dan/other"))
    t.release(HELLO, "0.1.3")


@g2_guarded.check
def _(t: T):
    p = t.package(HELLO)
    t.eq((p["current"]["version"], p["previous"]["version"]), ("0.1.2", "0.1.1"), "dan/hello history")
    t.eq(t.package("dan/other")["current"]["version"], "0.1.0", "dan/other installed")
    t.has(t.git(HELLO, "tag", "--list"), f"v0.1.3")
    for id in [HELLO, "dan/other"]:
        t.eq(t.git(t.worktree(id), "status", "--porcelain", "--ignored"), "", f"{id} clean clone")
        t.eq(t.git(t.worktree(id), "stash", "list"), "", f"{id} no stash")
