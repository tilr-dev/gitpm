"""UC-060: Fork status and upstream version visibility (P1).

g3_fork_catalog: vendor/p 1.0.0 in /fx/catalog and installed from it; mine/p forked,
released as mine/p@1.0.1 with `gitpm commit` and installed from its author repo.
"""

from gitpmtest import T, case

V, M, O = "vendor/p", "mine/p", "other/p"


def row(out: str, id: str) -> str:
    for line in out.splitlines():
        cells = line.split()
        if cells and cells[0] == id:
            return line
    raise AssertionError(f"no check row for {id} in:\n{out}")


def mine_payload(t: T):
    return t.snap(t.ws / "state.json", t.worktree(M), t.dist(M), git=True)


@case(fixture="g3_fork_catalog", spec=["fork-replacement/fork-metadata", "cli/json-output-for-state-and-catalog#list"])
def list_shows_fork_relation(t: T):
    """1. list: mine/p has forkOf {id, version, commit, source} and replaces vendor/p 1.0.0; vendor/p none."""
    base = t.rev(V, "v1.0.0")
    before = mine_payload(t)
    out = t.gitpm_ok("list").json()
    mine, vendor = out["packages"][M], out["packages"][V]
    t.eq(mine["forkOf"], {"id": V, "version": "1.0.0", "commit": base, "source": str(t.repo(V))}, "forkOf")
    t.eq(mine["replaces"], {"id": V, "version": "1.0.0"}, "replaces")
    t.ok("forkOf" not in vendor and "replaces" not in vendor, "vendor/p has neither")
    resolved = {p["id"]: p for p in out["resolved"]}
    t.eq(resolved[M]["forkOf"], mine["forkOf"], "resolved forkOf")
    t.ok("forkOf" not in resolved[V], "resolved vendor/p has no forkOf")
    t.same(before, "mine/p state and payload")


@case(fixture="g3_fork_catalog", spec=["fork-replacement/check-shows-fork-upstream"])
def check_compares_fork_with_own_releases(t: T):
    """2. mine/p is compared with releases of mine/p only (not in the catalog: its origin); a
    line below the table shows the original vendor/p, base 1.0.0, latest 1.0.0, not newer."""
    before = mine_payload(t)
    out = t.gitpm_ok("check").out
    t.has(row(out, M), "origin", "up to date")
    t.has(row(out, V), "up to date")
    t.has(out, f"{M}: fork of {V} (base 1.0.0); original in catalog: 1.0.0")
    t.lacks(out, "newer than the fork base")
    t.same(before, "mine/p state and payload")


@case(fixture="g3_fork_catalog", spec=["fork-replacement/fork-metadata"])
def fork_of_fork_points_to_immediate_parent(t: T):
    """3. other/p forked from mine/p: forkOf is mine/p (immediate parent)."""
    mine = t.package(M)["current"]
    dest = t.root / "authors/other-p"
    t.gitpm_ok("fork", M, "--ns", "other", "--into", dest)
    t.gitpm_ok("install", dest, "--dev")
    out = t.gitpm_ok("list").json()
    t.eq(out["packages"][O]["forkOf"],
         {"id": M, "version": mine["version"], "commit": mine["commit"], "source": t.package(M)["source"]}, "forkOf")
    t.eq(out["packages"][O]["replaces"], {"id": M, "version": mine["version"]}, "replaces")


@case(fixture="g3_fork_catalog", spec=["fork-replacement/check-shows-fork-upstream#original-has-a-newer-release"])
def newer_original_release_is_visible(t: T):
    """4. vendor/p 1.1.0 published: check shows the original's 1.1.0 as newer than the fork base
    1.0.0 and vendor/p "update available"; mine/p state/payload unchanged; list unchanged."""
    list_before = t.gitpm_ok("list").json()["packages"][M]
    t.release(V, "1.1.0")
    t.gitpm_ok("register", t.repo(V), "--ref", "v1.1.0")
    before = mine_payload(t)
    out = t.gitpm_ok("check").out
    t.has(row(out, V), "update available", "1.1.0")
    t.has(out, f"{M}: fork of {V} (base 1.0.0); original in catalog: 1.1.0 — newer than the fork base")
    t.has(row(out, M), "origin", "up to date")
    t.eq(t.gitpm_ok("list").json()["packages"][M], list_before, "mine/p in list")
    t.same(before, "mine/p state and payload")


@case(fixture="g3_fork_catalog", spec=["fork-replacement/check-shows-fork-upstream",
                                       "catalog/a-catalog-is-optional#check-without-a-catalog"])
def upstream_from_fork_of_source_without_a_catalog(t: T):
    """5. No catalog: every row comes from its origin and the fork's upstream from
    forkOf.source; a new tag v1.1.0 (not registered anywhere) shows as newer."""
    def check() -> str:
        r = t.run("env", "-u", "GITPM_CATALOG", "gitpm", "check")
        t.eq(r.code, 0, "check exit")
        return r.out
    before = t.snap(t.ws)
    out = check()
    t.has(out, "Catalog: none")
    t.has(row(out, V), "origin", "up to date")
    t.has(row(out, M), "origin", "up to date")
    t.has(out, f"{M}: fork of {V} (base 1.0.0); original at origin: 1.0.0")
    t.lacks(out, "newer than the fork base")
    t.release(V, "1.1.0")
    out = check()
    t.has(row(out, V), "origin", "update available", "1.1.0")
    t.has(out, f"{M}: fork of {V} (base 1.0.0); original at origin: 1.1.0 — newer than the fork base")
    t.same(before, "workspace (check never updates)")
