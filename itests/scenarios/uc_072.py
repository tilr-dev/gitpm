"""UC-072: Multi-package repositories (P1, gitpm 0.4+).

/fx/repos/suite is made with `gitpm init tilr/store --into suite --multi` plus `init`
of tilr/theme and tilr/panel into it (fixture fork_suite: gitpm.json `packages/*`, one
release 0.1.1 per package). fork_suite_installed adds tilr/store and tilr/theme installed
with `install SRC ID`, sharing one clone `ws/repos/suite-<hash>`.
"""

import json
import re

from gitpmtest import T, case

SUITE = "suite"
STORE, THEME, PANEL = "tilr/store", "tilr/theme", "tilr/panel"


def suite(t: T):
    return t.repo(SUITE)


def shared_clone(t: T):
    return t.ws / "repos" / t.package(STORE)["clone"]


@case(fixture=None, spec=["authoring/init-options#start-a-multi-package-repository-from-nothing",
                          "authoring/init-into-a-multi-package-repository"])
def init_multi_then_add_packages(t: T):
    """An added package inherits the repository version and waits for the next release."""
    repo = t.root / "authors/suite"
    r = t.gitpm_ok("init", STORE, "--into", repo, "--multi")
    t.has(r.out, f"ID: {STORE}", "Git commit and tag v0.1.0")
    t.gitpm_ok("commit", repo, "0.2.0")
    r = t.gitpm_ok("init", THEME, "--into", repo)
    t.eq(t.git(repo, "tag", "--list").split(), ["v0.1.0", "v0.2.0"], "addition creates no tag")
    t.eq(json.loads((repo / "packages/theme/plugin.json").read_text())["version"], "0.2.0", "inherited version")
    t.eq(t.git(repo, "status", "--porcelain"), "", "clean worktree")
    t.gitpm_ok("commit", repo, "0.2.1")
    for name in ("store", "theme"):
        t.eq(json.loads((repo / "packages" / name / "plugin.json").read_text())["version"], "0.2.1", name)


@case(fixture="fork_suite", spec=["multi-package-repos/choosing-a-package-at-install#missing-package-id"])
def install_without_package_id_lists_packages(t: T):
    """`install SRC` of a repository with several packages: refused with the list; nothing created."""
    r = t.gitpm_err("install", suite(t), kind="invalid_argument",
                    contains=("repository has 3 packages at commit", "choose one by ID:",
                              f"{PANEL} (packages/panel)", f"{STORE} (packages/store)", f"{THEME} (packages/theme)"))
    t.lacks(r.out, "installed")
    t.ok(not (t.ws / "state.json").exists() or not t.state().get("packages"), "nothing installed")
    t.ok(not (t.ws / "repos").exists() or not any((t.ws / "repos").iterdir()), "no clone left")


@case(fixture="fork_suite", spec=["multi-package-repos/one-shared-clone-per-source",
                                  "multi-package-repos/choosing-a-package-at-install"])
def two_packages_share_one_clone(t: T):
    """`install SRC ID` twice: one clone repos/suite-<hash>, state records clone and path, only
    <path>/dist is deployed for each, no packages/<id> clone."""
    for id in (STORE, THEME):
        r = t.gitpm_ok("install", suite(t), id)
        t.has(r.out, f"installed {id}@0.1.1 from {suite(t)}")
    clones = sorted(p.name for p in (t.ws / "repos").iterdir())
    t.eq(len(clones), 1, "one shared clone")
    t.ok(re.fullmatch(r"suite-[0-9a-f]{8,}", clones[0]) is not None, f"clone name {clones[0]} is <name>-<hash>")
    for id, folder in ((STORE, "packages/store"), (THEME, "packages/theme")):
        p = t.package(id)
        t.eq((p["clone"], p["path"]), (clones[0], folder), f"{id} clone and path")
        t.ok(not t.clone(id).exists(), f"no packages/{id} clone")
        t.eq(sorted(f.name for f in t.dist(id).iterdir()), ["index.js"], f"{id}: only its dist/ deployed")
        t.has((t.dist(id) / "index.js").read_text(), f"{id.split('/')[1]} 0.1.0")
    t.ok(not t.dist(PANEL).exists(), "panel not installed")


@case(fixture="fork_suite_installed", spec=["multi-package-repos/repository-releases"])
def repository_release_and_individual_updates(t: T):
    """Publishing a version bumps all manifests; installed packages can update separately."""
    t.write(suite(t), {"packages/store/dist/index.js": "export const greeting = 'store 0.2.0';\n"})
    r = t.gitpm_ok("commit", suite(t), "0.2.0")
    t.has(r.out, "committed 0.2.0", "tag v0.2.0")
    t.eq(t.git(suite(t), "tag", "--list").split(), ["v0.1.1", "v0.2.0"], "repository tags")
    for name in ("store", "theme", "panel"):
        t.eq(json.loads((suite(t) / "packages" / name / "plugin.json").read_text())["version"], "0.2.0", name)
    theme_before = t.snap(t.dist(THEME))
    t.gitpm_ok("update", STORE)
    t.eq(t.package(THEME)["current"]["version"], "0.1.1", "theme not updated yet")
    t.gitpm_ok("update", THEME)
    t.eq(t.package(STORE)["current"]["version"], "0.2.0", "store updated")
    t.eq(t.package(THEME)["current"]["version"], "0.2.0", "theme updated")
    t.same(theme_before, "theme payload unchanged")


@case(fixture="fork_suite_installed", spec=["multi-package-repos/one-shared-clone-per-source#uninstall-one-of-two",
                                            "multi-package-repos/shared-clone-head-handling"])
def uninstall_one_of_two_keeps_shared_clone(t: T):
    """Uninstalling tilr/store keeps the shared clone; tilr/theme keeps working; uninstalling the
    last package removes the clone."""
    clone = shared_clone(t)
    t.gitpm_ok("uninstall", STORE)
    t.ok(STORE not in t.state()["packages"] and not t.dist(STORE).exists(), "store removed")
    t.ok(clone.is_dir(), "shared clone kept")
    t.has((t.dist(THEME) / "index.js").read_text(), "theme 0.1.0")
    t.eq(t.rev(clone), t.package(THEME)["current"]["commit"], "clone HEAD at theme's installed revision")
    t.write(suite(t), {"packages/theme/dist/index.js": "export const greeting = 'theme 0.1.2';\n"})
    t.gitpm_ok("commit", suite(t), "0.1.2")
    t.gitpm_ok("update", THEME)
    t.eq(t.package(THEME)["current"]["version"], "0.1.2", "theme still updatable")
    t.gitpm_ok("uninstall", THEME)
    t.ok(not clone.exists(), "clone removed with its last package")


@case(fixture="fork_suite", spec=["catalog/register-one-version", "catalog/catalog-generated-from-git-release-tags",
                                  "multi-package-repos/repository-releases"])
def register_package_and_catalog_update(t: T):
    """`register SRC --ref TAG --package ID` adds one package's release; `catalog-update` from a
    sources list naming the repository publishes every package; install-catalog uses the shared clone."""
    single = t.root / "single-catalog"
    r = t.gitpm_ok("--catalog", single, "register", suite(t), "--ref", f"v0.1.1", "--package", THEME)
    t.eq(r.json()["manifest"]["id"], THEME, "registered manifest")
    t.ok(t.catalog_release(THEME, "0.1.1", single).is_file(), "theme release registered")
    t.ok(not t.catalog_index(STORE, single).exists(), "only the named package registered")
    (t.root / "sources.txt").write_text(f"{suite(t)}\n")
    t.gitpm_ok("catalog-update")
    for id in (STORE, THEME, PANEL):
        index = json.loads(t.catalog_index(id).read_text())
        t.eq(sorted(index["versions"]), ["0.1.1"], f"{id} published")
        release = json.loads(t.catalog_release(id, "0.1.1").read_text())
        t.eq(release["manifest"]["id"], id, f"{id} release manifest")
    t.gitpm_ok("install-catalog", STORE)
    t.gitpm_ok("install-catalog", THEME)
    t.eq(t.package(STORE)["clone"], t.package(THEME)["clone"], "catalog installs share one clone")
    t.eq((t.package(STORE)["path"], t.package(THEME)["path"]), ("packages/store", "packages/theme"), "paths")


@case(fixture="fork_suite_installed", spec=["multi-package-repos/forking-packages-of-one-repository",
                                            "fork-replacement/fork-metadata#fork-manifest"])
def fork_one_package_of_the_repository(t: T):
    """`fork tilr/theme --ns me --into DIR`: the new fork keeps every package and the original layout,
    renames installed and uninstalled siblings; originals stay unchanged."""
    before = t.snap(t.ws, git=True)
    dest = t.root / "forks/suite"
    r = t.gitpm_ok("fork", THEME, "--ns", "me", "--into", dest)
    t.has(r.out, "me/theme forked from tilr/theme (packages/theme)",
          "Next steps (or all at once: add --replace --version NEW_VERSION):", "gitpm replace tilr/theme --with me/theme")
    t.eq(json.loads((dest / "gitpm.json").read_text()), {"packages": ["packages/*"]}, "fork gitpm.json")
    t.eq(sorted(p.name for p in (dest / "packages").iterdir()), ["panel", "store", "theme"], "all package folders preserved")
    for name in ("panel", "store", "theme"):
        sibling = json.loads((dest / "packages" / name / "plugin.json").read_text())
        t.eq(sibling["id"], f"me/{name}", f"{name} renamed")
        t.eq(sibling["forkOf"]["id"], f"tilr/{name}", f"{name} provenance")
    m = json.loads((dest / "packages/theme/plugin.json").read_text())
    theme = t.package(THEME)["current"]
    t.eq((m["id"], m["forkOf"], m["replaces"]),
         ("me/theme", {"id": THEME, "version": "0.1.1", "commit": theme["commit"],
                       "source": t.package(THEME)["source"]}, {"id": THEME, "version": "0.1.1"}),
         "fork manifest")
    t.same(before, "workspace")
    t.gitpm_ok("commit", dest, "0.1.2")
    t.gitpm_ok("install", dest, "me/theme")
    t.eq(t.package("me/theme")["forkOf"]["id"], THEME, "installed fork provenance")
    t.has((t.dist("me/theme") / "index.js").read_text(), "theme")
