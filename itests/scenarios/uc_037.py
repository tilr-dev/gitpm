"""UC-037: V1 migration and backward compatibility (P0).

Fixture g4_v1: /fx/v1/catalog.json in the monolithic v1 format of the lifecycle
tests: legacy `oldie` 0.1.0, 0.2.0 without files/build; dan/hello 0.1.0, 0.1.1 with
files, without build.
"""

import json
import shutil

from gitpmtest import T, case

OLD, HELLO = "oldie", "dan/hello"


def v1(t: T):
    return t.root / "v1"


def original(t: T) -> dict:
    return json.loads((v1(t) / "catalog.json").read_text())


def release_path(t: T, id: str, version: str, root=None):
    root = root or v1(t)
    folder = ("namespaces/" if "/" in id else "legacy/") + id
    return root / folder / "versions" / version / "release.json"


def check_migrated(t: T, root, data: dict, original_bytes: bytes):
    t.eq((root / ".catalog-v1.json").read_bytes(), original_bytes, "backup is the original v1 JSON")
    t.eq(json.loads((root / "catalog.json").read_text()),
         {"schema": 2, "directories": {"legacy": "legacy/", "namespaces": "namespaces/"}}, "v2 root")
    for id, plugin in data["plugins"].items():
        folder = root / (("namespaces/" if "/" in id else "legacy/") + id)
        index = json.loads((folder / "plugin.json").read_text())
        t.eq(index["source"], plugin["source"], f"{id} source")
        t.ok(set(plugin["versions"]) <= set(index["versions"]), f"{id}: all old versions linked")
        for version, old in plugin["versions"].items():
            new = json.loads(release_path(t, id, version, root).read_text())
            for key in ["source", "commit", "reference"]:
                t.eq(new[key], old[key], f"{id} {version} {key}")
            for key, value in old["manifest"].items():
                t.eq(new["manifest"].get(key), value, f"{id} {version} manifest.{key}")
            t.eq(new.get("files"), old.get("files"), f"{id} {version} files (absent hashes not invented)")
            t.ok("build" not in new, f"{id} {version} no build invented")


@case(fixture="g4_v1", spec=["catalog/v1-catalogs-stay-readable", "catalog/release-record#legacy-release-without-hashes"])
def read_and_install_before_migration(t: T):
    """The v1 catalog is readable and installable as is (legacy without files, scoped
    with files), check works; the catalog file is not modified."""
    before = t.snap(v1(t))
    t.has(t.gitpm_ok("--catalog", v1(t), "info").out, "Format: v1", "oldie — latest: 0.2.0")
    t.gitpm_ok("--catalog", v1(t), "install-catalog", OLD)
    t.gitpm_ok("--catalog", v1(t), "install-catalog", HELLO, "--version", "0.1.0")
    t.eq(t.plugin(OLD)["current"], {"version": "0.2.0", "commit": t.rev(OLD, "v0.2.0")}, "oldie installed")
    t.eq(t.plugin(HELLO)["current"]["commit"], t.rev(HELLO, f"{HELLO}@0.1.0"), "hello installed")
    t.has((t.dist(OLD) / "lib/util.js").read_text(), "util 0.2.0")
    out = t.gitpm_ok("--catalog", v1(t), "check").out
    t.has(next(l for l in out.splitlines() if l.startswith(HELLO)), "update available")
    t.has(next(l for l in out.splitlines() if l.startswith(OLD)), "up to date")
    t.same(before, "v1 catalog (read-only)")


@case(fixture="g4_v1", spec=["catalog/v1-catalogs-stay-readable#migrate"])
def migrate_keeps_backup_and_releases(t: T):
    """migrate-catalog: .catalog-v1.json is the original, every old release survives
    byte-for-byte in meaning, absent files/build are not invented; installs still work."""
    data, raw = original(t), (v1(t) / "catalog.json").read_bytes()
    t.gitpm_ok("--catalog", v1(t), "migrate-catalog")
    check_migrated(t, v1(t), data, raw)
    t.ok("files" not in json.loads(release_path(t, OLD, "0.1.0").read_text()), "legacy release still has no files key")
    t.gitpm_ok("--catalog", v1(t), "install-catalog", OLD, "--version", "0.1.0")
    t.gitpm_ok("--catalog", v1(t), "install-catalog", HELLO)
    t.eq(t.plugin(HELLO)["current"]["version"], "0.1.1", "latest after migration")
    t.eq(t.plugin(OLD)["current"]["commit"], t.rev(OLD, "v0.1.0"), "legacy exact version after migration")


@case(fixture="g4_v1", spec=["catalog/v1-catalogs-stay-readable#migrate"])
def repeated_migration_is_safe(t: T):
    """A second migrate-catalog changes nothing (backup included)."""
    t.gitpm_ok("--catalog", v1(t), "migrate-catalog")
    before = t.snap(v1(t))
    t.gitpm_ok("--catalog", v1(t), "migrate-catalog")
    t.same(before, "migrated catalog after a repeated migration")


@case(fixture="g4_v1", spec=["catalog/published-versions-are-immutable", "catalog/release-record"])
def registrations_after_migration(t: T):
    """New registrations calculate hashes; re-registering an old legacy release keeps it
    unchanged (no hashes added)."""
    t.gitpm_ok("--catalog", v1(t), "migrate-catalog")
    old = t.snap(v1(t) / "legacy/oldie/versions")
    t.gitpm_ok("--catalog", v1(t), "register", t.repo(OLD), "--ref", "v0.1.0")
    t.same(old, "legacy releases after identical re-registration")
    t.gitpm_ok("--catalog", v1(t), "register", t.repo(HELLO), "--ref", f"{HELLO}@0.2.0")
    new = json.loads(release_path(t, HELLO, "0.2.0").read_text())
    t.ok(new["files"].get("index.js", {}).get("size", 0) > 0, "new release has file hashes")
    t.release(OLD, "0.3.0", tag=False)
    t.tag(OLD, "v0.3.0")
    t.gitpm_ok("--catalog", v1(t), "register", t.repo(OLD), "--ref", "v0.3.0")
    t.ok(json.loads(release_path(t, OLD, "0.3.0").read_text()).get("files"), "new legacy release has hashes")


@case(fixture="g4_v1", spec=["catalog/v1-catalogs-stay-readable", "catalog/register-one-version"])
def register_auto_migrates(t: T):
    """register into an untouched v1 copy migrates it first: backup, v2 tree, old releases
    kept; the new release carries hashes."""
    copy = t.root / "v1-copy"
    shutil.copytree(v1(t), copy)
    data, raw = original(t), (copy / "catalog.json").read_bytes()
    t.gitpm_ok("--catalog", copy, "register", t.repo(HELLO), "--ref", f"{HELLO}@0.2.0")
    check_migrated(t, copy, data, raw)
    new = json.loads(release_path(t, HELLO, "0.2.0", copy).read_text())
    t.ok(new["files"], "new release has hashes")
    t.eq(sorted(json.loads((copy / "namespaces/dan/hello/plugin.json").read_text())["versions"]),
         ["0.1.0", "0.1.1", "0.2.0"], "hello versions")
    t.eq((v1(t) / "catalog.json").read_bytes(), raw, "the other copy untouched")
