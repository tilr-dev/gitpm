"""UC-062: Repeated mutations (idempotency) (P1).

Every case runs a mutation twice and snapshots state.json, the workspace tree, the catalog
and the author repository (incl. .git) before the second call.
"""

from gitpmtest import T, case

B = "03c25ec4-f583-513d-adfb-092fb98d8dba"


def everything(t: T):
    return t.snap(t.ws, t.catalog, t.root / "repos", git=True)


def second_ok(t: T, *args):
    t.gitpm_ok(*args)
    before = everything(t)
    t.gitpm_ok(*args)
    t.same(before, "everything after the second call")


def second_err(t: T, first: list, again: list | None = None, **kw):
    t.gitpm_ok(*first)
    before = everything(t)
    r = t.gitpm_err(*(again or first), **kw)
    t.same(before, "everything after the second call")
    return r


@case(fixture="g3_trio_installed", spec=["dist/enable-and-disable-deploy-and-remove", "dist/repeating-enable-and-disable-is-idempotent#repeat-enable"])
def enable_enabled(t: T):
    second_ok(t, "enable", B)
    t.eq(t.package(B)["enabled"], True, "enabled")


@case(fixture="g3_trio_installed", spec=["dist/enable-and-disable-deploy-and-remove", "dist/repeating-enable-and-disable-is-idempotent#repeat-disable"])
def disable_disabled(t: T):
    second_ok(t, "disable", B)
    t.eq(t.package(B)["enabled"], False, "disabled")


@case(fixture="g3_trio_installed", spec=["install-uninstall/install-is-idempotent-for-the-same-source#repeat-install"])
def install_installed(t: T):
    """install of an installed ID from the same source: `ID already installed (V)`, exit 0, nothing changes."""
    before = everything(t)
    t.eq(t.gitpm_ok("install", t.repo(B)).out, f"{B} already installed (0.1.0)\n", "output")
    t.same(before, "everything")


@case(fixture="g3_trio_installed", spec=["errors/errors-name-the-package#repeated-forget"])
def forget_forgotten(t: T):
    second_err(t, ["forget", B], kind="not_installed", contains=f"package {B} is not installed")


@case(fixture="g3_trio_installed", spec=["errors/stable-error-kinds#removal-of-an-unknown-package"])
def uninstall_uninstalled(t: T):
    """Unknown-ID error naming it (not_installed or repository_not_found)."""
    r = second_err(t, ["uninstall", B], contains=B)
    t.ok("Error kind: not_installed" in r.err or "Error kind: repository_not_found" in r.err,
         "unknown-ID kind")


@case(fixture="g3_trio_installed", spec=["install-uninstall/restore-preconditions"])
def restore_registered(t: T):
    """restore of an already registered ID: non-zero or no-op, state byte-identical."""
    before = everything(t)
    r = t.gitpm("restore", B)
    print(f"  restore of a registered ID: exit {r.code}")
    t.same(before, "everything")
    if r.code:
        t.has(r.err, B)


@case(fixture="catalog", spec=["catalog/published-versions-are-immutable"])
def register_same_release(t: T):
    """README: repeated register of the same release is allowed; catalog byte-identical."""
    second_ok(t, "register", t.repo("3dce1568-569c-5b53-9321-37185e751947"), "--ref", "v0.1.1")


@case(fixture="catalog", spec=["catalog/published-versions-are-immutable#regenerate"])
def catalog_update_unchanged_sources(t: T):
    (t.root / "sources.txt").write_text(f"{t.repo('3dce1568-569c-5b53-9321-37185e751947')}\n{t.repo('2190eee8-4be5-5a91-81d2-6ca95c1e61d0')}\n")
    second_ok(t, "catalog-update")
    t.has(t.catalog_index("3dce1568-569c-5b53-9321-37185e751947").read_text(), '"0.2.0"')


@case(fixture="hello", spec=["authoring/existing-tags-are-never-moved"])
def commit_existing_version(t: T):
    """Second commit with the same version: tag already exists; HEAD and tags unchanged."""
    repo = t.repo("3dce1568-569c-5b53-9321-37185e751947")
    t.write(repo, {"dist/index.js": "export const greeting = 'next';\n"})
    t.gitpm_ok("commit", repo, "0.1.1")
    t.write(repo, {"dist/index.js": "export const greeting = 'again';\n"})
    head, tags = t.rev(repo), t.git(repo, "tag", "--list")
    before = everything(t)
    r = t.gitpm_err("commit", repo, "0.1.1", kind="version_conflict")
    t.has(r.err.lower(), "tag already exists: v0.1.1")
    t.same(before, "everything (incl. the uncommitted edit)")
    t.eq((t.rev(repo), t.git(repo, "tag", "--list")), (head, tags), "HEAD and tags")


@case(fixture="hello", spec=["authoring/remote-sets-origin-without-network#set-origin"])
def remote_same_url(t: T):
    repo = t.repo("3dce1568-569c-5b53-9321-37185e751947")
    second_ok(t, "remote", repo, "https://example.invalid/3dce1568-569c-5b53-9321-37185e751947.git")
    t.eq(t.git(repo, "remote", "get-url", "origin"), "https://example.invalid/3dce1568-569c-5b53-9321-37185e751947.git", "origin")


@case(fixture="hello_installed", spec=["updates/stable-update-moves-to-the-newest-release#update-and-repeat"])
def update_nothing_new(t: T):
    second_ok(t, "update", "3dce1568-569c-5b53-9321-37185e751947")
    t.eq(t.package("3dce1568-569c-5b53-9321-37185e751947")["current"]["version"], "0.1.0", "version")
