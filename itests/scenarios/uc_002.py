"""UC-002: Environment and path precedence (P1).

gitpm resolves its settings from a flag, then the environment, then a default
(`--root`/`GITPM_ROOT`/`./gitpm-workspace`, `--catalog`/`GITPM_CATALOG`/none,
`--sources`/`GITPM_SOURCES`/`./sources.txt`); there is no config file and the old
stand variables are ignored. The scenario containers set GITPM_ROOT=/fx/ws,
GITPM_CATALOG=/fx/catalog and GITPM_SOURCES=/fx/sources.txt.
"""

import json

from gitpmtest import T, case

ID = "dan/hello"
SETTINGS = "cli/settings-come-from-flags-environment-defaults"
GITPM_VARS = ["GITPM_ROOT", "GITPM_CATALOG", "GITPM_SOURCES"]
# The stand's variable, spelled in two parts: scenarios must not reference it otherwise.
LEGACY_ROOT = "PM" "_ROOT"


def without_gitpm_env(*argv) -> list[str]:
    out = ["env"]
    for name in GITPM_VARS:
        out += ["-u", name]
    return out + [str(a) for a in argv]


@case(fixture="hello", spec=[SETTINGS])
def root_option_beats_env(t: T):
    """--root wins over GITPM_ROOT."""
    other = t.root / "other-ws"
    t.gitpm_ok("--root", other, "install", t.repo(ID))
    state = json.loads((other / "state.json").read_text())
    t.ok(ID in state["packages"], "installed into --root")
    t.ok(not t.ws.exists(), "GITPM_ROOT untouched")


@case(fixture="hello", spec=[SETTINGS])
def root_env_is_used_without_option(t: T):
    """Without --root the workspace is GITPM_ROOT."""
    t.gitpm_ok("install", t.repo(ID))
    t.ok(ID in t.state()["packages"], "installed into GITPM_ROOT")
    t.ok(not (t.root / "gitpm-workspace").exists(), "no default workspace")


@case(fixture="hello", spec=[SETTINGS])
def catalog_option_beats_env(t: T):
    """--catalog wins over GITPM_CATALOG."""
    other = t.root / "other-catalog"
    t.gitpm_ok("--catalog", other, "register", t.repo(ID), "--ref", f"{ID}@0.1.0")
    t.ok(t.catalog_index(ID, other).is_file(), "release in --catalog")
    t.ok(not t.catalog.exists(), "GITPM_CATALOG untouched")


@case(fixture="hello", spec=[SETTINGS])
def sources_option_beats_env(t: T):
    """--sources wins over GITPM_SOURCES; without it GITPM_SOURCES is used."""
    t.new_package("dan/other", "1.0.0")
    (t.root / "sources.txt").write_text(f"{t.repo('dan/other')}\n")   # GITPM_SOURCES
    (t.root / "explicit.txt").write_text(f"{t.repo(ID)}\n")
    t.gitpm_ok("catalog-update", "--sources", t.root / "explicit.txt")
    t.ok(t.catalog_index(ID).is_file(), "explicit source registered")
    t.ok(not t.catalog_index("dan/other").exists(), "GITPM_SOURCES not read when --sources is given")
    t.gitpm_ok("catalog-update")
    t.ok(t.catalog_index("dan/other").is_file(), "GITPM_SOURCES used without --sources")


@case(fixture="hello", spec=[SETTINGS])
def defaults_in_isolated_cwd(t: T):
    """Without GITPM_*, the workspace is ./gitpm-workspace, sources ./sources.txt, and there is no catalog."""
    iso = t.root / "iso"
    iso.mkdir()
    r = t.run(*without_gitpm_env("gitpm", "install", t.repo(ID)), cwd=iso)
    t.eq(r.code, 0, "install exit")
    t.ok(ID in json.loads((iso / "gitpm-workspace/state.json").read_text())["packages"], "./gitpm-workspace")
    (iso / "sources.txt").write_text(f"{t.repo(ID)}\n")
    r = t.run(*without_gitpm_env("gitpm", "--catalog", "cat", "catalog-update"), cwd=iso)
    t.eq(r.code, 0, "catalog-update exit")
    t.ok(t.catalog_index(ID, iso / "cat").is_file(), "./sources.txt read by default")
    r = t.run(*without_gitpm_env("gitpm", "info"), cwd=iso)
    t.ok(r.code != 0, "info without a catalog fails")
    t.has(r.err, "info needs a catalog: pass --catalog DIR_OR_JSON_OR_URL or set GITPM_CATALOG")
    for p in (t.ws, t.catalog, t.root / "sources.txt"):
        t.ok(not p.exists(), f"environment path {p} untouched")


@case(fixture="hello", spec=[f"{SETTINGS}#legacy-variable"])
def legacy_root_variable_is_ignored(t: T):
    """The stand's root variable is ignored: without GITPM_ROOT the workspace is ./gitpm-workspace."""
    iso = t.root / "iso"
    iso.mkdir()
    elsewhere = t.root / "elsewhere"
    r = t.run(*without_gitpm_env("env", f"{LEGACY_ROOT}={elsewhere}", "gitpm", "install", t.repo(ID)), cwd=iso)
    t.eq(r.code, 0, "install exit")
    t.ok((iso / "gitpm-workspace/state.json").is_file(), "./gitpm-workspace used")
    t.ok(not elsewhere.exists(), "legacy root untouched")


@case(spec=["cli/commands-that-need-a-catalog-say-so"])
def catalog_commands_without_catalog(t: T):
    """Catalog commands without --catalog/GITPM_CATALOG name the missing catalog and create nothing."""
    iso = t.root / "iso"
    iso.mkdir()
    for argv in (["info"], ["catalog"], ["catalog-update"],
                 ["register", "/fx/none", "--ref", "x"], ["install-catalog", ID]):
        r = t.run(*without_gitpm_env("gitpm", *argv), cwd=iso)
        t.ok(r.code != 0, f"{argv[0]} fails")
        t.has(r.err, f"{argv[0]} needs a catalog: pass --catalog DIR_OR_JSON_OR_URL or set GITPM_CATALOG")
    t.eq(sorted(p.name for p in iso.iterdir()), [], "nothing created")
