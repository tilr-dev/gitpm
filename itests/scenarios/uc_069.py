"""UC-069: dist holds exactly the active packages (gitpm 0.5).

`dist/<id>/` exists exactly when the package is installed, enabled and not replaced:
disable, forget and replace remove it; enable, restore and `replace ORIGINAL --with ORIGINAL` deploy it
again from the clone's committed tree; folders of no known package are left alone with a
warning; an active package whose clone is gone is skipped with a warning.

Fixtures: g2_demos_installed (A = dd3387ed-a5a3-5059-81ad-b1f6987a28f2, B = be517c52-cf84-5615-9a18-9fae7d82c819, unrelated), g6_forked
(c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 with consumers 9692fc07-23c7-5f20-8779-f09374f1a571, f5984d44-92bd-51ac-8e8f-d911dd824ef3 and the fork db33e43e-e652-5767-8e21-27fb3cef9723 1.0.1 installed, no
replacement), g6_replaced (the same with `replace c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 --with db33e43e-e652-5767-8e21-27fb3cef9723`).
"""

import hashlib
import json
import shutil
import subprocess

from gitpmtest import T, case

A, B = "dd3387ed-a5a3-5059-81ad-b1f6987a28f2", "be517c52-cf84-5615-9a18-9fae7d82c819"
ORIGINAL, FORK = "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "db33e43e-e652-5767-8e21-27fb3cef9723"
FORK_PAYLOAD = "export const greeting = 'fork payload';\n"


def committed(t: T, id: str, path: str = "dist/index.js") -> str:
    """The bytes of `path` at the installed commit of `id` (from its clone)."""
    commit = t.package(id)["current"]["commit"]
    return t.run("git", "-C", t.clone(id), "show", f"{commit}:{path}", quiet=True, check=True).out


def resolved(t: T) -> dict:
    return {p["id"]: p for p in t.gitpm_ok("list").json()["resolved"]}


def deployed(t: T, id: str):
    t.eq((t.dist(id) / "index.js").read_text(), committed(t, id), f"dist/{id} holds the committed payload")


@case(fixture="g2_demos_installed", spec=["dist/enable-and-disable-deploy-and-remove#disable-and-enable",
                                          "dist/dist-holds-exactly-the-active-packages#installed-and-enabled"])
def disable_removes_enable_redeploys(t: T):
    """disable A removes dist/A (B untouched, A not active in list); enable deploys it again."""
    deployed(t, A)
    b = t.snap(t.dist(B))
    r = t.gitpm_ok("disable", A)
    t.eq(r.out, f"{A} disabled\n", "output")
    t.ok(not t.dist(A).exists(), "dist/A removed")
    t.same(b, "dist of B")
    view = resolved(t)
    t.eq((view[A]["enabled"], view[A]["active"]), (False, False), "A disabled and inactive")
    t.eq(view[B]["active"], True, "B active")
    t.eq(t.gitpm_ok("enable", A).out, f"{A} enabled\n", "output")
    deployed(t, A)
    t.eq(resolved(t)[A]["active"], True, "A active again")
    t.same(b, "dist of B")


@case(fixture="g2_demos_installed", spec=["dist/deploys-copy-the-committed-tree#uncommitted-edits-are-not-deployed"])
def enable_deploys_committed_tree_not_working_tree(t: T):
    """An uncommitted edit in A's clone is never deployed by enable."""
    t.gitpm_ok("disable", A)
    (t.clone(A) / "dist/index.js").write_text("export const greeting = 'uncommitted';\n")
    t.gitpm_ok("enable", A)
    deployed(t, A)
    t.lacks((t.dist(A) / "index.js").read_text(), "uncommitted")


@case(fixture="catalog", spec=["dist/deploys-copy-the-committed-tree#export-ignore-in-the-package-repository"])
def export_ignore_deploys_and_hashes_committed_bytes(t: T):
    """Catalog registration, protected install and update use the deployed Git blobs."""
    id = "2fe4fa03-ed3c-54ab-ad3b-31ab1470e966"
    attributes = "dist/debug.js export-ignore\ndist/subst.txt export-subst\n"
    dist_attributes = "debug.js export-ignore\nsubst.txt export-subst\n"
    literal = "$Format:%H$\n"
    first = t.new_package(id, "1.0.0", files={
        ".gitattributes": attributes,
        "dist/.gitattributes": dist_attributes,
        "dist/debug.js": "debug one\n",
        "dist/subst.txt": literal,
    })
    second = t.release(id, "1.1.0", files={
        "dist/debug.js": "debug two\n",
        "dist/subst.txt": literal,
    })
    for version, commit in [("1.0.0", first), ("1.1.0", second)]:
        t.gitpm_ok("register", t.repo(id), "--ref", f"v{version}")
        release = json.loads(t.catalog_release(id, version).read_text())
        blobs = {}
        for name in (".gitattributes", "debug.js", "index.js", "subst.txt"):
            data = subprocess.run(
                ["git", "-C", str(t.repo(id)), "cat-file", "blob", f"{commit}:dist/{name}"],
                capture_output=True, check=True,
            ).stdout
            blobs[name] = data
        t.eq(release["files"], {
            name: {"sha256": hashlib.sha256(data).hexdigest(), "size": len(data)}
            for name, data in blobs.items()
        }, f"catalog hashes committed bytes at {version}")
        t.eq(blobs["subst.txt"], literal.encode(), "export-subst placeholder is literal")
        if version == "1.0.0":
            first_blobs = blobs
        else:
            second_blobs = blobs

    t.edit_json(t.catalog / "catalog.json", lambda c: c.update(verified=True))
    t.gitpm_ok("install-catalog", id, "--version", "1.0.0")
    for name, data in first_blobs.items():
        t.eq((t.dist(id) / name).read_bytes(), data, f"installed {name} equals hashed blob")
    t.gitpm_ok("update", id)
    for name, data in second_blobs.items():
        t.eq((t.dist(id) / name).read_bytes(), data, f"updated {name} equals hashed blob")


@case(fixture="catalog", spec=["catalog/register-one-version#submodule-inside-dist",
                               "catalog/catalog-generated-from-git-release-tags#submodule-inside-dist",
                               "dist/deploys-copy-the-committed-tree#submodule-inside-dist"])
def gitlinks_warn_at_publication_and_deploy(t: T):
    """Authors and users see warnings; protected deploys keep gitlinks empty."""
    id = "b7222c4f-97a2-57f1-9a18-648afa1243e4"
    t.new_package(id, "1.0.0")
    repo = t.repo(id)
    sha = t.rev(repo)
    t.git(repo, "update-index", "--add", "--cacheinfo", f"160000,{sha},dist/vendor")
    t.git(repo, "commit", "-q", "-m", "gitlink in dist")
    t.git(repo, "tag", "-f", f"v1.0.0")

    def warned(result):
        t.has(result.err, "Warning:", "dist/vendor is a Git submodule",
              "its files will not be installed", "directory will be empty",
              "probably a mistake", "commit built files to dist instead of a submodule")

    warned(t.gitpm_ok("register", repo, "--ref", f"v1.0.0"))
    registered = json.loads(t.catalog_release(id, "1.0.0").read_text())
    t.eq(sorted(registered["files"]), sorted(["index.js"]), "gitlink has no file digest")
    sources = t.root / "gitlink-sources.txt"
    t.write(sources.parent, {sources.name: str(repo) + "\n"})
    generated = t.root / "gitlink-catalog"
    warned(t.gitpm_ok("catalog-update", "--catalog", generated, "--sources", sources))
    record = t.catalog_release(id, "1.0.0", generated)
    t.eq(json.loads(record.read_text())["files"], registered["files"], "publication paths hash identical bytes")
    t.edit_json(t.catalog / "catalog.json", lambda c: c.update(verified=True))
    warned(t.gitpm_ok("install-catalog", id))
    vendor = t.dist(id) / "vendor"
    t.ok(vendor.is_dir(), "protected install keeps the empty gitlink directory")
    t.eq(list(vendor.iterdir()), [], "no submodule files installed")
    t.edit_json(repo / "plugin.json", lambda m: m.update(version="1.1.0"))
    t.git(repo, "add", "plugin.json")
    t.git(repo, "commit", "-q", "-m", "next release with gitlink")
    t.git(repo, "tag", f"v1.1.0")
    warned(t.gitpm_ok("register", repo, "--ref", f"v1.1.0"))
    warned(t.gitpm_ok("update", id))
    t.eq(list(vendor.iterdir()), [], "protected update keeps gitlink empty")
    t.gitpm_ok("disable", id)
    warned(t.gitpm_ok("enable", id))
    t.eq(list(vendor.iterdir()), [], "enable uses the same exporter")
    shutil.rmtree(t.dist(id))
    warned(t.gitpm_ok("sync-dist"))
    t.ok(vendor.is_dir(), "explicit reconcile restores the gitlink directory")
    t.eq(list(vendor.iterdir()), [], "reconciled gitlink remains empty")


@case(fixture="g2_demos_installed", spec=["dist/forget-restore-and-uninstall-settle-dist",
                                          "dist/empty-parent-folders-are-removed#last-package-removed"])
def forget_removes_restore_redeploys(t: T):
    """forget A removes dist/A (and the empty dist/vendor) but keeps the clone; restore deploys it."""
    clone = t.snap(t.clone(A), git=True)
    t.gitpm_ok("forget", A)
    t.ok(not t.dist(A).exists(), "dist/A removed")
    t.ok(not (t.ws / "dist/vendor").exists(), "empty dist/vendor removed")
    t.same(clone, "retained clone")
    t.eq(t.gitpm_ok("restore", A).out, f"{A} restored 0.1.0\n", "output")
    deployed(t, A)


@case(fixture="g2_demos_installed", spec=["dist/empty-parent-folders-are-removed#last-package-removed"])
def uninstall_removes_empty_namespace_folders(t: T):
    """uninstall A (the only package of namespace vendor) removes dist/vendor and its unused clone."""
    t.eq(t.gitpm_ok("uninstall", A).out, f"{A} uninstalled\n", "output")
    t.not_installed(A)
    t.ok(not (t.ws / "dist/vendor").exists(), "dist/vendor removed")
    t.ok(not t.clone(A).exists(), "unused clone removed")
    deployed(t, B)


@case(fixture="g2_demos_installed", spec=["dist/unknown-paths-are-left-alone#unknown-folder"])
def unknown_folder_is_left_alone_and_warned(t: T):
    """dist/custom (no known package) survives a mutating command, which warns about it."""
    t.write(t.ws / "dist/custom", {"keep.js": "custom\n"})
    custom = t.snap(t.ws / "dist/custom")
    r = t.gitpm_ok("disable", A)
    t.has(r.err, "Warning: dist/custom belongs to no known package or is not a folder; left as it is")
    t.same(custom, "dist/custom")
    t.ok(not t.dist(A).exists(), "dist/A still removed")


@case(fixture="g2_demos_installed", spec=["dist/undeployable-packages-do-not-block-others#clone-deleted-by-hand"])
def missing_clone_blocks_only_its_own_dist(t: T):
    """A's clone and dist deleted by hand: disabling B succeeds and warns that A is not deployed."""
    shutil.rmtree(t.clone(A))
    shutil.rmtree(t.dist(A))
    r = t.gitpm_ok("disable", B)
    t.has(r.err, f"Warning: {A}: dist not deployed (clone missing); uninstall or forget {A} to drop its record")
    t.ok(not t.dist(B).exists(), "dist/B removed")
    t.ok(not t.dist(A).exists(), "dist/A not fabricated")


@case(fixture="g2_demos_installed", spec=["install-uninstall/removing-a-package-whose-clone-is-gone#clone-deleted-by-hand"])
def uninstall_of_a_package_whose_clone_is_gone(t: T):
    """A's clone deleted by hand: `uninstall A` (no --force, no TTY) drops the record and dist/A,
    and later commands warn no more."""
    shutil.rmtree(t.clone(A))
    r = t.gitpm_ok("uninstall", A)
    t.eq(r.out, f"{A} uninstalled\n", "result line")
    t.not_installed(A)
    t.ok(A not in t.state().get("forgotten", {}), "no forgotten record")
    t.ok(not t.dist(A).exists(), "dist/A removed")
    r = t.gitpm_ok("disable", B)
    t.lacks(r.err, "dist not deployed")


@case(fixture="g2_demos_installed", spec=["install-uninstall/removing-a-package-whose-clone-is-gone#forget-without-a-clone"])
def forget_of_a_package_whose_clone_is_gone(t: T):
    """A's clone deleted by hand: `forget A` drops the record and dist/A and keeps no forgotten
    record (nothing to restore)."""
    shutil.rmtree(t.clone(A))
    r = t.gitpm_ok("forget", A)
    t.eq(r.out, f"{A} forgotten (no clone; record and dist removed)\n", "result line")
    t.not_installed(A)
    t.ok(A not in t.state().get("forgotten", {}), "no forgotten record")
    t.ok(not t.dist(A).exists(), "dist/A removed")
    t.gitpm_err("restore", A, kind="repository_not_found")


@case(fixture="g6_forked", spec=["dist/dist-holds-exactly-the-active-packages#enabled-but-replaced",
                                 "dist/switching-back-redeploys-the-original#back-to-the-original"])
def replace_removes_original_back_redeploys(t: T):
    """replace c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 --with db33e43e-e652-5767-8e21-27fb3cef9723 removes dist/c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 (fork's dist stays, original
    stays enabled but inactive); replace ORIGINAL --with ORIGINAL deploys it again and keeps the fork's dist."""
    fork = t.snap(t.dist(FORK))
    r = t.gitpm_ok("replace", ORIGINAL, "--with", FORK)
    t.has(r.out, f"{ORIGINAL} replaced by {FORK} 1.0.1")
    t.ok(not t.dist(ORIGINAL).exists(), "dist/c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 removed")
    t.same(fork, "dist/db33e43e-e652-5767-8e21-27fb3cef9723")
    t.eq((t.dist(FORK) / "index.js").read_text(), FORK_PAYLOAD, "fork payload")
    view = resolved(t)
    t.eq((view[ORIGINAL]["enabled"], view[ORIGINAL]["active"]), (True, False), "original enabled, inactive")
    r = t.gitpm_ok("replace", ORIGINAL, "--with", ORIGINAL)
    t.has(r.out, f"{ORIGINAL} switched back from {FORK}")
    deployed(t, ORIGINAL)
    t.same(fork, "dist/db33e43e-e652-5767-8e21-27fb3cef9723 after --back")
    t.eq(resolved(t)[ORIGINAL]["active"], True, "original active again")


@case(fixture="g6_forked", spec=["fork-replacement/replacement-is-explicit-and-global#fork-not-usable-as-the-replacing-package"])
def replace_with_disabled_fork_is_refused(t: T):
    """A disabled fork cannot replace: invalid_argument, dist/c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 stays."""
    t.gitpm_ok("disable", FORK)
    before = t.snap(t.ws, git=True)
    t.gitpm_err("replace", ORIGINAL, "--with", FORK, kind="invalid_argument",
                contains=f"{FORK} is disabled; enable it first")
    t.same(before, "workspace")
    deployed(t, ORIGINAL)


@case(fixture="g6_replaced", spec=["dist/inactive-packages-change-state-and-clone-only#update-of-a-replaced-package"])
def update_of_replaced_package_deploys_nothing_until_back(t: T):
    """A replaced c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 updated to 1.1.0: state and clone move, no dist until replace ORIGINAL --with ORIGINAL (1.1.0)."""
    commit = t.release(ORIGINAL, "1.1.0")
    t.gitpm_ok("update", ORIGINAL)
    t.eq(t.package(ORIGINAL)["current"], {"version": "1.1.0", "commit": commit}, "state")
    t.eq(t.rev(t.clone(ORIGINAL)), commit, "clone HEAD")
    t.ok(not t.dist(ORIGINAL).exists(), "no dist/c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 while replaced")
    t.gitpm_ok("replace", ORIGINAL, "--with", ORIGINAL)
    t.has((t.dist(ORIGINAL) / "index.js").read_text(), f"{ORIGINAL} 1.1.0")


@case(fixture="g6_replaced", spec=["fork-replacement/active-fork-rules#uninstall-the-fork"])
def uninstalling_the_fork_redeploys_the_original(t: T):
    """uninstall db33e43e-e652-5767-8e21-27fb3cef9723 (the active replacement) needs no --force while the installed
    c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 1.0.0 satisfies 9692fc07-23c7-5f20-8779-f09374f1a571 and f5984d44-92bd-51ac-8e8f-d911dd824ef3; it drops the binding and deploys
    dist/c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 again in the same command."""
    t.ok(not t.dist(ORIGINAL).exists(), "precondition: original not deployed")
    t.gitpm_ok("uninstall", FORK)
    t.not_installed(FORK)
    t.ok(not t.state().get("replacements"), "binding dropped")
    deployed(t, ORIGINAL)


@case(fixture="g6_replaced", spec=["dist/enable-and-disable-deploy-and-remove#forced-disable-of-an-active-replacement"])
def forced_disable_of_active_replacement(t: T):
    """c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 uninstalled while replaced: disable db33e43e-e652-5767-8e21-27fb3cef9723 refuses (9692fc07-23c7-5f20-8779-f09374f1a571, f5984d44-92bd-51ac-8e8f-d911dd824ef3 would
    break); --force removes dist/db33e43e-e652-5767-8e21-27fb3cef9723 and drops the binding, nothing is deployed for
    c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 and the consumers are reported broken."""
    t.gitpm_ok("uninstall", ORIGINAL)
    t.gitpm_err("disable", FORK, kind="has_dependents", contains="required by 9692fc07-23c7-5f20-8779-f09374f1a571, f5984d44-92bd-51ac-8e8f-d911dd824ef3")
    t.gitpm_ok("disable", FORK, "--force")
    t.ok(not t.dist(FORK).exists(), "dist/db33e43e-e652-5767-8e21-27fb3cef9723 removed")
    t.ok(not t.dist(ORIGINAL).exists(), "dist/c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 not deployed")
    t.ok(not t.state().get("replacements"), "binding dropped")
    for consumer in ["9692fc07-23c7-5f20-8779-f09374f1a571", "f5984d44-92bd-51ac-8e8f-d911dd824ef3"]:
        row = next(r for r in resolved(t)[consumer]["dependencies"] if r["id"] == ORIGINAL)
        t.eq(row["satisfied"], False, f"{consumer} broken")


@case(fixture="g6_replaced", spec=["fork-replacement/active-fork-rules#disable-the-fork", "dependencies/dependents-guard-removal#installed-original-satisfies-the-consumers"])
def disabling_the_fork_drops_the_binding_when_the_original_serves(t: T):
    """The installed c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 satisfies 9692fc07-23c7-5f20-8779-f09374f1a571 and f5984d44-92bd-51ac-8e8f-d911dd824ef3: disable db33e43e-e652-5767-8e21-27fb3cef9723 needs no --force,
    drops the binding and deploys dist/c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9; enabling the fork again keeps it unbound."""
    t.gitpm_ok("disable", FORK)
    t.ok(not t.dist(FORK).exists(), "dist/db33e43e-e652-5767-8e21-27fb3cef9723 removed")
    deployed(t, ORIGINAL)
    t.ok(not t.state().get("replacements"), "binding dropped")
    t.gitpm_ok("enable", FORK)
    t.ok(not t.state().get("replacements"), "enable does not restore the binding")
    t.ok(t.dist(FORK).exists() and t.dist(ORIGINAL).exists(), "both deployed")
