"""UC-025: Fork invalid destinations (P0).

vendor/demo 0.1.0 installed (g3_vendor_installed); legacy `demo` installed for the
legacy-to-scoped fork (g3_legacy_installed). `fork ID --ns NS --into DIR` accepts only a new
folder or a repository with gitpm.json; every refusal is checked for: non-zero exit, Error
kind, no destination created, and byte-identical workspace + author repositories.
"""

import json
import os

from gitpmtest import T, case

ID = "vendor/demo"


def refused(t: T, args: list, kind: str, contains, created=()):
    before = t.snap(t.ws, t.root / "repos", git=True)
    t.gitpm_err("fork", *args, kind=kind, contains=contains)
    t.same(before, "workspace and author repositories")
    for path in created:
        t.ok(not os.path.lexists(path), f"{path} not created")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-requires-explicit-namespace-and-destination#same-namespace"])
def same_namespace(t: T):
    """Fork into the same namespace (same ID) is refused."""
    dest = t.root / "authors/same"
    refused(t, [ID, "--ns", "vendor", "--into", dest], "invalid_argument", "different namespace", [dest])


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-requires-explicit-namespace-and-destination#missing-ns"])
def missing_namespace(t: T):
    """Without --ns nothing is derived from the ID: refused, nothing created."""
    dest = t.root / "authors/none"
    before = t.snap(t.ws, t.root / "repos", git=True)
    t.gitpm_err("fork", ID, "--into", dest, contains="fork needs --ns NAMESPACE")
    t.same(before, "workspace and author repositories")
    t.ok(not os.path.lexists(dest), f"{dest} not created")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-requires-explicit-namespace-and-destination#version-without-replace"])
def version_without_replace(t: T):
    """--version is only valid with --replace."""
    dest = t.root / "authors/none"
    before = t.snap(t.ws, t.root / "repos", git=True)
    t.gitpm_err("fork", ID, "--ns", "author", "--into", dest, "--version", "2.0.0",
                contains="--version is not valid for fork")
    t.same(before, "workspace and author repositories")
    t.ok(not os.path.lexists(dest), f"{dest} not created")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-copies-an-installed-package"])
def unknown_plugin(t: T):
    dest = t.root / "authors/none"
    refused(t, ["dan/missing", "--ns", "author", "--into", dest], "not_installed",
            "plugin dan/missing is not installed", [dest])


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-requires-explicit-namespace-and-destination"])
def existing_destination_with_sentinel(t: T):
    dest = t.root / "authors/existing"
    t.write(dest, {"sentinel.txt": "keep me\n"})
    sentinel = t.snap(dest)
    refused(t, [ID, "--ns", "author", "--into", dest], "invalid_argument", "destination already exists and is not a multi-package repository (gitpm.json)")
    t.same(sentinel, "destination with sentinel")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-requires-explicit-namespace-and-destination"])
def empty_existing_destination(t: T):
    dest = t.root / "authors/empty"
    dest.mkdir(parents=True)
    refused(t, [ID, "--ns", "author", "--into", dest], "invalid_argument", "destination already exists and is not a multi-package repository (gitpm.json)")
    t.eq(list(dest.iterdir()), [], "still empty")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-requires-explicit-namespace-and-destination"])
def dangling_symlink_destination(t: T):
    dest = t.root / "authors/link"
    dest.parent.mkdir(parents=True)
    dest.symlink_to(t.root / "nowhere")
    refused(t, [ID, "--ns", "author", "--into", dest], "invalid_argument", "destination already exists and is not a multi-package repository (gitpm.json)")
    t.ok(not (t.root / "nowhere").exists(), "symlink target not created")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-requires-explicit-namespace-and-destination"])
def source_repository_as_destination(t: T):
    """The original's source (author repository) is never a destination."""
    refused(t, [ID, "--ns", "author", "--into", t.repo(ID)], "invalid_argument",
            "is the source repository of the forked package")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-requires-explicit-namespace-and-destination#destination-inside-the-workspace"])
def installed_clone_as_destination(t: T):
    refused(t, [ID, "--ns", "author", "--into", t.clone(ID)], "invalid_argument",
            "fork destination must be outside the managed workspace")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-requires-explicit-namespace-and-destination#destination-inside-the-workspace"])
def new_path_inside_workspace(t: T):
    """Non-existing managed paths: plugins/author/demo, dist/author/demo, ws/forks/x."""
    for dest in [t.ws / "plugins/author/demo", t.ws / "dist/author/demo", t.ws / "forks/x"]:
        refused(t, [ID, "--ns", "author", "--into", dest], "invalid_argument",
                "outside the managed workspace", [dest])
    t.ok(not (t.ws / "plugins/author").exists() and not (t.ws / "forks").exists(), "no parents created")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-requires-explicit-namespace-and-destination#destination-inside-the-workspace"])
def workspace_through_symlink(t: T):
    link = t.root / "ws-link"
    link.symlink_to(t.ws)
    refused(t, [ID, "--ns", "author", "--into", link / "forks/x"], "invalid_argument",
            "outside the managed workspace", [t.ws / "forks"])


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-requires-explicit-namespace-and-destination"])
def parent_dir_component(t: T):
    dest = t.root / "authors/../escape"
    refused(t, [ID, "--ns", "author", "--into", dest], "invalid_argument", "'..'", [t.root / "escape"])


@case(fixture="g3_legacy_installed", spec=["fork-replacement/fork-metadata#fork-manifest", "package-ids/release-tag-names#legacy-tag"])
def legacy_to_scoped_fork(t: T):
    """Legacy `demo` forks into dan/demo with legacy provenance; both coexist."""
    installed = t.plugin("demo")["current"]
    dest = t.root / "authors/dan-demo"
    t.gitpm_ok("fork", "demo", "--ns", "dan", "--into", dest)
    m = json.loads((dest / "plugin.json").read_text())
    t.eq(m["id"], "dan/demo", "id")
    t.eq(m["forkOf"], {"id": "demo", "version": "0.1.0", "commit": installed["commit"]}, "forkOf")
    t.eq(m["replaces"], {"id": "demo", "version": "0.1.0"}, "replaces")
    before = t.snap(t.clone("demo"), t.dist("demo"), git=True)
    t.gitpm_ok("install", dest, "--channel", "dev")
    p = t.plugin("dan/demo")
    t.eq(p["forkOf"], m["forkOf"], "installed forkOf")
    t.eq(t.plugin("demo")["current"], installed, "legacy installation unchanged")
    t.same(before, "legacy clone and dist")
    t.has((t.dist("dan/demo") / "index.js").read_text(), "demo 0.1.0")
