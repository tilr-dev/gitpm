"""Multi-package repository fixtures (UC-072): one repository /fx/repos/suite with gitpm.json."""

import json

from gitpmtest import T, fixture

SUITE = ["76a9b591-60f3-5c7f-9e1c-31648fc73bfd", "f851f609-bb81-52d6-bfef-33798624782c", "955bf29f-2893-56ef-9439-369c50455ae7"]


@fixture(gitpm=True)
def fork_suite(t: T):
    """/fx/repos/suite authored with fixed package UUIDs: packages/store, packages/theme,
    packages/panel, released together at 0.1.1."""
    repo = t.repo("suite")
    repo.mkdir(parents=True)
    t.git(repo, "init", "-q", "-b", "main")
    files = {"gitpm.json": json.dumps({"packages": ["packages/*"]}) + "\n"}
    for id, name in zip(SUITE, ["store", "theme", "panel"]):
        files[f"packages/{name}/plugin.json"] = json.dumps({"id": id, "name": name, "version": "0.1.0"}) + "\n"
        files[f"packages/{name}/dist/index.js"] = f"export const greeting = 'Hello from {name} 0.1.0';\n"
    t.commit(repo, "Initial multi-package fixture", files)
    t.gitpm_ok("commit", repo, "0.1.1")


@fork_suite.check
def _(t: T):
    repo = t.repo("suite")
    t.eq(json.loads((repo / "gitpm.json").read_text()), {"packages": ["packages/*"]}, "gitpm.json")
    t.eq(sorted(t.git(repo, "tag", "--list").split()), ["v0.1.1"], "tags")
    t.eq(t.git(repo, "status", "--porcelain"), "", "clean worktree")
    for id, name in zip(SUITE, ["store", "theme", "panel"]):
        m = json.loads((repo / "packages" / name / "plugin.json").read_text())
        t.eq((m["id"], m["version"]), (id, "0.1.1"), f"{id} manifest")
    t.ok(not t.ws.exists(), "no workspace")


@fixture(requires=["fork_suite"], gitpm=True)
def fork_suite_installed(t: T):
    """fork_suite plus 76a9b591-60f3-5c7f-9e1c-31648fc73bfd and f851f609-bb81-52d6-bfef-33798624782c installed from it (one shared clone)."""
    for id in SUITE[:2]:
        t.gitpm_ok("install", t.repo("suite"), id)


@fork_suite_installed.check
def _(t: T):
    state = t.state()["packages"]
    t.eq(sorted(state), ["76a9b591-60f3-5c7f-9e1c-31648fc73bfd", "f851f609-bb81-52d6-bfef-33798624782c"], "installed")
    t.eq(len({state[id]["clone"] for id in state}), 1, "one shared clone")
    t.eq(t.git(t.clone(SUITE[0]), "rev-parse", "--is-bare-repository"), "true", "shared bare repository")
    for id, name in zip(SUITE[:2], ["store", "theme"]):
        t.ok((t.worktree(id) / ".git").is_file(), f"{id} has a linked worktree")
        t.has((t.dist(id) / "index.js").read_text(), name)
