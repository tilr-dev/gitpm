"""UC-071: update all — joint upgrade of every installed package.

Fixture g6_core_deps_installed: 9692fc07-23c7-5f20-8779-f09374f1a571 (757fd5ca-2b6a-5b2f-8a5d-60980b028665 ^1.0), 1a620c0a-aa3c-533c-9960-3d49016fa231 (757fd5ca-2b6a-5b2f-8a5d-60980b028665 ^1), 757fd5ca-2b6a-5b2f-8a5d-60980b028665 1.1.0
pulled in from the catalog, and a768d335-0cf7-597a-b1ba-0eb542495aec 1.0.0. 757fd5ca-2b6a-5b2f-8a5d-60980b028665 2.0.0 exists upstream and in the
catalog, so 757fd5ca-2b6a-5b2f-8a5d-60980b028665 is kept back by its dependents' constraints. `update all` prints one
line per installed package (state order) and a totals line; `--dry-run` changes nothing.
"""

from gitpmtest import T, case

CORE = "757fd5ca-2b6a-5b2f-8a5d-60980b028665"
KEPT = f"{CORE} kept back at 1.1.0 (latest 2.0.0: dependency constraints)"


def lines(text: str) -> list[str]:
    return text.splitlines()


@case(fixture="g6_core_deps_installed", spec=["updates/update-all-dry-run#preview",
                                              "updates/update-all-never-removes-or-downgrades#kept-back",
                                              "cli/one-output-line-per-affected-package#update-all-lines"])
def dry_run_previews_and_changes_nothing(t: T):
    """a768d335-0cf7-597a-b1ba-0eb542495aec 1.1.0 released: --dry-run prints `would be updated`, the kept-back line and
    `Nothing was changed.`; the workspace (Git internals included) is byte-identical."""
    t.release("a768d335-0cf7-597a-b1ba-0eb542495aec", "1.1.0")
    before = t.snap(t.ws, git=True)
    r = t.gitpm_ok("update", "all", "--dry-run")
    t.eq(lines(r.out), ["9692fc07-23c7-5f20-8779-f09374f1a571 up to date (1.0.0)", "1a620c0a-aa3c-533c-9960-3d49016fa231 up to date (1.0.0)", KEPT,
                        "a768d335-0cf7-597a-b1ba-0eb542495aec would be updated 1.0.0 → 1.1.0",
                        "1 updates, 0 installs, 1 kept back. Nothing was changed."], "output")
    t.same(before, "workspace after --dry-run")


@case(fixture="g6_core_deps_installed", spec=["updates/joint-upgrade-of-all-packages",
                                              "updates/update-all-never-removes-or-downgrades#kept-back",
                                              "cli/one-output-line-per-affected-package#update-all-lines"])
def apply_updates_and_keeps_back(t: T):
    """update all moves a768d335-0cf7-597a-b1ba-0eb542495aec to 1.1.0 (state, clone, dist); 757fd5ca-2b6a-5b2f-8a5d-60980b028665 stays at 1.1.0."""
    commit = t.release("a768d335-0cf7-597a-b1ba-0eb542495aec", "1.1.0")
    r = t.gitpm_ok("update", "all")
    t.eq(lines(r.out), ["9692fc07-23c7-5f20-8779-f09374f1a571 up to date (1.0.0)", "1a620c0a-aa3c-533c-9960-3d49016fa231 up to date (1.0.0)", KEPT,
                        "a768d335-0cf7-597a-b1ba-0eb542495aec updated 1.0.0 → 1.1.0", "1 updates, 0 installs, 1 kept back."], "output")
    t.eq(t.package("a768d335-0cf7-597a-b1ba-0eb542495aec")["current"], {"version": "1.1.0", "commit": commit}, "a768d335-0cf7-597a-b1ba-0eb542495aec state")
    t.eq(t.rev(t.clone("a768d335-0cf7-597a-b1ba-0eb542495aec")), commit, "a768d335-0cf7-597a-b1ba-0eb542495aec clone HEAD")
    t.has((t.dist("a768d335-0cf7-597a-b1ba-0eb542495aec") / "index.js").read_text(), "a768d335-0cf7-597a-b1ba-0eb542495aec 1.1.0")
    t.eq(t.package(CORE)["current"]["version"], "1.1.0", "757fd5ca-2b6a-5b2f-8a5d-60980b028665 not upgraded past its dependents")
    t.eq(sorted(t.state()["packages"]), sorted(["9692fc07-23c7-5f20-8779-f09374f1a571", "1a620c0a-aa3c-533c-9960-3d49016fa231", CORE, "a768d335-0cf7-597a-b1ba-0eb542495aec"]), "nothing removed")


@case(fixture="g6_core_deps_installed", spec=["updates/joint-upgrade-of-all-packages"])
def new_dependency_is_installed_from_the_catalog(t: T):
    """1a620c0a-aa3c-533c-9960-3d49016fa231 1.1.0 needs f76d6a0b-273a-57be-aaf1-9f59fdaaef33 ^1 (only in the catalog): update all installs it as a new dependency."""
    t.new_package("f76d6a0b-273a-57be-aaf1-9f59fdaaef33", "1.0.0")
    t.gitpm_ok("register", t.repo("f76d6a0b-273a-57be-aaf1-9f59fdaaef33"), "--ref", "v1.0.0")
    t.release("1a620c0a-aa3c-533c-9960-3d49016fa231", "1.1.0", deps={CORE: "^1", "f76d6a0b-273a-57be-aaf1-9f59fdaaef33": "^1"})
    r = t.gitpm_ok("update", "all")
    t.eq(lines(r.out), ["9692fc07-23c7-5f20-8779-f09374f1a571 up to date (1.0.0)", "1a620c0a-aa3c-533c-9960-3d49016fa231 updated 1.0.0 → 1.1.0", KEPT,
                        "a768d335-0cf7-597a-b1ba-0eb542495aec up to date (1.0.0)", "f76d6a0b-273a-57be-aaf1-9f59fdaaef33 installed 1.0.0 (new dependency)",
                        "1 updates, 1 installs, 1 kept back."], "output")
    t.eq(t.package("f76d6a0b-273a-57be-aaf1-9f59fdaaef33")["current"]["version"], "1.0.0", "f76d6a0b-273a-57be-aaf1-9f59fdaaef33 installed")
    t.has((t.dist("f76d6a0b-273a-57be-aaf1-9f59fdaaef33") / "index.js").read_text(), "f76d6a0b-273a-57be-aaf1-9f59fdaaef33 1.0.0")


@case(fixture="g6_core_deps_installed",
      spec=["updates/joint-upgrade-of-all-packages#app-and-dependency-cross-a-major-boundary-together"])
def app_and_dependency_cross_a_major_boundary_together(t: T):
    """9692fc07-23c7-5f20-8779-f09374f1a571 2.0.0 and 1a620c0a-aa3c-533c-9960-3d49016fa231 2.0.0 require 757fd5ca-2b6a-5b2f-8a5d-60980b028665 ^2: update all moves all three in one command, each line marked (major)."""
    t.release("9692fc07-23c7-5f20-8779-f09374f1a571", "2.0.0", deps={CORE: "^2.0"})
    t.release("1a620c0a-aa3c-533c-9960-3d49016fa231", "2.0.0", deps={CORE: "^2.0"})
    r = t.gitpm_ok("update", "all")
    t.eq(lines(r.out), ["9692fc07-23c7-5f20-8779-f09374f1a571 updated 1.0.0 → 2.0.0 (major)", "1a620c0a-aa3c-533c-9960-3d49016fa231 updated 1.0.0 → 2.0.0 (major)",
                        f"{CORE} updated 1.1.0 → 2.0.0 (major)", "a768d335-0cf7-597a-b1ba-0eb542495aec up to date (1.0.0)",
                        "3 updates, 0 installs, 0 kept back."], "output")
    for id, v in [("9692fc07-23c7-5f20-8779-f09374f1a571", "2.0.0"), ("1a620c0a-aa3c-533c-9960-3d49016fa231", "2.0.0"), (CORE, "2.0.0")]:
        t.eq(t.package(id)["current"]["version"], v, f"{id} version")
        t.has((t.dist(id) / "index.js").read_text(), f"{id} {v}")


@case(fixture="g6_core_deps_installed",
      spec=["updates/update-all-honors-each-package-s-mode-and-refuses-local-work#update-all-with-ref"])
def ref_dev_and_release_are_refused(t: T):
    """update all --ref / --dev / --release: refused, it follows each recorded mode; nothing changes."""
    before = t.snap(t.ws, git=True)
    for extra in [("--ref", "x"), ("--dev",), ("--release",)]:
        r = t.gitpm("update", "all", *extra)
        t.ok(r.code != 0, f"update all {' '.join(extra)} refused")
        t.has(r.err, "update all takes no --ref, --dev or --release; it follows each package's recorded mode")
        t.same(before, "workspace")


@case(fixture="g6_core_deps_installed", spec=["updates/update-all-honors-each-package-s-mode-and-refuses-local-work"])
def local_work_refuses_without_partial_deploy(t: T):
    """A dirty a768d335-0cf7-597a-b1ba-0eb542495aec clone blocks update all (local_changes) even for the other packages."""
    t.release("a768d335-0cf7-597a-b1ba-0eb542495aec", "1.1.0")
    t.release("9692fc07-23c7-5f20-8779-f09374f1a571", "1.0.1", deps={CORE: "^1.0"})
    (t.clone("a768d335-0cf7-597a-b1ba-0eb542495aec") / "dist/index.js").write_text("local edit\n")
    before = t.snap(t.ws, git=True)
    t.gitpm_err("update", "all", kind="local_changes")
    t.same(before, "workspace")


@case(fixture="g6_core_deps_installed",
      spec=["dependencies/broken-consumers-are-reported-not-enforced#update-all-after-forced-removal",
            "updates/update-all-never-removes-or-downgrades#broken-state-does-not-fail-the-run"])
def forced_removal_skips_broken_consumers(t: T):
    """757fd5ca-2b6a-5b2f-8a5d-60980b028665 uninstalled with --force: update all, with the catalog and without one, updates
    a768d335-0cf7-597a-b1ba-0eb542495aec, skips 9692fc07-23c7-5f20-8779-f09374f1a571 and 1a620c0a-aa3c-533c-9960-3d49016fa231 with their reason, exits 0 and never installs 757fd5ca-2b6a-5b2f-8a5d-60980b028665 again;
    list keeps reporting the dependency unsatisfied."""
    t.gitpm_ok("uninstall", CORE, "--force")
    skipped = [f"{id} skipped at 1.0.0 (dependency {CORE} is not installed)" for id in ["9692fc07-23c7-5f20-8779-f09374f1a571", "1a620c0a-aa3c-533c-9960-3d49016fa231"]]
    t.release("a768d335-0cf7-597a-b1ba-0eb542495aec", "1.1.0")
    r = t.gitpm_ok("update", "all")
    t.eq(lines(r.out), [*skipped, "a768d335-0cf7-597a-b1ba-0eb542495aec updated 1.0.0 → 1.1.0", "1 updates, 0 installs, 0 kept back."],
         "output with a catalog")
    t.release("a768d335-0cf7-597a-b1ba-0eb542495aec", "1.2.0")
    r = t.gitpm_ok("update", "all", env={"GITPM_CATALOG": ""})
    t.eq(lines(r.out), [*skipped, "a768d335-0cf7-597a-b1ba-0eb542495aec updated 1.1.0 → 1.2.0", "1 updates, 0 installs, 0 kept back."],
         "output without a catalog")
    t.eq(sorted(t.state()["packages"]), sorted(["9692fc07-23c7-5f20-8779-f09374f1a571", "1a620c0a-aa3c-533c-9960-3d49016fa231", "a768d335-0cf7-597a-b1ba-0eb542495aec"]), "757fd5ca-2b6a-5b2f-8a5d-60980b028665 not installed again")
    t.not_installed(CORE)
    resolved = {p["id"]: p for p in t.gitpm_ok("list").json()["resolved"]}
    t.eq([d["satisfied"] for d in resolved["9692fc07-23c7-5f20-8779-f09374f1a571"]["dependencies"]], [False], "9692fc07-23c7-5f20-8779-f09374f1a571 still unsatisfied")
    t.has(t.gitpm_ok("check").out, f"Unsatisfied dependency: 9692fc07-23c7-5f20-8779-f09374f1a571 → {CORE} ^1.0: not installed",
          f"Unsatisfied dependency: 1a620c0a-aa3c-533c-9960-3d49016fa231 → {CORE} ^1: not installed")


@case(fixture="g6_core_deps_installed", spec=["updates/update-all-never-removes-or-downgrades#kept-back",
                                              "updates/update-all-never-removes-or-downgrades#reasons"])
def new_dependency_without_a_catalog_is_kept_back(t: T):
    """Without a catalog, a768d335-0cf7-597a-b1ba-0eb542495aec 1.1.0 needs 07581add-b594-5dd8-9973-ed1289d477b5 ^1 (not installed): update all keeps a768d335-0cf7-597a-b1ba-0eb542495aec
    back with that reason and exits 0."""
    t.release("a768d335-0cf7-597a-b1ba-0eb542495aec", "1.1.0", deps={"07581add-b594-5dd8-9973-ed1289d477b5": "^1"})
    r = t.gitpm_ok("update", "all", env={"GITPM_CATALOG": ""})
    t.has(r.out, "a768d335-0cf7-597a-b1ba-0eb542495aec kept back at 1.0.0 (latest 1.1.0: needs 07581add-b594-5dd8-9973-ed1289d477b5 ^1, which is not installed "
                 "and no catalog is configured)")
    t.has(r.out, KEPT)
    t.has(r.out, "0 updates, 0 installs, 2 kept back.")
    t.eq(t.package("a768d335-0cf7-597a-b1ba-0eb542495aec")["current"]["version"], "1.0.0", "a768d335-0cf7-597a-b1ba-0eb542495aec stays")
