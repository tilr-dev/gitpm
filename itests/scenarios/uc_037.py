"""UC-037: v1 catalogs are refused, there is no migration (P0).

Fixture g4_v1: /fx/v1/catalog.json in the monolithic v1 format gitpm 0.5 still read:
old/oldie 0.1.0, 0.2.0 without files/build; dan/hello 0.1.0, 0.1.1 with files. No
backward compatibility before 1.0: every catalog read fails with `catalog`, nothing is
written, and `migrate-catalog` is an unknown command. The way forward is catalog-update
into an empty folder.
"""

from gitpmtest import T, case

OLD, HELLO = "old/oldie", "dan/hello"


def v1(t: T):
    return t.root / "v1"


@case(fixture="g4_v1", spec=["catalog/catalog-is-a-tree-of-static-files", "errors/stable-error-kinds"])
def every_read_is_refused(t: T):
    """info, install-catalog, check and update fail with `catalog`; the v1 file and the
    workspace keep their bytes."""
    before = t.snap(v1(t))
    for argv in (["info"], ["install-catalog", HELLO], ["install-catalog", OLD]):
        t.gitpm_err("--catalog", v1(t), *argv, kind="catalog", contains="v1 catalogs")
    t.ok(not t.state().get("packages"), "nothing installed")
    t.gitpm_ok("install", t.repo(HELLO), "--ref", f"v0.1.0")
    t.gitpm_err("--catalog", v1(t), "check", kind="catalog", contains="v1 catalogs")
    t.same(before, "v1 catalog")


@case(fixture="g4_v1", spec=["catalog/catalog-is-a-tree-of-static-files", "catalog/register-one-version"])
def writes_into_a_v1_catalog_are_refused(t: T):
    """register and catalog-update into the v1 folder fail with `catalog` and write nothing
    (no backup, no catalog tree)."""
    before = t.snap(v1(t))
    t.gitpm_err("--catalog", v1(t), "register", t.repo(HELLO), "--ref", f"v0.2.0",
                kind="catalog", contains="v1 catalogs")
    sources = t.root / "sources.txt"
    sources.write_text(f"{t.repo(HELLO)}\n")
    t.gitpm_err("--catalog", v1(t), "catalog-update", "--sources", sources, kind="catalog", contains="v1 catalogs")
    t.same(before, "v1 catalog")
    t.ok(not (v1(t) / ".catalog-v1.json").exists() and not (v1(t) / "namespaces").exists(), "nothing written")


@case(fixture="g4_v1", spec=["cli/command-set#removed-command"])
def migrate_catalog_is_unknown(t: T):
    """migrate-catalog is no command any more; the v1 catalog is untouched."""
    before = t.snap(v1(t))
    r = t.run("gitpm", "--catalog", v1(t), "migrate-catalog")
    t.ok(r.code != 0, "migrate-catalog fails")
    t.has(r.err, "unknown command: migrate-catalog; use --help")
    t.same(before, "v1 catalog")


@case(fixture="g4_v1", spec=["catalog/catalog-generated-from-git-release-tags"])
def regenerate_into_an_empty_folder(t: T):
    """The migration path: catalog-update from the release tags into an empty folder gives
    a v2 catalog with file hashes for every release."""
    fresh = t.root / "fresh"
    sources = t.root / "sources.txt"
    sources.write_text(f"{t.repo(HELLO)}\n{t.repo(OLD)}\n")
    t.gitpm_ok("--catalog", fresh, "catalog-update", "--sources", sources)
    for id in (HELLO, OLD):
        t.ok(t.catalog_index(id, fresh).is_file(), f"{id} index")
    t.ok(t.catalog_release(OLD, "0.1.0", fresh).is_file(), "old/oldie 0.1.0 published")
    t.gitpm_ok("--catalog", fresh, "install-catalog", OLD)
    t.eq(t.package(OLD)["current"]["version"], "0.2.0", "installed from the regenerated catalog")
