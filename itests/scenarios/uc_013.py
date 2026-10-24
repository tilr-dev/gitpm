"""UC-013: Independent namespaces and workspaces (P0).

Fixture g2_demos_installed: vendor/demo and author/demo (same basename, different payloads)
installed in /fx/ws.
"""

import json

from gitpmtest import T, case

V, A = "vendor/demo", "author/demo"


def entry(t: T, root, id: str):
    return json.loads((root / "state.json").read_text())["packages"].get(id)


def files_of(t: T, root, id: str):
    """Clone (with full .git) and dist of one installation."""
    return t.snap(t.clone(id, root), root / "dist" / id, git=True)


def authors(t: T):
    return t.snap(t.repo(V), t.repo(A), git=True)


@case(fixture="g2_demos_installed", spec=["package-ids/ids-are-matched-exactly", "workspace-layout/workspace-folders"])
def same_basename_coexists(t: T):
    """Both IDs coexist with source-keyed clones, separate dist and state entries."""
    state = t.gitpm_ok("list").json()
    t.eq(sorted(state["packages"]), [A, V], "state keys are full IDs")
    t.eq(sorted(p["id"] for p in state["resolved"]), [A, V], "resolved list")
    for id in [V, A]:
        t.eq(t.package(id)["source"], str(t.repo(id).resolve()), f"{id} source")
        t.eq(t.rev(t.clone(id)), t.rev(id), f"{id} clone HEAD")
        t.eq((t.dist(id) / "index.js").read_text(), f'export const greeting = "{id} 0.1.0";\n', f"{id} dist")
    t.ok(t.rev(t.clone(V)) != t.rev(t.clone(A)), "distinct clones")
    t.ok(not (t.ws / "repos/demo").exists() and not (t.ws / "dist/demo").exists(), "no basename-only paths")


@case(fixture="g2_demos_installed", spec=["transactions-locking/neighbouring-folders-are-never-touched"])
def second_workspace_is_independent(t: T):
    """Installing and removing vendor/demo in another --root never touches the first workspace."""
    first, sources = t.snap(t.ws, git=True), authors(t)
    other = t.root / "ws2"
    t.gitpm_ok("--root", other, "install", t.repo(V))
    t.eq(sorted(json.loads((other / "state.json").read_text())["packages"]), [V], "other root holds only vendor/demo")
    t.eq((other / "dist" / V / "index.js").read_text(), (t.dist(V) / "index.js").read_text(), "same payload")
    t.same(first, "first workspace")
    t.gitpm_ok("--root", other, "uninstall", V)
    t.eq(json.loads((other / "state.json").read_text())["packages"], {}, "other root empty again")
    t.ok(not (t.clone(V, other)).exists() and not (other / "dist" / V).exists(), "other root files removed")
    t.same(first, "first workspace")
    t.same(sources, "author repositories")


@case(fixture="g2_demos_installed", spec=["transactions-locking/transactions-stage-only-touched-paths"])
def update_one_leaves_others(t: T):
    """update vendor/demo in ws: author/demo in ws, vendor/demo in ws2 and author repos stay identical."""
    other = t.root / "ws2"
    t.gitpm_ok("--root", other, "install", t.repo(V))
    a_entry, a_files = entry(t, t.ws, A), files_of(t, t.ws, A)
    ws2 = t.snap(other, git=True)
    commit = t.release(V, "0.2.0")
    a_source = t.snap(t.repo(A), git=True)
    t.gitpm_ok("update", V)
    t.eq(t.package(V)["current"], {"version": "0.2.0", "commit": commit}, "vendor/demo updated")
    t.has((t.dist(V) / "index.js").read_text(), "vendor/demo 0.2.0")
    t.eq(entry(t, t.ws, A), a_entry, "author/demo state entry")
    t.same(a_files, "author/demo clone and dist")
    t.same(ws2, "second workspace")
    t.same(a_source, "author/demo author repository")


@case(fixture="g2_demos_installed", spec=["dist/enable-and-disable-deploy-and-remove", "transactions-locking/transactions-stage-only-touched-paths"])
def disable_one_leaves_others(t: T):
    """disable author/demo changes only its enabled flag and removes its dist; vendor/demo and ws2 identical."""
    other = t.root / "ws2"
    t.gitpm_ok("--root", other, "install", t.repo(A))
    v_entry, v_files = entry(t, t.ws, V), files_of(t, t.ws, V)
    a_clone = t.snap(t.clone(A), git=True)
    ws2, sources = t.snap(other, git=True), authors(t)
    a_before = entry(t, t.ws, A)
    t.gitpm_ok("disable", A)
    t.eq(entry(t, t.ws, A), dict(a_before, enabled=False), "only author/demo enabled changed")
    t.eq(entry(t, t.ws, V), v_entry, "vendor/demo state entry")
    t.same(v_files, "vendor/demo files")
    t.same(a_clone, "author/demo clone")
    t.ok(not t.dist(A).exists(), "dist/author/demo removed (dist holds only active packages)")
    t.same(ws2, "second workspace (author/demo there still enabled)")
    t.eq(entry(t, other, A)["enabled"], True, "author/demo in ws2 enabled")
    t.same(sources, "author repositories")


@case(fixture="g2_demos_installed", spec=["install-uninstall/only-manager-owned-files-are-deleted"])
def uninstall_one_leaves_others(t: T):
    """uninstall vendor/demo removes only its clone, dist and entry."""
    other = t.root / "ws2"
    t.gitpm_ok("--root", other, "install", t.repo(V))
    a_entry, a_files = entry(t, t.ws, A), files_of(t, t.ws, A)
    ws2, sources = t.snap(other, git=True), authors(t)
    t.gitpm_ok("uninstall", V)
    t.not_installed(V)
    t.eq(entry(t, t.ws, A), a_entry, "author/demo state entry")
    t.same(a_files, "author/demo files")
    t.same(ws2, "second workspace (vendor/demo still installed there)")
    t.same(sources, "author repositories")
