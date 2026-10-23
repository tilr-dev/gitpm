"""UC-072: Multi-package repositories (P1, gitpm 0.4+).

/fx/repos/suite is made with `gitpm init tilr/store --into suite --multi` plus `init`
of tilr/theme and tilr/panel into it (fixture fork_suite: gitpm.json `packages/*`, one
release 0.1.0 per package). fork_suite_installed adds tilr/store and tilr/theme installed
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
    """`init --multi` creates gitpm.json and packages/NAME with release 0.1.0; a second `init`
    into the repository adds packages/NAME2 with its own tag."""
    repo = t.root / "authors/suite"
    r = t.gitpm_ok("init", STORE, "--into", repo, "--multi")
    t.has(r.out, f"ID: {STORE}", f"Git commit and tag {STORE}@0.1.0")
    t.ok(any(" install " in line and str(repo) in line and line.endswith(f" {STORE}") for line in r.out.splitlines()),
         "next step names `install REPO ID`")
    t.eq(json.loads((repo / "gitpm.json").read_text()), {"packages": ["packages/*"]}, "gitpm.json")
    t.ok((repo / "packages/store/dist/index.js").is_file(), "package folder with dist")
    r = t.gitpm_ok("init", THEME, "--into", repo)
    t.has(r.out, f"Git commit and tag {THEME}@0.1.0", f"--package {THEME}")
    t.eq(sorted(t.git(repo, "tag", "--list").split()), [f"{STORE}@0.1.0", f"{THEME}@0.1.0"], "one tag per package")
    t.eq(t.git(repo, "status", "--porcelain"), "", "clean worktree")
    t.eq(json.loads((repo / "packages/theme/plugin.json").read_text())["id"], THEME, "second package id")


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
        t.has(r.out, f"installed {id}@0.1.0 from {suite(t)}")
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


@case(fixture="fork_suite_installed", spec=["multi-package-repos/per-package-releases#independent-releases",
                                            "authoring/commit-releases-a-version#release-one-package-of-a-multi-package-repository"])
def per_package_release_and_update(t: T):
    """`commit SRC 0.2.0 --package tilr/store` tags only tilr/store; `update tilr/store` moves it,
    tilr/theme stays at 0.1.0 and keeps its dist."""
    t.write(suite(t), {"packages/store/dist/index.js": "export const greeting = 'store 0.2.0';\n"})
    r = t.gitpm_ok("commit", suite(t), "0.2.0", "--package", STORE)
    t.has(r.out, f"{STORE} committed 0.2.0", f"tag {STORE}@0.2.0")
    t.eq(sorted(t.git(suite(t), "tag", "--list").split()),
         sorted([f"{STORE}@0.1.0", f"{STORE}@0.2.0", f"{THEME}@0.1.0", f"{PANEL}@0.1.0"]), "tags")
    theme_before = t.snap(t.dist(THEME))
    theme_entry = t.package(THEME)
    t.has(t.gitpm_ok("update", STORE).out, f"{STORE} updated 0.1.0 → 0.2.0")
    t.has(t.gitpm_ok("update", THEME).out, f"{THEME} up to date (0.1.0)")
    t.eq(t.package(STORE)["current"]["version"], "0.2.0", "store updated")
    t.eq((t.dist(STORE) / "index.js").read_text(), "export const greeting = 'store 0.2.0';\n", "store dist")
    t.eq(t.package(THEME), theme_entry, "theme state")
    t.same(theme_before, "theme dist")


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
    t.write(suite(t), {"packages/theme/dist/index.js": "export const greeting = 'theme 0.1.1';\n"})
    t.gitpm_ok("commit", suite(t), "0.1.1", "--package", THEME)
    t.gitpm_ok("update", THEME)
    t.eq(t.package(THEME)["current"]["version"], "0.1.1", "theme still updatable")
    t.gitpm_ok("uninstall", THEME)
    t.ok(not clone.exists(), "clone removed with its last package")


@case(fixture="fork_suite", spec=["catalog/register-one-version", "catalog/catalog-generated-from-git-release-tags",
                                  "multi-package-repos/per-package-releases"])
def register_package_and_catalog_update(t: T):
    """`register SRC --ref TAG --package ID` adds one package's release; `catalog-update` from a
    sources list naming the repository publishes every package; install-catalog uses the shared clone."""
    single = t.root / "single-catalog"
    r = t.gitpm_ok("--catalog", single, "register", suite(t), "--ref", f"{THEME}@0.1.0", "--package", THEME)
    t.eq(r.json()["manifest"]["id"], THEME, "registered manifest")
    t.ok(t.catalog_release(THEME, "0.1.0", single).is_file(), "theme release registered")
    t.ok(not t.catalog_index(STORE, single).exists(), "only the named package registered")
    (t.root / "sources.txt").write_text(f"{suite(t)}\n")
    t.gitpm_ok("catalog-update")
    for id in (STORE, THEME, PANEL):
        index = json.loads(t.catalog_index(id).read_text())
        t.eq(sorted(index["versions"]), ["0.1.0"], f"{id} published")
        release = json.loads(t.catalog_release(id, "0.1.0").read_text())
        t.eq(release["manifest"]["id"], id, f"{id} release manifest")
    t.gitpm_ok("install-catalog", STORE)
    t.gitpm_ok("install-catalog", THEME)
    t.eq(t.package(STORE)["clone"], t.package(THEME)["clone"], "catalog installs share one clone")
    t.eq((t.package(STORE)["path"], t.package(THEME)["path"]), ("packages/store", "packages/theme"), "paths")


@case(fixture="fork_suite_installed", spec=["multi-package-repos/forking-packages-of-one-repository",
                                            "fork-replacement/fork-metadata#fork-manifest",
                                            "fork-replacement/fork-isolates-source-and-destination-state#repository-starting-revision"])
def fork_one_package_of_the_repository(t: T):
    """Fork uses the newest installed revision, excluding packages introduced later."""
    before = t.snap(t.ws, suite(t), git=True)
    dest = t.root / "forks/suite"
    r = t.gitpm_ok("fork", THEME, "--ns", "me", "--into", dest)
    t.has(r.out, "me/theme forked from tilr/theme (packages/theme)",
          "Next steps:", "--package me/theme", "gitpm replace tilr/theme --with me/theme")
    t.eq(json.loads((dest / "gitpm.json").read_text()), {"packages": ["packages/*"]}, "fork gitpm.json")
    t.eq(sorted(p.name for p in (dest / "packages").iterdir()), ["store", "theme"], "package folders at newest installed revision")
    t.eq(t.rev(dest, "HEAD~1"), t.package(THEME)["current"]["commit"], "newest installed revision")
    for name in ("store", "theme"):
        sibling = json.loads((dest / "packages" / name / "plugin.json").read_text())
        t.eq(sibling["id"], f"me/{name}", f"{name} renamed")
        current = t.package(f"tilr/{name}")["current"]
        t.eq(sibling["forkOf"], {"id": f"tilr/{name}", **current,
                               "source": t.package(f"tilr/{name}")["source"]}, f"{name} provenance")
    m = json.loads((dest / "packages/theme/plugin.json").read_text())
    theme = t.package(THEME)["current"]
    t.eq((m["id"], m["forkOf"], m["replaces"]),
         ("me/theme", {"id": THEME, "version": "0.1.0", "commit": theme["commit"],
                       "source": t.package(THEME)["source"]}, {"id": THEME, "version": "0.1.0"}),
         "fork manifest")
    t.same(before, "workspace")
    t.gitpm_ok("commit", dest, "0.1.1", "--package", "me/theme")
    t.gitpm_ok("install", dest, "me/theme")
    t.eq(t.package("me/theme")["forkOf"]["id"], THEME, "installed fork provenance")
    t.has((t.dist("me/theme") / "index.js").read_text(), "theme")


@case(fixture="fork_suite_installed", spec=["fork-replacement/fork-isolates-source-and-destination-state#repository-starting-revision",
                                            "fork-replacement/fork-metadata"])
def fork_uses_all_installed_revisions_and_starting_revision_siblings(t: T):
    """An unrequested installed sibling selects the base; older installed folders are restored."""
    source = suite(t)
    store = t.package(STORE)["current"]
    store_dist = t.git(source, "show", f"{store['commit']}:packages/store/dist/index.js")
    theme_manifest = json.loads((source / "packages/theme/plugin.json").read_text())
    theme_manifest["version"] = "0.1.1"
    base = t.commit(source, "Release theme with an uninstalled sibling", {
        "packages/theme/plugin.json": json.dumps(theme_manifest) + "\n",
        "packages/store/dist/index.js": "newer store work\n",
        "packages/panel/dist/index.js": "panel at installed base\n",
        "base-marker.txt": "installed base\n",
    })
    t.tag(source, f"{THEME}@0.1.1")
    t.gitpm_ok("update", THEME)
    t.eq(t.package(THEME)["current"]["commit"], base, "newest installed sibling")
    t.commit(source, "Later source-only work", {
        "packages/panel/dist/index.js": "later panel work\n",
        "base-marker.txt": "later source head\n",
    })
    before = t.snap(t.ws, source, git=True)
    dest = t.root / "forks/revision-suite"
    t.gitpm_ok("fork", STORE, "--ns", "me", "--into", dest)
    t.eq(t.rev(dest, "HEAD~1"), base, "base includes unrequested installed theme")
    t.eq((dest / "base-marker.txt").read_text(), "installed base\n", "root at starting revision")
    t.eq((dest / "packages/store/dist/index.js").read_text().strip(), store_dist, "older installed store restored")
    t.eq((dest / "packages/panel/dist/index.js").read_text(), "panel at installed base\n", "uninstalled panel at starting revision")
    for name, version, commit in (("store", "0.1.0", store["commit"]),
                                  ("theme", "0.1.1", base), ("panel", "0.1.0", base)):
        manifest = json.loads((dest / "packages" / name / "plugin.json").read_text())
        t.eq(manifest["id"], f"me/{name}", f"{name} renamed")
        t.eq(manifest["forkOf"], {"id": f"tilr/{name}", "version": version,
                                  "commit": commit, "source": str(source)}, f"{name} provenance")
    t.same(before, "installation and source")


@case(fixture="fork_suite_installed", spec=["fork-replacement/fork-isolates-source-and-destination-state#existing-destination"])
def existing_multi_package_fork_is_refused_unchanged(t: T):
    dest = t.root / "forks/existing-suite"
    t.gitpm_ok("fork", THEME, "--ns", "me", "--into", dest)
    before = t.snap(t.ws, suite(t), dest, git=True)
    t.gitpm_err("fork", STORE, "--ns", "other", "--into", dest,
                kind="invalid_argument", contains="destination already exists; pass a new folder")
    t.same(before, "existing multi-package fork, installation and source")
