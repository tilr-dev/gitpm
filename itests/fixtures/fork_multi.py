"""Multi-package repository fixtures (UC-072): one repository /fx/repos/suite with gitpm.json."""

import json

from gitpmtest import T, fixture

SUITE = ["tilr/store", "tilr/theme", "tilr/panel"]


@fixture(gitpm=True)
def fork_suite(t: T):
    """/fx/repos/suite made with `gitpm init --multi`: packages/store, packages/theme,
    packages/panel (tilr/*), each released at 0.1.0 by init."""
    repo = t.repo("suite")
    t.gitpm_ok("init", SUITE[0], "--into", repo, "--multi")
    for id in SUITE[1:]:
        t.gitpm_ok("init", id, "--into", repo)


@fork_suite.check
def _(t: T):
    repo = t.repo("suite")
    t.eq(json.loads((repo / "gitpm.json").read_text()), {"packages": ["packages/*"]}, "gitpm.json")
    t.eq(sorted(t.git(repo, "tag", "--list").split()), sorted(f"{id}@0.1.0" for id in SUITE), "tags")
    t.eq(t.git(repo, "status", "--porcelain"), "", "clean worktree")
    for id in SUITE:
        m = json.loads((repo / "packages" / id.split("/")[1] / "plugin.json").read_text())
        t.eq((m["id"], m["version"]), (id, "0.1.0"), f"{id} manifest")
    t.ok(not t.ws.exists(), "no workspace")


@fixture(requires=["fork_suite"], gitpm=True)
def fork_suite_installed(t: T):
    """fork_suite plus tilr/store and tilr/theme installed from it (one shared clone)."""
    for id in SUITE[:2]:
        t.gitpm_ok("install", t.repo("suite"), id)


@fork_suite_installed.check
def _(t: T):
    state = t.state()["packages"]
    t.eq(sorted(state), ["tilr/store", "tilr/theme"], "installed")
    t.eq(len({state[id]["clone"] for id in state}), 1, "one shared clone")
    t.ok((t.ws / "repos" / state["tilr/store"]["clone"] / ".git").exists(), "shared clone exists")
    for id in state:
        t.has((t.dist(id) / "index.js").read_text(), id.split("/")[1])
