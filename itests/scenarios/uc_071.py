"""UC-071: update all — joint upgrade of every installed package.

Fixture g6_core_deps_installed: app/a (lib/core ^1.0), app/c (lib/core "1"), lib/core 1.1.0
pulled in from the catalog, and other/x 1.0.0. lib/core 2.0.0 exists upstream and in the
catalog, so lib/core is kept back by its dependents' constraints. `update all` prints one
line per installed package (state order) and a totals line; `--dry-run` changes nothing.
"""

from gitpmtest import T, case

CORE = "lib/core"
KEPT = f"{CORE} kept back at 1.1.0 (latest 2.0.0: dependency constraints)"


def lines(text: str) -> list[str]:
    return text.splitlines()


@case(fixture="g6_core_deps_installed", spec=["updates/update-all-dry-run#preview",
                                              "updates/update-all-never-removes-or-downgrades#kept-back",
                                              "cli/one-output-line-per-affected-package#update-all-lines"])
def dry_run_previews_and_changes_nothing(t: T):
    """other/x 1.1.0 released: --dry-run prints `would be updated`, the kept-back line and
    `Nothing was changed.`; the workspace (Git internals included) is byte-identical."""
    t.release("other/x", "1.1.0")
    before = t.snap(t.ws, git=True)
    r = t.gitpm_ok("update", "all", "--dry-run")
    t.eq(lines(r.out), ["app/a up to date (1.0.0)", "app/c up to date (1.0.0)", KEPT,
                        "other/x would be updated 1.0.0 → 1.1.0",
                        "1 updates, 0 installs, 1 kept back. Nothing was changed."], "output")
    t.same(before, "workspace after --dry-run")


@case(fixture="g6_core_deps_installed", spec=["updates/joint-upgrade-of-all-packages",
                                              "updates/update-all-never-removes-or-downgrades#kept-back",
                                              "cli/one-output-line-per-affected-package#update-all-lines"])
def apply_updates_and_keeps_back(t: T):
    """update all moves other/x to 1.1.0 (state, clone, dist); lib/core stays at 1.1.0."""
    commit = t.release("other/x", "1.1.0")
    r = t.gitpm_ok("update", "all")
    t.eq(lines(r.out), ["app/a up to date (1.0.0)", "app/c up to date (1.0.0)", KEPT,
                        "other/x updated 1.0.0 → 1.1.0", "1 updates, 0 installs, 1 kept back."], "output")
    t.eq(t.package("other/x")["current"], {"version": "1.1.0", "commit": commit}, "other/x state")
    t.eq(t.rev(t.clone("other/x")), commit, "other/x clone HEAD")
    t.has((t.dist("other/x") / "index.js").read_text(), "other/x 1.1.0")
    t.eq(t.package(CORE)["current"]["version"], "1.1.0", "lib/core not upgraded past its dependents")
    t.eq(sorted(t.state()["packages"]), ["app/a", "app/c", CORE, "other/x"], "nothing removed")


@case(fixture="g6_core_deps_installed", spec=["updates/joint-upgrade-of-all-packages"])
def new_dependency_is_installed_from_the_catalog(t: T):
    """app/c 1.1.0 needs dep/new ^1 (only in the catalog): update all installs it as a new dependency."""
    t.new_package("dep/new", "1.0.0")
    t.gitpm_ok("register", t.repo("dep/new"), "--ref", "dep/new@1.0.0")
    t.release("app/c", "1.1.0", deps={CORE: "1", "dep/new": "^1"})
    r = t.gitpm_ok("update", "all")
    t.eq(lines(r.out), ["app/a up to date (1.0.0)", "app/c updated 1.0.0 → 1.1.0", KEPT,
                        "other/x up to date (1.0.0)", "dep/new installed 1.0.0 (new dependency)",
                        "1 updates, 1 installs, 1 kept back."], "output")
    t.eq(t.package("dep/new")["current"]["version"], "1.0.0", "dep/new installed")
    t.has((t.dist("dep/new") / "index.js").read_text(), "dep/new 1.0.0")


@case(fixture="g6_core_deps_installed",
      spec=["updates/joint-upgrade-of-all-packages#app-and-dependency-cross-a-major-boundary-together"])
def app_and_dependency_cross_a_major_boundary_together(t: T):
    """app/a 2.0.0 and app/c 2.0.0 require lib/core ^2: update all moves all three in one command."""
    t.release("app/a", "2.0.0", deps={CORE: "^2.0"})
    t.release("app/c", "2.0.0", deps={CORE: "^2.0"})
    r = t.gitpm_ok("update", "all")
    t.eq(lines(r.out), ["app/a updated 1.0.0 → 2.0.0", "app/c updated 1.0.0 → 2.0.0",
                        f"{CORE} updated 1.1.0 → 2.0.0", "other/x up to date (1.0.0)",
                        "3 updates, 0 installs, 0 kept back."], "output")
    for id, v in [("app/a", "2.0.0"), ("app/c", "2.0.0"), (CORE, "2.0.0")]:
        t.eq(t.package(id)["current"]["version"], v, f"{id} version")
        t.has((t.dist(id) / "index.js").read_text(), f"{id} {v}")


@case(fixture="g6_core_deps_installed",
      spec=["updates/update-all-is-stable-only-and-refuses-local-work#update-all-with-ref"])
def ref_dev_and_release_are_refused(t: T):
    """update all --ref / --dev / --release: refused, it follows each recorded mode; nothing changes."""
    before = t.snap(t.ws, git=True)
    for extra in [("--ref", "x"), ("--dev",), ("--release",)]:
        r = t.gitpm("update", "all", *extra)
        t.ok(r.code != 0, f"update all {' '.join(extra)} refused")
        t.has(r.err, "update all takes no --ref, --dev or --release; it follows each package's recorded mode")
        t.same(before, "workspace")


@case(fixture="g6_core_deps_installed", spec=["updates/update-all-is-stable-only-and-refuses-local-work"])
def local_work_refuses_without_partial_deploy(t: T):
    """A dirty other/x clone blocks update all (local_changes) even for the other packages."""
    t.release("other/x", "1.1.0")
    t.release("app/a", "1.0.1", deps={CORE: "^1.0"})
    (t.clone("other/x") / "dist/index.js").write_text("local edit\n")
    before = t.snap(t.ws, git=True)
    t.gitpm_err("update", "all", kind="local_changes")
    t.same(before, "workspace")
