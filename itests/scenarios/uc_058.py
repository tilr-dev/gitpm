"""UC-058: Required package dependencies without replacement (P0, mandatory).

Fixtures g6_core / g6_core_catalog: lib/core 1.0.0, 1.1.0, 2.0.0; app/a (lib/core ^1.0),
app/b (^2.0), app/c (^1), app/lonely (lib/missing ^1, not in the catalog), the cycle
x/a ↔ x/b, other/x — all registered in /fx/catalog (GITPM_CATALOG).
"""

import json

from gitpmtest import T, case

# No catalog configured: an empty GITPM_CATALOG counts as unset.
NO_CATALOG = {"GITPM_CATALOG": ""}

CORE = "lib/core"


def resolved(t: T, root=None) -> dict:
    args = ("--root", root) if root else ()
    return {p["id"]: p for p in t.gitpm_ok(*args, "list").json()["resolved"]}


def registered(t: T, id: str, version: str) -> dict:
    return json.loads(t.catalog_release(id, version).read_text())


# ------------------------------------------------------------ 1. manifest validation


@case(fixture="g6_core", spec=["dependencies/requirement-syntax", "dependencies/state-keeps-dependency-edges"])
def valid_requirements_accepted(t: T):
    """1a. npm-style ranges lib/core 1.1.0 satisfies are accepted (direct install with lib/core installed)."""
    t.gitpm_ok("install", t.repo(CORE), "--ref", f"v1.1.0")
    for i, req in enumerate(["^1.0", "1.1.0", "~1.1", ">=1.0.0 <2.0.0", "=1.1.0", ">= 1.1", "*", "^1.1.0-beta"]):
        id = f"ok/r{i}"
        t.new_package(id, "1.0.0", deps={CORE: req})
        t.gitpm_ok("install", t.repo(id))
        t.eq(t.package(id)["dependencies"], {CORE: req}, f"{id} dependency edge")


@case(fixture="g6_core", spec=["dependencies/requirement-syntax#bare-version-is-exact"])
def bare_version_is_exact(t: T):
    """1f. A bare version is exact: lib/core 1.1.0 installed does not satisfy "1.0.0"."""
    t.gitpm_ok("install", t.repo(CORE), "--ref", f"v1.1.0")
    t.new_package("ok/exact", "1.0.0", deps={CORE: "1.0.0"})
    t.gitpm_err("install", t.repo("ok/exact"), kind="dependency_conflict", contains=CORE)
    t.not_installed("ok/exact")


def refused(t: T, id: str, deps: dict, kind: str, *needles: str):
    t.new_package(id, "1.0.0", deps=deps)
    # An existing workspace (lib/core installed) so "nothing changed" is byte-exact; an empty
    # workspace skeleton on a refused install into a missing one is accepted by the owner.
    t.gitpm_ok("install", t.repo(CORE), "--ref", f"v1.1.0")
    before = t.snap(t.ws)
    t.gitpm_err("install", t.repo(id), kind=kind, contains=needles)
    t.same(before, "workspace")
    t.not_installed(id)
    t.gitpm_err("--catalog", t.root / "check-catalog", "register", t.repo(id), "--ref", f"v1.0.0",
             kind=kind, contains=needles)


@case(fixture="g6_core", spec=["dependencies/requirement-syntax"])
def banana_requirement_refused(t: T):
    """1b. "banana" is not a requirement: invalid_manifest on install and register."""
    refused(t, "bad/banana", {CORE: "banana"}, "invalid_manifest", "banana")


@case(fixture="g6_core", spec=["dependencies/requirement-syntax#old-cargo-syntax"])
def cargo_syntax_refused(t: T):
    """1c. Cargo commas, wildcards, bare partial versions, || and hyphen ranges are refused."""
    for i, req in enumerate([">=1.0, <2", "1.*", "1.1", "^1 || ^2", "1.0.0 - 2.0.0"]):
        refused(t, f"bad/cargo{i}", {CORE: req}, "invalid_manifest", "invalid version requirement")


@case(fixture="g6_core", spec=["dependencies/requirement-syntax#self-dependency"])
def self_dependency_refused(t: T):
    """1d. A package depending on itself is refused."""
    refused(t, "bad/self", {"bad/self": "^1"}, "invalid_manifest", "depends on itself")


@case(fixture="g6_core", spec=["dependencies/requirement-syntax", "package-ids/package-id-syntax#unsafe-components-are-refused"])
def invalid_dependency_id_refused(t: T):
    """1e. An invalid dependency ID is refused with invalid_id."""
    refused(t, "bad/id", {"Lib/../core": "^1"}, "invalid_id")


# ------------------------------------------------------------ 2–7. catalog resolution


@case(fixture="g6_core_catalog", spec=["dependencies/install-resolves-dependencies#dependency-pulled-from-the-catalog", "dependencies/state-keeps-dependency-edges#list-after-install"])
def catalog_install_pulls_dependency(t: T):
    """2. install-catalog app/a into an empty workspace installs lib/core 1.1.0 in the same command."""
    t.gitpm_ok("install-catalog", "app/a")
    state = t.state()
    t.eq(sorted(state["packages"]), ["app/a", CORE], "installed")
    core = state["packages"][CORE]
    t.eq(core["current"], {"version": "1.1.0", "commit": registered(t, CORE, "1.1.0")["commit"]},
         "lib/core current (max matching ^1.0)")
    t.eq(state["packages"]["app/a"]["dependencies"], {CORE: "^1.0"}, "dependency edge in state")
    t.eq(t.rev(t.clone(CORE)), core["current"]["commit"], "lib/core clone HEAD")
    t.has((t.dist(CORE) / "index.js").read_text(), "lib/core 1.1.0")
    view = resolved(t)
    row = view["app/a"]["dependencies"][0]
    t.eq((row["id"], row["provider"], row["version"], row["satisfied"]), (CORE, CORE, "1.1.0", True),
         "resolved dependency")
    t.eq(view[CORE].get("dependents"), ["app/a"], "lib/core dependents")


@case(fixture="g6_core_catalog", spec=["dependencies/conflicts-are-explained"])
def missing_dependency_refused(t: T):
    """3. app/lonely needs lib/missing (not in the catalog): dependency_conflict naming it; byte-identical."""
    t.gitpm_ok("install-catalog", "app/a")
    before = t.snap(t.ws)
    t.gitpm_err("install-catalog", "app/lonely", kind="dependency_conflict", contains="lib/missing")
    t.same(before, "workspace")
    t.not_installed("app/lonely")


@case(fixture="g6_core_catalog", spec=["dependencies/conflicts-are-explained", "install-uninstall/refused-input-creates-nothing"])
def missing_dependency_refused_in_empty_workspace(t: T):
    """3'. The same into an empty workspace leaves no package, clone, dist or transaction."""
    t.gitpm_err("install-catalog", "app/lonely", kind="dependency_conflict", contains="lib/missing")
    t.not_installed("app/lonely")
    t.eq(t.state().get("packages", {}), {}, "no packages")


@case(fixture="g6_core_catalog", spec=["dependencies/conflicts-are-explained#two-consumers-disagree"])
def version_conflict_names_both_consumers(t: T):
    """4. app/b (^2.0) while app/a (^1.0) is installed: dependency_conflict naming app/a and app/b."""
    t.gitpm_ok("install-catalog", "app/a")
    before = t.snap(t.ws)
    t.gitpm_err("install-catalog", "app/b", kind="dependency_conflict", contains=("app/a", "app/b", CORE))
    t.same(before, "workspace")
    t.not_installed("app/b")


@case(fixture="g6_core_catalog", spec=["dependencies/install-resolves-dependencies"])
def cycle_resolves_deterministically(t: T):
    """5. x/a ↔ x/b resolves and installs within 60 s, identically in two workspaces."""
    r1 = t.gitpm("install-catalog", "x/a", timeout=60)
    r2 = t.gitpm("--root", t.root / "ws2", "install-catalog", "x/a", timeout=60)
    t.eq((r1.code, r2.code), (0, 0), "exit codes")
    one = t.state()["packages"]
    two = json.loads((t.root / "ws2/state.json").read_text())["packages"]
    t.eq(sorted(one), ["x/a", "x/b"], "installed")
    t.eq({k: v["current"] for k, v in one.items()}, {k: v["current"] for k, v in two.items()}, "same result")
    view = resolved(t)
    t.eq(view["x/a"]["dependencies"][0]["provider"], "x/b", "x/a → x/b")
    t.eq(view["x/b"]["dependencies"][0]["provider"], "x/a", "x/b → x/a")


@case(fixture="g6_core_catalog", spec=["dependencies/updates-keep-dependents-working#breaking-update"])
def update_breaking_dependent_refused(t: T):
    """6. update lib/core (latest 2.0.0) while app/a needs ^1.0: refused naming app/a; 1.1.0 untouched."""
    t.gitpm_ok("install-catalog", "app/a")
    before = t.snap(t.ws)
    t.gitpm_err("update", CORE, kind="dependency_conflict", contains="app/a")
    t.same(before, "workspace")
    t.eq(t.package(CORE)["current"]["version"], "1.1.0", "lib/core version")


@case(fixture="g6_core_catalog", spec=["dependencies/install-resolves-dependencies", "dependencies/state-keeps-dependency-edges"])
def shared_dependency_single_copy(t: T):
    """7. app/a and app/c share one lib/core; uninstalling app/a keeps it."""
    t.gitpm_ok("install-catalog", "app/a")
    t.gitpm_ok("install-catalog", "app/c")
    t.eq(sorted(t.state()["packages"]), ["app/a", "app/c", CORE], "one lib/core")
    t.eq(resolved(t)[CORE].get("dependents"), ["app/a", "app/c"], "dependents")
    core = t.snap(t.clone(CORE), t.dist(CORE))
    t.gitpm_ok("uninstall", "app/a")
    t.not_installed("app/a")
    t.same(core, "lib/core clone and dist")
    t.eq(resolved(t)[CORE].get("dependents"), ["app/c"], "dependents after uninstall")


# ------------------------------------------------------------- 8. direct Git install


@case(fixture="g6_core_catalog", spec=["dependencies/install-resolves-dependencies#no-catalog"])
def direct_install_without_catalog_refused(t: T):
    """8a. No catalog configured and lib/core not installed: refused naming lib/core and "no catalog configured"."""
    t.gitpm_err("install", t.repo("app/a"), env=NO_CATALOG, kind="dependency_conflict",
             contains=(CORE, "no catalog configured"))
    t.not_installed("app/a")
    t.not_installed(CORE)


@case(fixture="g6_core_catalog", spec=["cli/commands-that-need-a-catalog-say-so"])
def missing_catalog_location_is_refused(t: T):
    """8a'. A configured catalog location that does not exist is no "no catalog": `--catalog ./missing install`
    fails with the library's invalid_argument before anything is created."""
    before = t.snap(t.root, git=True)
    t.gitpm_err("--catalog", "./missing", "install", t.repo("app/a"), kind="invalid_argument",
             contains="catalog location ./missing does not exist")
    t.same(before, "fixture (workspace, catalog, repos)")
    t.ok(not (t.root / "missing").exists(), "no catalog created")


@case(fixture="g6_core_catalog", spec=["dependencies/install-resolves-dependencies"])
def direct_install_with_dependency_installed(t: T):
    """8b. lib/core already installed (from Git), no catalog: app/a installs and resolves to it."""
    before = t.snap(t.catalog)
    t.gitpm_ok("install", t.repo(CORE), "--ref", f"v1.1.0", env=NO_CATALOG)
    t.gitpm_ok("install", t.repo("app/a"), env=NO_CATALOG)
    row = resolved(t)["app/a"]["dependencies"][0]
    t.eq((row["provider"], row["satisfied"]), (CORE, True), "resolved")
    t.eq(t.package("app/a").get("catalog"), None, "direct install records no catalog")
    t.same(before, "catalog untouched")


@case(fixture="g6_core_catalog", spec=["dependencies/install-resolves-dependencies"])
def direct_install_with_catalog(t: T):
    """8c. With the catalog: app/a from Git, lib/core 1.1.0 pulled from the catalog."""
    t.gitpm_ok("install", t.repo("app/a"))
    t.eq(sorted(t.state()["packages"]), ["app/a", CORE], "installed")
    t.eq(t.package(CORE)["current"]["version"], "1.1.0", "lib/core version")
    row = resolved(t)["app/a"]["dependencies"][0]
    t.eq((row["provider"], row["satisfied"]), (CORE, True), "resolved")


# ------------------------------------------------- 9. download, strict enable (D7, TILR-36)


@case(fixture="g6_core_catalog", spec=["install-uninstall/download-without-enabling#download-with-a-missing-dependency",
                                       "install-uninstall/download-without-enabling#enable-after-download",
                                       "dist/enable-and-disable-deploy-and-remove#refused-enable"])
def download_then_enable_in_dependency_order(t: T):
    """9a. download-catalog app/a records app/a and the pulled lib/core disabled, deploys nothing and prints one
    line per package; enable app/a is refused (lib/core disabled); enable lib/core then app/a works offline."""
    r = t.gitpm_ok("download-catalog", "app/a")
    lines = r.out.splitlines()
    t.eq(len(lines), 2, "one line per package")
    t.ok(lines[0].startswith("downloaded app/a@1.0.0 from ") and lines[0].endswith(" (disabled)"), lines[0])
    t.ok(lines[1].startswith(f"downloaded {CORE}@1.1.0 from ") and lines[1].endswith(" (disabled)"), lines[1])
    for id in ["app/a", CORE]:
        t.eq(t.package(id)["enabled"], False, f"{id} disabled")
        t.ok((t.clone(id) / ".git").is_dir(), f"{id} clone")
        t.ok(not t.dist(id).exists(), f"no dist/{id}")
    before = t.snap(t.ws)
    r = t.gitpm_err("enable", "app/a", kind="dependency_conflict", contains=f"{CORE} is disabled; enable it")
    t.has(r.err, "Hint: `gitpm enable app/a --force` enables it anyway")
    t.same(before, "workspace")
    t.gitpm_ok("enable", CORE, env=NO_CATALOG)
    t.eq(t.gitpm_ok("enable", "app/a", env=NO_CATALOG).out, "app/a enabled\n", "enable line")
    t.has((t.dist("app/a") / "index.js").read_text(), "app/a 1.0.0")
    t.has((t.dist(CORE) / "index.js").read_text(), "lib/core 1.1.0")


@case(fixture="g6_core_catalog", spec=["dependencies/enable-needs-active-dependencies#forced-enable",
                                       "dependencies/disabled-packages-and-dependencies#dependency-on-a-disabled-package-in-list"])
def forced_enable_leaves_the_consumer_broken(t: T):
    """9b. enable app/a --force enables only app/a; list and check report its dependency on the disabled lib/core."""
    t.gitpm_ok("download-catalog", "app/a")
    t.gitpm_ok("enable", "app/a", "--force")
    t.ok((t.dist("app/a") / "index.js").is_file(), "dist/app/a deployed")
    t.ok(not t.dist(CORE).exists(), "lib/core stays disabled")
    t.eq(t.package(CORE)["enabled"], False, "lib/core disabled")
    row = resolved(t)["app/a"]["dependencies"][0]
    t.eq((row["provider"], row["satisfied"], row.get("problem")), (CORE, False, f"{CORE} is disabled"), "dependency")
    t.has(t.gitpm_ok("check").out, f"Unsatisfied dependency: app/a → {CORE} ^1.0: {CORE} is disabled")


@case(fixture="g6_core_catalog", spec=["dependencies/disabled-packages-and-dependencies#install-next-to-a-disabled-dependency"])
def install_next_to_a_disabled_dependency(t: T):
    """9c. lib/core installed but disabled: install-catalog app/a and a direct install of app/a are refused
    with dependency_conflict naming lib/core as disabled; the workspace is unchanged."""
    t.gitpm_ok("install-catalog", CORE, "--version", "1.1.0")
    t.gitpm_ok("disable", CORE)
    before = t.snap(t.ws)
    t.gitpm_err("install-catalog", "app/a", kind="dependency_conflict", contains=f"{CORE} is disabled; enable it")
    t.gitpm_err("install", t.repo("app/a"), kind="dependency_conflict", contains=f"{CORE} is disabled; enable it")
    t.same(before, "workspace")
    t.ok("app/a" not in t.state()["packages"], "app/a not recorded")
    # After enabling the dependency the install goes through.
    t.gitpm_ok("enable", CORE)
    t.gitpm_ok("install-catalog", "app/a")
    t.ok((t.dist("app/a") / "index.js").is_file(), "dist/app/a deployed")


@case(fixture="g6_core_catalog", spec=["dist/inactive-packages-change-state-and-clone-only#download-deploys-nothing",
                                       "install-uninstall/download-without-enabling"])
def direct_download_deploys_nothing(t: T):
    """9d. download GIT_SOURCE records the package disabled with its clone, prints `(disabled)` and creates no dist."""
    r = t.gitpm_ok("download", t.repo("other/x"))
    t.ok(r.out.startswith("downloaded other/x@1.0.0 from ") and r.out.endswith(" (disabled)\n"), r.out)
    t.eq(t.package("other/x")["enabled"], False, "disabled")
    t.ok((t.clone("other/x") / ".git").is_dir(), "clone")
    t.ok(not t.dist("other/x").exists(), "no dist")


@case(fixture="g6_core_catalog", spec=["install-uninstall/install-is-idempotent-for-the-same-source#install-over-a-download"])
def install_over_a_download_enables_it(t: T):
    """9e. install-catalog over a download is refused while lib/core is disabled, `--force` enables only app/a
    (an install line, not "already installed"); a second install is "already installed"."""
    t.gitpm_ok("download-catalog", "app/a")
    before = t.snap(t.ws)
    t.gitpm_err("install-catalog", "app/a", kind="dependency_conflict", contains=f"{CORE} is disabled; enable it")
    t.same(before, "workspace")
    r = t.gitpm_ok("install-catalog", "app/a", "--force")
    t.ok(r.out.startswith("installed app/a@1.0.0 from "), r.out)
    t.eq((t.package("app/a")["enabled"], t.package(CORE)["enabled"]), (True, False), "only app/a enabled")
    t.ok((t.dist("app/a") / "index.js").is_file(), "dist/app/a deployed")
    t.has(t.gitpm_ok("install-catalog", "app/a").out, "app/a already installed (1.0.0)")


@case(fixture="g6_core_catalog", spec=["install-uninstall/install-the-latest-release-of-a-git-source#forced-install-next-to-a-disabled-dependency"])
def forced_install_next_to_a_disabled_dependency(t: T):
    """9f. install --force of app/a and install-catalog --force of app/c next to the disabled lib/core enable
    and deploy them only."""
    t.gitpm_ok("install-catalog", CORE, "--version", "1.1.0")
    t.gitpm_ok("disable", CORE)
    t.gitpm_ok("install", t.repo("app/a"), "--force")
    t.ok((t.dist("app/a") / "index.js").is_file(), "dist/app/a deployed")
    t.ok(not t.dist(CORE).exists(), "lib/core stays disabled")
    t.gitpm_err("install-catalog", "app/c", kind="dependency_conflict", contains=f"{CORE} is disabled; enable it")
    t.gitpm_ok("install-catalog", "app/c", "--force")
    t.ok((t.dist("app/c") / "index.js").is_file(), "dist/app/c deployed")


@case(fixture="g6_core_catalog", spec=["dependencies/disabled-packages-and-dependencies#rollback-and-restore-next-to-a-disabled-dependency"])
def rollback_and_restore_refuse_a_disabled_dependency(t: T):
    """9g. rollback of an active app/a and restore of its enabled record refuse the disabled lib/core."""
    t.gitpm_ok("install-catalog", "app/a")
    t.release("app/a", "1.1.0", deps={CORE: "^1.0"})
    t.gitpm_ok("update", "app/a")
    t.gitpm_ok("disable", CORE, "--force")
    before = t.snap(t.ws)
    t.gitpm_err("rollback", "app/a", kind="dependency_conflict", contains=f"{CORE} is disabled; enable it")
    t.same(before, "workspace")
    t.gitpm_ok("forget", "app/a")
    before = t.snap(t.ws)
    t.gitpm_err("restore", "app/a", kind="dependency_conflict", contains=f"{CORE} is disabled; enable it")
    t.same(before, "workspace")
