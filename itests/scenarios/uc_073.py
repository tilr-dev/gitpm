"""UC-073: Package metadata: title, description, provides, tags (P1).

gitpm stores and reports the optional keys of plugin.json without interpreting them:
`list` shows them in `resolved[]` and in `packages.<id>.metadata` with all four keys
(absent string = null, absent list = []), `list --provides SERVICE` filters, wrong-typed
keys are ignored with a warning when installing and refused by `commit`, and `info`
prints the metadata of each package's latest catalog release. Packages are created by the
case (no fixture).
"""

import json

from gitpmtest import T, case

THEME, STORE, DARK = "tilr/theme", "tilr/store", "me/dark"
THEME_META = {"title": "Theme", "description": "Dark and light", "provides": "theme", "tags": ["ui", "colors"]}


def resolved(t: T, value: dict, id: str) -> dict:
    for entry in value["resolved"]:
        if entry["id"] == id:
            return entry
    raise AssertionError(f"{id} not in resolved: {[e['id'] for e in value['resolved']]}")


def ids(value: dict) -> list[str]:
    return sorted(e["id"] for e in value["resolved"])


def two_plugins(t: T):
    t.new_package(THEME, "1.0.0", extra=THEME_META)
    t.new_package(STORE, "1.0.0")
    for id in (THEME, STORE):
        t.gitpm_ok("install", t.repo(id))


@case(spec=["plugin-metadata/reported-shape", "plugin-metadata/metadata-keys#string-provides",
            "plugin-metadata/metadata-in-state"])
def list_reports_all_four_keys(t: T):
    """list: metadata in resolved[] and in packages.<id>.metadata; a string provides is an
    array; absent keys are null / []; the author's plugin.json keeps the string form."""
    two_plugins(t)
    value = t.gitpm_ok("list").json()
    theme = resolved(t, value, THEME)
    t.eq({k: theme[k] for k in ("title", "description", "provides", "tags")},
         {"title": "Theme", "description": "Dark and light", "provides": ["theme"], "tags": ["ui", "colors"]},
         "theme metadata in resolved[]")
    store = resolved(t, value, STORE)
    t.eq({k: store[k] for k in ("title", "description", "provides", "tags")},
         {"title": None, "description": None, "provides": [], "tags": []}, "absent metadata shape")
    for id, entry in [(THEME, theme), (STORE, store)]:
        stored = value["packages"][id]["metadata"]
        t.eq(sorted(stored), ["description", "provides", "tags", "title"], f"{id} state metadata keys")
        t.eq({k: stored[k] for k in stored}, {k: entry[k] for k in stored}, f"{id} same shape in state")
    t.eq(json.loads((t.worktree(THEME) / "plugin.json").read_text())["provides"], "theme",
         "the manifest keeps the string form")


@case(spec=["plugin-metadata/reported-shape#only-a-title"])
def only_a_title(t: T):
    """A package with only `title`: description null, provides and tags []."""
    t.new_package("dan/titled", "1.0.0", extra={"title": "Titled"})
    t.gitpm_ok("install", t.repo("dan/titled"))
    entry = resolved(t, t.gitpm_ok("list").json(), "dan/titled")
    t.eq((entry["title"], entry["description"], entry["provides"], entry["tags"]), ("Titled", None, [], []), "shape")


@case(spec=["plugin-metadata/filter-by-provided-service"])
def list_provides_filters_plugins_and_resolved(t: T):
    """list --provides theme keeps only tilr/theme in packages and resolved; a service nobody
    provides gives empty lists; --provides on another command is a usage error."""
    two_plugins(t)
    only = t.gitpm_ok("list", "--provides", "theme").json()
    t.eq(ids(only), [THEME], "resolved filtered")
    t.eq(sorted(only["packages"]), [THEME], "packages filtered")
    none = t.gitpm_ok("list", "--provides", "store").json()
    t.eq((ids(none), sorted(none["packages"])), ([], []), "nothing provides store")
    before = t.snap(t.ws)
    t.gitpm_err("check", "--provides", "theme", contains="--provides requires list")
    t.same(before, "workspace after a usage error")


@case(spec=["plugin-metadata/filter-by-provided-service#replaced-original-still-listed"])
def replaced_original_still_listed(t: T):
    """me/dark replaces tilr/theme, both provide `theme`: list --provides theme lists both,
    with the original inactive."""
    t.new_package(THEME, "1.0.0", extra=THEME_META)
    t.gitpm_ok("install", t.repo(THEME))
    t.new_package(DARK, "1.0.0", extra={"provides": ["theme"], "replaces": {"id": THEME, "version": "1.0.0"}})
    t.gitpm_ok("install", t.repo(DARK))
    t.gitpm_ok("replace", THEME, "--with", DARK)
    value = t.gitpm_ok("list", "--provides", "theme").json()
    t.eq(ids(value), [DARK, THEME], "both providers listed")
    t.eq(resolved(t, value, THEME)["active"], False, "replaced original inactive")
    t.eq(resolved(t, value, DARK)["active"], True, "fork active")
    t.eq(value.get("replacements"), {THEME: DARK}, "replacements printed unfiltered")


@case(spec=["plugin-metadata/lenient-reading-strict-authoring#old-release-with-bad-tags"])
def wrong_typed_tags_ignored_on_install(t: T):
    """A release with "tags": "ui" installs; tags is [] and stderr warns."""
    t.new_package("dan/bad", "1.0.0", extra={"tags": "ui"})
    r = t.gitpm_ok("install", t.repo("dan/bad"))
    t.has(r.err, "Warning: dan/bad 1.0.0: plugin.json tags must be an array of strings; ignored")
    t.eq(resolved(t, t.gitpm_ok("list").json(), "dan/bad")["tags"], [], "tags ignored")


@case(spec=["plugin-metadata/lenient-reading-strict-authoring#authoring-with-bad-tags"])
def wrong_typed_tags_refused_by_commit(t: T):
    """commit with "tags": "ui" fails with invalid_manifest; nothing is committed or tagged."""
    t.new_package("dan/bad", "1.0.0")
    repo = t.repo("dan/bad")
    manifest = json.loads((repo / "plugin.json").read_text())
    manifest["tags"] = "ui"
    t.write(repo, {"plugin.json": json.dumps(manifest, indent=2) + "\n"})
    head = t.rev(repo)
    t.gitpm_err("commit", repo, "1.1.0", kind="invalid_manifest")
    t.eq(t.rev(repo), head, "no commit")
    t.eq(t.git(repo, "tag", "--list"), "v1.0.0", "no new tag")


@case(spec=["plugin-metadata/metadata-survives-rewrites#fork-keeps-the-title"])
def fork_and_commit_keep_metadata(t: T):
    """fork tilr/theme as me/theme, then commit 1.0.1: title, description, provides (string
    form) and tags are unchanged while id/version/forkOf are rewritten."""
    t.new_package(THEME, "1.0.0", extra=THEME_META)
    t.gitpm_ok("install", t.repo(THEME))
    fork = t.root / "forks/theme"
    t.gitpm_ok("fork", THEME, "--ns", "me", "--into", fork)
    t.gitpm_ok("commit", fork, "1.0.1")
    m = json.loads(t.git(fork, "show", "HEAD:plugin.json"))
    t.eq((m["id"], m["version"], m["forkOf"]["id"]), ("me/theme", "1.0.1", THEME), "rewritten keys")
    t.eq({k: m[k] for k in THEME_META}, THEME_META, "metadata kept exactly")


@case(spec=["plugin-metadata/metadata-in-the-catalog#info-shows-metadata"])
def info_shows_latest_release_metadata(t: T):
    """register two releases; info prints Title/Description/Provides/Tags of the latest
    (1.1.0) under the package; a package without metadata prints no such lines."""
    t.new_package(THEME, "1.0.0", extra={"title": "Old"})
    t.release(THEME, "1.1.0", extra=THEME_META)
    t.new_package(STORE, "1.0.0")
    for ref in (f"v1.0.0", f"v1.1.0"):
        t.gitpm_ok("register", t.repo(THEME), "--ref", ref)
    t.gitpm_ok("register", t.repo(STORE), "--ref", f"v1.0.0")
    out = t.gitpm_ok("info").out
    lines = out.splitlines()
    i = next(n for n, l in enumerate(lines) if l.strip().startswith(f"{THEME} — latest: 1.1.0"))
    block = "\n".join(lines[i:i + 6])
    t.has(block, "Title: Theme", "Description: Dark and light", "Provides: theme", "Tags: ui, colors")
    t.lacks(out, "Title: Old")
    j = next(n for n, l in enumerate(lines) if l.strip().startswith(f"{STORE} — latest: 1.0.0"))
    t.ok(not any(l.strip().startswith(("Title:", "Provides:", "Tags:", "Description:")) for l in lines[j + 1:j + 3]),
         "no metadata lines for tilr/store")
    release = json.loads(t.catalog_release(THEME, "1.1.0").read_text())
    t.eq({k: release["manifest"][k] for k in THEME_META}, THEME_META, "release.json carries the metadata")


@case(spec=["plugin-metadata/metadata-in-the-catalog"])
def broken_latest_release_keeps_index_metadata(t: T):
    """A broken release record does not affect display metadata from the index."""
    t.new_package(THEME, "1.0.0", extra=THEME_META)
    t.gitpm_ok("register", t.repo(THEME), "--ref", f"v1.0.0")
    t.catalog_release(THEME, "1.0.0").write_text("{broken")
    out = t.gitpm_ok("info").out
    t.has(out, f"{THEME} — latest: 1.0.0")
    t.has(out, "Title: Theme")


@case(spec=["plugin-metadata/metadata-in-the-catalog#overview-without-per-package-release-reads",
            "plugin-metadata/metadata-in-the-catalog#one-broken-summary", "catalog/bounded-overview"])
def browse_reads_only_indexes(t: T):
    """A remote overview reads 40 indexes once each, no releases; bad summaries stay local."""
    t.catalog.mkdir(parents=True, exist_ok=True)
    directories = {}
    for n in range(45):
        name = f"p{n:02}"
        directories[name] = f"{name}/"
        folder = t.catalog / name
        folder.mkdir()
        summary = {"version": "1.0.0", "title": name, "description": None, "provides": [], "tags": []}
        if n == 0:
            summary["provides"] = False
        index = {"schema": 1, "id": f"tilr/{name}", "source": "https://example.invalid/source.git",
                 "versions": {"1.0.0": "versions/1.0.0/release.json"}, "latest": summary}
        if n == 1:
            del index["latest"]
        (folder / "plugin.json").write_text(json.dumps(index))
    (t.catalog / "catalog.json").write_text(json.dumps({"schema": 1, "directories": directories}))
    server = t.serve(t.catalog)
    try:
        out = t.gitpm_ok("--catalog", server.url, "info").out
        paths = server.take()
    finally:
        server.httpd.shutdown()
        server.httpd.server_close()
    t.eq(paths, ["/catalog.json"] + [f"/p{n:02}/plugin.json" for n in range(40)], "bounded index requests only")
    t.has(out, "tilr/p00", "tilr/p01", "Title: p02", "Title: p39")
    t.lacks(out, "Title: p00", "Title: p01", "tilr/p40")
